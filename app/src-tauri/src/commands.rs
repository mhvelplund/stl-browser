//! Tauri commands. Each command is a thin wrapper over an `_impl` fn that tests can call directly.
use serde::Serialize;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::Ordering;

use stl_core::vtree::tags::{TagKind, UserTags, tag_keys};
use stl_index::{scanstore, tagstore};

use crate::browse::{self, Found, Listing, NodeTags, SearchMode, TagView, View};
use crate::indexer::{self, BUSY};
use crate::state::{AppState, IndexGuard};
use crate::thumbs::ThumbReady;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub app_dir: String,
    pub db_path: String,
    pub schema_version: i32,
}

pub fn app_info_impl(state: &AppState) -> Result<AppInfo, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let schema_version = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(AppInfo {
        app_dir: state.app_dir.display().to_string(),
        db_path: state.db_path().display().to_string(),
        schema_version,
    })
}

#[tauri::command]
pub fn app_info(state: tauri::State<'_, AppState>) -> Result<AppInfo, String> {
    app_info_impl(&state)
}

pub fn list_folder_impl(state: &AppState, view: View, path: Vec<String>) -> Result<Listing, String> {
    let views = state.views()?;
    let selected = selected_thumbs(state)?;
    let user = user_tags(state)?;
    Ok(browse::list(&views, view, &path, &selected, &user, &state.shown))
}

/// Every user-set and hidden tag.
fn user_tags(state: &AppState) -> Result<UserTags, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    tagstore::load(&conn).map_err(|e| e.to_string())
}

