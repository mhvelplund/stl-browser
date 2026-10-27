use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad archive: {0}")]
    Format(String),
    #[error("archive is encrypted")]
    Encrypted,
    #[error("unsupported: {0}")]
    Unsupported(String),
}
