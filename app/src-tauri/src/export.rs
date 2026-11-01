//! Drag-out and Extract to…: both write the dragged items' subtrees as the current view shows
//! them. A drag hands Explorer virtual files (`stl-drag`); Extract to… writes them to a folder
//! on a background thread with `extract-progress` / `extract-done` events.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde::Serialize;
use stl_core::vtree::NodeId;
use stl_core::vtree::export::export_plan;
use stl_index::export::{ExportFile, resolve_plan, unique_dest, write_tree};
use stl_scan::extract::SourceKind;
use stl_scan::session::ExtractSession;
use tauri::{AppHandle, Emitter};

use crate::browse::{Target, View, Views, visible};
use crate::state::AppState;

pub const EXTRACT_BUSY: &str = "an extraction is already running";
pub const INDEX_BUSY: &str = "the index is being updated; try again when it is done";

/// The nodes of `targets` (each a child of the folder at exactly its `path`), in order and
/// without repeats; a target inside another target is dropped, so nothing is written twice.
/// An error when the UI is stale.
pub fn nodes_for(views: &Views, view: View, targets: &[Target]) -> Result<Vec<NodeId>, String> {
    let t = &views.get(view).tree;
    let lower = |v: &[String]| v.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>();
    let full: Vec<Vec<String>> = targets.iter().map(|g| [lower(&g.path), vec![g.name.to_lowercase()]].concat()).collect();
    let mut ids = Vec::new();
    for (i, g) in targets.iter().enumerate() {
        // below another target (whose full path is a proper prefix of this one's)
        if full.iter().any(|o| o.len() < full[i].len() && full[i].starts_with(o)) {
            continue;
        }
        let (folder, used) = t.walk(&g.path);
        if used != g.path.len() {
            return Err(format!("the folder {} no longer exists", g.path.join("/")));
        }
        let name = g.name.to_lowercase();
        let id = t.nodes[folder].children.iter().copied().find(|&c| t.nodes[c].name == name).ok_or(format!("{name} no longer exists"))?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// The files and folders to write for `targets`, and how many items were skipped (locked
/// archives).
pub fn build_export(state: &AppState, view: View, targets: &[Target]) -> Result<(Vec<ExportFile>, u32), String> {
    // during a run, re-listed sources reuse entry ids while the views still hold the old ones
    if state.is_indexing() {
        return Err(INDEX_BUSY.into());
    }
    let views = state.views()?;
    let ids = nodes_for(&views, view, targets)?;
    let plan = export_plan(&views.get(view).tree, &ids, &|n| visible(&state.shown, n));
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let files = resolve_plan(&conn, &plan).map_err(|e| e.to_string())?;
    Ok((files, plan.skipped))
}

/// `extract-progress`: before each item; `index` is 1-based.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractProgress {
    pub index: usize,
    pub total: usize,
    pub name: String,
}

/// `extract-done`: once per extraction.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractDone {
    pub ok: bool,
    pub error: Option<String>,
    pub files: u32,
    pub dirs: u32,
    /// Locked archives left out.
    pub skipped: u32,
    /// `(rel, error)` per file that could not be written.
    pub failed: Vec<(String, String)>,
    /// Where the first item went.
    pub dest: String,
    pub seconds: f64,
    /// Stopped by the user before every file was written.
    pub cancelled: bool,
}

/// Write `files` into `dest_dir`, renaming each top-level item that already exists there to
/// `name (2)`… so nothing is overwritten.
pub fn extract_worker(
    files: Vec<ExportFile>,
    skipped: u32,
    dest_dir: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(ExtractProgress),
) -> ExtractDone {
    let t = Instant::now();
    let mut renamed: Vec<(String, String)> = Vec::new();
    let mut first: Option<PathBuf> = None;
    let mut files = files;
    for f in &mut files {
        let (top, rest) = match f.rel.split_once('/') {
            Some((top, rest)) => (top.to_string(), Some(rest.to_string())),
            None => (f.rel.clone(), None),
        };
        let new_top = match renamed.iter().find(|(old, _)| *old == top) {
            Some((_, new)) => new.clone(),
            None => {
                let p = unique_dest(dest_dir, &top, f.is_dir || rest.is_some());
                let new = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(top.clone());
                first.get_or_insert(p);
                renamed.push((top, new.clone()));
                new
            }
        };
        f.rel = match rest {
            Some(rest) => format!("{new_top}/{rest}"),
            None => new_top,
        };
    }
    let total = files.len();
    let mut session = ExtractSession::new();
    let r = write_tree(dest_dir, &files, &mut session, &mut |i, rel| progress(ExtractProgress { index: i + 1, total, name: rel.to_string() }), cancel);
    ExtractDone {
        ok: r.failed.is_empty(),
        error: None,
        files: r.files,
        dirs: r.dirs,
        skipped,
        failed: r.failed,
        dest: first.map(|p| p.display().to_string()).unwrap_or_else(|| dest_dir.display().to_string()),
        seconds: t.elapsed().as_secs_f64(),
        cancelled: cancel.load(Ordering::SeqCst),
    }
}

