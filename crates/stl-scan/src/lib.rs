//! Native archive reading (ZIP, 7Z, RAR), nested listing, extraction and source discovery.
pub mod archive;
mod error;

pub use error::ScanError;
