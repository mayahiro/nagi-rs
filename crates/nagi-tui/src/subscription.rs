use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use crate::CancelToken;
use crate::wake::WakeHandle;

/// A stable identity for one long-lived subscription source
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SubscriptionKey(String);

impl SubscriptionKey {
    /// Creates a subscription key from an application-defined stable value
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

impl fmt::Display for SubscriptionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<&str> for SubscriptionKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for SubscriptionKey {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Bounded delivery behavior for one subscription source
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeliveryPolicy {
    pub(crate) kind: DeliveryKind,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum DeliveryKind {
    Reliable,
    Latest,
    Batch {
        maximum_messages: usize,
        maximum_delay: Duration,
    },
}

impl DeliveryPolicy {
    /// Preserves every value in FIFO order and blocks a full stream inbox
    #[must_use]
    pub const fn reliable() -> Self {
        Self {
            kind: DeliveryKind::Reliable,
        }
    }

    /// Retains only the newest value that has not entered application update
    #[must_use]
    pub const fn latest() -> Self {
        Self {
            kind: DeliveryKind::Latest,
        }
    }

    /// Releases FIFO values after `maximum_messages` or `maximum_delay`
    ///
    /// # Panics
    ///
    /// Panics when `maximum_messages` is zero
    #[must_use]
    pub fn batch(maximum_messages: usize, maximum_delay: Duration) -> Self {
        assert!(
            maximum_messages > 0,
            "batch delivery maximum must be positive"
        );
        Self {
            kind: DeliveryKind::Batch {
                maximum_messages,
                maximum_delay,
            },
        }
    }

    /// Reports whether every value is delivered reliably
    #[must_use]
    pub const fn is_reliable(self) -> bool {
        matches!(self.kind, DeliveryKind::Reliable)
    }

    /// Reports whether only the newest pending value is retained
    #[must_use]
    pub const fn is_latest(self) -> bool {
        matches!(self.kind, DeliveryKind::Latest)
    }

    /// Returns the message count and delay for batch delivery
    #[must_use]
    pub const fn batch_limits(self) -> Option<(usize, Duration)> {
        match self.kind {
            DeliveryKind::Batch {
                maximum_messages,
                maximum_delay,
            } => Some((maximum_messages, maximum_delay)),
            DeliveryKind::Reliable | DeliveryKind::Latest => None,
        }
    }
}

impl Default for DeliveryPolicy {
    fn default() -> Self {
        Self::reliable()
    }
}

/// A value returned because its subscription sink was closed
pub struct SubscriptionClosed<Message> {
    message: Message,
}

impl<Message> SubscriptionClosed<Message> {
    /// Returns the value that could not be delivered
    #[must_use]
    pub fn into_inner(self) -> Message {
        self.message
    }
}

impl<Message> fmt::Debug for SubscriptionClosed<Message> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionClosed(..)")
    }
}

impl<Message> fmt::Display for SubscriptionClosed<Message> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("subscription sink is closed")
    }
}

impl<Message> Error for SubscriptionClosed<Message> {}

/// A cancellation-aware sender owned by one Stream subscription producer
pub struct SubscriptionSink<Message> {
    pub(crate) inbox: Arc<SubscriptionInbox<Message>>,
}

impl<Message> Clone for SubscriptionSink<Message> {
    fn clone(&self) -> Self {
        Self {
            inbox: Arc::clone(&self.inbox),
        }
    }
}

impl<Message> SubscriptionSink<Message> {
    /// Sends a value according to the source delivery policy
    ///
    /// Reliable and Batch delivery can block while the bounded inbox is full.
    /// Stopping the subscription wakes blocked senders and returns their value
    pub fn send(&self, message: Message) -> Result<(), SubscriptionClosed<Message>> {
        self.inbox.send(message)
    }

    /// Reports whether the runtime stopped this subscription generation
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.inbox.is_stopped()
    }

    /// Blocks until the runtime stops this subscription generation
    pub fn wait_closed(&self) {
        self.inbox.wait_stopped();
    }
}

/// A declarative set of long-lived application message sources
pub struct Subscription<Message> {
    pub(crate) kind: SubscriptionKind<Message>,
}