/// Write `targets` (items of `view`) into `dest` in the background. Returns
/// once the work is queued; one extraction runs at a time.
#[tauri::command(async)]
pub fn extract_to(app: AppHandle, state: tauri::State<'_, AppState>, view: View, targets: Vec<Target>, dest: String) -> Result<(), String> {
    let guard = state.try_begin_extract().ok_or(EXTRACT_BUSY)?;
    let (files, skipped) = build_export(&state, view, &targets)?;
    let dest = PathBuf::from(dest);
    if !dest.is_dir() {
        return Err(format!("{} is not a folder", dest.display()));
    }
    let cancel = state.cancel_extract.clone();
    std::thread::spawn(move || {
        let _guard = guard;
        let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            extract_worker(files, skipped, &dest, &cancel, &mut |p| {
                let _ = app.emit("extract-progress", p);
            })
        }))
        .unwrap_or_else(|_| ExtractDone { error: Some("internal error".into()), ..Default::default() });
        let _ = app.emit("extract-done", done);
    });
    Ok(())
}

/// Stop the running Extract to… before its next file.
#[tauri::command]
pub fn cancel_extract(state: tauri::State<'_, AppState>) {
    state.cancel_extract.store(true, Ordering::SeqCst);
}

/// The folder that files opened from archives are written to; emptied at app start.
pub fn open_dir() -> PathBuf {
    std::env::temp_dir().join("stl-browser-open")
}

/// Files a double-click may hand to the OS: archives are untrusted, so never programs,
/// scripts, shortcuts or web pages.
fn openable(name: &str) -> bool {
    use stl_core::names::{Kind, ext, kind_of};
    match kind_of(name) {
        Kind::Model | Kind::Scene | Kind::Image => true,
        Kind::Doc | Kind::Meta => !matches!(ext(name).as_str(), ".html" | ".htm"),
        _ => false,
    }
}

/// Mark a file written from an archive as downloaded, so the OS treats it with the same care as
/// the archive it came from: Mark-of-the-Web on Windows, the quarantine xattr on macOS. Best
/// effort: not every disk has streams or extended attributes.
#[cfg(windows)]
fn mark_downloaded(path: &Path) {
    let mut ads = path.as_os_str().to_owned();
    ads.push(":Zone.Identifier");
    let _ = std::fs::write(ads, "[ZoneTransfer]\r\nZoneId=3\r\n");
}

