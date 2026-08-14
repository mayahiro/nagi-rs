use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};

use crate::{SubscriptionKey, TaskKey};

/// An asynchronous lifecycle event that does not produce an application message
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeNoticeKind {
    /// An Effect task panicked and its execution boundary recovered it
    EffectPanicked,
    /// The operating system refused to start an Effect worker
    EffectSpawnFailed,
    /// A Stream returned while its subscription generation was still active
    SubscriptionStreamCompleted,
    /// A Stream panicked and the worker boundary recovered it
    SubscriptionStreamPanicked,
    /// The operating system refused to start a Stream worker
    SubscriptionSpawnFailed,
}

/// One recovered failure or unexpected asynchronous lifecycle transition
///
/// Panic payloads are intentionally omitted. A keyed Effect includes its
/// [`TaskKey`] and generation; a Stream notice includes its
/// [`SubscriptionKey`] and generation
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeNotice {
    kind: RuntimeNoticeKind,
    task: Option<(TaskKey, u64)>,
    subscription: Option<(SubscriptionKey, u64)>,
}

impl RuntimeNotice {
    /// Returns the lifecycle event kind
    #[must_use]
    pub const fn kind(&self) -> RuntimeNoticeKind {
        self.kind
    }

    /// Returns keyed Effect identity when the notice belongs to a Latest Effect
    #[must_use]
    pub fn task(&self) -> Option<(&TaskKey, u64)> {
        self.task
            .as_ref()
            .map(|(key, generation)| (key, *generation))
    }

    /// Returns Stream identity when the notice belongs to a Subscription
    #[must_use]
    pub fn subscription(&self) -> Option<(&SubscriptionKey, u64)> {
        self.subscription
            .as_ref()
            .map(|(key, generation)| (key, *generation))
    }

    pub(crate) fn effect(kind: RuntimeNoticeKind, task: Option<&(TaskKey, u64)>) -> Self {
        Self {
            kind,
            task: task.cloned(),
            subscription: None,
        }
    }

    pub(crate) fn subscription_stream(
        kind: RuntimeNoticeKind,
        key: SubscriptionKey,
        generation: u64,
    ) -> Self {
        Self {
            kind,
            task: None,
            subscription: Some((key, generation)),
        }
    }
}

/// Counters for the bounded Runtime notice queue
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeNoticeDiagnostics {
    dropped: u64,
}

impl RuntimeNoticeDiagnostics {
    /// Returns notices discarded because the bounded queue was full
    #[must_use]
    pub const fn dropped(self) -> u64 {
        self.dropped
    }
}

#[derive(Default)]
struct RuntimeNoticeQueueState {
    items: VecDeque<RuntimeNotice>,
    dropped: u64,
}

pub(crate) struct RuntimeNoticeQueue {
    capacity: usize,
    state: Mutex<RuntimeNoticeQueueState>,
}

impl RuntimeNoticeQueue {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            state: Mutex::new(RuntimeNoticeQueueState {
                items: VecDeque::with_capacity(capacity.min(16)),
                dropped: 0,
            }),
        }
    }

    pub(crate) fn push(&self, notice: RuntimeNotice) {
        let mut state = self.state();
        if state.items.len() >= self.capacity {
            state.dropped = state.dropped.saturating_add(1);
            return;
        }
        state.items.push_back(notice);
    }

    pub(crate) fn drain(&self) -> Vec<RuntimeNotice> {
        self.state().items.drain(..).collect()
    }

    pub(crate) fn pending(&self) -> usize {
        self.state().items.len()
    }

    pub(crate) fn diagnostics(&self) -> RuntimeNoticeDiagnostics {
        RuntimeNoticeDiagnostics {
            dropped: self.state().dropped,
        }
    }

    fn state(&self) -> MutexGuard<'_, RuntimeNoticeQueueState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::{RuntimeNotice, RuntimeNoticeKind, RuntimeNoticeQueue};

    #[test]
    fn bounded_queue_preserves_oldest_notice() {
        let queue = RuntimeNoticeQueue::new(1);
        queue.push(RuntimeNotice::effect(
            RuntimeNoticeKind::EffectPanicked,
            None,
        ));
        queue.push(RuntimeNotice::effect(
            RuntimeNoticeKind::EffectSpawnFailed,
            None,
        ));

        let notices = queue.drain();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].kind(), RuntimeNoticeKind::EffectPanicked);
        assert_eq!(queue.diagnostics().dropped(), 1);
    }
}
