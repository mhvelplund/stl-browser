//! Thumbnail bookkeeping in the `thumbs` table: which entries have a cached thumbnail, which
//! failed, and the jobs for the rest. A row only counts while its file is on disk.
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use stl_scan::extract::SourceKind;
use stl_thumb::cache::{cache_rel, content_name, is_complete, is_valid_rel, location_dir};
use stl_thumb::generate::{Failure, Job, Outcome};
use stl_thumb::thumb_kind;

use crate::scanstore::IndexError;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Lookup {
    /// `(entry id, cache_rel)` of thumbnails on disk.
    pub ready: Vec<(i64, String)>,
    /// Entries whose thumbnail failed; not retried until the entry changes.
    pub failed: Vec<i64>,
    /// Everything else that can have a thumbnail, and entries in `ready` still missing the rest
    /// (a mesh's turntable strip, a 3MF's carousel).
    pub jobs: Vec<Job>,
}

/// Sort `ids` into ready, failed and to-do, in the order given. Unknown ids and files without a
/// thumbnail kind are left out. Its few writes (stale rows, CRC hits) share one transaction.
pub fn lookup(conn: &Connection, thumbs_dir: &Path, ids: &[i64]) -> Result<Lookup, IndexError> {
    let tx = conn.unchecked_transaction()?;
    let mut out = Lookup::default();
    {
        let mut st = tx.prepare_cached(
            "SELECT e.chain, e.size, e.crc, s.kind, s.path, t.status, t.cache_rel
             FROM entries e JOIN sources s ON s.id = e.source_id LEFT JOIN thumbs t ON t.entry_id = e.id
             WHERE e.id = ?",
        )?;
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
            let Some(name) = chain.last().filter(|n| thumb_kind(n).is_some()).cloned() else { continue };
            // a still cached without the rest (a mesh's strip, a 3MF's carousel) shows now and
            // is queued for the rest
            let mut shown = false;
            match (status.as_deref(), rel) {
                (Some("ok"), Some(rel)) if is_valid_rel(&rel) && thumbs_dir.join(&rel).is_file() => {
                    let complete = is_complete(thumbs_dir, &rel, &name);
                    out.ready.push((id, rel));
                    if complete {
                        continue;
                    }
                    shown = true;
                }
                (Some("error"), _) => {
                    out.failed.push(id);
                    continue;
                }
                // a row whose file is gone (cache deleted) doesn't count
                (Some(_), _) => {
                    tx.execute("DELETE FROM thumbs WHERE entry_id = ?", [id])?;
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
            if let Some(content) = content_name(&job.crc, job.size).filter(|_| !shown) {
                let rel = cache_rel(&location_dir(&job.source_path, &job.chain), &content);
                if thumbs_dir.join(&rel).is_file() {
                    record(&tx, &job, &Ok(rel.clone()))?;
                    let complete = is_complete(thumbs_dir, &rel, &name);
                    out.ready.push((id, rel));
                    if complete {
                        continue;
                    }
                }
            }
            out.jobs.push(job);
        }
    }
    tx.commit()?;
    Ok(out)
}

/// Store a job's outcome; true if stored. Nothing is written when the entry no longer is the
/// job's file (it vanished, or a re-listed source handed its id to another file), or when the
/// file was only unavailable for now.
pub fn record(conn: &Connection, job: &Job, outcome: &Outcome) -> Result<bool, IndexError> {
    let (status, rel, error) = match outcome {
        Ok(rel) => ("ok", Some(rel.as_str()), None),
        Err(Failure::Bad(e)) => ("error", None, Some(e.as_str())),
        Err(Failure::Unavailable(_)) => return Ok(false),
    };
    let chain = serde_json::to_string(&job.chain).expect("chain serializes");
    let n = conn.execute(
        "INSERT INTO thumbs(entry_id, cache_rel, status, error)
         SELECT ?1, ?2, ?3, ?4 WHERE EXISTS (
             SELECT 1 FROM entries e JOIN sources s ON s.id = e.source_id
             WHERE e.id = ?1 AND e.chain = ?5 AND s.path = ?6 AND coalesce(e.crc, '') = ?7)
         ON CONFLICT(entry_id) DO UPDATE SET cache_rel = ?2, status = ?3, error = ?4",
        params![job.entry_id, rel, status, error, chain, job.source_path, job.crc],
    )?;
    Ok(n > 0)
}
