//! The platform-neutral half of a promise drag (macOS): the drag's descriptors grouped into
//! top-level items, and writing one item into the folder the drop target names.
use std::fs::File;
use std::io::{BufWriter, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::{Contents, Descriptor};

/// A top-level folder or file of the drag: what one promise stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopItem {
    pub name: String,
    pub is_dir: bool,
    /// Its own descriptor; `None` for a folder that only appears as its members' parent.
    pub index: Option<usize>,
    /// The descriptors below it, in descriptor order.
    pub members: Vec<usize>,
}

/// The descriptors grouped by their first `/` segment, in order of first appearance.
pub fn top_items(descs: &[Descriptor]) -> Vec<TopItem> {
    let mut items: Vec<TopItem> = Vec::new();
    for (i, d) in descs.iter().enumerate() {
        let (top, below) = match d.rel.split_once('/') {
            Some((top, _)) => (top, true),
            None => (d.rel.as_str(), false),
        };
        let pos = match items.iter().position(|t| t.name == top) {
            Some(p) => p,
            None => {
                items.push(TopItem { name: top.to_string(), is_dir: false, index: None, members: Vec::new() });
                items.len() - 1
            }
        };
        let t = &mut items[pos];
        if below {
            t.members.push(i);
            t.is_dir = true;
        } else {
            t.index = Some(i);
            t.is_dir |= d.is_dir;
        }
    }
    items
}

/// `dest`, or if something is there already, the first free `stem N.ext` (`name N` for a folder
/// or a name without an extension), counting from 2 as Finder does. `is_dir` is the kind of
/// the item being written, not of whatever is in the way.
pub fn free_dest(dest: &Path, is_dir: bool) -> PathBuf {
    if !dest.exists() {
        return dest.to_path_buf();
    }
    let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !is_dir => (stem.to_string(), format!(".{ext}")),
        _ => (name.clone(), String::new()),
    };
    (2..)
        .map(|n| dest.with_file_name(format!("{stem} {n}{ext}")))
        .find(|p| !p.exists())
        .expect("some name is free")
}

/// A promise drag's items and the contents behind them. The contents are released once every
/// item has been written, or by `release` (a cancelled drag, or the next drag starting).
pub struct Promises {
    descs: Vec<Descriptor>,
    items: Vec<TopItem>,
    contents: Mutex<Option<Arc<dyn Contents>>>,
    done: Vec<AtomicBool>,
    remaining: AtomicUsize,
    /// A write has begun (set under the `contents` lock).
    started: AtomicBool,
}

impl Promises {
    pub fn new(descs: Vec<Descriptor>, contents: Arc<dyn Contents>) -> Self {
        let items = top_items(&descs);
        let n = items.len();
        Promises {
            descs,
            items,
            contents: Mutex::new(Some(contents)),
            done: (0..n).map(|_| AtomicBool::new(false)).collect(),
            remaining: AtomicUsize::new(n),
            started: AtomicBool::new(false),
        }
    }

    pub fn items(&self) -> &[TopItem] {
        &self.items
    }

    /// Drop the contents; later writes fail. A write already running keeps its own reference.
    pub fn release(&self) {
        self.contents.lock().unwrap_or_else(|e| e.into_inner()).take();
    }

    /// Release the contents if no write has begun (a drop target that never asked for the
    /// files); a drag that is being written releases itself after its last item.
    pub fn release_if_idle(&self) -> bool {
        let mut c = self.contents.lock().unwrap_or_else(|e| e.into_inner());
        if self.started.load(Ordering::SeqCst) {
            return false;
        }
        c.take();
        true
    }

    /// The contents are gone: every item was written, or the drag was released.
    pub fn finished(&self) -> bool {
        self.contents.lock().unwrap_or_else(|e| e.into_inner()).is_none()
    }