/// Every saved carousel choice: node key → image key (`composite` or an `f:` entry key).
fn selected_thumbs(state: &AppState) -> Result<HashMap<String, String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut st = conn.prepare("SELECT node_key, image_key FROM selected_thumb").map_err(|e| e.to_string())?;
    let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub fn select_thumb_impl(state: &AppState, node_key: String, image_key: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO selected_thumb(node_key, image_key) VALUES (?1, ?2)
         ON CONFLICT(node_key) DO UPDATE SET image_key = ?2",
        [&node_key, &image_key],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Thumbnails for these entries: the cached ones (and known failures) now, the rest later as
/// `thumb-ready` events. `high` is what the user sees; a new request replaces the old queue.
#[tauri::command(async)]
pub fn request_thumbs(state: tauri::State<'_, AppState>, high: Vec<i64>, low: Vec<i64>) -> Result<ThumbBatch, String> {
    let service = state.thumbs.get().ok_or("thumbnails are not started")?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let ready = service.request(&conn, &high, &low)?;
    Ok(ThumbBatch { ready, pending: service.pending() })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbBatch {
    pub ready: Vec<ThumbReady>,
    /// Jobs queued or running after this request.
    pub pending: usize,
}

/// Remember which carousel slot a tile shows (user data: it survives re-indexing).
#[tauri::command(async)]
pub fn select_thumb(state: tauri::State<'_, AppState>, node_key: String, image_key: String) -> Result<(), String> {
    select_thumb_impl(&state, node_key, image_key)
}

/// List the folder at `path` (name segments from the root); falls back to the deepest folder
/// that still exists, and says so in `Listing::path`.
#[tauri::command(async)]
pub fn list_folder(state: tauri::State<'_, AppState>, view: View, path: Vec<String>) -> Result<Listing, String> {
    list_folder_impl(&state, view, path)
}

pub fn locate_impl(state: &AppState, from: View, path: &[String], to: View) -> Result<Vec<String>, String> {
    let views = state.views()?;
    Ok(stl_core::vtree::locate::locate(&views.get(from).tree, path, &views.get(to).tree))
}

/// The folder of view `to` that matches the folder at `path` of view `from` (switching views
/// keeps the user's place).
#[tauri::command(async)]
pub fn locate(state: tauri::State<'_, AppState>, from: View, path: Vec<String>, to: View) -> Result<Vec<String>, String> {
    locate_impl(&state, from, &path, to)
}

pub fn search_impl(
    state: &AppState,
    view: View,
    path: Vec<String>,
    query: String,
    mode: SearchMode,
    case_sensitive: bool,
) -> Result<Found, String> {
    let views = state.views()?;
    let selected = selected_thumbs(state)?;
    let user = user_tags(state)?;
    let q = browse::Query { text: &query, mode, case_sensitive };
    browse::search(&views, view, &path, &q, &selected, &user, &state.shown)
}

/// The folders and files below `path` whose names match `query` (at most `SEARCH_LIMIT`).
#[tauri::command(async)]
pub fn search(
    state: tauri::State<'_, AppState>,
    view: View,
    path: Vec<String>,
    query: String,
    mode: SearchMode,
    case_sensitive: bool,
) -> Result<Found, String> {
    search_impl(&state, view, path, query, mode, case_sensitive)
}

pub fn node_tags_impl(state: &AppState, view: View, path: Vec<String>, name: String) -> Result<NodeTags, String> {
    let views = state.views()?;
    let user = user_tags(state)?;
    browse::node_tags(&views, view, &path, &name, &user)
}

/// A node's own, inherited (from its folders) and hidden tags.
#[tauri::command(async)]
pub fn node_tags(state: tauri::State<'_, AppState>, view: View, path: Vec<String>, name: String) -> Result<NodeTags, String> {
    node_tags_impl(&state, view, path, name)
}

/// The tag keys of the node at `(path, name)` (one per real path), and the parsed kind.
fn tag_target(state: &AppState, view: View, path: &[String], name: &str, kind: &str) -> Result<(Vec<String>, TagKind), String> {
    let views = state.views()?;
    let at = browse::locate_node(&views, view, path, name)?;
    let keys = tag_keys(&views.get(view).tree.nodes[at.id]);
    if keys.is_empty() {
        let shown = if name.is_empty() { path.last().map_or("the library", String::as_str) } else { name };
        return Err(format!("{shown} can't be tagged"));
    }
    let kind = TagKind::parse(kind).ok_or_else(|| format!("unknown tag kind {kind}"))?;
    Ok((keys, kind))
}

type Write = fn(&rusqlite::Connection, &[String], TagKind, &str) -> Result<(), stl_index::scanstore::IndexError>;

fn write_tag(state: &AppState, view: View, path: Vec<String>, name: String, kind: String, tag: String, write: Write) -> Result<(), String> {
    let (keys, kind) = tag_target(state, view, &path, &name, &kind)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    write(&conn, &keys, kind, &tag).map_err(|e| e.to_string())
}

pub fn add_tag_impl(state: &AppState, view: View, path: Vec<String>, name: String, kind: String, tag: String) -> Result<(), String> {
    write_tag(state, view, path, name, kind, tag, tagstore::add)
}

pub fn remove_tag_impl(state: &AppState, view: View, path: Vec<String>, name: String, kind: String, tag: String) -> Result<(), String> {
    write_tag(state, view, path, name, kind, tag, tagstore::remove)
}

pub fn hide_tag_impl(state: &AppState, view: View, path: Vec<String>, name: String, kind: String, tag: String) -> Result<(), String> {
    write_tag(state, view, path, name, kind, tag, tagstore::hide)
}

pub fn unhide_tag_impl(state: &AppState, view: View, path: Vec<String>, name: String, kind: String, tag: String) -> Result<(), String> {
    write_tag(state, view, path, name, kind, tag, tagstore::unhide)
}

/// Tag a node with a user value (a scale, support or creator replaces the old one of its kind).
#[tauri::command(async)]
pub fn add_tag(state: tauri::State<'_, AppState>, view: View, path: Vec<String>, name: String, kind: String, tag: String) -> Result<(), String> {
    add_tag_impl(&state, view, path, name, kind, tag)
}

#[tauri::command(async)]
pub fn remove_tag(state: tauri::State<'_, AppState>, view: View, path: Vec<String>, name: String, kind: String, tag: String) -> Result<(), String> {
    remove_tag_impl(&state, view, path, name, kind, tag)
}

/// Hide a generated tag on a node.
#[tauri::command(async)]
pub fn hide_tag(state: tauri::State<'_, AppState>, view: View, path: Vec<String>, name: String, kind: String, tag: String) -> Result<(), String> {
    hide_tag_impl(&state, view, path, name, kind, tag)
}

#[tauri::command(async)]
pub fn unhide_tag(state: tauri::State<'_, AppState>, view: View, path: Vec<String>, name: String, kind: String, tag: String) -> Result<(), String> {
    unhide_tag_impl(&state, view, path, name, kind, tag)
}

/// The user-set tags in use plus every generated tag in the Models tree, sorted by (kind, name).
pub fn all_tags_impl(state: &AppState) -> Result<Vec<TagView>, String> {
    let views = state.views()?;
    let mut all: Vec<stl_core::vtree::tags::Tag> = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        tagstore::names(&conn).map_err(|e| e.to_string())?
    }
    .into_iter()
    .map(|(kind, name)| stl_core::vtree::tags::Tag { kind, name, by_user: true })
    .collect();
    all.extend(browse::generated_tags(&views));
    all.sort();
    all.dedup();
    Ok(all.iter().map(TagView::from).collect())
}

#[tauri::command(async)]
pub fn all_tags(state: tauri::State<'_, AppState>) -> Result<Vec<TagView>, String> {
    all_tags_impl(&state)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootInfo {
    pub id: i64,
    pub name: String,
    pub path: String,
    /// The folder no longer exists; its indexed content is kept.
    pub missing: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub roots: Vec<RootInfo>,
    pub models: i64,
    /// Indexed files (entries).
    pub files: i64,
    pub indexing: bool,
    /// Why `config.yaml` was ignored (the defaults apply), if it was.
    pub config_warning: Option<String>,
}

fn root_info(id: i64, name: String, path: String) -> RootInfo {
    let missing = !Path::new(&path).is_dir();
    RootInfo { id, name, path, missing }
}

pub fn status_impl(state: &AppState) -> Result<Status, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let q = || -> rusqlite::Result<Status> {
        let mut st = conn.prepare("SELECT id, name, path FROM roots ORDER BY id")?;
        let roots = st
            .query_map([], |r| Ok(root_info(r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        Ok(Status {
            roots,
            models: conn.query_row("SELECT count(*) FROM models", [], |r| r.get(0))?,
            files: conn.query_row("SELECT count(*) FROM entries", [], |r| r.get(0))?,
            indexing: state.is_indexing(),
            config_warning: state.config_warning.clone(),
        })
    };
    q().map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn status(state: tauri::State<'_, AppState>) -> Result<Status, String> {
    status_impl(&state)
}

/// Scan the folder at `path` of `view` (every root at the library root) and resolve what
/// changed, in the background (`index-progress`, `index-done`).
#[tauri::command(async)]
pub fn rescan(app: tauri::AppHandle, state: tauri::State<'_, AppState>, view: View, path: Vec<String>) -> Result<(), String> {
    // everything, even when the views can't be built: a full rescan is the way to recover
    let scope = if path.is_empty() { None } else { browse::scope(&*state.views()?, view, &path)? };
    indexer::start_index(app, scope)
}

/// Stop the running index after its current source.
#[tauri::command]
pub fn cancel_index(state: tauri::State<'_, AppState>) {
    if state.is_indexing() {
        state.cancel_index.store(true, Ordering::SeqCst);
    }
}

/// Claim the index run, then register the folder as a root. The claim is returned so the
/// caller can hand it to the index worker: nothing can start in between.
pub fn add_folder_impl(state: &AppState, path: &str) -> Result<(RootInfo, IndexGuard), String> {
    let guard = state.try_begin_index().ok_or(BUSY)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let id = scanstore::add_root(&conn, Path::new(path)).map_err(|e| e.to_string())?;
    let info = conn
        .query_row("SELECT id, name, path FROM roots WHERE id = ?", [id], |r| Ok(root_info(r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| e.to_string())?;
    Ok((info, guard))
}

/// Register a folder as a root, then index in the background.
#[tauri::command(async)]
pub fn add_folder(app: tauri::AppHandle, state: tauri::State<'_, AppState>, path: String) -> Result<RootInfo, String> {
    let (info, guard) = add_folder_impl(&state, &path)?;
    indexer::start_index_with(app, guard, None);
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_folder_holds_the_index_claim() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(dir.path().join("a")).unwrap();
        std::fs::create_dir_all(dir.path().join("b")).unwrap();
        let (info, guard) = add_folder_impl(&state, &dir.path().join("a").display().to_string()).unwrap();
        assert_eq!(info.name, "a");
        // the claim is taken before the root is added, so a second add is refused and adds nothing
        assert_eq!(add_folder_impl(&state, &dir.path().join("b").display().to_string()).err().as_deref(), Some(BUSY));
        assert_eq!(status_impl(&state).unwrap().roots.len(), 1);
        drop(guard);
        assert!(add_folder_impl(&state, &dir.path().join("b").display().to_string()).is_ok());
    }

    #[test]
    fn status_flags_missing_root() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::testutil::library(dir.path());
        let s = status_impl(&state).unwrap();
        assert_eq!(s.roots.len(), 1);
        assert_eq!(s.roots[0].name, "Vol");
        assert!(!s.roots[0].missing);
        assert_eq!((s.models, s.files), (2, 4));
        assert!(!s.indexing);
        std::fs::rename(dir.path().join("lib/Vol"), dir.path().join("lib/Gone")).unwrap();
        let s = status_impl(&state).unwrap();
        assert!(s.roots[0].missing);
        // still browsable from the index
        assert_eq!(list_folder_impl(&state, View::Models, vec![]).unwrap().items.len(), 1);
    }

    #[test]
    fn locate_maps_between_views() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::testutil::library(dir.path());
        let segs = |p: &[&str]| p.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(locate_impl(&state, View::Models, &segs(&["vol", "coll", "orc"]), View::Files).unwrap(), ["vol", "coll", "orc"]);
        assert_eq!(locate_impl(&state, View::Files, &segs(&["vol", "coll"]), View::Models).unwrap(), ["vol", "coll"]);
    }

    #[test]
    fn app_info_reports_schema() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        let info = app_info_impl(&state).unwrap();
        assert_eq!(info.schema_version, stl_index::db::SCHEMA_VERSION);
        assert!(info.db_path.ends_with("index.db"), "{}", info.db_path);
    }
}
