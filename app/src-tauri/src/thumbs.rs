//! The thumbnail service: answers requests from the cache, queues the rest for the workers,
//! reports finished thumbnails, and serves the files through the `thumb` protocol.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::Connection;
use serde::Serialize;
use stl_index::{db, thumbstore};
use stl_thumb::cache::{extra_rels, is_valid_rel};
use stl_thumb::generate::{Failure, Job, generate};
use stl_thumb::queue::{Queue, default_threads};

/// `thumb-ready`: a thumbnail finished (`url: None` if it failed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbReady {
    pub entry_id: i64,
    pub url: Option<String>,
    /// More pictures of the file after `url`, for its carousel (a 3MF's plates and photos).
    pub extras: Vec<String>,
    /// Jobs still queued or running.
    pub pending: usize,
}

/// The urls of the carousel pictures cached after the still at `rel`.
fn extra_urls(dir: &Path, rel: &str) -> Vec<String> {
    extra_rels(dir, rel).iter().map(|r| thumb_url(r)).collect()
}

pub type Notify = Arc<dyn Fn(ThumbReady) + Send + Sync>;

/// Where the webview loads a cache file from.
pub fn thumb_url(rel: &str) -> String {
    if cfg!(windows) { format!("http://thumb.localhost/{rel}") } else { format!("thumb://localhost/{rel}") }
}

pub struct ThumbService {
    dir: PathBuf,
    queue: Queue,
}

impl ThumbService {
    /// Start the workers for `<app_dir>/thumbs`. They share one DB connection for their results.
    pub fn new(app_dir: &Path, notify: Notify) -> ThumbService {
        let dir = app_dir.join("thumbs");
        let db_path = app_dir.join("index.db");
        let queue_cell: Arc<OnceLock<Queue>> = Arc::default();
        let conn: Arc<Mutex<Option<Connection>>> = Arc::default();
        let (run_dir, run_queue) = (dir.clone(), queue_cell.clone());
        let queue = Queue::new(
            default_threads(),
            Arc::new(move |batch: Vec<Job>| {
                let mut done = 0;
                generate(&run_dir, &batch, &mut |job, outcome| {
                    done += 1;
                    let stored = {
                        let mut slot = conn.lock().unwrap_or_else(|e| e.into_inner());
                        if slot.is_none() {
                            *slot = db::open(&db_path).ok();
                        }
                        slot.as_ref().is_some_and(|c| thumbstore::record(c, job, &outcome).unwrap_or(false))
                    };
                    // a job from before an index run whose entry id now names another file says
                    // nothing about that file; an unavailable file is still worth a "no picture"
                    if !stored && !matches!(outcome, Err(Failure::Unavailable(_))) {
                        return;
                    }
                    // the whole batch counts as in flight until it returns
                    let pending = run_queue.get().map_or(0, |q| q.pending()).saturating_sub(done);
                    let extras = outcome.as_ref().map_or(Vec::new(), |r| extra_urls(&run_dir, r));
                    notify(ThumbReady { entry_id: job.entry_id, url: outcome.ok().map(|r| thumb_url(&r)), extras, pending });
                });
            }),
        );
        let _ = queue_cell.set(queue.clone());
        ThumbService { dir, queue }
    }

    /// Answer what the cache already has (and what failed before); queue the rest, `high`
    /// first, replacing whatever was queued for an earlier request.
    pub fn request(&self, conn: &Connection, high: &[i64], low: &[i64]) -> Result<Vec<ThumbReady>, String> {
        let ids: Vec<i64> = high.iter().chain(low).copied().collect();
        let l = thumbstore::lookup(conn, &self.dir, &ids).map_err(|e| e.to_string())?;
        let hot: HashSet<i64> = high.iter().copied().collect();
        let (h, lo): (Vec<Job>, Vec<Job>) = l.jobs.into_iter().partition(|j| hot.contains(&j.entry_id));
        self.queue.replace(h, lo);
        let pending = self.queue.pending();
        let ready = l.ready.into_iter().map(|(entry_id, rel)| ThumbReady {
            entry_id,
            url: Some(thumb_url(&rel)),
            extras: extra_urls(&self.dir, &rel),
            pending,
        });
        let failed = l.failed.into_iter().map(|entry_id| ThumbReady { entry_id, url: None, extras: Vec::new(), pending });
        Ok(ready.chain(failed).collect())
    }

