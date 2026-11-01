//! Make the thumbnails of one batch of jobs: every job in a batch lives in the same archive (or
//! folder), which is opened and read in a single pass.
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

pub use stl_scan::extract::SourceKind;
use stl_scan::ScanError;
use stl_scan::extract::extract_many_entries;

use ::image::RgbaImage;

use crate::cache::{cache_rel, content_hash, content_name, extra_rel, is_complete, location_dir, pics_rel, spin_rel};
use crate::image::{encode_jpeg, image_thumb};
use crate::mesh::load_mesh;
use crate::preview::{preview_bytes, threemf_pictures};
use crate::raster::{render, render_spin};
use crate::{ThumbKind, has_extras, thumb_kind};

const MAX_IMAGE: u64 = 64 * 1024 * 1024;
const MAX_MESH: u64 = 256 * 1024 * 1024;
/// Scenes are read whole for the small picture inside; the biggest real ones are ~350 MB.
const MAX_PREVIEW: u64 = 512 * 1024 * 1024;

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

/// Why a job made no thumbnail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The file itself is the problem (corrupt, unsupported, too large, missing from its
    /// archive): recorded, and not retried until the entry changes.
    Bad(String),
    /// The file couldn't be read right now (drive unplugged, file locked, disk full): not
    /// recorded, so a later request tries again.
    Unavailable(String),
}

impl Failure {
    pub fn message(&self) -> &str {
        match self {
            Failure::Bad(m) | Failure::Unavailable(m) => m,
        }
    }
}

/// `Ok(cache_rel)` or why not.
pub type Outcome = Result<String, Failure>;

/// Reads the named members of a job group's archive (or folder), handing each to the callback.
type Extract<'a> = dyn FnMut(&HashSet<String>, &mut dyn FnMut(&str, &[u8]) -> Result<(), ScanError>) -> Result<(), ScanError> + 'a;

/// An i/o error that says "not now" rather than "this file is broken".
fn unavailable(e: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    // Windows: 21 device not ready, 32 sharing violation, 33 lock violation, 1167 device not connected
    matches!(e.kind(), NotFound | PermissionDenied | Interrupted | WouldBlock | TimedOut | StorageFull)
        || matches!(e.raw_os_error(), Some(21 | 32 | 33 | 1167))
}

fn scan_failure(e: &ScanError) -> Failure {
    match e {
        ScanError::Io(io) if unavailable(io) => Failure::Unavailable(e.to_string()),
        _ => Failure::Bad(e.to_string()),
    }
}

/// Make the thumbnails of `jobs`, which must share one `group()`; `done` is called once per job.
pub fn generate(thumbs_dir: &Path, jobs: &[Job], done: &mut dyn FnMut(&Job, Outcome)) {
    let Some(first) = jobs.first() else { return };
    let outer = first.chain[..first.chain.len().saturating_sub(1)].to_vec();
    let (kind, source) = (first.source_kind, first.source_path.clone());
    generate_with(thumbs_dir, jobs, done, &mut |names, f| extract_many_entries(kind, Path::new(&source), &outer, names, f));
}

