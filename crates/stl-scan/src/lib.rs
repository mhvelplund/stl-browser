//! Native archive reading (ZIP, 7Z, RAR), nested listing, extraction and source discovery.
pub mod archive;
pub mod discover;
pub mod extract;
pub mod nested;
mod error;

pub use error::ScanError;

// unrar_sys's C++ code calls Crypt*/Reg*/OpenProcessToken but doesn't link their library.
#[cfg(windows)]
#[link(name = "advapi32")]
unsafe extern "system" {}
