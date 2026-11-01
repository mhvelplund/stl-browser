//! Drag files out of the app as files that are written only when the drop target asks for
//! them: Windows OLE virtual files (`windows`), macOS file promises (`macos`). On other targets
//! only the platform-neutral types and the `promise` writer are built.

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{Prepared, prepare, primary_button_down, run_drag};

/// One folder or file of the drag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    /// `/`-separated, relative to the drop folder.
    pub rel: String,
    pub is_dir: bool,
    pub size: Option<u64>,
}

/// Writes a file's bytes when the drop target asks for them. `index` is the file's index in the
/// descriptor list given to `prepare`. Called on worker threads, possibly concurrently.
pub trait Contents: Send + Sync + 'static {
    fn write(&self, index: usize, out: &mut dyn std::io::Write) -> Result<(), String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOutcome {
    /// The button was already up: no drag was started.
    Skipped,
    Dropped,
    Cancelled,
    /// macOS: the drag was started and goes on after `run_drag` returns.
    Started,
}
