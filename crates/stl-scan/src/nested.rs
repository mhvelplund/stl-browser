//! List an archive including the archives nested inside it. A nested archive is one more path
//! segment (`a.zip/b.7z/c.stl`); its member chain says how to extract it (outermost first).
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use stl_core::names::{self, Kind};
use tempfile::NamedTempFile;

use crate::archive::{self, Archive, ArchiveInput};
use crate::error::ScanError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedEntry {
    pub path: String,
    pub chain: Vec<String>,
    pub size: u64,
    pub crc: String,
}

#[derive(Debug, Clone)]
pub struct ListOptions {
    /// How many archive levels below the outer one are opened.
    pub max_depth: usize,
    /// Nested archives up to this size are held in memory; larger ones go to a temp file.
    pub mem_limit: u64,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self { max_depth: 3, mem_limit: 256 * 1024 * 1024 }
    }
}

static SPILLS: AtomicUsize = AtomicUsize::new(0);

/// How many nested archives were spilled to temp files so far (diagnostics and tests).
pub fn spill_count() -> usize {
    SPILLS.load(Ordering::Relaxed)
}

/// List the archive at `path`. `Err` only when the outer archive fails; a nested archive that
/// can't be read adds a `"nested <path>: <error>"` warning and keeps its own entry.
pub fn list_archive(path: &Path, opts: &ListOptions) -> Result<(Vec<ScannedEntry>, Vec<String>), ScanError> {
    let mut outer = archive::open(ArchiveInput::Path(path.to_path_buf()))?;
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    list_into(outer.as_mut(), "", &[], 0, opts, &mut out, &mut warnings)?;
    Ok((out, warnings))
}

fn list_into(
    a: &mut dyn Archive,
    prefix: &str,
    chain: &[String],
    depth: usize,
    opts: &ListOptions,
    out: &mut Vec<ScannedEntry>,
    warnings: &mut Vec<String>,
) -> Result<(), ScanError> {
    let members = a.list()?;
    for m in &members {
        out.push(ScannedEntry {
            path: format!("{prefix}{}", m.path),
            chain: chain.iter().cloned().chain([m.raw_name.clone()]).collect(),
            size: m.size,
            crc: m.crc.clone(),
        });
    }
    if depth >= opts.max_depth {
        return Ok(());
    }
    for m in members.iter().filter(|m| names::kind_of(&m.path) == Kind::Archive) {
        let path = format!("{prefix}{}", m.path);
        let sub_chain: Vec<String> = chain.iter().cloned().chain([m.raw_name.clone()]).collect();
        let r = open_member(a, &m.raw_name, opts).and_then(|(mut child, _temp)| {
            let mut sub_out = Vec::new();
            list_into(child.as_mut(), &format!("{path}/"), &sub_chain, depth + 1, opts, &mut sub_out, warnings)?;
            out.extend(sub_out);
            Ok(())
        });
        if let Err(e) = r {
            warnings.push(format!("nested {path}: {e}"));
        }
    }
    Ok(())
}

/// Open the archive stored as member `raw_name` of `parent`. It is held in memory unless it
/// grows past `opts.mem_limit` or is a RAR (which must be read from disk); then it is spilled
/// to a temp file that the caller keeps alive as long as the returned archive is used.
pub(crate) fn open_member(
    parent: &mut dyn Archive,
    raw_name: &str,
    opts: &ListOptions,
) -> Result<(Box<dyn Archive>, Option<NamedTempFile>), ScanError> {
    let ext = names::ext(raw_name);
    let limit = if ext == ".rar" { 0 } else { opts.mem_limit };
    let mut w = SpillWriter { mem: Vec::new(), file: None, limit, ext: ext.clone() };
    parent.extract(raw_name, &mut w)?;
    match w.file {
        Some(mut f) => {
            f.flush()?;
            SPILLS.fetch_add(1, Ordering::Relaxed);
            let a = archive::open(ArchiveInput::Path(f.path().to_path_buf()))?;
            Ok((a, Some(f)))
        }
        None => Ok((archive::open(ArchiveInput::Bytes(Arc::new(w.mem), format!("member{ext}")))?, None)),
    }
}

/// Buffers in memory until `limit` bytes, then moves everything to a temp file.
struct SpillWriter {
    mem: Vec<u8>,
    file: Option<NamedTempFile>,
    limit: u64,
    ext: String,
}

impl Write for SpillWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.file.is_none() && (self.mem.len() + buf.len()) as u64 > self.limit {
            let mut f = tempfile::Builder::new().prefix("stl-browser-").suffix(&self.ext).tempfile()?;
            f.write_all(&self.mem)?;
            self.mem = Vec::new();
            self.file = Some(f);
        }
        match &mut self.file {
            Some(f) => f.write_all(buf)?,
            None => self.mem.extend_from_slice(buf),
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.file {
            Some(f) => f.flush(),
            None => Ok(()),
        }
    }
}
