//! Make the thumbnails of one batch of jobs: every job in a batch lives in the same archive (or
//! folder), which is opened and read in a single pass.
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

pub use stl_scan::extract::SourceKind;
use stl_scan::extract::extract_many_entries;

use crate::cache::{cache_rel, content_hash, content_name, location_dir};
use crate::image::{encode_webp, image_thumb};
use crate::mesh::load_mesh;
use crate::raster::render;
use crate::{ThumbKind, thumb_kind};

const MAX_IMAGE: u64 = 64 * 1024 * 1024;
const MAX_MESH: u64 = 256 * 1024 * 1024;

/// One file to make a thumbnail of, as the index knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub entry_id: i64,
    pub source_kind: SourceKind,
    pub source_path: String,
    /// Archive members to follow, the file itself last (for a folder: its path below the folder).
    pub chain: Vec<String>,
    pub size: u64,
    /// The archive's stored CRC, `""` if none.
    pub crc: String,
}

impl Job {
    /// Jobs with the same group live in the same innermost archive (or folder).
    pub fn group(&self) -> String {
        let outer = &self.chain[..self.chain.len().saturating_sub(1)];
        std::iter::once(self.source_path.as_str()).chain(outer.iter().map(String::as_str)).collect::<Vec<_>>().join("\0")
    }

    fn name(&self) -> &str {
        self.chain.last().map_or("", String::as_str)
    }
}

/// `Ok(cache_rel)` or `Err(message)`.
pub type Outcome = Result<String, String>;

/// Make the thumbnails of `jobs`, which must share one `group()`; `done` is called once per job.
pub fn generate(thumbs_dir: &Path, jobs: &[Job], done: &mut dyn FnMut(&Job, Outcome)) {
    let Some(first) = jobs.first() else { return };
    debug_assert!(jobs.iter().all(|j| j.group() == first.group()));
    let mut todo: HashMap<&str, Vec<&Job>> = HashMap::new();
    for job in jobs {
        let cap = match thumb_kind(job.name()) {
            Some(ThumbKind::Image) => MAX_IMAGE,
            Some(ThumbKind::Mesh) => MAX_MESH,
            None => {
                done(job, Err("no thumbnail for this file type".into()));
                continue;
            }
        };
        if job.size > cap {
            done(job, Err("too large".into()));
            continue;
        }
        // with a stored CRC the cache name is known without reading the file
        if let Some(content) = content_name(&job.crc, job.size) {
            let rel = cache_rel(&location_dir(&job.source_path, &job.chain), &content);
            if thumbs_dir.join(&rel).is_file() {
                done(job, Ok(rel));
                continue;
            }
        }
        todo.entry(job.name()).or_default().push(job);
    }
    if todo.is_empty() {
        return;
    }
    let names: HashSet<String> = todo.keys().map(|n| n.to_string()).collect();
    let outer = &first.chain[..first.chain.len() - 1];
    let mut seen: HashSet<String> = HashSet::new();
    let result = extract_many_entries(first.source_kind, Path::new(&first.source_path), outer, &names, &mut |name, bytes| {
        for job in todo.get(name).into_iter().flatten() {
            done(job, make(thumbs_dir, job, bytes));
        }
        seen.insert(name.to_string());
        Ok(())
    });
    let missing = match result {
        Ok(()) => "not found in archive".to_string(),
        Err(e) => e.to_string(),
    };
    for (name, jobs) in &todo {
        if !seen.contains(*name) {
            for job in jobs {
                done(job, Err(missing.clone()));
            }
        }
    }
}

