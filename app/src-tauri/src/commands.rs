//! Tauri commands. Each command is a thin wrapper over an `_impl` fn that tests can call directly.
use serde::Serialize;

use std::path::Path;

use stl_index::scanstore;

use crate::browse::{self, Listing, View};
use crate::indexer::{self, BUSY};
use crate::state::AppState;

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
    Ok(browse::list(&*state.views()?, view, &path))
}

/// List the folder at `path` (name segments from the root); falls back to the deepest folder
/// that still exists, and says so in `Listing::path`.
#[tauri::command]
pub fn list_folder(state: tauri::State<'_, AppState>, view: View, path: Vec<String>) -> Result<Listing, String> {
    list_folder_impl(&state, view, path)
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
        })
    };
    q().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn status(state: tauri::State<'_, AppState>) -> Result<Status, String> {
    status_impl(&state)
}

/// Scan every root and resolve what changed, in the background (`index-progress`, `index-done`).
#[tauri::command]
pub fn rescan(app: tauri::AppHandle) -> Result<(), String> {
    indexer::start_index(app)
}

/// Register a folder as a root, then index in the background.
#[tauri::command]
pub fn add_folder(app: tauri::AppHandle, state: tauri::State<'_, AppState>, path: String) -> Result<RootInfo, String> {
    if state.is_indexing() {
        return Err(BUSY.into());
    }
    let info = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let id = scanstore::add_root(&conn, Path::new(&path)).map_err(|e| e.to_string())?;
        conn.query_row("SELECT id, name, path FROM roots WHERE id = ?", [id], |r| Ok(root_info(r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?
    };
    indexer::start_index(app)?;
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn app_info_reports_schema() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        let info = app_info_impl(&state).unwrap();
        assert_eq!(info.schema_version, stl_index::db::SCHEMA_VERSION);
        assert!(info.db_path.ends_with("index.db"), "{}", info.db_path);
    }
}