#[cfg(target_os = "macos")]
fn mark_downloaded(path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let Ok(p) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return };
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let value = format!("0081;{secs:x};STL Browser;");
    unsafe {
        libc::setxattr(p.as_ptr(), c"com.apple.quarantine".as_ptr(), value.as_ptr().cast(), value.len(), 0, 0);
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn mark_downloaded(_: &Path) {}

/// A path to open the file `name` (in the folder at `path` of `view`) from: a loose file's own
/// path, or a copy of an archive member written to a new folder below `tmp`.
pub fn stage_for_open(state: &AppState, view: View, path: &[String], name: &str, tmp: &Path) -> Result<PathBuf, String> {
    if !openable(name) {
        return Err(format!("{name} is not a model, picture or document, so it is not opened (use Extract to…)"));
    }
    let (files, skipped) = build_export(state, view, &[Target { path: path.to_vec(), name: name.to_string() }])?;
    let f = match files.as_slice() {
        [f] if !f.is_dir => f,
        [] if skipped > 0 => return Err(format!("{name} is locked")),
        _ => return Err(format!("{name} is not a file")),
    };
    let loc = f.loc.as_ref().ok_or("the file is no longer in the index")?;
    if loc.kind == SourceKind::Folder {
        let rel = loc.chain.first().ok_or("the file has no path")?;
        return Ok(rel.split('/').fold(PathBuf::from(&loc.source_path), |p, part| p.join(part)));
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = tmp.join(nanos.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let r = write_tree(&dir, &files, &mut ExtractSession::new(), &mut |_, _| {}, &AtomicBool::new(false));
    if let Some((_, e)) = r.failed.first() {
        return Err(e.clone());
    }
    let out = dir.join(&f.rel);
    mark_downloaded(&out);
    Ok(out)
}

/// Open the file `name` (in the folder at `path` of `view`) in its default app; a file in an
/// archive is extracted to a temp folder first.
#[tauri::command(async)]
pub fn open_file(state: tauri::State<'_, AppState>, view: View, path: Vec<String>, name: String) -> Result<(), String> {
    let p = stage_for_open(&state, view, &path, &name, &open_dir())?;
    tauri_plugin_opener::open_path(&p, None::<&str>).map_err(|e| e.to_string())
}

/// What a drag carries.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragInfo {
    pub files: usize,
    /// Locked archives and paths too long for Explorer, left out.
    pub skipped: usize,
}

#[cfg(any(windows, target_os = "macos"))]
mod drag {
    use std::sync::{Arc, Mutex};

    use stl_index::export::{ExportFile, FileServer};

    /// Temp-disk room for files read ahead from 7z and RAR archives during one drag.
    const READ_AHEAD: u64 = 2 << 30;

    /// A drag's files, extracted on the drop target's request.
    pub struct DragContents(Mutex<FileServer>);

    impl DragContents {
        pub fn new(files: Vec<ExportFile>) -> Self {
            DragContents(Mutex::new(FileServer::new(files, READ_AHEAD)))
        }
    }

    impl stl_drag::Contents for DragContents {
        fn write(&self, index: usize, out: &mut dyn std::io::Write) -> Result<(), String> {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).write(index, out).map(|_| ())
        }
    }

    pub fn descriptors(files: &[ExportFile]) -> Vec<stl_drag::Descriptor> {
        files
            .iter()
            .map(|f| stl_drag::Descriptor { rel: f.rel.clone(), is_dir: f.is_dir, size: f.loc.as_ref().and_then(|l| l.size) })
            .collect()
    }

    pub fn prepare(files: Vec<ExportFile>) -> Result<stl_drag::Prepared, String> {
        let items = descriptors(&files);
        stl_drag::prepare(items, Arc::new(DragContents::new(files)))
    }
}

/// Start dragging `targets` (items of `view`) out of the window. The drag runs
/// on the main thread; it is skipped if the mouse button was released in the meantime.
#[tauri::command(async)]
pub fn begin_drag(window: tauri::WebviewWindow, state: tauri::State<'_, AppState>, view: View, targets: Vec<Target>) -> Result<DragInfo, String> {
    let (files, skipped) = build_export(&state, view, &targets)?;
    if files.is_empty() {
        return Err("nothing to drag: locked archives can't be extracted".into());
    }
    start_drag(&window, files, skipped)
}

#[cfg(windows)]
fn start_drag(window: &tauri::WebviewWindow, files: Vec<ExportFile>, skipped: u32) -> Result<DragInfo, String> {
    let n = files.iter().filter(|f| !f.is_dir).count();
    let prepared = drag::prepare(files)?;
    let info = DragInfo { files: n, skipped: skipped as usize + prepared.too_long };
    window
        .run_on_main_thread(move || {
            if let Err(e) = stl_drag::run_drag(prepared) {
                eprintln!("drag failed: {e}");
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(info)
}

#[cfg(target_os = "macos")]
fn start_drag(window: &tauri::WebviewWindow, files: Vec<ExportFile>, skipped: u32) -> Result<DragInfo, String> {
    let n = files.iter().filter(|f| !f.is_dir).count();
    let prepared = drag::prepare(files)?;
    let info = DragInfo { files: n, skipped: skipped as usize + prepared.too_long };
    // a raw pointer isn't Send; the view is only touched on the main thread
    let view = window.ns_view().map_err(|e| e.to_string())? as usize;
    window
        .run_on_main_thread(move || {
            if let Err(e) = stl_drag::run_drag(prepared, view as *mut std::ffi::c_void) {
                eprintln!("drag failed: {e}");
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(info)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn start_drag(_: &tauri::WebviewWindow, _: Vec<ExportFile>, _: u32) -> Result<DragInfo, String> {
    Err("dragging files out needs Windows or macOS; use Extract to…".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::library;
    use stl_core::vtree::filter::Shown;

    fn segs(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    fn tg(path: &[&str], name: &str) -> Target {
        Target { path: segs(path), name: name.into() }
    }

    fn tgs(path: &[&str], names: &[&str]) -> Vec<Target> {
        names.iter().map(|n| tg(path, n)).collect()
    }

    fn rels(files: &[ExportFile]) -> Vec<&str> {
        files.iter().map(|f| f.rel.as_str()).collect()
    }

    #[test]
    fn a_loose_file_opens_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let tmp = dir.path().join("open");
        let p = stage_for_open(&state, View::Files, &segs(&["vol", "coll", "orc"]), "orc.stl", &tmp).unwrap();
        assert_eq!(p, dir.path().join("lib/Vol/Coll/Orc/orc.stl"));
        assert!(!tmp.exists());
        assert!(stage_for_open(&state, View::Files, &segs(&["vol", "coll"]), "orc", &tmp).is_err());
    }

    #[test]
    fn an_archive_member_is_written_to_temp() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        w.start_file("kit/k.stl", zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut w, b"kk").unwrap();
        std::fs::write(dir.path().join("lib/Vol/Coll/Kit.zip"), w.finish().unwrap().into_inner()).unwrap();
        let (done, views) = crate::indexer::run_index(&state.db_path(), None, &AtomicBool::new(false), &mut |_| {});
        assert!(done.ok, "{:?}", done.error);
        state.set_views(views.unwrap());
        let tmp = dir.path().join("open");
        let p = stage_for_open(&state, View::Files, &segs(&["vol", "coll", "kit", "kit"]), "k.stl", &tmp).unwrap();
        assert!(p.starts_with(&tmp), "{}", p.display());
        assert_eq!(p.file_name().unwrap(), "k.stl");
        assert_eq!(std::fs::read(&p).unwrap(), b"kk");
        let mut ads = p.as_os_str().to_owned();
        ads.push(":Zone.Identifier");
        #[cfg(windows)]
        assert!(std::fs::read_to_string(&ads).unwrap().contains("ZoneId=3"));
        #[cfg(not(windows))]
        assert!(!Path::new(&ads).exists(), "no stray Zone.Identifier file");
        #[cfg(target_os = "macos")]
        {
            let q = quarantine_of(&p).expect("quarantine xattr");
            assert!(q.starts_with("0081;"), "{q}");
            assert!(q.ends_with(";STL Browser;"), "{q}");
        }
    }

    /// The `com.apple.quarantine` xattr of `p`, if it has one.
    #[cfg(target_os = "macos")]
    fn quarantine_of(p: &Path) -> Option<String> {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(p.as_os_str().as_bytes()).ok()?;
        let mut buf = [0u8; 256];
        let n = unsafe {
            libc::getxattr(path.as_ptr(), c"com.apple.quarantine".as_ptr(), buf.as_mut_ptr().cast(), buf.len(), 0, 0)
        };
        (n >= 0).then(|| String::from_utf8_lossy(&buf[..n as usize]).into_owned())
    }

    #[test]
    fn programs_and_web_pages_are_not_opened() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let tmp = dir.path().join("open");
        for name in ["setup.exe", "run.bat", "x.lnk", "page.html", "a.js"] {
            let e = stage_for_open(&state, View::Models, &segs(&["vol", "coll", "orc"]), name, &tmp).unwrap_err();
            assert!(e.contains("not opened"), "{name}: {e}");
        }
        assert!(openable("notes.txt") && openable("guide.pdf") && openable("orc.STL") && openable("x.lys") && openable("p.jpg"));
    }

    #[test]
    fn nodes_for_finds_targets_in_different_folders() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let views = state.views().unwrap();
        let ids = nodes_for(&views, View::Files, &[tg(&["vol", "coll"], "Orc"), tg(&["vol", "coll", "elf"], "elf.stl")]).unwrap();
        let names: Vec<_> = ids.iter().map(|&i| views.files.tree.nodes[i].name.as_str()).collect();
        assert_eq!(names, ["orc", "elf.stl"]);
    }

    #[test]
    fn nodes_for_rejects_stale_names() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let views = state.views().unwrap();
        assert!(nodes_for(&views, View::Files, &[tg(&["vol", "coll"], "gone")]).is_err());
        assert!(nodes_for(&views, View::Files, &[tg(&["vol", "nope"], "orc")]).is_err());
    }

    #[test]
    fn targets_drop_nested() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let both = [tg(&["vol", "coll", "orc"], "orc.stl"), tg(&["Vol", "coll"], "Orc")];
        let views = state.views().unwrap();
        let ids = nodes_for(&views, View::Files, &both).unwrap();
        let names: Vec<_> = ids.iter().map(|&i| views.files.tree.nodes[i].name.as_str()).collect();
        assert_eq!(names, ["orc"]);
        drop(views);
        let (files, _) = build_export(&state, View::Files, &both).unwrap();
        assert_eq!(rels(&files), ["orc", "orc/notes.txt", "orc/orc.jpg", "orc/orc.stl"]);
    }

    #[test]
    fn build_export_mixed_folders_renames_clashes() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        std::fs::write(dir.path().join("lib/Vol/Coll/Elf/orc.stl"), b"x").unwrap();
        let (done, views) = crate::indexer::run_index(&state.db_path(), None, &AtomicBool::new(false), &mut |_| {});
        assert!(done.ok, "{:?}", done.error);
        state.set_views(views.unwrap());
        let (files, _) = build_export(&state, View::Files, &[tg(&["vol", "coll", "orc"], "orc.stl"), tg(&["vol", "coll", "elf"], "orc.stl")]).unwrap();
        let r = rels(&files);
        assert_eq!(r.len(), 2, "{r:?}");
        assert_eq!(r[0], "orc.stl");
        assert!(r[1] != "orc.stl" && r[1].ends_with(".stl"), "{r:?}");
    }

    #[test]
    fn build_export_takes_the_shown_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = library(dir.path());
        let (files, _) = build_export(&state, View::Files, &tgs(&["vol", "coll"], &["orc"])).unwrap();
        assert_eq!(rels(&files), ["orc", "orc/notes.txt", "orc/orc.jpg", "orc/orc.stl"]);
        state.shown = Shown::new(["stl", "jpg"]);
        let (files, skipped) = build_export(&state, View::Files, &tgs(&["vol", "coll"], &["orc"])).unwrap();
        assert_eq!(rels(&files), ["orc", "orc/orc.jpg", "orc/orc.stl"]);
        assert_eq!(skipped, 0);
        assert!(files[2].loc.is_some());
        let (files, _) = build_export(&state, View::Models, &tgs(&["vol", "coll"], &["orc"])).unwrap();
        assert!(!rels(&files).contains(&"orc/docs/notes.txt"), "{:?}", rels(&files));
        state.shown = Shown::defaults();
        let (files, _) = build_export(&state, View::Models, &tgs(&["vol", "coll"], &["orc"])).unwrap();
        assert!(rels(&files).contains(&"orc/docs/notes.txt"), "{:?}", rels(&files));
    }

    #[test]
    fn extract_to_writes_tree_and_renames() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = library(dir.path());
        state.shown = Shown::new(["stl", "jpg"]);
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        let run = || {
            let (files, skipped) = build_export(&state, View::Files, &tgs(&["vol", "coll"], &["orc", "elf"])).unwrap();
            let mut seen = 0;
            let done = extract_worker(files, skipped, &out, &AtomicBool::new(false), &mut |p| {
                seen += 1;
                assert_eq!(p.index, seen);
                assert_eq!(p.total, 5);
            });
            assert_eq!(seen, 5);
            done
        };
        let a = run();
        assert!(a.ok, "{:?}", a.failed);
        assert_eq!((a.files, a.dirs), (3, 2));
        assert_eq!(a.dest, out.join("orc").display().to_string());
        let b = run();
        assert_eq!(b.dest, out.join("orc (2)").display().to_string());
        assert_eq!(std::fs::read(out.join("orc (2)/orc.stl")).unwrap(), b"o");
        assert_eq!(std::fs::read(out.join("elf (2)/elf.stl")).unwrap(), b"e");
        assert_eq!(std::fs::read(out.join("orc/orc.stl")).unwrap(), b"o");
    }

    #[test]
    fn export_refused_while_indexing() {
        // an index run re-lists sources, and entry ids get reused before the views are rebuilt
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let g = state.try_begin_index().unwrap();
        let err = build_export(&state, View::Files, &tgs(&["vol", "coll"], &["orc"])).unwrap_err();
        assert_eq!(err, INDEX_BUSY);
        drop(g);
        assert!(build_export(&state, View::Files, &tgs(&["vol", "coll"], &["orc"])).is_ok());
    }

    #[test]
    fn extract_guard_is_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let g = state.try_begin_extract().unwrap();
        assert!(state.try_begin_extract().is_none());
        drop(g);
        assert!(state.try_begin_extract().is_some());
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn drag_contents_extract_by_index() {
        use stl_drag::Contents;
        let dir = tempfile::tempdir().unwrap();
        let mut state = library(dir.path());
        state.shown = Shown::new(["stl", "jpg"]);
        let (files, _) = build_export(&state, View::Files, &tgs(&["vol", "coll"], &["orc"])).unwrap();
        let d = drag::descriptors(&files);
        assert_eq!(d[2].rel, "orc/orc.stl");
        assert_eq!(d[2].size, Some(1));
        assert!(d[0].is_dir);
        let c = drag::DragContents::new(files);
        let mut out = Vec::new();
        c.write(2, &mut out).unwrap();
        assert_eq!(out, b"o");
        assert!(c.write(0, &mut Vec::new()).is_err()); // a folder has no contents
    }

    /// The whole drag pipeline on real data, dropped on the shell's own drop target for a temp
    /// folder (what Explorer does on a real drop). Needs an indexed `STL_BROWSER_HOME` and
    /// `STL_BROWSER_DRAG_CHECK="models|<folder/path>|<name>"`; prints where the files went.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn drag_real_item_to_shell_folder() {
        use std::time::{Duration, Instant};
        use windows::Win32::Foundation::POINTL;
        use windows::Win32::System::Ole::{DROPEFFECT_COPY, IDropTarget, OleInitialize, OleUninitialize};
        use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
        use windows::Win32::UI::Shell::{BHID_SFUIObject, IShellItem, SHCreateItemFromParsingName};
        use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage};

        let check = std::env::var("STL_BROWSER_DRAG_CHECK").expect("STL_BROWSER_DRAG_CHECK");
        let parts: Vec<&str> = check.split('|').collect();
        let view = if parts[0] == "files" { View::Files } else { View::Models };
        let path: Vec<String> = parts[1].split('/').map(String::from).collect();
        let state = AppState::open(stl_index::paths::app_dir()).unwrap();
        let (files, _) = build_export(&state, view, &[Target { path, name: parts[2].to_string() }]).unwrap();
        let want: Vec<(String, u64)> = files.iter().filter(|f| !f.is_dir).map(|f| (f.rel.clone(), f.loc.as_ref().unwrap().size.unwrap())).collect();
        let prepared = drag::prepare(files).unwrap();
        let dest = tempfile::tempdir().unwrap().keep();
        let d = dest.clone();
        let t = Instant::now();
        std::thread::spawn(move || unsafe {
            OleInitialize(None).unwrap();
            let obj = prepared.data_object().unwrap();
            let item: IShellItem = SHCreateItemFromParsingName(&windows_core::HSTRING::from(d.as_os_str()), None).unwrap();
            let target: IDropTarget = item.BindToHandler(None, &BHID_SFUIObject).unwrap();
            let mut effect = DROPEFFECT_COPY;
            target.DragEnter(&obj, MODIFIERKEYS_FLAGS(MK_LBUTTON.0), POINTL::default(), &mut effect).unwrap();
            target.Drop(&obj, MODIFIERKEYS_FLAGS(0), POINTL::default(), &mut effect).unwrap();
            drop((obj, target, item));
            let size = |rel: &str| std::fs::metadata(d.join(rel)).map(|m| m.len()).ok();
            while want.iter().any(|(rel, n)| size(rel) != Some(*n)) && t.elapsed() < Duration::from_secs(300) {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let missing: Vec<_> = want.iter().filter(|(rel, n)| size(rel) != Some(*n)).collect();
            assert!(missing.is_empty(), "not copied (or wrong size): {missing:?}");
            OleUninitialize();
        })
        .join()
        .unwrap();
        println!("dropped into {} in {:.1} s", dest.display(), t.elapsed().as_secs_f64());
    }
}
