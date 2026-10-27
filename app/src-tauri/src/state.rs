//! Shared app state managed by Tauri.
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use stl_index::db;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub app_dir: PathBuf,
}

impl AppState {
    /// Open `<app_dir>/index.db`, migrating it if needed.
    pub fn open(app_dir: PathBuf) -> rusqlite::Result<Self> {
        let conn = db::open(&app_dir.join("index.db"))?;
        Ok(Self { db: Mutex::new(conn), app_dir })
    }

    pub fn db_path(&self) -> PathBuf {
        self.app_dir.join("index.db")
    }
}
