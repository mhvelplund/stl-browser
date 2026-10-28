//! Exports: find where each file of an export plan lives, and write a plan to a folder (the
//! Extract to… path; a drag-out extracts the same files one by one through `stl-drag`).
use std::collections::HashMap;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, OptionalExtension};
use stl_core::names;
use stl_core::vtree::export::{ExportKind, ExportPlan};
use stl_scan::extract::SourceKind;
use stl_scan::session::ExtractSession;

use crate::scanstore::IndexError;

/// Where an entry's bytes are: enough to extract it without trusting its id again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryLoc {
    pub kind: SourceKind,
    pub source_path: String,
    pub chain: Vec<String>,
    pub size: Option<u64>,
}

/// The locations of `ids`; unknown ids are left out.
pub fn locate(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, EntryLoc>, IndexError> {
    let mut st = conn.prepare_cached(
        "SELECT e.chain, e.size, s.kind, s.path FROM entries e JOIN sources s ON s.id = e.source_id WHERE e.id = ?",
    )?;
    let mut out = HashMap::new();
    for &id in ids {
        let row = st
            .query_row([id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
            })
            .optional()?;
        let Some((chain, size, kind, source_path)) = row else { continue };
        out.insert(
            id,
            EntryLoc {
                kind: if kind == "folder" { SourceKind::Folder } else { SourceKind::Archive },
                source_path,
                chain: serde_json::from_str(&chain).unwrap_or_default(),
                size: size.and_then(|s| u64::try_from(s).ok()),
            },
        );
    }
    Ok(out)
}

/// One plan item, ready to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFile {
    /// `/`-separated.
    pub rel: String,
    pub is_dir: bool,
    /// `None` for a dir, and for a file whose entry is gone (it fails when written).
    pub loc: Option<EntryLoc>,
}

/// Every item of `plan`, in order, with its file's location.
pub fn resolve_plan(conn: &Connection, plan: &ExportPlan) -> Result<Vec<ExportFile>, IndexError> {
    let ids: Vec<i64> = plan
        .items
        .iter()
        .filter_map(|i| match i.kind {
            ExportKind::File { entry_id } => Some(entry_id),
            ExportKind::Dir => None,
        })
        .collect();
    let locs = locate(conn, &ids)?;
    Ok(plan
        .items
        .iter()
        .map(|i| match i.kind {
            ExportKind::Dir => ExportFile { rel: i.rel.clone(), is_dir: true, loc: None },
            ExportKind::File { entry_id } => ExportFile { rel: i.rel.clone(), is_dir: false, loc: locs.get(&entry_id).cloned() },
        })
        .collect())
}

/// `dir/name` if it is free, else `dir/name (2)`, `(3)`… (before a file's extension).
pub fn unique_dest(dir: &Path, name: &str, is_dir: bool) -> PathBuf {
    let (stem, ext) = if is_dir { (name, "") } else { (names::stem(name), &name[names::stem(name).len()..]) };
    let mut p = dir.join(name);
    let mut n = 1;
    while p.symlink_metadata().is_ok() {
        n += 1;
        p = dir.join(format!("{stem} ({n}){ext}"));
    }
    p
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WriteReport {
    pub files: u32,
    pub dirs: u32,
    /// `(rel, error)` of every file or dir that could not be written.
    pub failed: Vec<(String, String)>,
    pub bytes: u64,
}

/// Write `files` below `dest`. A failed file is reported and the rest are still written; a
/// file is written as `<name>.part` and renamed when complete. `progress(index, rel)` runs
/// before each item; `cancel` stops before the next one.
pub fn write_tree(
    dest: &Path,
    files: &[ExportFile],
    session: &mut ExtractSession,
    progress: &mut dyn FnMut(usize, &str),
    cancel: &AtomicBool,
) -> WriteReport {
    let mut report = WriteReport::default();
    for (i, f) in files.iter().enumerate() {
        progress(i, &f.rel);
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let path = dest.join(f.rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if f.is_dir {
            match std::fs::create_dir_all(&path) {
                Ok(()) => report.dirs += 1,
                Err(e) => report.failed.push((f.rel.clone(), e.to_string())),
            }
            continue;
        }
        match write_file(&path, f.loc.as_ref(), session) {
            Ok(n) => {
                report.files += 1;
                report.bytes += n;
            }
            Err(e) => report.failed.push((f.rel.clone(), e)),
        }
    }
    report
}

fn write_file(path: &Path, loc: Option<&EntryLoc>, session: &mut ExtractSession) -> Result<u64, String> {
    let loc = loc.ok_or("the file is no longer in the index")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let r = (|| {
        let mut out = std::io::BufWriter::new(std::fs::File::create(&part).map_err(|e| e.to_string())?);
        let n = extract_caught(session, loc, &mut out)?;
        out.flush().map_err(|e| e.to_string())?;
        drop(out);
        std::fs::rename(&part, path).map_err(|e| e.to_string())?;
        Ok(n)
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    r
}

/// Extract one file; a panic inside an archive reader becomes an error and a fresh session.
pub fn extract_caught(session: &mut ExtractSession, loc: &EntryLoc, out: &mut dyn Write) -> Result<u64, String> {
    match catch_unwind(AssertUnwindSafe(|| session.extract(loc.kind, Path::new(&loc.source_path), &loc.chain, out))) {
        Ok(r) => r.map_err(|e| e.to_string()),
        Err(_) => {
            *session = ExtractSession::new();
            Err("internal error in the archive reader".into())
        }
    }
}
