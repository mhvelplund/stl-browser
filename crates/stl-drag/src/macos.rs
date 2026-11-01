//! Drag files out to Finder as file promises: one `NSFilePromiseProvider` per top-level item.
//! Finder asks the delegate to write each item into the drop folder, on a background
//! `NSOperationQueue`, so the extraction never blocks the UI. The writing itself is
//! `promise::Promises`, which is platform-neutral and tested everywhere.
use std::cell::RefCell;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;

use block2::DynBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, available, define_class, msg_send};
use objc2_app_kit::{
    NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent, NSEventModifierFlags,
    NSEventType, NSFilePromiseProvider, NSFilePromiseProviderDelegate, NSView, NSWorkspace,
};
use objc2_foundation::{
    NSArray, NSDictionary, NSError, NSLocalizedDescriptionKey, NSNumber, NSObject, NSObjectProtocol, NSOperationQueue, NSPoint,
    NSProcessInfo, NSRect, NSSize, NSString, NSURL, ns_string,
};
use objc2_uniform_type_identifiers::{UTType, UTTypeData};

use crate::promise::{Promises, TopItem};
use crate::{Contents, Descriptor, DragOutcome};

/// A drag's promises, waiting for `run_drag` on the main thread.
pub struct Prepared {
    promises: Arc<Promises>,
    /// Always 0: promises have no path length limit.
    pub too_long: usize,
}

pub fn prepare(items: Vec<Descriptor>, contents: Arc<dyn Contents>) -> Result<Prepared, String> {
    Ok(Prepared { promises: Arc::new(Promises::new(items, contents)), too_long: 0 })
}

thread_local! {
    /// The sources (each holding its delegate, which AppKit keeps only weakly) of every drag
    /// that may still be asked for files. When a drag starts, earlier drags that never began
    /// writing are released (a drop target that never asked for its files would otherwise keep
    /// the read-ahead cache), and finished ones are dropped. A drag being written stays alive
    /// until its last item is done.
    static DRAGS: RefCell<Vec<Retained<Source>>> = const { RefCell::new(Vec::new()) };
}

/// Start the drag from `ns_view` (the window's content view); call on the main thread. Returns
/// once the drag has started: Finder asks for the files after the drop.
pub fn run_drag(p: Prepared, ns_view: *mut c_void) -> Result<DragOutcome, String> {
    let mtm = MainThreadMarker::new().ok_or("the drag must start on the main thread")?;
    if NSEvent::pressedMouseButtons() & 1 == 0 {
        return Ok(DragOutcome::Skipped);
    }
    if ns_view.is_null() {
        return Err("no window to drag from".into());
    }
    // SAFETY: Tauri's `ns_view` is the window's live content view, used on the main thread.
    let view: &NSView = unsafe { &*(ns_view as *const NSView) };
    let window = view.window().ok_or("the view has no window")?;
    DRAGS.with(|d| {
        d.borrow_mut().retain(|s| {
            let p = &s.ivars().promises;
            p.release_if_idle();
            !p.finished()
        })
    });

    let at = window.mouseLocationOutsideOfEventStream();
    let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        NSEventType::LeftMouseDragged,
        at,
        NSEventModifierFlags::empty(),
        NSProcessInfo::processInfo().systemUptime(),
        window.windowNumber(),
        None,
        0,
        1,
        1.0,
    )
    .ok_or("could not make the drag event")?;

    let delegate = Delegate::new(p.promises.clone());
    let origin = view.convertPoint_fromView(at, None);
    let workspace = NSWorkspace::sharedWorkspace();
    let items: Vec<Retained<NSDraggingItem>> = p
        .promises
        .items()
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let provider = NSFilePromiseProvider::initWithFileType_delegate(
                NSFilePromiseProvider::alloc(),
                &promise_type(t),
                ProtocolObject::from_ref(&*delegate),
            );
            let index = NSNumber::numberWithUnsignedInteger(i);
            // SAFETY: the delegate reads `userInfo` back as an `NSNumber`.
            unsafe { provider.setUserInfo(Some(&index)) };
            let item = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), ProtocolObject::from_ref(&*provider));
            let icon_type = match (t.is_dir, Path::new(&t.name).extension()) {
                (false, Some(ext)) => NSString::from_str(&ext.to_string_lossy()),
                (false, None) => NSString::from_str("public.data"),
                (true, _) => NSString::from_str("public.folder"),
            };
            #[allow(deprecated)]
            let icon = workspace.iconForFileType(&icon_type);
            let off = 4.0 * i.min(8) as f64;
            let frame = NSRect::new(NSPoint::new(origin.x - 16.0 + off, origin.y - 16.0 - off), NSSize::new(32.0, 32.0));
            let contents: &AnyObject = &icon;
            // SAFETY: an `NSImage` is a valid dragging image.
            unsafe { item.setDraggingFrame_contents(frame, Some(contents)) };
            item
        })
        .collect();
    let items = NSArray::from_retained_slice(&items);

    let source = Source::new(p.promises, delegate, mtm);
    view.beginDraggingSessionWithItems_event_source(&items, &event, ProtocolObject::from_ref(&*source));
    DRAGS.with(|d| d.borrow_mut().push(source));
    Ok(DragOutcome::Started)
}