pub(crate) enum SubscriptionKind<Message> {
    None,
    Batch(Vec<Subscription<Message>>),
    Source(SubscriptionSource<Message>),
}

pub(crate) struct SubscriptionSource<Message> {
    pub(crate) key: SubscriptionKey,
    pub(crate) policy: DeliveryPolicy,
    pub(crate) producer: SubscriptionProducer<Message>,
}

pub(crate) enum SubscriptionProducer<Message> {
    Every {
        interval: Duration,
        factory: Arc<dyn Fn() -> Message + Send + Sync + 'static>,
    },
    Stream(Box<dyn FnOnce(CancelToken, SubscriptionSink<Message>) + Send + 'static>),
}

impl<Message> Subscription<Message> {
    /// Creates an empty subscription set
    #[must_use]
    pub const fn none() -> Self {
        Self {
            kind: SubscriptionKind::None,
        }
    }

    /// Combines subscription declarations while preserving declaration order
    #[must_use]
    pub fn batch(subscriptions: impl IntoIterator<Item = Self>) -> Self {
        Self {
            kind: SubscriptionKind::Batch(subscriptions.into_iter().collect()),
        }
    }

    /// Creates a runtime-clock interval source
    ///
    /// The first value is produced after one full interval
    ///
    /// # Panics
    ///
    /// Panics when `interval` is zero
    #[must_use]
    pub fn every(
        key: impl Into<SubscriptionKey>,
        interval: Duration,
        policy: DeliveryPolicy,
        factory: impl Fn() -> Message + Send + Sync + 'static,
    ) -> Self {
        assert!(
            !interval.is_zero(),
            "subscription interval must be positive"
        );
        Self {
            kind: SubscriptionKind::Source(SubscriptionSource {
                key: key.into(),
                policy,
                producer: SubscriptionProducer::Every {
                    interval,
                    factory: Arc::new(factory),
                },
            }),
        }
    }

    /// Creates a cooperatively cancellable long-lived stream source
    #[must_use]
    pub fn stream(
        key: impl Into<SubscriptionKey>,
        policy: DeliveryPolicy,
        producer: impl FnOnce(CancelToken, SubscriptionSink<Message>) + Send + 'static,
    ) -> Self {
        Self {
            kind: SubscriptionKind::Source(SubscriptionSource {
                key: key.into(),
                policy,
                producer: SubscriptionProducer::Stream(Box::new(producer)),
            }),
        }
    }

    /// Reports whether this declaration contains no sources
    #[must_use]
    pub fn is_none(&self) -> bool {
        match &self.kind {
            SubscriptionKind::None => true,
            SubscriptionKind::Batch(subscriptions) => {
                subscriptions.iter().all(Subscription::is_none)
            }
            SubscriptionKind::Source(_) => false,
        }
    }
}

impl<Message> Default for Subscription<Message> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Message> fmt::Debug for Subscription<Message> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            SubscriptionKind::None => formatter.write_str("Subscription::None"),
            SubscriptionKind::Batch(subscriptions) => formatter
                .debug_tuple("Subscription::Batch")
                .field(&subscriptions.len())
                .finish(),
            SubscriptionKind::Source(source) => formatter
                .debug_struct("Subscription::Source")
                .field("key", &source.key)
                .field("policy", &source.policy)
                .finish_non_exhaustive(),
        }
    }
}

pub(crate) struct BufferedSubscriptionMessage<Message> {
    pub(crate) sequence: u64,
    pub(crate) message: Message,
}

#[derive(Default)]
pub(crate) struct SubscriptionAtomicDiagnostics {
    pub(crate) blocked_sends: AtomicU64,
    pub(crate) latest_replacements: AtomicU64,
    pub(crate) discarded_messages: AtomicU64,
    pub(crate) producer_panics: AtomicU64,
}

struct SubscriptionInboxState<Message> {
    stopped: bool,
    messages: VecDeque<BufferedSubscriptionMessage<Message>>,
}

