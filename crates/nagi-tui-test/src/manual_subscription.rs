use std::error::Error;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use nagi_tui::{DeliveryPolicy, Subscription, SubscriptionKey, SubscriptionSink};

/// The reason a manual subscription value was not sent
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManualSubscriptionSendErrorKind {
    /// No subscription generation has started
    NotStarted,
    /// The current subscription generation stopped before accepting the value
    Closed,
}

/// A value rejected by a manual subscription source
pub struct ManualSubscriptionSendError<Message> {
    kind: ManualSubscriptionSendErrorKind,
    message: Message,
}

impl<Message> ManualSubscriptionSendError<Message> {
    /// Returns why the value was rejected
    #[must_use]
    pub const fn kind(&self) -> ManualSubscriptionSendErrorKind {
        self.kind
    }

    /// Returns the rejected value
    #[must_use]
    pub fn into_inner(self) -> Message {
        self.message
    }
}

impl<Message> fmt::Debug for ManualSubscriptionSendError<Message> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManualSubscriptionSendError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl<Message> fmt::Display for ManualSubscriptionSendError<Message> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ManualSubscriptionSendErrorKind::NotStarted => {
                formatter.write_str("manual subscription has not started")
            }
            ManualSubscriptionSendErrorKind::Closed => {
                formatter.write_str("manual subscription is closed")
            }
        }
    }
}

impl<Message> Error for ManualSubscriptionSendError<Message> {}

struct ManualSubscriptionState<Message> {
    inner: Mutex<ManualSubscriptionInner<Message>>,
    changed: Condvar,
}

struct ManualSubscriptionInner<Message> {
    next_source_id: u64,
    active_source_id: Option<u64>,
    sink: Option<SubscriptionSink<Message>>,
    starts: u64,
    stops: u64,
}

/// A deterministic handle that declares, observes, and feeds a Stream source
pub struct ManualSubscription<Message> {
    state: Arc<ManualSubscriptionState<Message>>,
}

impl<Message> Clone for ManualSubscription<Message> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<Message: Send + 'static> ManualSubscription<Message> {
    /// Creates a Stream declaration backed by this handle
    #[must_use]
    pub fn subscription(
        &self,
        key: impl Into<SubscriptionKey>,
        policy: DeliveryPolicy,
    ) -> Subscription<Message> {
        let source_id = {
            let mut inner = self.lock();
            inner.next_source_id = inner.next_source_id.saturating_add(1);
            inner.next_source_id
        };
        let state = Arc::clone(&self.state);
        Subscription::stream(key, policy, move |_token, sink| {
            {
                let mut inner = state
                    .inner
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                inner.active_source_id = Some(source_id);
                inner.sink = Some(sink.clone());
                inner.starts = inner.starts.saturating_add(1);
                state.changed.notify_all();
            }
            sink.wait_closed();
            let mut inner = state
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if inner.active_source_id == Some(source_id) {
                inner.active_source_id = None;
                inner.sink = None;
            }
            inner.stops = inner.stops.saturating_add(1);
            state.changed.notify_all();
        })
    }

    /// Sends one value through the current subscription generation
    pub fn send(&self, message: Message) -> Result<(), ManualSubscriptionSendError<Message>> {
        let Some(sink) = self.lock().sink.clone() else {
            return Err(ManualSubscriptionSendError {
                kind: ManualSubscriptionSendErrorKind::NotStarted,
                message,
            });
        };
        sink.send(message)
            .map_err(|error| ManualSubscriptionSendError {
                kind: ManualSubscriptionSendErrorKind::Closed,
                message: error.into_inner(),
            })
    }

    /// Blocks until at least one source generation starts
    pub fn wait_started(&self) {
        let mut inner = self.lock();
        while inner.starts == 0 {
            inner = self
                .state
                .changed
                .wait(inner)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// Blocks until at least one source generation stops
    pub fn wait_stopped(&self) {
        let mut inner = self.lock();
        while inner.stops == 0 {
            inner = self
                .state
                .changed
                .wait(inner)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// Reports whether a source generation is currently active
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.lock().active_source_id.is_some()
    }

    /// Returns the number of observed source starts
    #[must_use]
    pub fn starts(&self) -> u64 {
        self.lock().starts
    }

    /// Returns the number of observed source stops
    #[must_use]
    pub fn stops(&self) -> u64 {
        self.lock().stops
    }

    fn lock(&self) -> MutexGuard<'_, ManualSubscriptionInner<Message>> {
        self.state
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Creates a deterministic manual Stream subscription handle
#[must_use]
pub fn manual_subscription<Message>() -> ManualSubscription<Message> {
    ManualSubscription {
        state: Arc::new(ManualSubscriptionState {
            inner: Mutex::new(ManualSubscriptionInner {
                next_source_id: 0,
                active_source_id: None,
                sink: None,
                starts: 0,
                stops: 0,
            }),
            changed: Condvar::new(),
        }),
    }
}