/// The type a promise declares: `public.folder`, the type macOS knows for the file's extension
/// (macOS 11+), or `public.data`.
fn promise_type(t: &TopItem) -> Retained<NSString> {
    if t.is_dir {
        return NSString::from_str("public.folder");
    }
    if available!(macos = 11.0)
        && let Some(ext) = Path::new(&t.name).extension()
        && let Some(ut) = UTType::typeWithFilenameExtension(&NSString::from_str(&ext.to_string_lossy()))
        // SAFETY: `UTTypeData` is a constant the system defines on macOS 11+.
        && !ut.isDynamic() && ut.conformsToType(unsafe { UTTypeData })
    {
        return ut.identifier();
    }
    NSString::from_str("public.data")
}

struct DelegateIvars {
    promises: Arc<Promises>,
    queue: Retained<NSOperationQueue>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `Delegate` does not implement Drop.
    #[unsafe(super(NSObject))]
    #[ivars = DelegateIvars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSFilePromiseProviderDelegate for Delegate {
        #[unsafe(method_id(filePromiseProvider:fileNameForType:))]
        fn file_name(&self, provider: &NSFilePromiseProvider, _file_type: &NSString) -> Retained<NSString> {
            // a panic must not unwind into AppKit
            let name = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                item_of(provider).and_then(|i| self.ivars().promises.items().get(i)).map(|t| t.name.clone())
            }));
            NSString::from_str(name.ok().flatten().as_deref().unwrap_or("item"))
        }

        #[unsafe(method(filePromiseProvider:writePromiseToURL:completionHandler:))]
        fn write_promise(&self, provider: &NSFilePromiseProvider, url: &NSURL, done: &DynBlock<dyn Fn(*mut NSError)>) {
            // a panic must not unwind into AppKit, and the completion handler must always be called
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match (item_of(provider), url.path()) {
                (Some(i), Some(path)) => self.ivars().promises.write(i, Path::new(&path.to_string())),
                _ => Err("Finder asked for an unknown item".into()),
            }))
            .unwrap_or_else(|_| Err("internal error".into()));
            match r {
                Ok(_) => done.call((std::ptr::null_mut(),)),
                Err(e) => {
                    let message = NSString::from_str(&e);
                    let info = NSDictionary::from_slices(&[unsafe { NSLocalizedDescriptionKey }], &[&*message as &AnyObject]);
                    // SAFETY: the user info maps an error key to a string.
                    let error = unsafe { NSError::errorWithDomain_code_userInfo(ns_string!("stl-browser"), 1, Some(&info)) };
                    done.call((Retained::as_ptr(&error) as *mut NSError,));
                }
            }
        }

        #[unsafe(method_id(operationQueueForFilePromiseProvider:))]
        fn queue(&self, _provider: &NSFilePromiseProvider) -> Retained<NSOperationQueue> {
            self.ivars().queue.clone()
        }
    }
);

impl Delegate {
    fn new(promises: Arc<Promises>) -> Retained<Self> {
        let queue = NSOperationQueue::new();
        queue.setMaxConcurrentOperationCount(1);
        let this = Self::alloc().set_ivars(DelegateIvars { promises, queue });
        // SAFETY: NSObject's `init` takes no arguments.
        unsafe { msg_send![super(this), init] }
    }
}

/// The item index stored in a provider's `userInfo`.
fn item_of(provider: &NSFilePromiseProvider) -> Option<usize> {
    provider.userInfo()?.downcast::<NSNumber>().ok().map(|n| n.unsignedIntegerValue())
}

struct SourceIvars {
    promises: Arc<Promises>,
    _delegate: Retained<Delegate>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `Source` does not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = SourceIvars]
    struct Source;

    unsafe impl NSObjectProtocol for Source {}

    unsafe impl NSDraggingSource for Source {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn operations(&self, _session: &NSDraggingSession, _context: NSDraggingContext) -> NSDragOperation {
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn ended(&self, _session: &NSDraggingSession, _at: NSPoint, operation: NSDragOperation) {
            if operation == NSDragOperation::None {
                self.ivars().promises.release();
            }
        }
    }
);

impl Source {
    fn new(promises: Arc<Promises>, delegate: Retained<Delegate>, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(SourceIvars { promises, _delegate: delegate });
        // SAFETY: NSObject's `init` takes no arguments.
        unsafe { msg_send![super(this), init] }
    }
}
