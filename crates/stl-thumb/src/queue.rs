//! The thumbnail job queue: two priority levels, batches grouped by archive, and `replace`
//! semantics so a folder the user left doesn't keep the workers busy.
use std::collections::{HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::generate::Job;

/// Jobs per batch, at most.
const BATCH: usize = 64;

/// Runs one batch (jobs sharing a `Job::group`).
pub type Runner = Arc<dyn Fn(Vec<Job>) + Send + Sync>;

#[derive(Default)]
struct State {
    high: VecDeque<Job>,
    low: VecDeque<Job>,
    in_flight: HashSet<i64>,
}

struct Shared {
    state: Mutex<State>,
    work: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Clone)]
pub struct Queue {
    shared: Arc<Shared>,
}

/// One thread per core but one, at least one.
pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get()).saturating_sub(1).max(1)
}

impl Queue {
    /// Start `threads` workers that hand each batch to `run`.
    pub fn new(threads: usize, run: Runner) -> Queue {
        let shared = Arc::new(Shared { state: Mutex::default(), work: Condvar::new() });
        for i in 0..threads.max(1) {
            let (shared, run) = (shared.clone(), run.clone());
            std::thread::Builder::new()
                .name(format!("thumb-{i}"))
                .spawn(move || worker(&shared, &run))
                .expect("spawn a thumbnail worker");
        }
        Queue { shared }
    }

    /// Replace everything still queued; batches already running finish. An id in both lists is
    /// high; duplicates and ids in flight are dropped.
    pub fn replace(&self, high: Vec<Job>, low: Vec<Job>) {
        let mut st = self.shared.lock();
        let mut seen: HashSet<i64> = st.in_flight.clone();
        st.high = high.into_iter().filter(|j| seen.insert(j.entry_id)).collect();
        st.low = low.into_iter().filter(|j| seen.insert(j.entry_id)).collect();
        drop(st);
        self.shared.work.notify_all();
    }

    /// Jobs queued or running.
    pub fn pending(&self) -> usize {
        let st = self.shared.lock();
        st.high.len() + st.low.len() + st.in_flight.len()
    }
}

