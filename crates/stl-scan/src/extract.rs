//! Extract one indexed file, following its chain through nested archives.
use std::io::Write;
use std::path::Path;

use crate::archive::{self, ArchiveInput};
use crate::error::ScanError;
use crate::nested::{ListOptions, open_member};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Archive,
    Folder,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Archive => "archive",
            SourceKind::Folder => "folder",
        }
    }
}

/// Write the file named by `chain` to `out`; returns the byte count. For a folder source,
/// `chain[0]` is the path relative to `source_path`.
pub fn extract_entry(kind: SourceKind, source_path: &Path, chain: &[String], out: &mut dyn Write) -> Result<u64, ScanError> {
    let (last, outer) = chain.split_last().ok_or_else(|| ScanError::Format("empty chain".into()))?;
    let mut counter = CountingWriter { inner: out, n: 0 };
    if kind == SourceKind::Folder {
        let mut f = std::fs::File::open(source_path.join(last))?;
        std::io::copy(&mut f, &mut counter)?;
        return Ok(counter.n);
    }
    let opts = ListOptions::default();
    let mut current = archive::open(ArchiveInput::Path(source_path.to_path_buf()))?;
    let mut _temps = Vec::new(); // spilled nested archives must outlive their readers
    for member in outer {
        let (next, temp) = open_member(current.as_mut(), member, &opts)?;
        current = next;
        _temps.push(temp);
    }
    current.extract(last, &mut counter)?;
    Ok(counter.n)
}

struct CountingWriter<'a> {
    inner: &'a mut dyn Write,
    n: u64,
}

impl Write for CountingWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let k = self.inner.write(buf)?;
        self.n += k as u64;
        Ok(k)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