    /// Write item `item` to `dest` (or the free name next to it, `free_dest`); returns the path
    /// written. A folder's files are all tried; the error names how many failed and the first.
    pub fn write(&self, item: usize, dest: &Path) -> Result<PathBuf, String> {
        let contents = {
            let c = self.contents.lock().unwrap_or_else(|e| e.into_inner());
            let contents = c.clone().ok_or("the drag is over")?;
            self.started.store(true, Ordering::SeqCst);
            contents
        };
        let t = self.items.get(item).ok_or("no such item")?;
        let r = self.write_item(&*contents, t, dest);
        drop(contents);
        if !self.done[item].swap(true, Ordering::SeqCst) && self.remaining.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.release();
        }
        r
    }

    fn write_item(&self, contents: &dyn Contents, t: &TopItem, dest: &Path) -> Result<PathBuf, String> {
        if !t.is_dir {
            let i = t.index.ok_or("no such item")?;
            return write_file(contents, i, self.descs[i].size, dest, true);
        }
        // create_dir fails on a name taken since free_dest looked (another drop into the same folder)
        let out = loop {
            let out = free_dest(dest, true);
            match std::fs::create_dir(&out) {
                Ok(()) => break out,
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("{}: {e}", out.display())),
            }
        };
        let (mut files, mut failed, mut failed_dirs, mut first) = (0, 0, 0, None);
        for &m in &t.members {
            let d = &self.descs[m];
            let below = d.rel.split_once('/').map_or("", |(_, r)| r);
            let path = below.split('/').fold(out.clone(), |p, s| p.join(s));
            let r = if d.is_dir {
                std::fs::create_dir_all(&path).map_err(|e| e.to_string())
            } else {
                files += 1;
                let parent = path.parent().map(|p| std::fs::create_dir_all(p).map_err(|e| e.to_string()));
                parent.unwrap_or(Ok(())).and_then(|()| write_file(contents, m, d.size, &path, false).map(|_| ()))
            };
            if let Err(e) = r {
                if d.is_dir {
                    failed_dirs += 1;
                } else {
                    failed += 1;
                }
                first.get_or_insert(format!("{}: {e}", d.rel));
            }
        }
        let Some(first) = first else { return Ok(out) };
        let mut what = Vec::new();
        if failed > 0 {
            what.push(format!("{failed} of {files} files"));
        }
        match failed_dirs {
            0 => {}
            1 => what.push("1 folder".to_string()),
            n => what.push(format!("{n} folders")),
        }
        Err(format!("{} could not be written; first: {first}", what.join(" and ")))
    }
}

/// Write file `index` to a uniquely named `.part` file next to `path`, then move it to `path`
/// without ever replacing a file there. With `next_free`, a taken `path` moves on to the next
/// free name (`free_dest`); otherwise it fails. Returns where the file went; nothing is left on
/// failure (the temp file deletes itself).
fn write_file(contents: &dyn Contents, index: usize, size: Option<u64>, path: &Path, next_free: bool) -> Result<PathBuf, String> {
    let dir = path.parent().ok_or("no folder to write to")?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut tmp = tempfile::Builder::new().prefix(&format!(".{name}.")).suffix(".part").tempfile_in(dir).map_err(|e| e.to_string())?;
    let mut w = Limited { out: BufWriter::new(tmp.as_file_mut()), written: 0, size };
    contents.write(index, &mut w)?;
    w.out.flush().map_err(|e| e.to_string())?;
    drop(w);
    loop {
        let target = if next_free { free_dest(path, false) } else { path.to_path_buf() };
        match tmp.persist_noclobber(&target) {
            Ok(_) => return Ok(target),
            Err(e) if next_free && e.error.kind() == ErrorKind::AlreadyExists => tmp = e.file,
            Err(e) => return Err(format!("{}: {}", target.display(), e.error)),
        }
    }
}

/// Fails a write that goes past the size in the index (a stale index) instead of truncating.
struct Limited<'a> {
    out: BufWriter<&'a mut File>,
    written: u64,
    size: Option<u64>,
}

