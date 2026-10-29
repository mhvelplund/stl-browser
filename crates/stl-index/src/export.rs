//! Exports: find where each file of an export plan lives, and write a plan to a folder (the
//! Extract to… path; a drag-out extracts the same files one by one through `stl-drag`).
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Seek, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, OptionalExtension};
use stl_core::names;
use stl_core::vtree::export::{ExportKind, ExportPlan};
use stl_scan::extract::{SourceKind, extract_many_entries};
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
    /// Files written by a single pass over their archive.
    pub batched: u32,
}

/// Files that share this key come out of one sequential pass over their archive level (7z
/// and RAR decode from the start for every member, and solid ones can't skip). `None` for
/// loose files and zip members, which are read directly.
pub fn batch_key(loc: &EntryLoc) -> Option<(String, Vec<String>)> {
    if loc.kind != SourceKind::Archive || loc.chain.is_empty() {
        return None;
    }
    let outer = &loc.chain[..loc.chain.len() - 1];
    let container = outer.last().unwrap_or(&loc.source_path);
    (names::ext(container) != ".zip").then(|| (loc.source_path.clone(), outer.to_vec()))
}

/// Read the files `locs` (all with one `batch_key`) in one pass; `f(index, bytes)` for each
/// one found; `f` returns `false` to stop the pass (an error). A panic in an archive reader
/// becomes an error.
pub fn extract_batch(locs: &[(usize, &EntryLoc)], f: &mut dyn FnMut(usize, &[u8]) -> bool) -> Result<(), String> {
    let Some(&(_, first)) = locs.first() else { return Ok(()) };
    let outer = &first.chain[..first.chain.len().saturating_sub(1)];
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, l) in locs {
        if let Some(name) = l.chain.last() {
            by_name.entry(name.as_str()).or_default().push(*i);
        }
    }
    let wanted: HashSet<String> = by_name.keys().map(|n| n.to_string()).collect();
    let r = catch_unwind(AssertUnwindSafe(|| {
        extract_many_entries(first.kind, Path::new(&first.source_path), outer, &wanted, &mut |name, bytes| {
            for &i in by_name.get(name).map(Vec::as_slice).unwrap_or_default() {
                if !f(i, bytes) {
                    return Err(stl_scan::ScanError::Format("stopped".into()));
                }
            }
            Ok(())
        })
    }));
    match r {
        Ok(r) => r.map_err(|e| e.to_string()),
        Err(_) => Err("internal error in the archive reader".into()),
    }
}

