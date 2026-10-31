//! Tauri commands. Each command is a thin wrapper over an `_impl` fn that tests can call directly.
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::Ordering;

use stl_core::vtree::tags::{Tag, TagKind, UserTags, check_writable, normalize};
use stl_index::{scanstore, tagstore};

use crate::browse::{self, Found, Listing, NodeTags, SearchMode, TagView, Target, View};
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

/// The tag keys of the node at `(path, name)` (one per real path), and the parsed kind. A node
/// that shares its folder with its owner is refused.
fn tag_target(state: &AppState, view: View, path: &[String], name: &str, kind: &str) -> Result<(Vec<String>, TagKind), String> {
    let views = state.views()?;
    let at = browse::locate_node(&views, view, path, name)?;
    let t = &views.get(view).tree;
    let keys = t.nodes[at.id].info.tag_keys.clone();
    if keys.is_empty() {
        let shown = if name.is_empty() { path.last().map_or("the library", String::as_str) } else { name };
        return Err(format!("{shown} can't be tagged"));
    }
    check_writable(t, at.id)?;
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

/// One target's tags for the multi-item tag panel.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TargetTags {
    pub own: Vec<TagView>,
    pub hidden: Vec<TagView>,
    /// Has tag keys and doesn't share its folder with an owner.
    pub editable: bool,
    /// Not editable, but its owner is one of the targets (so it is tagged through the owner).
    pub covered: bool,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EditOp {
    /// Add to every target (a single-value kind replaces the old value).
    Add,
    /// Remove the user-set value and hide the generated one, where present.
    Remove,
    /// Give it to the targets that lack it: unhide where hidden, else add.
    Fill,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    /// Targets that got a write.
    pub changed: u32,
    /// Targets that couldn't be edited (not counting ones covered by their owner).
    pub skipped: u32,
}

/// What a bulk edit needs of a target, read before the DB is locked.
struct Facts {
    keys: Vec<String>,
    editable: bool,
    covered: bool,
    own: Vec<Tag>,
    hidden: Vec<Tag>,
    generated: Vec<Tag>,
}

fn target_facts(state: &AppState, view: View, targets: &[Target], user: &UserTags) -> Result<Vec<Facts>, String> {
    let views = state.views()?;
    let t = &views.get(view).tree;
    let ids = targets.iter().map(|g| browse::locate_node(&views, view, &g.path, &g.name).map(|a| a.id)).collect::<Result<Vec<_>, _>>()?;
    Ok(ids
        .iter()
        .map(|&id| {
            let n = &t.nodes[id];
            Facts {
                keys: n.info.tag_keys.clone(),
                editable: !n.info.tag_keys.is_empty() && n.info.tag_owner.is_none(),
                covered: n.info.tag_owner.is_some_and(|o| ids.contains(&o)),
                own: user.effective(n),
                hidden: user.hidden_of(n),
                generated: n.info.tags.clone(),
            }
        })
        .collect())
}

pub fn nodes_tags_impl(state: &AppState, view: View, targets: Vec<Target>) -> Result<Vec<TargetTags>, String> {
    let user = user_tags(state)?;
    let views = |tags: &[Tag]| tags.iter().map(TagView::from).collect();
    Ok(target_facts(state, view, &targets, &user)?
        .into_iter()
        .map(|f| TargetTags { own: views(&f.own), hidden: views(&f.hidden), editable: f.editable, covered: f.covered })
        .collect())
}

/// Each target's own and hidden tags, in `targets` order.
#[tauri::command(async)]
pub fn nodes_tags(state: tauri::State<'_, AppState>, view: View, targets: Vec<Target>) -> Result<Vec<TargetTags>, String> {
    nodes_tags_impl(&state, view, targets)
}

pub fn edit_tags_impl(state: &AppState, view: View, targets: Vec<Target>, op: EditOp, kind: String, tag: String) -> Result<EditResult, String> {
    let kind = TagKind::parse(&kind).ok_or_else(|| format!("unknown tag kind {kind}"))?;
    let name = normalize(&tag).ok_or("a tag name can't be empty")?;
    if op != EditOp::Remove && !kind.settable() {
        return Err(format!("{} tags can't be set", kind.as_str()));
    }
    let user = user_tags(state)?;
    let facts = target_facts(state, view, &targets, &user)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut out = EditResult::default();
    let is = |t: &Tag| t.kind == kind && t.name == name;
    for f in &facts {
        if !f.editable {
            if !f.covered {
                out.skipped += 1;
            }
            continue;
        }
        let write = |w: Write| w(&conn, &f.keys, kind, &name).map_err(|e| e.to_string());
        let set: Vec<&Tag> = f.keys.iter().filter_map(|k| user.set.get(k)).flatten().collect();
        let user_set = set.iter().any(|t| is(t));
        let hidden = f.hidden.iter().any(is);
        // generated and not hidden, and not overridden by a different user-set value of its kind
        // (a user-set copy of the same value goes too, so the generated one would show after it)
        let overridden_by_other = kind != TagKind::User && set.iter().any(|t| t.kind == kind && t.name != name);
        let shown_generated = f.generated.iter().any(is) && !hidden && !overridden_by_other;
        let changed = match op {
            EditOp::Add => {
                write(tagstore::add)?;
                true
            }
            EditOp::Remove => {
                if user_set {
                    write(tagstore::remove)?;
                }
                if shown_generated {
                    write(tagstore::hide)?;
                }
                user_set || shown_generated
            }
            EditOp::Fill if f.own.iter().any(is) => false,
            EditOp::Fill => {
                if hidden {
                    write(tagstore::unhide)?;
                }
                // a user-set value of a single-value kind hides the generated one: set it instead
                let overridden = kind != TagKind::User && set.iter().any(|t| t.kind == kind);
                if !hidden || overridden {
                    write(tagstore::add)?;
                }
                true
            }
        };
        if changed {
            out.changed += 1;
        }
    }
    Ok(out)
}

/// Add, remove or fill one tag over many targets; shadowed targets are skipped.
#[tauri::command(async)]
pub fn edit_tags(state: tauri::State<'_, AppState>, view: View, targets: Vec<Target>, op: EditOp, kind: String, tag: String) -> Result<EditResult, String> {
    edit_tags_impl(&state, view, targets, op, kind, tag)
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
    all.extend(views.generated.iter().cloned());
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
