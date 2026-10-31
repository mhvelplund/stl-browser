//! Shared app state managed by Tauri.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use rusqlite::Connection;
use stl_core::vtree::filter::Shown;
use stl_index::{config, db};

use crate::browse::{Views, build_views};
use crate::thumbs::ThumbService;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub app_dir: PathBuf,
    /// Both view trees, built on first use and replaced after each index run.
    views: RwLock<Option<Arc<Views>>>,
    /// Set while an index run is in progress.
    indexing: Arc<AtomicBool>,
    /// Set while an Extract to… runs.
    extracting: Arc<AtomicBool>,
    /// Set to stop the running index after its current source; cleared when a run starts.
    pub cancel_index: Arc<AtomicBool>,
    /// Set to stop the running Extract to…; cleared when one starts.
    pub cancel_extract: Arc<AtomicBool>,
    /// Started at app setup (it needs the app handle for its events).
    pub thumbs: OnceLock<ThumbService>,
    /// The file endings the views list, from `config.yaml` (read once, at startup).
    pub shown: Shown,
    /// Why `config.yaml` was ignored, if it was.
    pub config_warning: Option<String>,
}

/// Held by the one running index (or extraction); clears its busy flag when dropped (also on
/// panic).
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
        let cfg = config::load_from(&app_dir);
        Ok(Self {
            db: Mutex::new(conn),
            app_dir,
            views: RwLock::new(None),
            indexing: Arc::default(),
            extracting: Arc::default(),
            cancel_index: Arc::default(),
            cancel_extract: Arc::default(),
            thumbs: OnceLock::new(),
            shown: cfg.shown,
            config_warning: cfg.warning,
        })
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
        self.indexing.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).ok()?;
        self.cancel_index.store(false, Ordering::SeqCst);
        Some(IndexGuard(self.indexing.clone()))
    }

    /// Claim the extraction, or `None` if one is already running.
    pub fn try_begin_extract(&self) -> Option<IndexGuard> {
        self.extracting.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).ok()?;
        self.cancel_extract.store(false, Ordering::SeqCst);
        Some(IndexGuard(self.extracting.clone()))
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
    fn a_new_run_clears_an_old_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        let g = state.try_begin_index().unwrap();
        state.cancel_index.store(true, Ordering::SeqCst);
        // a refused claim leaves the running one's cancel alone
        assert!(state.try_begin_index().is_none());
        assert!(state.cancel_index.load(Ordering::SeqCst));
        drop(g);
        let _g = state.try_begin_index().unwrap();
        assert!(!state.cancel_index.load(Ordering::SeqCst));
        state.cancel_extract.store(true, Ordering::SeqCst);
        let _e = state.try_begin_extract().unwrap();
        assert!(!state.cancel_extract.load(Ordering::SeqCst));
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
