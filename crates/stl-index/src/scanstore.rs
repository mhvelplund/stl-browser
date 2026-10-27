//! Add roots and scan them into the DB incrementally: a source whose size and mtime are
//! unchanged keeps its cached listing, changed ones are re-listed, vanished ones are removed.
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use stl_scan::discover::{SourceInfo, discover};
use stl_scan::extract::SourceKind;
use stl_scan::ScanError;
use stl_scan::nested::{ListOptions, ScannedEntry, list_archive};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("folder not found: {0}")]
    RootMissing(PathBuf),
    #[error("no root with id {0}")]
    UnknownRoot(i64),
    /// The resolver panicked on a collection; it is left empty until one of its sources changes.
    #[error("resolving {collection}: internal error: {message}")]
    ResolvePanic { collection: String, message: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanStats {
    pub sources: usize,
    pub listed: usize,
    pub reused: usize,
    pub removed: usize,
    pub errors: usize,
    /// Password-protected sources found this run (stored with no entries, never opened).
    pub locked: usize,
    /// Collections with a source listed or removed: they need resolving again.
    pub dirty: BTreeSet<String>,
}

impl ScanStats {
    pub fn add(&mut self, o: &ScanStats) {
        self.sources += o.sources;
        self.listed += o.listed;
        self.reused += o.reused;
        self.removed += o.removed;
        self.errors += o.errors;
        self.locked += o.locked;
        self.dirty.extend(o.dirty.iter().cloned());
    }
}

#[derive(Debug)]
pub enum Progress<'a> {
    Source {
        index: usize,
        total: usize,
        name: &'a str,
    },
    Warning(&'a str),
}

fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    secs.to_string()
}