impl Write for Limited<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(size) = self.size.filter(|&s| self.written + buf.len() as u64 > s) {
            return Err(std::io::Error::other(format!("the file has more bytes than the {size} in the index")));
        }
        let n = self.out.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Contents, Descriptor};
    use std::path::Path;
    use std::sync::Arc;

    struct Bodies(Vec<Vec<u8>>);

    impl Contents for Bodies {
        fn write(&self, index: usize, out: &mut dyn std::io::Write) -> Result<(), String> {
            let b = &self.0[index];
            if b == b"!fail" {
                return Err("gone".into());
            }
            out.write_all(b).map_err(|e| e.to_string())
        }
    }

    fn d(rel: &str, is_dir: bool, size: Option<u64>) -> Descriptor {
        Descriptor { rel: rel.into(), is_dir, size }
    }

    fn promises(items: &[(&str, bool, &[u8])]) -> Promises {
        let descs = items.iter().map(|(rel, dir, b)| d(rel, *dir, if *dir { None } else { Some(b.len() as u64) })).collect();
        let bodies = items.iter().map(|(_, _, b)| b.to_vec()).collect();
        Promises::new(descs, Arc::new(Bodies(bodies)))
    }

    fn parts_under(p: &Path) -> Vec<String> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(p).unwrap() {
            let e = e.unwrap();
            if e.path().is_dir() {
                out.extend(parts_under(&e.path()));
            } else if e.file_name().to_string_lossy().ends_with(".part") {
                out.push(e.path().display().to_string());
            }
        }
        out
    }

    #[test]
    fn top_items_groups_by_first_segment() {
        let descs = [
            d("m", true, None),
            d("m/a.stl", false, Some(1)),
            d("m/sub", true, None),
            d("m/sub/b.jpg", false, Some(1)),
            d("x.stl", false, Some(1)),
            d("n/c.stl", false, Some(1)),
        ];
        let items = top_items(&descs);
        assert_eq!(items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["m", "x.stl", "n"]);
        assert_eq!(items.iter().map(|i| i.is_dir).collect::<Vec<_>>(), [true, false, true]);
        assert_eq!(items.iter().map(|i| i.index).collect::<Vec<_>>(), [Some(0), Some(4), None]);
        assert_eq!(items[0].members, [1, 2, 3]);
        assert_eq!(items[2].members, [5]);
    }

    #[test]
    fn file_item_written_and_renamed() {
        let tmp = tempfile::tempdir().unwrap();
        let p = promises(&[("x.stl", false, b"solid")]);
        let out = p.write(0, &tmp.path().join("x.stl")).unwrap();
        assert_eq!(out, tmp.path().join("x.stl"));
        assert_eq!(std::fs::read(&out).unwrap(), b"solid");
        assert!(parts_under(tmp.path()).is_empty());
    }

    #[test]
    fn folder_item_written_with_subfolders() {
        let tmp = tempfile::tempdir().unwrap();
        let p = promises(&[("m", true, b""), ("m/a.stl", false, b"a"), ("m/sub", true, b""), ("m/sub/b.jpg", false, b"bb")]);
        let out = p.write(0, &tmp.path().join("m")).unwrap();
        assert_eq!(std::fs::read(out.join("a.stl")).unwrap(), b"a");
        assert_eq!(std::fs::read(out.join("sub").join("b.jpg")).unwrap(), b"bb");
        assert!(out.join("sub").is_dir());
    }

    #[test]
    fn free_dest_skips_taken_names() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        std::fs::write(t.join("a.stl"), b"1").unwrap();
        std::fs::write(t.join("a 2.stl"), b"2").unwrap();
        std::fs::create_dir(t.join("m")).unwrap();
        assert_eq!(free_dest(&t.join("a.stl"), false), t.join("a 3.stl"));
        assert_eq!(free_dest(&t.join("m"), true), t.join("m 2"));
        assert_eq!(free_dest(&t.join("free.stl"), false), t.join("free.stl"));

        std::fs::write(t.join("x.stl"), b"old").unwrap();
        let p = promises(&[("x.stl", false, b"new")]);
        let out = p.write(0, &t.join("x.stl")).unwrap();
        assert_eq!(out, t.join("x 2.stl"));
        assert_eq!(std::fs::read(t.join("x.stl")).unwrap(), b"old");
        assert_eq!(std::fs::read(out).unwrap(), b"new");
    }

    #[test]
    fn folder_item_keeps_going_after_a_failed_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = promises(&[("m", true, b""), ("m/a.stl", false, b"!fail"), ("m/b.stl", false, b"b")]);
        let e = p.write(0, &tmp.path().join("m")).unwrap_err();
        assert!(e.starts_with("1 of 2 files could not be written; first: m/a.stl:"), "{e}");
        assert_eq!(std::fs::read(tmp.path().join("m").join("b.stl")).unwrap(), b"b");
        assert!(!tmp.path().join("m").join("a.stl").exists());
        assert!(parts_under(tmp.path()).is_empty());
    }

    #[test]
    fn free_name_follows_the_item_kind_not_what_is_in_the_way() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        // a folder item blocked by a file: the dot is part of the folder's name
        std::fs::write(t.join("v1.2"), b"file").unwrap();
        let p = promises(&[("v1.2", true, b""), ("v1.2/a.stl", false, b"a")]);
        assert_eq!(p.write(0, &t.join("v1.2")).unwrap(), t.join("v1.2 2"));
        // a file item blocked by a folder: the extension stays at the end
        std::fs::create_dir(t.join("x.stl")).unwrap();
        let p = promises(&[("x.stl", false, b"x")]);
        assert_eq!(p.write(0, &t.join("x.stl")).unwrap(), t.join("x 2.stl"));
    }

    #[test]
    fn failed_folder_is_counted_apart_from_files() {
        let tmp = tempfile::tempdir().unwrap();
        // the file `sub` takes the name the folder `sub` needs, so the folder and its file fail
        let p = promises(&[("m", true, b""), ("m/sub", false, b"s"), ("m/sub", true, b""), ("m/sub/b.stl", false, b"b")]);
        let e = p.write(0, &tmp.path().join("m")).unwrap_err();
        assert!(e.starts_with("1 of 2 files and 1 folder could not be written; first: m/sub:"), "{e}");
    }

    #[test]
    fn file_longer_than_indexed_size_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Promises::new(vec![d("x.stl", false, Some(1))], Arc::new(Bodies(vec![b"ab".to_vec()])));
        assert!(p.write(0, &tmp.path().join("x.stl")).is_err());
        assert!(!tmp.path().join("x.stl").exists());
        assert!(!tmp.path().join("x.stl.part").exists());
    }

    #[test]
    fn write_file_never_replaces_an_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("x.stl");
        std::fs::write(&path, b"old").unwrap();
        let c = Bodies(vec![b"new".to_vec()]);
        assert!(write_file(&c, 0, Some(3), &path, false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        assert!(parts_under(tmp.path()).is_empty());
        // allowed to move on, it takes the next free name
        assert_eq!(write_file(&c, 0, Some(3), &path, true).unwrap(), tmp.path().join("x 2.stl"));
    }

    #[test]
    fn next_drag_keeps_a_drag_that_started_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let p = promises(&[("a.stl", false, b"a"), ("b.stl", false, b"b")]);
        p.write(0, &tmp.path().join("a.stl")).unwrap();
        assert!(!p.release_if_idle());
        assert!(!p.finished());
        p.write(1, &tmp.path().join("b.stl")).unwrap();
        assert!(p.finished());

        let idle = promises(&[("c.stl", false, b"c")]);
        assert!(idle.release_if_idle());
        assert!(idle.finished());
        assert_eq!(idle.write(0, &tmp.path().join("c.stl")).unwrap_err(), "the drag is over");
    }

    /// Holds a token whose strong count shows whether the contents are still alive.
    struct Held(#[allow(dead_code)] Arc<()>);

    impl Contents for Held {
        fn write(&self, _: usize, out: &mut dyn std::io::Write) -> Result<(), String> {
            out.write_all(b"h").map_err(|e| e.to_string())
        }
    }

    #[test]
    fn release_drops_contents_and_later_writes_fail() {
        let tmp = tempfile::tempdir().unwrap();
        let token = Arc::new(());
        let p = Promises::new(vec![d("x.stl", false, Some(1))], Arc::new(Held(token.clone())));
        assert_eq!(Arc::strong_count(&token), 2);
        p.release();
        assert_eq!(Arc::strong_count(&token), 1);
        assert_eq!(p.write(0, &tmp.path().join("x.stl")).unwrap_err(), "the drag is over");

        // writing every item releases them too
        let token = Arc::new(());
        let p = Promises::new(vec![d("a.stl", false, Some(1)), d("b.stl", false, Some(1))], Arc::new(Held(token.clone())));
        p.write(0, &tmp.path().join("a.stl")).unwrap();
        assert_eq!(Arc::strong_count(&token), 2);
        p.write(1, &tmp.path().join("b.stl")).unwrap();
        assert_eq!(Arc::strong_count(&token), 1);
    }
}