pub(crate) struct SubscriptionInbox<Message> {
    state: Mutex<SubscriptionInboxState<Message>>,
    changed: Condvar,
    capacity: usize,
    policy: DeliveryPolicy,
    sequence: Arc<AtomicU64>,
    diagnostics: Arc<SubscriptionAtomicDiagnostics>,
    wake: WakeHandle,
}

impl<Message> SubscriptionInbox<Message> {
    pub(crate) fn new(
        capacity: usize,
        policy: DeliveryPolicy,
        sequence: Arc<AtomicU64>,
        diagnostics: Arc<SubscriptionAtomicDiagnostics>,
        wake: WakeHandle,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(SubscriptionInboxState {
                stopped: false,
                messages: VecDeque::with_capacity(capacity.min(64)),
            }),
            changed: Condvar::new(),
            capacity,
            policy,
            sequence,
            diagnostics,
            wake,
        })
    }

    fn send(&self, message: Message) -> Result<(), SubscriptionClosed<Message>> {
        let mut state = self.lock();
        if state.stopped {
            return Err(SubscriptionClosed { message });
        }
        if matches!(self.policy.kind, DeliveryKind::Latest) {
            if !state.messages.is_empty() {
                self.diagnostics
                    .latest_replacements
                    .fetch_add(state.messages.len() as u64, Ordering::Relaxed);
                state.messages.clear();
            }
        } else if state.messages.len() >= self.capacity {
            self.diagnostics
                .blocked_sends
                .fetch_add(1, Ordering::Relaxed);
            while !state.stopped && state.messages.len() >= self.capacity {
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if state.stopped {
                return Err(SubscriptionClosed { message });
            }
        }
        state.messages.push_back(BufferedSubscriptionMessage {
            sequence: next_sequence(&self.sequence),
            message,
        });
        self.changed.notify_all();
        drop(state);
        self.wake.notify();
        Ok(())
    }

    pub(crate) fn push_runtime(&self, factory: &dyn Fn() -> Message) -> bool {
        let mut state = self.lock();
        if state.stopped {
            return false;
        }
        if matches!(self.policy.kind, DeliveryKind::Latest) {
            if !state.messages.is_empty() {
                self.diagnostics
                    .latest_replacements
                    .fetch_add(state.messages.len() as u64, Ordering::Relaxed);
                state.messages.clear();
            }
        } else if state.messages.len() >= self.capacity {
            return false;
        }
        let Ok(message) = catch_unwind(AssertUnwindSafe(factory)) else {
            self.diagnostics
                .producer_panics
                .fetch_add(1, Ordering::Relaxed);
            return true;
        };
        state.messages.push_back(BufferedSubscriptionMessage {
            sequence: next_sequence(&self.sequence),
            message,
        });
        true
    }

    pub(crate) fn front_sequence(&self) -> Option<u64> {
        self.lock().messages.front().map(|message| message.sequence)
    }

    pub(crate) fn pop_if_sequence(
        &self,
        sequence: u64,
    ) -> Option<BufferedSubscriptionMessage<Message>> {
        let mut state = self.lock();
        if state.messages.front().map(|message| message.sequence) != Some(sequence) {
            return None;
        }
        let message = state.messages.pop_front();
        self.changed.notify_all();
        message
    }

    pub(crate) fn len(&self) -> usize {
        self.lock().messages.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lock().messages.is_empty()
    }

    pub(crate) fn stop(&self) -> usize {
        let mut state = self.lock();
        if state.stopped {
            return 0;
        }
        state.stopped = true;
        let discarded = state.messages.len();
        state.messages.clear();
        self.diagnostics
            .discarded_messages
            .fetch_add(discarded as u64, Ordering::Relaxed);
        self.changed.notify_all();
        discarded
    }

    fn is_stopped(&self) -> bool {
        self.lock().stopped
    }

    fn wait_stopped(&self) {
        let mut state = self.lock();
        while !state.stopped {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn lock(&self) -> MutexGuard<'_, SubscriptionInboxState<Message>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn next_sequence(sequence: &AtomicU64) -> u64 {
    sequence
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            Some(current.saturating_add(1))
        })
        .unwrap_or(u64::MAX)
        .saturating_add(1)
}