/// Render one file's thumbnail into the cache (unless it is already there). A decoder that
/// panics on a corrupt file only fails this job.
fn make(thumbs_dir: &Path, job: &Job, bytes: &[u8]) -> Outcome {
    let content = content_name(&job.crc, job.size).unwrap_or_else(|| content_hash(bytes));
    let rel = cache_rel(&location_dir(&job.source_path, &job.chain), &content);
    let path = thumbs_dir.join(&rel);
    if path.is_file() {
        return Ok(rel);
    }
    let img = catch_unwind(AssertUnwindSafe(|| match thumb_kind(job.name()) {
        Some(ThumbKind::Image) => image_thumb(bytes),
        Some(ThumbKind::Mesh) => load_mesh(job.name(), bytes).map(|t| render(&t)),
        None => Err("no thumbnail for this file type".into()),
    }))
    .unwrap_or_else(|_| Err("internal error while decoding".into()))?;
    let webp = encode_webp(&img)?;
    let dir = path.parent().expect("cache_rel has a directory");
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // write then rename, so a reader never sees half a file
    let tmp = path.with_extension(format!("{}.tmp", job.entry_id));
    std::fs::write(&tmp, &webp).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        // another worker got there first (Windows won't rename over an existing file)
        if !path.is_file() {
            return Err(e.to_string());
        }
    }
    Ok(rel)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use super::*;
    use crate::fixtures::{cube, png, stl_bytes, zip};
    use stl_scan::nested::{ListOptions, list_archive};

    /// Jobs for every member of the archive at `p`, with the CRCs the scanner stores.
    fn archive_jobs(p: &Path) -> Vec<Job> {
        let (entries, _) = list_archive(p, &ListOptions::default()).unwrap();
        entries
            .into_iter()
            .enumerate()
            .map(|(i, e)| Job {
                entry_id: i as i64 + 1,
                source_kind: SourceKind::Archive,
                source_path: p.display().to_string(),
                chain: e.chain,
                size: e.size,
                crc: e.crc,
            })
            .collect()
    }

    fn run(dir: &Path, jobs: &[Job]) -> HashMap<i64, Outcome> {
        let mut out = HashMap::new();
        generate(dir, jobs, &mut |j, o| {
            assert!(out.insert(j.entry_id, o).is_none(), "done twice for {}", j.entry_id);
        });
        out
    }

    #[test]
    fn batch_writes_thumbs_once_per_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("kit.zip");
        std::fs::write(&p, zip(&[("Kit/x.png", &png(40, 30)), ("Kit/y.stl", &stl_bytes(&cube(2.0)))])).unwrap();
        let jobs = archive_jobs(&p);
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].group(), jobs[1].group());
        let thumbs = tmp.path().join("thumbs");
        let out = run(&thumbs, &jobs);
        let mut mtimes = Vec::new();
        for j in &jobs {
            let rel = out[&j.entry_id].clone().unwrap();
            assert!(rel.ends_with(&format!("-{}.webp", j.size)), "{rel}"); // CRC-named
            let file = thumbs.join(&rel);
            let img = ::image::open(&file).unwrap();
            assert_eq!((img.width(), img.height()), (128, 128));
            mtimes.push(std::fs::metadata(&file).unwrap().modified().unwrap());
        }
        // a second run finds the files and leaves them alone
        let again = run(&thumbs, &jobs);
        for (j, m) in jobs.iter().zip(mtimes) {
            assert_eq!(again[&j.entry_id], out[&j.entry_id]);
            assert_eq!(std::fs::metadata(thumbs.join(again[&j.entry_id].as_ref().unwrap())).unwrap().modified().unwrap(), m);
        }
    }

    #[test]
    fn bad_file_does_not_stop_the_batch() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("mixed.zip");
        std::fs::write(&p, zip(&[("bad.jpg", b"garbage"), ("good.png", &png(8, 8)), ("empty.obj", b"v 0 0 0\n")])).unwrap();
        let jobs = archive_jobs(&p);
        let out = run(&tmp.path().join("thumbs"), &jobs);
        assert_eq!(out.len(), 3);
        let by_name = |n: &str| &out[&jobs.iter().find(|j| j.chain.last().unwrap() == n).unwrap().entry_id];
        assert!(by_name("good.png").is_ok());
        assert!(by_name("bad.jpg").is_err());
        assert_eq!(by_name("empty.obj").as_ref().unwrap_err(), "no triangles");
    }

    #[test]
    fn folder_jobs_hash_content() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = png(10, 10);
        for d in ["a", "b"] {
            std::fs::create_dir_all(tmp.path().join(d)).unwrap();
            std::fs::write(tmp.path().join(d).join("p.png"), &bytes).unwrap();
        }
        let job = |id: i64, d: &str| Job {
            entry_id: id,
            source_kind: SourceKind::Folder,
            source_path: tmp.path().join(d).display().to_string(),
            chain: vec!["p.png".into()],
            size: bytes.len() as u64,
            crc: String::new(),
        };
        let thumbs = tmp.path().join("thumbs");
        let a = run(&thumbs, &[job(1, "a")])[&1].clone().unwrap();
        let b = run(&thumbs, &[job(2, "b")])[&2].clone().unwrap();
        let (da, fa) = a.split_once('/').unwrap();
        let (db, fb) = b.split_once('/').unwrap();
        assert_eq!(fa, fb);
        assert_eq!(fa.len(), 32 + ".webp".len());
        assert_ne!(da, db);
    }

    #[test]
    fn too_large_is_an_error_without_reading() {
        let tmp = tempfile::tempdir().unwrap();
        let job = Job {
            entry_id: 1,
            source_kind: SourceKind::Archive,
            source_path: tmp.path().join("missing.zip").display().to_string(),
            chain: vec!["huge.stl".into()],
            size: 300 * 1024 * 1024,
            crc: "00000001".into(),
        };
        assert_eq!(run(&tmp.path().join("thumbs"), &[job])[&1], Err("too large".to_string()));
    }

    #[test]
    fn unreadable_archive_fails_every_job() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("broken.zip");
        std::fs::write(&p, b"not a zip").unwrap();
        let job = |id: i64, n: &str| Job {
            entry_id: id,
            source_kind: SourceKind::Archive,
            source_path: p.display().to_string(),
            chain: vec![n.into()],
            size: 10,
            crc: String::new(),
        };
        let out = run(&tmp.path().join("thumbs"), &[job(1, "a.png"), job(2, "b.png")]);
        assert!(out[&1].is_err() && out[&2].is_err());
    }
}
