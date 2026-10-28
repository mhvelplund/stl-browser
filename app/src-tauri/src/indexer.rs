//! Background index runs: scan + resolve on a connection of their own, then rebuild the views.
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::time::Instant;

use serde::Serialize;
use stl_index::scanstore::Progress;
use stl_index::{db, resolvestore};
use stl_scan::nested::ListOptions;
use tauri::{AppHandle, Emitter, Manager};

use crate::browse::{Views, build_views};
use crate::state::{AppState, IndexGuard};

/// `index-progress`: one per source; `index` is 1-based.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub index: usize,
    pub total: usize,
    pub name: String,
}

/// `index-done`: once per run, after the new views are in place.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDone {
    pub ok: bool,
    pub error: Option<String>,
    pub sources: usize,
    pub listed: usize,
    pub removed: usize,
    pub errors: usize,
    pub locked: usize,
    pub resolved: usize,
    pub models: usize,
    pub warnings: Vec<String>,
    pub seconds: f64,
}

/// Scan and resolve every root, then build the views. Errors and panics end up in `IndexDone`.
pub fn run_index(db_path: &Path, progress: &mut dyn FnMut(IndexProgress)) -> (IndexDone, Option<Views>) {
    let t = Instant::now();
    let mut warnings = Vec::new();
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<_, String> {
        let mut conn = db::open(db_path).map_err(|e| e.to_string())?;
        let stats = resolvestore::index_all(&mut conn, &ListOptions::default(), &mut |p| match p {
            Progress::Source { index, total, name } => {
                progress(IndexProgress { index: index + 1, total, name: name.to_string() })
            }
            Progress::Warning(w) => warnings.push(w.to_string()),
        })
        .map_err(|e| e.to_string())?;
        let views = build_views(&conn).map_err(|e| e.to_string())?;
        Ok((stats, views))
    }));
    let result = result.unwrap_or_else(|p| {
        let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned());
        Err(format!("internal error: {}", msg.unwrap_or_default()))
    });
    let seconds = t.elapsed().as_secs_f64();
    match result {
        Ok((stats, views)) => {
            let s = &stats.scan;
            let done = IndexDone {
                ok: true,
                error: None,
                sources: s.sources,
                listed: s.listed,
                removed: s.removed,
                errors: s.errors,
                locked: s.locked,
                resolved: stats.resolved,
                models: stats.models,
                warnings,
                seconds,
            };
            (done, Some(views))
        }
        Err(e) => (IndexDone { ok: false, error: Some(e), warnings, seconds, ..Default::default() }, None),
    }
}

/// Start an index run on a background thread, or refuse if one is running.
pub fn start_index(app: AppHandle) -> Result<(), String> {
    let guard = app.state::<AppState>().try_begin_index().ok_or(BUSY)?;
    start_index_with(app, guard);
    Ok(())
}

/// Run the index on a background thread under a claim the caller already holds.
pub fn start_index_with(app: AppHandle, guard: IndexGuard) {
    let db_path = app.state::<AppState>().db_path();
    std::thread::spawn(move || {
        let _guard = guard;
        let (done, views) = run_index(&db_path, &mut |p| {
            let _ = app.emit("index-progress", p);
        });
        if let Some(v) = views {
            app.state::<AppState>().set_views(v);
        }
        drop(_guard);
        let _ = app.emit("index-done", done);
    });
}

pub const BUSY: &str = "an index run is already in progress";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browse::{self, View};
    use crate::testutil::library;

    #[test]
    fn run_index_reports_progress_and_builds_views() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        std::fs::write(dir.path().join("lib/Vol/Coll/Elf/elf2.stl"), b"x").unwrap();
        let mut seen = Vec::new();
        let (done, views) = run_index(&state.db_path(), &mut |p| seen.push(p.name));
        assert!(done.ok, "{:?}", done.error);
        assert!(!seen.is_empty());
        assert_eq!(done.resolved, 1);
        assert_eq!(done.listed, 1);
        let v = views.unwrap();
        let l = browse::list(&v, View::Files, &["vol".into(), "coll".into(), "elf".into()], &Default::default());
        assert_eq!(l.items.len(), 2);
    }

    #[test]
    fn run_index_on_unopenable_db_reports_error() {
        let dir = tempfile::tempdir().unwrap();
        let (done, views) = run_index(&dir.path().join("no/such/dir/index.db"), &mut |_| {});
        assert!(!done.ok);
        assert!(done.error.is_some());
        assert!(views.is_none());
    }

    #[test]
    fn missing_root_is_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        std::fs::rename(dir.path().join("lib/Vol"), dir.path().join("lib/Gone")).unwrap();
        let (done, views) = run_index(&state.db_path(), &mut |_| {});
        assert!(done.ok, "{:?}", done.error);
        assert!(done.warnings.iter().any(|w| w.contains("not found")), "{:?}", done.warnings);
        assert_eq!(views.unwrap().models.counts[0].models, 2);
    }
}
