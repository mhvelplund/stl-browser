//! Thumbnail bookkeeping in the `thumbs` table: which entries have a cached thumbnail, which
//! failed, and the jobs for the rest. A row only counts while its file is on disk.
use std::path::Path;

use rusqlite::{Connection, ErrorCode, OptionalExtension, params};
use stl_scan::extract::SourceKind;
use stl_thumb::cache::{cache_rel, content_name, is_valid_rel, location_dir};
use stl_thumb::generate::{Job, Outcome};
use stl_thumb::thumb_kind;

use crate::scanstore::IndexError;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Lookup {
    /// `(entry id, cache_rel)` of thumbnails on disk.
    pub ready: Vec<(i64, String)>,
    /// Entries whose thumbnail failed; not retried until the entry changes.
    pub failed: Vec<i64>,
    /// Everything else that can have a thumbnail.
    pub jobs: Vec<Job>,
}

/// Sort `ids` into ready, failed and to-do, in the order given. Unknown ids and files without a
/// thumbnail kind are left out.
pub fn lookup(conn: &Connection, thumbs_dir: &Path, ids: &[i64]) -> Result<Lookup, IndexError> {
    let mut st = conn.prepare_cached(
        "SELECT e.chain, e.size, e.crc, s.kind, s.path, t.status, t.cache_rel
         FROM entries e JOIN sources s ON s.id = e.source_id LEFT JOIN thumbs t ON t.entry_id = e.id
         WHERE e.id = ?",
    )?;
    let mut out = Lookup::default();
    for &id in ids {
        let row = st
            .query_row([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            })
            .optional()?;
        let Some((chain, size, crc, kind, source_path, status, rel)) = row else { continue };
        let chain: Vec<String> = serde_json::from_str(&chain).unwrap_or_default();
        if chain.last().and_then(|n| thumb_kind(n)).is_none() {
            continue;
        }
        match (status.as_deref(), rel) {
            (Some("ok"), Some(rel)) if is_valid_rel(&rel) && thumbs_dir.join(&rel).is_file() => {
                out.ready.push((id, rel));
                continue;
            }
            (Some("error"), _) => {
                out.failed.push(id);
                continue;
            }
            // a row whose file is gone (cache deleted) doesn't count
            (Some(_), _) => {
                conn.execute("DELETE FROM thumbs WHERE entry_id = ?", [id])?;
            }
            (None, _) => {}
        }
        let job = Job {
            entry_id: id,
            source_kind: if kind == "folder" { SourceKind::Folder } else { SourceKind::Archive },
            source_path,
            chain,
            size: size.unwrap_or(0).max(0) as u64,
            crc: crc.unwrap_or_default(),
        };
        // an archive's stored CRC names the cache file without reading the file
        if let Some(content) = content_name(&job.crc, job.size) {
            let rel = cache_rel(&location_dir(&job.source_path, &job.chain), &content);
            if thumbs_dir.join(&rel).is_file() {
                record(conn, id, &Ok(rel.clone()))?;
                out.ready.push((id, rel));
                continue;
            }
        }
        out.jobs.push(job);
    }
    Ok(out)
}

/// Store a job's outcome. An entry that vanished meanwhile (re-indexed) is ignored.
pub fn record(conn: &Connection, entry_id: i64, outcome: &Outcome) -> Result<(), IndexError> {
    let (status, rel, error) = match outcome {
        Ok(rel) => ("ok", Some(rel.as_str()), None),
        Err(e) => ("error", None, Some(e.as_str())),
    };
    let r = conn.execute(
        "INSERT INTO thumbs(entry_id, cache_rel, status, error) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(entry_id) DO UPDATE SET cache_rel = ?2, status = ?3, error = ?4",
        params![entry_id, rel, status, error],
    );
    match r {
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::ConstraintViolation => Ok(()),
        r => r.map(|_| ()).map_err(Into::into),
    }
}
