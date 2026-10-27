//! Shared app state managed by Tauri.
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use rusqlite::Connection;
use stl_index::db;

use crate::browse::{Views, build_views};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub app_dir: PathBuf,
    /// Both view trees, built on first use and replaced after each index run.
    views: RwLock<Option<Arc<Views>>>,
}

impl AppState {
    /// Open `<app_dir>/index.db`, migrating it if needed.
    pub fn open(app_dir: PathBuf) -> rusqlite::Result<Self> {
        let conn = db::open(&app_dir.join("index.db"))?;
        Ok(Self { db: Mutex::new(conn), app_dir, views: RwLock::new(None) })
    }

    pub fn db_path(&self) -> PathBuf {
        self.app_dir.join("index.db")
    }

    /// The cached views, built from the DB if there are none yet.
    pub fn views(&self) -> Result<Arc<Views>, String> {
        if let Some(v) = self.views.read().map_err(|e| e.to_string())?.as_ref() {
            return Ok(v.clone());
        }
        let mut slot = self.views.write().map_err(|e| e.to_string())?;
        if let Some(v) = slot.as_ref() {
            return Ok(v.clone());
        }
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        let v = Arc::new(build_views(&conn).map_err(|e| e.to_string())?);
        *slot = Some(v.clone());
        Ok(v)
    }

    pub fn set_views(&self, v: Views) {
        let mut slot = self.views.write().unwrap_or_else(|e| e.into_inner());
        *slot = Some(Arc::new(v));
    }
}
