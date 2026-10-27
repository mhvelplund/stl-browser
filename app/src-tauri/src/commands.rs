//! Tauri commands. Each command is a thin wrapper over an `_impl` fn that tests can call directly.
use serde::Serialize;

use crate::browse::{self, Listing, View};
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_schema() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        let info = app_info_impl(&state).unwrap();
        assert_eq!(info.schema_version, stl_index::db::SCHEMA_VERSION);
        assert!(info.db_path.ends_with("index.db"), "{}", info.db_path);
    }
}
