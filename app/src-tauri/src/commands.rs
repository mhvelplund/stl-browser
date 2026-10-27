//! Tauri commands. Each command is a thin wrapper over an `_impl` fn that tests can call directly.
use serde::Serialize;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_schema() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        let info = app_info_impl(&state).unwrap();
        assert_eq!(info.schema_version, 1);
        assert!(info.db_path.ends_with("index.db"), "{}", info.db_path);
    }
}
