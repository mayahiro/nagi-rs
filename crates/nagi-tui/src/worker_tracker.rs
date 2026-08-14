use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub(crate) struct WorkerTracker {
    shared: Arc<WorkerTrackerShared>,
}

#[derive(Default)]
struct WorkerTrackerShared {
    state: Mutex<WorkerTrackerState>,
    changed: Condvar,
}

#[derive(Default)]
struct WorkerTrackerState {
    running: usize,
}

impl WorkerTracker {
    pub(crate) fn track(&self) -> WorkerGuard {
        let mut state = self.state();
        state.running = state.running.saturating_add(1);
        drop(state);
        WorkerGuard {
            tracker: self.clone(),
        }
    }

    pub(crate) fn wait(&self) {
        let mut state = self.state();
        while state.running != 0 {
            state = self
                .shared
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    pub(crate) fn wait_timeout(&self, timeout: Duration) -> bool {
        let started = Instant::now();
        let mut state = self.state();
        while state.running != 0 {
            let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
                return false;
            };
            let (next, result) = self
                .shared
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner());
            state = next;
            if result.timed_out() && state.running != 0 {
                return false;
            }
        }
        true
    }

    fn finish(&self) {
        let mut state = self.state();
        state.running = state.running.saturating_sub(1);
        if state.running == 0 {
            self.shared.changed.notify_all();
        }
    }

    fn state(&self) -> MutexGuard<'_, WorkerTrackerState> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

pub(crate) struct WorkerGuard {
    tracker: WorkerTracker,
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.tracker.finish();
    }
}