fn generate_with(thumbs_dir: &Path, jobs: &[Job], done: &mut dyn FnMut(&Job, Outcome), extract: &mut Extract<'_>) {
    let Some(first) = jobs.first() else { return };
    debug_assert!(jobs.iter().all(|j| j.group() == first.group()));
    let mut todo: HashMap<&str, Vec<&Job>> = HashMap::new();
    for job in jobs {
        let cap = match thumb_kind(job.name()) {
            Some(ThumbKind::Image) => MAX_IMAGE,
            Some(ThumbKind::Mesh) => MAX_MESH,
            Some(ThumbKind::Preview) => MAX_PREVIEW,
            None => {
                done(job, Err(Failure::Bad("no thumbnail for this file type".into())));
                continue;
            }
        };
        if job.size > cap {
            done(job, Err(Failure::Bad("too large".into())));
            continue;
        }
        // with a stored CRC the cache name is known without reading the file
        if let Some(content) = content_name(&job.crc, job.size) {
            let rel = cache_rel(&location_dir(&job.source_path, &job.chain), &content);
            if is_complete(thumbs_dir, &rel, job.name()) {
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
    let mut seen: HashSet<String> = HashSet::new();
    // decompressing corrupt data can panic inside an archive library; the jobs not done yet
    // then fail like any other unreadable member instead of hanging
    let result = catch_unwind(AssertUnwindSafe(|| {
        extract(&names, &mut |name, bytes| {
            for job in todo.get(name).into_iter().flatten() {
                done(job, make(thumbs_dir, job, bytes));
            }
            seen.insert(name.to_string());
            Ok(())
        })
    }));
    let missing = match result {
        Ok(Ok(())) => Failure::Bad("not found in archive".into()),
        Ok(Err(e)) => scan_failure(&e),
        Err(_) => Failure::Bad("internal error while extracting".into()),
    };
    for (name, jobs) in &todo {
        if !seen.contains(*name) {
            for job in jobs {
                done(job, Err(missing.clone()));
            }
        }
    }
}

/// Render one file's thumbnail into the cache (unless it is already there), and for a mesh its
/// turntable strip. A decoder that panics on a corrupt file only fails this job; a strip that
/// fails leaves the still alone (the tile just doesn't turn).
fn make(thumbs_dir: &Path, job: &Job, bytes: &[u8]) -> Outcome {
    let content = content_name(&job.crc, job.size).unwrap_or_else(|| content_hash(bytes));
    let rel = cache_rel(&location_dir(&job.source_path, &job.chain), &content);
    let (path, spin_path) = (thumbs_dir.join(&rel), thumbs_dir.join(spin_rel(&rel)));
    let kind = thumb_kind(job.name());
    let have_still = path.is_file();
    if is_complete(thumbs_dir, &rel, job.name()) {
        return Ok(rel);
    }
    // the still (unless it is there), a mesh's triangles for its strip, a 3MF's other pictures
    type Decoded = (Option<RgbaImage>, Vec<crate::mesh::Tri>, Option<Vec<Vec<u8>>>);
    let decoded = catch_unwind(AssertUnwindSafe(|| -> Result<Decoded, String> {
        match kind {
            Some(ThumbKind::Image) => Ok((Some(image_thumb(bytes)?), Vec::new(), None)),
            Some(ThumbKind::Preview) if has_extras(job.name()) => {
                let (default, extras) = threemf_pictures(bytes)?;
                Ok(((!have_still).then(|| image_thumb(&default)).transpose()?, Vec::new(), Some(extras)))
            }
            Some(ThumbKind::Preview) => Ok((Some(image_thumb(&preview_bytes(job.name(), bytes)?)?), Vec::new(), None)),
            Some(ThumbKind::Mesh) => {
                let tris = load_mesh(job.name(), bytes)?;
                Ok(((!have_still).then(|| render(&tris)), tris, None))
            }
            None => Err("no thumbnail for this file type".into()),
        }
    }))
    .unwrap_or_else(|_| Err("internal error while decoding".into()))
    .map_err(Failure::Bad)?;
    let (still, tris, extras) = decoded;
    if let Some(img) = still {
        write_atomic(&path, &encode_jpeg(&img).map_err(Failure::Bad)?, job.entry_id)?;
    }
    if !tris.is_empty() {
        let strip = catch_unwind(AssertUnwindSafe(|| encode_jpeg(&render_spin(&tris)))).unwrap_or_else(|_| Err("panic".into()));
        if let Ok(jpg) = strip {
            let _ = write_atomic(&spin_path, &jpg, job.entry_id);
        }
    }
    if let Some(extras) = extras {
        write_extras(thumbs_dir, &rel, &extras, job.entry_id);
    }
    Ok(rel)
}

/// A 3MF's other pictures as `.p<i>.jpg` (one that won't decode is skipped), then their count.
/// Failures only cost carousel slots.
fn write_extras(thumbs_dir: &Path, rel: &str, extras: &[Vec<u8>], entry_id: i64) {
    let mut n = 0;
    for pic in extras {
        let jpg = catch_unwind(AssertUnwindSafe(|| image_thumb(pic).and_then(|img| encode_jpeg(&img)))).unwrap_or_else(|_| Err("panic".into()));
        if let Ok(jpg) = jpg
            && write_atomic(&thumbs_dir.join(extra_rel(rel, n + 1)), &jpg, entry_id).is_ok()
        {
            n += 1;
        }
    }
    let _ = write_atomic(&thumbs_dir.join(pics_rel(rel)), n.to_string().as_bytes(), entry_id);
}

/// Write a cache file via a temp file and a rename, so a reader never sees half a file.
fn write_atomic(path: &Path, bytes: &[u8], entry_id: i64) -> Result<(), Failure> {
    // writing the cache can fail for reasons that pass (disk full, a locked folder)
    let io = |e: std::io::Error| Failure::Unavailable(e.to_string());
    std::fs::create_dir_all(path.parent().expect("cache_rel has a directory")).map_err(io)?;
    let tmp = path.with_extension(format!("{entry_id}.tmp"));
    std::fs::write(&tmp, bytes).map_err(io)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        // another worker may have written the same file meanwhile
        if !path.is_file() {
            return Err(io(e));
        }
    }
    Ok(())
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
            assert!(rel.ends_with(&format!("-{}.jpg", j.size)), "{rel}"); // CRC-named
            let file = thumbs.join(&rel);
            let img = ::image::open(&file).unwrap();
            // the 40×30 picture keeps its shape; the render is square
            let want = if j.chain[0].ends_with(".png") { (256, 192) } else { (256, 256) };
            assert_eq!((img.width(), img.height()), want);
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
    fn meshes_get_a_turntable_strip_images_do_not() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("kit.zip");
        std::fs::write(&p, zip(&[("x.png", &png(8, 8)), ("y.stl", &stl_bytes(&cube(2.0)))])).unwrap();
        let jobs = archive_jobs(&p);
        let thumbs = tmp.path().join("thumbs");
        let out = run(&thumbs, &jobs);
        for j in &jobs {
            let spin = thumbs.join(spin_rel(out[&j.entry_id].as_ref().unwrap()));
            if j.chain[0].ends_with(".stl") {
                let img = ::image::open(&spin).unwrap();
                assert_eq!((img.width(), img.height()), (256 * crate::SPIN_FRAMES, 256));
            } else {
                assert!(!spin.exists());
            }
        }
    }

    #[test]
    fn a_missing_strip_is_made_without_touching_the_still() {
        // thumbnails cached before strips existed get one the next time they are asked for
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("kit.zip");
        std::fs::write(&p, zip(&[("y.stl", &stl_bytes(&cube(2.0)))])).unwrap();
        let jobs = archive_jobs(&p);
        let thumbs = tmp.path().join("thumbs");
        let rel = run(&thumbs, &jobs)[&1].clone().unwrap();
        std::fs::remove_file(thumbs.join(spin_rel(&rel))).unwrap();
        std::fs::write(thumbs.join(&rel), b"old still").unwrap();
        assert_eq!(run(&thumbs, &jobs)[&1].as_ref().unwrap(), &rel);
        assert!(thumbs.join(spin_rel(&rel)).is_file());
        assert_eq!(std::fs::read(thumbs.join(&rel)).unwrap(), b"old still");
    }

    #[test]
    fn scenes_use_their_embedded_preview() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("kit.zip");
        let scene = crate::fixtures::lys(&[("m.bin", b"mesh"), ("preview.png", &png(64, 64))]);
        let project = zip(&[("Metadata/thumbnail.png", &png(40, 20))]);
        std::fs::write(&p, zip(&[("a.lys", &scene), ("b.3mf", &project), ("c.lys", b"junk")])).unwrap();
        let jobs = archive_jobs(&p);
        let thumbs = tmp.path().join("thumbs");
        let out = run(&thumbs, &jobs);
        let by_name = |n: &str| out[&jobs.iter().find(|j| j.chain[0] == n).unwrap().entry_id].clone();
        let dims = |rel: String| {
            assert!(!thumbs.join(spin_rel(&rel)).exists());
            let img = ::image::open(thumbs.join(rel)).unwrap();
            (img.width(), img.height())
        };
        assert_eq!(dims(by_name("a.lys").unwrap()), (256, 256));
        assert_eq!(dims(by_name("b.3mf").unwrap()), (256, 128));
        assert!(matches!(by_name("c.lys"), Err(Failure::Bad(_))));
    }

    #[test]
    fn a_3mf_writes_its_carousel() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("kit.zip");
        let project = zip(&[
            ("Metadata/plate_1.png", &png(40, 20)),
            ("Metadata/plate_2.png", &png(20, 40)),
            ("Metadata/plate_3.png", b"broken"),
            ("Auxiliaries/Model Pictures/a.png", &png(30, 30)),
        ]);
        std::fs::write(&p, zip(&[("b.3mf", &project)])).unwrap();
        let jobs = archive_jobs(&p);
        let thumbs = tmp.path().join("thumbs");
        let rel = run(&thumbs, &jobs)[&1].clone().unwrap();
        let dims = |r: &str| {
            let img = ::image::open(thumbs.join(r)).unwrap();
            (img.width(), img.height())
        };
        assert_eq!(dims(&rel), (256, 128)); // plate 1: there is no .rels
        let extras = crate::cache::extra_rels(&thumbs, &rel);
        assert_eq!(extras.len(), 2, "{extras:?}"); // the broken plate is skipped
        assert_eq!(dims(&extras[0]), (128, 256));
        assert_eq!(dims(&extras[1]), (256, 256));
        // an older still without the carousel is filled in and left alone
        std::fs::remove_file(thumbs.join(pics_rel(&rel))).unwrap();
        std::fs::write(thumbs.join(&rel), b"old still").unwrap();
        assert_eq!(run(&thumbs, &jobs)[&1].as_ref().unwrap(), &rel);
        assert_eq!(crate::cache::extra_rels(&thumbs, &rel).len(), 2);
        assert_eq!(std::fs::read(thumbs.join(&rel)).unwrap(), b"old still");
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
        assert_eq!(by_name("empty.obj").as_ref().unwrap_err(), &Failure::Bad("no triangles".into()));
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
        assert_eq!(fa.len(), 32 + ".jpg".len());
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
        assert_eq!(run(&tmp.path().join("thumbs"), &[job])[&1], Err(Failure::Bad("too large".into())));
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
        assert!(matches!(out[&1], Err(Failure::Bad(_))), "{:?}", out[&1]);
        assert!(matches!(out[&2], Err(Failure::Bad(_))));
    }

    #[test]
    fn missing_archive_is_unavailable_not_bad() {
        // a drive that is unplugged right now: retry later, don't record a failure
        let tmp = tempfile::tempdir().unwrap();
        let job = Job {
            entry_id: 1,
            source_kind: SourceKind::Archive,
            source_path: tmp.path().join("gone.zip").display().to_string(),
            chain: vec!["a.png".into()],
            size: 10,
            crc: String::new(),
        };
        assert!(matches!(run(&tmp.path().join("thumbs"), &[job])[&1], Err(Failure::Unavailable(_))));
    }

    #[test]
    fn extract_panic_fails_the_rest_of_the_batch() {
        let tmp = tempfile::tempdir().unwrap();
        let job = |id: i64, n: &str| Job {
            entry_id: id,
            source_kind: SourceKind::Archive,
            source_path: "D:/x.zip".into(),
            chain: vec![n.into()],
            size: 10,
            crc: String::new(),
        };
        let jobs = [job(1, "a.png"), job(2, "b.png")];
        let mut out = HashMap::new();
        let png = png(4, 4);
        generate_with(&tmp.path().join("thumbs"), &jobs, &mut |j, o| {
            assert!(out.insert(j.entry_id, o).is_none());
        }, &mut |_, f| {
            f("a.png", &png)?;
            panic!("corrupt stream");
        });
        assert_eq!(out.len(), 2);
        assert!(out[&1].is_ok());
        assert!(matches!(out[&2], Err(Failure::Bad(_))));
    }
}
