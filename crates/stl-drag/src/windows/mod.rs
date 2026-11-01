//! Drag files out to Explorer as Windows virtual files: an `IDataObject` offering
//! `CFSTR_FILEDESCRIPTORW` and one lazily written `CFSTR_FILECONTENTS` stream per file, with
//! `IDataObjectAsyncCapability` so Explorer copies on its own thread.
//!
//! The object lives in the MTA and the drag (on the OLE STA, Tauri's main thread) uses a
//! marshaled proxy, so Explorer's calls, and the extraction behind them, run on COM worker
//! threads instead of the UI thread.

mod data;
mod source;
mod stream;

use std::mem::ManuallyDrop;
use std::sync::{Arc, OnceLock};

use windows::Win32::System::Com::Marshal::CoReleaseMarshalData;
use windows::Win32::System::Com::Marshal::CoMarshalInterThreadInterfaceInStream;
use windows::Win32::System::Com::StructuredStorage::CoGetInterfaceAndReleaseStream;
use windows::Win32::System::Com::{CoIncrementMTAUsage, IDataObject, IStream};
use windows::Win32::System::Ole::{DROPEFFECT, DROPEFFECT_COPY, DoDragDrop, IDropSource};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};
use windows::Win32::Foundation::DRAGDROP_S_DROP;
use windows_core::Interface;

use crate::{Contents, Descriptor, DragOutcome};

/// A data object marshaled out of the MTA, waiting to be used on an STA thread.
pub struct Prepared {
    stream: Option<IStream>,
    /// Descriptors left out because their path is too long for a file descriptor.
    pub too_long: usize,
}

// The marshal stream from CoMarshalInterThreadInterfaceInStream is meant to cross threads.
unsafe impl Send for Prepared {}

impl Prepared {
    /// Unmarshal the proxy on the current thread.
    pub fn data_object(mut self) -> windows_core::Result<IDataObject> {
        // CoGetInterfaceAndReleaseStream releases the stream itself
        let stream = ManuallyDrop::new(self.stream.take().expect("not unmarshaled yet"));
        unsafe { CoGetInterfaceAndReleaseStream(&*stream) }
    }
}

impl Drop for Prepared {
    fn drop(&mut self) {
        if let Some(s) = self.stream.take() {
            unsafe {
                let _ = CoReleaseMarshalData(&s);
            }
        }
    }
}

static MTA: OnceLock<Result<(), String>> = OnceLock::new();

/// Create the data object in the MTA (kept alive for the life of the process) and marshal it.
/// Descriptors whose path is too long for a file descriptor are left out; the kept ones get `\`
/// separators.
pub fn prepare(items: Vec<Descriptor>, contents: Arc<dyn Contents>) -> Result<Prepared, String> {
    MTA.get_or_init(|| unsafe { CoIncrementMTAUsage().map(|_| ()).map_err(|e| e.to_string()) }).clone()?;
    let before = items.len();
    let kept: Vec<(usize, Descriptor)> = items
        .into_iter()
        .enumerate()
        .filter(|(_, d)| data::fits(&d.rel))
        .map(|(i, d)| (i, Descriptor { rel: d.rel.replace('/', "\\"), ..d }))
        .collect();
    let too_long = before - kept.len();
    // a fresh thread is in the (implicit) MTA, whatever apartment the caller is in
    let stream = std::thread::spawn(move || -> Result<SendStream, String> {
        let obj = data::DataObject::create(kept, contents);
        let s = unsafe { CoMarshalInterThreadInterfaceInStream(&IDataObject::IID, &obj) }.map_err(|e| e.to_string())?;
        Ok(SendStream(s))
    })
    .join()
    .map_err(|_| "creating the drag data panicked".to_string())??;
    Ok(Prepared { stream: Some(stream.0), too_long })
}

struct SendStream(IStream);
unsafe impl Send for SendStream {}

/// Whether the primary mouse button is down, from the physical button states.
pub fn primary_button_down(swapped: bool, left: bool, right: bool) -> bool {
    if swapped { right } else { left }
}

/// Run the drag; call on an OLE STA thread (Tauri's main thread). Returns when the drop is done
/// on our side; with async copy, Explorer goes on extracting through the MTA object.
pub fn run_drag(p: Prepared) -> Result<DragOutcome, String> {
    let down = unsafe {
        let swapped = GetSystemMetrics(SM_SWAPBUTTON) != 0;
        let pressed = |vk: i32| (GetAsyncKeyState(vk) as u16 & 0x8000) != 0;
        primary_button_down(swapped, pressed(VK_LBUTTON.0 as i32), pressed(VK_RBUTTON.0 as i32))
    };
    if !down {
        return Ok(DragOutcome::Skipped);
    }
    let obj = p.data_object().map_err(|e| e.to_string())?;
    let source: IDropSource = source::DropSource.into();
    let mut effect = DROPEFFECT(0);
    let hr = unsafe { DoDragDrop(&obj, &source, DROPEFFECT_COPY, &mut effect) };
    if hr.is_err() {
        return Err(hr.message());
    }
    Ok(if hr == DRAGDROP_S_DROP && effect != DROPEFFECT(0) { DragOutcome::Dropped } else { DragOutcome::Cancelled })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_button_down_honours_swap() {
        assert!(primary_button_down(false, true, false));
        assert!(!primary_button_down(false, false, true));
        assert!(!primary_button_down(true, true, false));
        assert!(primary_button_down(true, false, true));
    }
}
