use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::{NodeId, ScrollOffset};

/// A stable key for replacement and cancellation of one latest task
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskKey(String);

impl TaskKey {
    /// Creates a task key from an application-defined stable value
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the application-defined key
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<&str> for TaskKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for TaskKey {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// A stable key grouping tasks for explicit cooperative cancellation
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ScopeId(String);

impl ScopeId {
    /// Creates a scope ID from an application-defined stable value
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the application-defined key
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ScopeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<&str> for ScopeId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ScopeId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Cooperative cancellation state passed to an effect task
#[derive(Clone, Debug, Default)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    /// Reports whether the runtime requested cancellation
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// One standard-thread task producing an application message
pub type Task<Message> = Box<dyn FnOnce(CancelToken) -> Message + Send + 'static>;

/// Declarative follow-up work produced by application initialization or update
pub struct Effect<Message> {
    pub(crate) kind: EffectKind<Message>,
}

pub(crate) enum EffectKind<Message> {
    None,
    Exit,
    Focus(NodeId),
    ScrollTo {
        id: NodeId,
        offset: ScrollOffset,
    },
    Run(Task<Message>),
    Latest {
        key: TaskKey,
        task: Task<Message>,
    },
    Cancel(TaskKey),
    Scoped {
        scope: ScopeId,
        effect: Box<Effect<Message>>,
    },
    CancelScope(ScopeId),
    After {
        delay: Duration,
        message: Message,
    },
    Batch(Vec<Effect<Message>>),
    Sequence(Vec<Effect<Message>>),
}

pub(crate) enum RuntimeCommand {
    Exit,
    Focus(NodeId),
    ScrollTo { id: NodeId, offset: ScrollOffset },
}

impl<Message> Effect<Message> {
    /// Creates an effect that performs no work
    #[must_use]
    pub const fn none() -> Self {
        Self {
            kind: EffectKind::None,
        }
    }

    /// Requests normal application exit after the final dirty frame is rendered
    #[must_use]
    pub const fn exit() -> Self {
        Self {
            kind: EffectKind::Exit,
        }
    }

    /// Requests focus for a focusable node in the next application view
    #[must_use]
    pub fn focus(id: impl Into<NodeId>) -> Self {
        Self {
            kind: EffectKind::Focus(id.into()),
        }
    }

    /// Requests a ScrollViewport offset in the next application view
    #[must_use]
    pub fn scroll_to(id: impl Into<NodeId>, offset: ScrollOffset) -> Self {
        Self {
            kind: EffectKind::ScrollTo {
                id: id.into(),
                offset,
            },
        }
    }

    /// Runs one task on a supervised standard thread
    #[must_use]
    pub fn run(task: impl FnOnce(CancelToken) -> Message + Send + 'static) -> Self {
        Self {
            kind: EffectKind::Run(Box::new(task)),
        }
    }

    /// Replaces the current task for `key` and suppresses its stale result
    #[must_use]
    pub fn latest(
        key: impl Into<TaskKey>,
        task: impl FnOnce(CancelToken) -> Message + Send + 'static,
    ) -> Self {
        Self {
            kind: EffectKind::Latest {
                key: key.into(),
                task: Box::new(task),
            },
        }
    }

    /// Cancels the current latest task for `key`
    #[must_use]
    pub fn cancel(key: impl Into<TaskKey>) -> Self {
        Self {
            kind: EffectKind::Cancel(key.into()),
        }
    }

    /// Associates every task and timer in `effect` with a cancellation scope
    #[must_use]
    pub fn scoped(scope: impl Into<ScopeId>, effect: Self) -> Self {
        Self {
            kind: EffectKind::Scoped {
                scope: scope.into(),
                effect: Box::new(effect),
            },
        }
    }

    /// Cancels current tasks and timers in `scope`
    #[must_use]
    pub fn cancel_scope(scope: impl Into<ScopeId>) -> Self {
        Self {
            kind: EffectKind::CancelScope(scope.into()),
        }
    }

    /// Emits `message` after `delay` according to the runtime clock
    #[must_use]
    pub fn after(delay: Duration, message: Message) -> Self {
        Self {
            kind: EffectKind::After { delay, message },
        }
    }

    /// Starts child effects independently and completes when all finish
    #[must_use]
    pub fn batch(effects: impl IntoIterator<Item = Self>) -> Self {
        Self {
            kind: EffectKind::Batch(effects.into_iter().collect()),
        }
    }

    /// Runs child effects in order, starting each after the previous completes
    #[must_use]
    pub fn sequence(effects: impl IntoIterator<Item = Self>) -> Self {
        Self {
            kind: EffectKind::Sequence(effects.into_iter().collect()),
        }
    }

    /// Reports whether this effect performs no work
    #[must_use]
    pub const fn is_none(&self) -> bool {
        matches!(self.kind, EffectKind::None)
    }
}

impl<Message> Default for Effect<Message> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Message> fmt::Debug for Effect<Message> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match &self.kind {
            EffectKind::None => "None",
            EffectKind::Exit => "Exit",
            EffectKind::Focus(_) => "Focus",
            EffectKind::ScrollTo { .. } => "ScrollTo",
            EffectKind::Run(_) => "Run",
            EffectKind::Latest { .. } => "Latest",
            EffectKind::Cancel(_) => "Cancel",
            EffectKind::Scoped { .. } => "Scoped",
            EffectKind::CancelScope(_) => "CancelScope",
            EffectKind::After { .. } => "After",
            EffectKind::Batch(_) => "Batch",
            EffectKind::Sequence(_) => "Sequence",
        };
        formatter.debug_tuple("Effect").field(&name).finish()
    }
}
