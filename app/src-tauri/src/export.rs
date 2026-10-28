//! Drag-out and Extract to…: both write the dragged items' subtrees as the current view shows
//! them. A drag hands Explorer virtual files (`stl-drag`); Extract to… writes them to a folder
//! on a background thread with `extract-progress` / `extract-done` events.
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use serde::Serialize;
use stl_core::vtree::NodeId;
use stl_core::vtree::export::export_plan;
use stl_index::export::{ExportFile, resolve_plan, unique_dest, write_tree};
use stl_scan::session::ExtractSession;
use tauri::{AppHandle, Emitter};

use crate::browse::{View, Views, visible};
use crate::state::AppState;

pub const EXTRACT_BUSY: &str = "an extraction is already running";
pub const INDEX_BUSY: &str = "the index is being updated; try again when it is done";

/// The children named `names` of the folder at exactly `path`; an error when the UI is stale.
pub fn nodes_for(views: &Views, view: View, path: &[String], names: &[String]) -> Result<Vec<NodeId>, String> {
    let t = &views.get(view).tree;
    let (folder, used) = t.walk(path);
    if used != path.len() {
        return Err(format!("the folder {} no longer exists", path.join("/")));
    }
    names
        .iter()
        .map(|name| {
            let name = name.to_lowercase();
            t.nodes[folder].children.iter().copied().find(|&c| t.nodes[c].name == name).ok_or(format!("{name} no longer exists"))
        })
        .collect()
}

/// The files and folders to write for `names` in the folder at `path`, and how many items
/// were skipped (locked archives).
pub fn build_export(state: &AppState, view: View, path: &[String], names: &[String]) -> Result<(Vec<ExportFile>, u32), String> {
    // during a run, re-listed sources reuse entry ids while the views still hold the old ones
    if state.is_indexing() {
        return Err(INDEX_BUSY.into());
    }
    let views = state.views()?;
    let ids = nodes_for(&views, view, path, names)?;
    let plan = export_plan(&views.get(view).tree, &ids, &|n| visible(view, n));
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
}

/// Write `files` into `dest_dir`, renaming each top-level item that already exists there to
/// `name (2)`… so nothing is overwritten.
pub fn extract_worker(files: Vec<ExportFile>, skipped: u32, dest_dir: &Path, progress: &mut dyn FnMut(ExtractProgress)) -> ExtractDone {
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
    let r = write_tree(dest_dir, &files, &mut session, &mut |i, rel| progress(ExtractProgress { index: i + 1, total, name: rel.to_string() }), &AtomicBool::new(false));
    ExtractDone {
        ok: r.failed.is_empty(),
        error: None,
        files: r.files,
        dirs: r.dirs,
        skipped,
        failed: r.failed,
        dest: first.map(|p| p.display().to_string()).unwrap_or_else(|| dest_dir.display().to_string()),
        seconds: t.elapsed().as_secs_f64(),
    }
}

/// Write `names` (in the folder at `path` of `view`) into `dest` in the background. Returns
/// once the work is queued; one extraction runs at a time.
#[tauri::command(async)]
pub fn extract_to(app: AppHandle, state: tauri::State<'_, AppState>, view: View, path: Vec<String>, names: Vec<String>, dest: String) -> Result<(), String> {
    let guard = state.try_begin_extract().ok_or(EXTRACT_BUSY)?;
    let (files, skipped) = build_export(&state, view, &path, &names)?;
    let dest = PathBuf::from(dest);
    if !dest.is_dir() {
        return Err(format!("{} is not a folder", dest.display()));
    }
    std::thread::spawn(move || {
        let _guard = guard;
        let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            extract_worker(files, skipped, &dest, &mut |p| {
                let _ = app.emit("extract-progress", p);
            })
        }))
        .unwrap_or_else(|_| ExtractDone { error: Some("internal error".into()), ..Default::default() });
        let _ = app.emit("extract-done", done);
    });
    Ok(())
}

/// What a drag carries.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragInfo {
    pub files: usize,
    /// Locked archives and paths too long for Explorer, left out.
    pub skipped: usize,
}

#[cfg(windows)]
mod drag {
    use std::sync::{Arc, Mutex};

    use stl_index::export::{ExportFile, FileServer};

    /// Temp-disk room for files read ahead from 7z and RAR archives during one drag.
    const READ_AHEAD: u64 = 2 << 30;