/// Comparison key for root paths: lowercase, `\` separators, no trailing separator.
fn root_key(path: &str) -> String {
    path.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

fn all_roots(conn: &Connection) -> Result<Vec<(i64, String)>, IndexError> {
    let mut st = conn.prepare("SELECT id, path FROM roots ORDER BY id")?;
    let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Register a folder as a root (its name becomes the volume). Returns the existing id if it is
/// already a root, ignoring case and trailing separators.
pub fn add_root(conn: &Connection, path: &Path) -> Result<i64, IndexError> {
    let path = std::path::absolute(path)?;
    let mut key = path.to_string_lossy().into_owned();
    // keep "D:\" for a drive root ("D:" alone means the current dir on D:)
    while (key.ends_with('\\') || key.ends_with('/')) && !key.ends_with(":\\") && !key.ends_with(":/") {
        key.pop();
    }
    if let Some((id, _)) = all_roots(conn)?.into_iter().find(|(_, p)| root_key(p) == root_key(&key)) {
        return Ok(id);
    }
    let name = stl_core::names::volume_name(&key);
    let key = key.as_str();
    conn.execute(
        "INSERT OR IGNORE INTO roots(path, name, added_at) VALUES (?1, ?2, ?3)",
        params![key, name, now()],
    )?;
    Ok(conn.query_row("SELECT id FROM roots WHERE path = ?", [key], |r| r.get(0))?)
}

/// The outermost other root that contains this one, if any. A covered root's folders are
/// indexed under the covering root, so its sources are stored only once.
pub fn covering_root(conn: &Connection, root_id: i64) -> Result<Option<i64>, IndexError> {
    let roots = all_roots(conn)?;
    let Some((_, me)) = roots.iter().find(|(id, _)| *id == root_id) else {
        return Err(IndexError::UnknownRoot(root_id));
    };
    let me = root_key(me);
    Ok(roots
        .iter()
        .filter(|(id, p)| *id != root_id && me.starts_with(&format!("{}\\", root_key(p))))
        .min_by_key(|(_, p)| p.len())
        .map(|(id, _)| *id))
}

/// What listing a source produced.
enum Listing {
    Entries(Vec<ScannedEntry>, Vec<String>),
    /// Password-protected: not supported, so it is stored with no entries and never opened.
    Locked,
}

type Lister = dyn Fn(&SourceInfo, &ListOptions) -> Result<Listing, String>;

/// Scan one root. A missing root folder is an error and leaves its data untouched.
pub fn scan_root(
    conn: &mut Connection,
    root_id: i64,
    opts: &ListOptions,
    progress: &mut dyn FnMut(Progress),
) -> Result<ScanStats, IndexError> {
    scan_root_with(conn, root_id, opts, progress, &list_source)
}

fn scan_root_with(
    conn: &mut Connection,
    root_id: i64,
    opts: &ListOptions,
    progress: &mut dyn FnMut(Progress),
    lister: &Lister,
) -> Result<ScanStats, IndexError> {
    let root: String = conn
        .query_row("SELECT path FROM roots WHERE id = ?", [root_id], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or(IndexError::UnknownRoot(root_id))?;
    let root = PathBuf::from(root);
    if covering_root(conn, root_id)?.is_some() {
        return Ok(ScanStats::default());
    }
    if !root.is_dir() {
        return Err(IndexError::RootMissing(root));
    }
    let sources = discover(&root, &mut |w| progress(Progress::Warning(&w)))?;
    let mut stats = ScanStats {
        sources: sources.len(),
        ..ScanStats::default()
    };
    let mut seen = HashSet::new();
    for (index, src) in sources.iter().enumerate() {
        progress(Progress::Source {
            index,
            total: sources.len(),
            name: &src.rel_path,
        });
        let key = src.path.to_string_lossy().into_owned();
        seen.insert(key.clone());
        let cached: Option<(u64, f64, String)> = conn
            // a source that failed last time is always retried: the failure may have been transient
            .query_row("SELECT size, mtime, coalesce(fingerprint, '') FROM sources WHERE path = ? AND root_id = ? AND error IS NULL",
                params![key, root_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if cached == Some((src.size, src.mtime, src.fingerprint.clone())) {
            stats.reused += 1;
            continue;
        }
        let (entries, error, locked) = match list_guarded(lister, src, opts) {
            Ok(Listing::Entries(entries, warnings)) => {
                for w in &warnings {
                    progress(Progress::Warning(w));
                }
                (entries, None, false)
            }
            Ok(Listing::Locked) => {
                stats.locked += 1;
                (Vec::new(), None, true)
            }
            Err(e) => {
                let msg = e.to_string();
                progress(Progress::Warning(&format!("{}: {msg}", src.rel_path)));
                stats.errors += 1;
                (Vec::new(), Some(msg), false)
            }
        };
        store_listing(conn, root_id, src, &entries, error.as_deref(), locked)?;
        stats.listed += 1;
        stats.dirty.insert(src.collection.clone());
    }
    let stale: Vec<(i64, String, String)> = {
        let mut st = conn.prepare("SELECT id, path, coalesce(collection, '') FROM sources WHERE root_id = ?")?;
        st.query_map([root_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?
    };
    for (id, path, collection) in stale {
        if !seen.contains(&path) {
            let tx = conn.transaction()?;
            tx.execute("DELETE FROM sources WHERE id = ?", [id])?;
            tx.execute("DELETE FROM resolved WHERE root_id = ?1 AND collection = ?2", params![root_id, collection])?;
            tx.commit()?;
            stats.removed += 1;
            stats.dirty.insert(collection);
        }
    }
    Ok(stats)
}

/// Run `lister`, turning a panic in a parser on untrusted archive data into a source error
/// instead of taking the whole app down.
fn list_guarded(
    lister: &Lister,
    src: &SourceInfo,
    opts: &ListOptions,
) -> Result<Listing, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| lister(src, opts))).unwrap_or_else(|payload| {
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown".into());
        Err(format!("internal error while reading: {msg}"))
    })
}

fn list_source(
    src: &SourceInfo,
    opts: &ListOptions,
) -> Result<Listing, String> {
    match src.kind {
        SourceKind::Archive => match list_archive(&src.path, opts) {
            Ok((entries, warnings)) => Ok(Listing::Entries(entries, warnings)),
            Err(ScanError::Encrypted) => Ok(Listing::Locked),
            Err(e) => Err(e.to_string()),
        },
        SourceKind::Folder => {
            let mut out = Vec::new();
            for f in &src.loose_files {
                let size = std::fs::metadata(src.path.join(f))
                    .map_err(|e| e.to_string())?
                    .len();
                out.push(ScannedEntry {
                    path: f.clone(),
                    chain: vec![f.clone()],
                    size,
                    crc: String::new(),
                    locked: false,
                });
            }
            Ok(Listing::Entries(out, Vec::new()))
        }
    }
}

fn store_listing(
    conn: &mut Connection,
    root_id: i64,
    src: &SourceInfo,
    entries: &[ScannedEntry],
    error: Option<&str>,
    locked: bool,
) -> Result<(), IndexError> {
    let tx = conn.transaction()?;
    let key = src.path.to_string_lossy();
    // the collection (and the one the source was in before, maybe under another root) must be
    // resolved again: committed with the new entries, so an interrupted index can't lose it
    let old: Option<(i64, Option<String>)> = tx
        .query_row("SELECT root_id, collection FROM sources WHERE path = ?", [&key], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    for (rid, coll) in old.into_iter().map(|(r, c)| (r, c.unwrap_or_default())).chain([(root_id, src.collection.clone())]) {
        tx.execute("DELETE FROM resolved WHERE root_id = ?1 AND collection = ?2", params![rid, coll])?;
    }
    tx.execute(
        "INSERT INTO sources(root_id, kind, path, rel_path, name, collection, coll_path, size, mtime, scanned_at, error, fingerprint, locked)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(path) DO UPDATE SET root_id = ?1, kind = ?2, rel_path = ?4, name = ?5, collection = ?6,
             coll_path = ?7, size = ?8, mtime = ?9, scanned_at = ?10, error = ?11, fingerprint = ?12, locked = ?13",
        params![root_id, src.kind.as_str(), key, src.rel_path, src.name, src.collection, src.coll_path, src.size, src.mtime, now(), error, src.fingerprint, locked],
    )?;
    let sid: i64 = tx.query_row("SELECT id FROM sources WHERE path = ?", [&key], |r| {
        r.get(0)
    })?;
    tx.execute("DELETE FROM entries WHERE source_id = ?", [sid])?;
    {
        let mut ins = tx.prepare(
            "INSERT INTO entries(source_id, path, chain, size, crc, locked) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for e in entries {
            let chain = serde_json::to_string(&e.chain).expect("chain serializes");
            ins.execute(params![sid, e.path, chain, e.size, e.crc, e.locked])?;
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panicking_lister_is_recorded_as_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Vol");
        std::fs::create_dir_all(root.join("A")).unwrap();
        std::fs::write(root.join("A/boom.zip"), b"x").unwrap();
        std::fs::write(root.join("A/fine.zip"), b"y").unwrap();
        let mut conn = crate::db::open(&tmp.path().join("index.db")).unwrap();
        let id = add_root(&conn, &root).unwrap();
        let lister = |src: &SourceInfo, _: &ListOptions| -> Result<Listing, String> {
            if src.name == "boom" {
                panic!("parser exploded");
            }
            Ok(Listing::Entries(vec![ScannedEntry { path: "a.stl".into(), chain: vec!["a.stl".into()], size: 1, crc: String::new(), locked: false }], Vec::new()))
        };
        let s = scan_root_with(&mut conn, id, &ListOptions::default(), &mut |_| {}, &lister).unwrap();
        assert_eq!((s.listed, s.errors), (2, 1));
        let err: String = conn.query_row("SELECT error FROM sources WHERE name = 'boom'", [], |r| r.get(0)).unwrap();
        assert!(err.contains("parser exploded"), "{err}");
        let n: i64 = conn.query_row("SELECT count(*) FROM entries", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }
}