/// Write `files` below `dest`. A failed file is reported and the rest are still written; a
/// file is written as `<name>.part` and renamed when complete. Files sharing a `batch_key`
/// are written by one pass over their archive, so items may be written out of plan order;
/// `progress(n, rel)` runs before each, `n` counting from 0. `cancel` stops before the next.
pub fn write_tree(
    dest: &Path,
    files: &[ExportFile],
    session: &mut ExtractSession,
    progress: &mut dyn FnMut(usize, &str),
    cancel: &AtomicBool,
) -> WriteReport {
    let mut report = WriteReport::default();
    let mut done = vec![false; files.len()];
    let mut count = 0;
    let path_of = |f: &ExportFile| dest.join(f.rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    for (i, f) in files.iter().enumerate() {
        if done[i] {
            continue;
        }
        if let Some(key) = f.loc.as_ref().and_then(batch_key) {
            let group: Vec<(usize, &EntryLoc)> = files
                .iter()
                .enumerate()
                .skip(i)
                .filter(|(j, g)| !done[*j] && !g.is_dir)
                .filter_map(|(j, g)| g.loc.as_ref().filter(|l| batch_key(l).as_ref() == Some(&key)).map(|l| (j, l)))
                .collect();
            if group.len() > 1 {
                // a failed pass leaves its unwritten files to the one-by-one path below
                let _ = extract_batch(&group, &mut |j, bytes| {
                    if cancel.load(Ordering::SeqCst) {
                        return false;
                    }
                    if done[j] {
                        return true;
                    }
                    progress(count, &files[j].rel);
                    count += 1;
                    done[j] = true;
                    match write_bytes(&path_of(&files[j]), bytes) {
                        Ok(()) => {
                            report.files += 1;
                            report.batched += 1;
                            report.bytes += bytes.len() as u64;
                        }
                        Err(e) => report.failed.push((files[j].rel.clone(), e)),
                    }
                    true
                });
                if done[i] {
                    continue;
                }
            }
        }
        progress(count, &f.rel);
        count += 1;
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        done[i] = true;
        let path = path_of(f);
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

fn part_path(path: &Path) -> PathBuf {
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    PathBuf::from(part)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let part = part_path(path);
    let r = std::fs::write(&part, bytes).and_then(|()| std::fs::rename(&part, path)).map_err(|e| e.to_string());
    if r.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    r
}

fn write_file(path: &Path, loc: Option<&EntryLoc>, session: &mut ExtractSession) -> Result<u64, String> {
    let loc = loc.ok_or("the file is no longer in the index")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let part = part_path(path);
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

/// Hands out a drag's files one at a time, in whatever order Explorer asks. The first request
/// into a sequential archive level (see `batch_key`) reads the whole level once and keeps the
/// drag's other files from it in delete-on-close temp files, up to `cache_limit` bytes; the
/// rest go through an `ExtractSession`.
pub struct FileServer {
    files: Vec<ExportFile>,
    session: ExtractSession,
    cache: HashMap<usize, File>,
    cached: u64,
    cache_limit: u64,
    read: HashSet<(String, Vec<String>)>,
    batches: u32,
}

impl FileServer {
    pub fn new(files: Vec<ExportFile>, cache_limit: u64) -> Self {
        FileServer {
            files,
            session: ExtractSession::new(),
            cache: HashMap::new(),
            cached: 0,
            cache_limit,
            read: HashSet::new(),
            batches: 0,
        }
    }

    /// Write file `index` to `out`; returns its size.
    pub fn write(&mut self, index: usize, out: &mut dyn Write) -> Result<u64, String> {
        if let Some(mut f) = self.cache.remove(&index) {
            let n = f.stream_position().map_err(|e| e.to_string())?;
            self.cached -= n;
            f.rewind().map_err(|e| e.to_string())?;
            return std::io::copy(&mut f, out).map_err(|e| e.to_string());
        }
        let f = self.files.get(index).filter(|f| !f.is_dir).ok_or("not a file")?;
        let loc = f.loc.as_ref().ok_or("the file is no longer in the index")?;
        if let Some(key) = batch_key(loc).filter(|k| !self.read.contains(k)) {
            let group: Vec<(usize, &EntryLoc)> = self
                .files
                .iter()
                .enumerate()
                .filter_map(|(j, g)| g.loc.as_ref().filter(|l| !g.is_dir && batch_key(l).as_ref() == Some(&key)).map(|l| (j, l)))
                .collect();
            if group.len() > 1 {
                self.batches += 1;
                let (cache, cached, limit) = (&mut self.cache, &mut self.cached, self.cache_limit);
                let mut served: Option<Result<u64, String>> = None;
                let r = extract_batch(&group, &mut |j, bytes| {
                    if j == index {
                        if served.is_none() {
                            served = Some(out.write_all(bytes).map(|()| bytes.len() as u64).map_err(|e| e.to_string()));
                        }
                    } else if !cache.contains_key(&j) && *cached + bytes.len() as u64 <= limit {
                        // a spill that fails just leaves the file to the session later
                        if let Ok(mut t) = tempfile::tempfile() {
                            if t.write_all(bytes).is_ok() {
                                *cached += bytes.len() as u64;
                                cache.insert(j, t);
                            }
                        }
                    }
                    true
                });
                self.read.insert(key);
                match (served, r) {
                    (Some(s), _) => return s,
                    (None, Err(e)) => return Err(e),
                    (None, Ok(())) => {}
                }
            }
        }
        extract_caught(&mut self.session, loc, out)
    }

    /// Single-pass reads done so far.
    pub fn batches(&self) -> u32 {
        self.batches
    }

    /// Archives the one-by-one path opened so far.
    pub fn session_opens(&self) -> usize {
        self.session.opens()
    }
}