fn worker(shared: &Shared, run: &Runner) {
    loop {
        let batch = {
            let mut st = shared.lock();
            while st.high.is_empty() && st.low.is_empty() {
                st = shared.work.wait(st).unwrap_or_else(|e| e.into_inner());
            }
            let st = &mut *st;
            let first = st.high.pop_front().or_else(|| st.low.pop_front()).expect("a job is queued");
            let group = first.group();
            let mut batch = vec![first];
            for q in [&mut st.high, &mut st.low] {
                let mut rest = VecDeque::with_capacity(q.len());
                for j in q.drain(..) {
                    if batch.len() < BATCH && j.group() == group {
                        batch.push(j);
                    } else {
                        rest.push_back(j);
                    }
                }
                *q = rest;
            }
            st.in_flight.extend(batch.iter().map(|j| j.entry_id));
            batch
        };
        let ids: Vec<i64> = batch.iter().map(|j| j.entry_id).collect();
        // a panic in one batch must not take the worker down
        let _ = catch_unwind(AssertUnwindSafe(|| run(batch)));
        let mut st = shared.lock();
        for id in ids {
            st.in_flight.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::generate::{Job, SourceKind};

    /// A job in group `g` (all jobs of a group share one archive).
    fn job(id: i64, g: char) -> Job {
        Job {
            entry_id: id,
            source_kind: SourceKind::Archive,
            source_path: format!("D:\\{g}.zip"),
            chain: vec![format!("{id}.png")],
            size: 1,
            crc: String::new(),
        }
    }

    fn jobs(ids: &[(i64, char)]) -> Vec<Job> {
        ids.iter().map(|&(i, g)| job(i, g)).collect()
    }

    /// One worker; a batch holding id 0 blocks until the gate opens. Every batch's ids are
    /// sent to the returned receiver.
    fn gated() -> (Queue, Receiver<Vec<i64>>, Sender<()>) {
        let (tx, rx) = channel::<Vec<i64>>();
        let (gate_tx, gate_rx) = channel::<()>();
        let (tx, gate_rx) = (Mutex::new(tx), Mutex::new(gate_rx));
        let q = Queue::new(
            1,
            Arc::new(move |batch: Vec<Job>| {
                let ids: Vec<i64> = batch.iter().map(|j| j.entry_id).collect();
                let blocked = ids.contains(&0);
                tx.lock().unwrap().send(ids).unwrap();
                if blocked {
                    gate_rx.lock().unwrap().recv().unwrap();
                }
            }),
        );
        (q, rx, gate_tx)
    }

    fn next(rx: &Receiver<Vec<i64>>) -> Vec<i64> {
        rx.recv_timeout(Duration::from_secs(5)).expect("a batch")
    }

    fn settle(q: &Queue) {
        let t = Instant::now();
        while q.pending() > 0 {
            assert!(t.elapsed() < Duration::from_secs(5), "queue never drained");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn high_before_low_and_grouped() {
        let (q, rx, gate) = gated();
        q.replace(jobs(&[(0, 'z')]), vec![]);
        assert_eq!(next(&rx), [0]);
        q.replace(jobs(&[(3, 'b'), (4, 'c')]), jobs(&[(1, 'a'), (2, 'b')]));
        assert_eq!(q.pending(), 5); // 4 queued + 1 in flight
        gate.send(()).unwrap();
        assert_eq!(next(&rx), [3, 2]);
        assert_eq!(next(&rx), [4]);
        assert_eq!(next(&rx), [1]);
        settle(&q);
    }

    #[test]
    fn replace_drops_stale_jobs() {
        let (q, rx, gate) = gated();
        q.replace(jobs(&[(0, 'a')]), vec![]);
        assert_eq!(next(&rx), [0]);
        q.replace((2..=50).map(|i| job(i, 'b')).collect(), vec![]);
        q.replace(jobs(&[(60, 'c')]), vec![]);
        gate.send(()).unwrap();
        assert_eq!(next(&rx), [60]);
        settle(&q);
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
    }

    #[test]
    fn duplicates_and_in_flight_ids_are_dropped() {
        let (q, rx, gate) = gated();
        q.replace(jobs(&[(0, 'a')]), vec![]);
        assert_eq!(next(&rx), [0]);
        q.replace(jobs(&[(0, 'a'), (5, 'b'), (5, 'b')]), jobs(&[(5, 'b'), (6, 'c')]));
        assert_eq!(q.pending(), 3); // 0 in flight, 5 and 6 queued
        gate.send(()).unwrap();
        assert_eq!(next(&rx), [5]);
        assert_eq!(next(&rx), [6]);
        settle(&q);
    }

    #[test]
    fn batches_are_capped() {
        let (q, rx, gate) = gated();
        q.replace(jobs(&[(0, 'a')]), vec![]);
        assert_eq!(next(&rx), [0]);
        q.replace((1..=100).map(|i| job(i, 'b')).collect(), vec![]);
        gate.send(()).unwrap();
        assert_eq!(next(&rx).len(), 64);
        assert_eq!(next(&rx).len(), 36);
        settle(&q);
    }

    #[test]
    fn panicking_batch_keeps_the_worker() {
        let (tx, rx) = channel::<i64>();
        let tx = Mutex::new(tx);
        let q = Queue::new(
            1,
            Arc::new(move |batch: Vec<Job>| {
                if batch[0].entry_id == 1 {
                    panic!("boom");
                }
                tx.lock().unwrap().send(batch[0].entry_id).unwrap();
            }),
        );
        q.replace(jobs(&[(1, 'a')]), vec![]);
        settle(&q);
        q.replace(jobs(&[(2, 'b')]), vec![]);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
        settle(&q);
    }

    #[test]
    fn default_threads_leaves_a_core() {
        let n = std::thread::available_parallelism().map_or(1, |n| n.get());
        assert_eq!(default_threads(), (n - 1).max(1));
    }
}