    /// Drop everything queued: after an index run the queued entry ids may name other files.
    pub fn clear(&self) {
        self.queue.replace(Vec::new(), Vec::new());
    }

    pub fn pending(&self) -> usize {
        self.queue.pending()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// The bytes of the cache file at a request path (`/<rel>`), if it is a valid cache name.
pub fn serve(thumbs_dir: &Path, path: &str) -> Option<Vec<u8>> {
    let rel = path.strip_prefix('/').unwrap_or(path);
    if !is_valid_rel(rel) {
        return None;
    }
    std::fs::read(thumbs_dir.join(rel)).ok()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{Receiver, channel};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;
    use crate::state::AppState;
    use crate::testutil::library;

    fn entry(state: &AppState, path: &str) -> i64 {
        let conn = state.db.lock().unwrap();
        conn.query_row("SELECT id FROM entries WHERE path = ?", [path], |r| r.get(0)).unwrap()
    }

    fn service(state: &AppState) -> (ThumbService, Receiver<ThumbReady>) {
        let (tx, rx) = channel();
        let tx = Mutex::new(tx);
        let s = ThumbService::new(&state.app_dir, Arc::new(move |r| {
            let _ = tx.lock().unwrap().send(r);
        }));
        (s, rx)
    }

    fn request(s: &ThumbService, state: &AppState, high: &[i64]) -> Vec<ThumbReady> {
        s.request(&state.db.lock().unwrap(), high, &[]).unwrap()
    }

    #[test]
    fn request_generates_then_serves_from_cache() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let (s, rx) = service(&state);
        let id = entry(&state, "Orc/orc.jpg");
        assert!(request(&s, &state, &[id]).is_empty());
        let r = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(r.entry_id, id);
        let url = r.url.expect("a thumbnail");
        assert_eq!(r.pending, 0);
        let again = request(&s, &state, &[id]);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].url.as_deref(), Some(url.as_str()));
        let rel = url.rsplit_once(".localhost/").or_else(|| url.rsplit_once("localhost/")).unwrap().1;
        assert!(state.app_dir.join("thumbs").join(rel).is_file());
    }

    #[test]
    fn failed_thumb_is_reported_not_retried() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let (s, rx) = service(&state);
        let id = entry(&state, "Orc/orc.stl"); // holds "o", not a mesh
        assert!(request(&s, &state, &[id]).is_empty());
        let r = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!((r.entry_id, r.url), (id, None));
        let again = request(&s, &state, &[id]);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].url, None);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err()); // not queued again
    }

    #[test]
    fn thumb_url_shape() {
        if cfg!(windows) {
            assert_eq!(thumb_url("a/b.jpg"), "http://thumb.localhost/a/b.jpg");
        } else {
            assert_eq!(thumb_url("a/b.jpg"), "thumb://localhost/a/b.jpg");
        }
    }

    #[test]
    fn serve_refuses_bad_paths() {
        let dir = tempfile::tempdir().unwrap();
        let rel = "0123456789abcdef/1a2b3c4d-5.jpg";
        std::fs::create_dir_all(dir.path().join("0123456789abcdef")).unwrap();
        std::fs::write(dir.path().join(rel), b"JPEGdata").unwrap();
        std::fs::write(dir.path().join("secret.db"), b"x").unwrap();
        assert_eq!(serve(dir.path(), &format!("/{rel}")), Some(b"JPEGdata".to_vec()));
        for bad in ["/secret.db", "/../secret.db", "/0123456789abcdef/../secret.db", "/0123456789abcdef/missing.jpg", "/"] {
            assert_eq!(serve(dir.path(), bad), None, "{bad}");
        }
    }
}
