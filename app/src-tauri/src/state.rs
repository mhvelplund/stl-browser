//! Shared app state managed by Tauri.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use rusqlite::Connection;
use stl_index::db;

use crate::browse::{Views, build_views};
use crate::thumbs::ThumbService;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub app_dir: PathBuf,
    /// Both view trees, built on first use and replaced after each index run.
    views: RwLock<Option<Arc<Views>>>,
    /// Set while an index run is in progress.
    indexing: Arc<AtomicBool>,
    /// Started at app setup (it needs the app handle for its events).
    pub thumbs: OnceLock<ThumbService>,
}

/// Held by the one running index; clears the busy flag when dropped (also on panic).
pub struct IndexGuard(Arc<AtomicBool>);

impl Drop for IndexGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl AppState {
    /// Open `<app_dir>/index.db`, migrating it if needed.
    pub fn open(app_dir: PathBuf) -> rusqlite::Result<Self> {
        let conn = db::open(&app_dir.join("index.db"))?;
        Ok(Self { db: Mutex::new(conn), app_dir, views: RwLock::new(None), indexing: Arc::default(), thumbs: OnceLock::new() })
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

    /// Claim the index run, or `None` if one is already running.
    pub fn try_begin_index(&self) -> Option<IndexGuard> {
        self.indexing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| IndexGuard(self.indexing.clone()))
    }

    pub fn is_indexing(&self) -> bool {
        self.indexing.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_index_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        let g = state.try_begin_index().unwrap();
        assert!(state.is_indexing());
        assert!(state.try_begin_index().is_none());
        drop(g);
        assert!(!state.is_indexing());
    }

    #[test]
    fn busy_flag_released_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = state.try_begin_index().unwrap();
            panic!("boom");
        }));
        assert!(!state.is_indexing());
        assert!(state.try_begin_index().is_some());
    }
}
