use std::error::Error;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use nagi_tui::{CancelToken, Task};

/// An error while controlling a deterministic manual effect task
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManualTaskError {
    /// A result was already supplied to the task
    AlreadyCompleted,
}

impl fmt::Display for ManualTaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyCompleted => formatter.write_str("manual task already completed"),
        }
    }
}

impl Error for ManualTaskError {}

struct ManualState<Message> {
    inner: Mutex<ManualInner<Message>>,
    changed: Condvar,
}

struct ManualInner<Message> {
    token: Option<CancelToken>,
    result: Option<Message>,
    completed: bool,
}

/// A handle that observes and completes one deterministic effect task
pub struct ManualTask<Message> {
    state: Arc<ManualState<Message>>,
}

impl<Message> Clone for ManualTask<Message> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<Message> ManualTask<Message> {
    /// Blocks until the runtime starts the task
    pub fn wait_started(&self) {
        let mut inner = self.lock();
        while inner.token.is_none() {
            inner = self
                .state
                .changed
                .wait(inner)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// Reports whether the runtime started the task
    #[must_use]
    pub fn is_started(&self) -> bool {
        self.lock().token.is_some()
    }

    /// Reports whether the runtime requested cooperative cancellation
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.lock()
            .token
            .as_ref()
            .is_some_and(CancelToken::is_cancelled)
    }

    /// Supplies the task result and releases its worker
    pub fn complete(&self, message: Message) -> Result<(), ManualTaskError> {
        let mut inner = self.lock();
        if inner.completed {
            return Err(ManualTaskError::AlreadyCompleted);
        }
        inner.completed = true;
        inner.result = Some(message);
        self.state.changed.notify_all();
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, ManualInner<Message>> {
        self.state
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Creates a task and handle whose result is supplied manually
///
/// The task intentionally remains alive after cancellation until
/// [`ManualTask::complete`] is called, allowing stale-result tests
#[must_use]
pub fn manual_task<Message: Send + 'static>() -> (Task<Message>, ManualTask<Message>) {
    let state = Arc::new(ManualState {
        inner: Mutex::new(ManualInner {
            token: None,
            result: None,
            completed: false,
        }),
        changed: Condvar::new(),
    });
    let handle = ManualTask {
        state: Arc::clone(&state),
    };
    let task = Box::new(move |token| {
        let mut inner = state
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.token = Some(token);
        state.changed.notify_all();
        while !inner.completed {
            inner = state
                .changed
                .wait(inner)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        inner
            .result
            .take()
            .expect("a completed manual task has one result")
    });
    (task, handle)
}