    /// A drag's files, extracted on Explorer's request.
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
            .map(|f| stl_drag::Descriptor { rel: f.rel.replace('/', "\\"), is_dir: f.is_dir, size: f.loc.as_ref().and_then(|l| l.size) })
            .collect()
    }

    pub fn prepare(files: Vec<ExportFile>) -> Result<stl_drag::Prepared, String> {
        let items = descriptors(&files);
        stl_drag::prepare(items, Arc::new(DragContents::new(files)))
    }
}

/// Start dragging `names` (in the folder at `path` of `view`) out of the window. The drag runs
/// on the main thread; it is skipped if the mouse button was released in the meantime.
#[tauri::command(async)]
pub fn begin_drag(app: AppHandle, state: tauri::State<'_, AppState>, view: View, path: Vec<String>, names: Vec<String>) -> Result<DragInfo, String> {
    let (files, skipped) = build_export(&state, view, &path, &names)?;
    if files.is_empty() {
        return Err("nothing to drag: locked archives can't be extracted".into());
    }
    start_drag(&app, files, skipped)
}

#[cfg(windows)]
fn start_drag(app: &AppHandle, files: Vec<ExportFile>, skipped: u32) -> Result<DragInfo, String> {
    let n = files.iter().filter(|f| !f.is_dir).count();
    let prepared = drag::prepare(files)?;
    let info = DragInfo { files: n, skipped: skipped as usize + prepared.too_long };
    app.run_on_main_thread(move || {
        if let Err(e) = stl_drag::run_drag(prepared) {
            eprintln!("drag failed: {e}");
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(info)
}

#[cfg(not(windows))]
fn start_drag(_: &AppHandle, _: Vec<ExportFile>, _: u32) -> Result<DragInfo, String> {
    Err("dragging files out needs Windows; use Extract to…".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::library;

    fn segs(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    fn rels(files: &[ExportFile]) -> Vec<&str> {
        files.iter().map(|f| f.rel.as_str()).collect()
    }

    #[test]
    fn nodes_for_finds_named_children() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let views = state.views().unwrap();
        let ids = nodes_for(&views, View::Files, &segs(&["vol", "coll"]), &segs(&["Orc", "elf"])).unwrap();
        let names: Vec<_> = ids.iter().map(|&i| views.files.tree.nodes[i].name.as_str()).collect();
        assert_eq!(names, ["orc", "elf"]);
    }

    #[test]
    fn nodes_for_rejects_stale_names() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let views = state.views().unwrap();
        assert!(nodes_for(&views, View::Files, &segs(&["vol", "coll"]), &segs(&["gone"])).is_err());
        assert!(nodes_for(&views, View::Files, &segs(&["vol", "nope"]), &segs(&["orc"])).is_err());
    }

    #[test]
    fn build_export_files_view_hides_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let (files, skipped) = build_export(&state, View::Files, &segs(&["vol", "coll"]), &segs(&["orc"])).unwrap();
        assert_eq!(rels(&files), ["orc", "orc/orc.jpg", "orc/orc.stl"]);
        assert_eq!(skipped, 0);
        assert!(files[2].loc.is_some());
        let (files, _) = build_export(&state, View::Models, &segs(&["vol", "coll"]), &segs(&["orc"])).unwrap();
        assert!(rels(&files).contains(&"orc/docs/notes.txt"), "{:?}", rels(&files));
    }

    #[test]
    fn extract_to_writes_tree_and_renames() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        let run = || {
            let (files, skipped) = build_export(&state, View::Files, &segs(&["vol", "coll"]), &segs(&["orc", "elf"])).unwrap();
            let mut seen = 0;
            let done = extract_worker(files, skipped, &out, &mut |p| {
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
        let err = build_export(&state, View::Files, &segs(&["vol", "coll"]), &segs(&["orc"])).unwrap_err();
        assert_eq!(err, INDEX_BUSY);
        drop(g);
        assert!(build_export(&state, View::Files, &segs(&["vol", "coll"]), &segs(&["orc"])).is_ok());
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

    #[cfg(windows)]
    #[test]
    fn drag_contents_extract_by_index() {
        use stl_drag::Contents;
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let (files, _) = build_export(&state, View::Files, &segs(&["vol", "coll"]), &segs(&["orc"])).unwrap();
        let d = drag::descriptors(&files);
        assert_eq!(d[2].rel, "orc\\orc.stl");
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
        let (files, _) = build_export(&state, view, &path, &[parts[2].to_string()]).unwrap();
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
