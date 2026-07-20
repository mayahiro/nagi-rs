use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use crate::subscription::{
    BufferedSubscriptionMessage, DeliveryKind, DeliveryPolicy, SubscriptionAtomicDiagnostics,
    SubscriptionInbox, SubscriptionKind, SubscriptionProducer, SubscriptionSource,
};
use crate::{CancelToken, Subscription, SubscriptionKey, SubscriptionSink, Timestamp};

/// Counters describing subscription lifecycle, backpressure, and failures
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionDiagnostics {
    starts: u64,
    stops: u64,
    blocked_sends: u64,
    latest_replacements: u64,
    batch_flushes: u64,
    discarded_messages: u64,
    producer_panics: u64,
    spawn_failures: u64,
}

impl SubscriptionDiagnostics {
    /// Returns the number of started subscription generations
    #[must_use]
    pub const fn starts(self) -> u64 {
        self.starts
    }

    /// Returns the number of stopped subscription generations
    #[must_use]
    pub const fn stops(self) -> u64 {
        self.stops
    }

    /// Returns the number of stream sends that encountered a full inbox
    #[must_use]
    pub const fn blocked_sends(self) -> u64 {
        self.blocked_sends
    }

    /// Returns the number of pending Latest values replaced before delivery
    #[must_use]
    pub const fn latest_replacements(self) -> u64 {
        self.latest_replacements
    }

    /// Returns the number of count- or time-triggered Batch releases
    #[must_use]
    pub const fn batch_flushes(self) -> u64 {
        self.batch_flushes
    }

    /// Returns values discarded when their subscription generation stopped
    #[must_use]
    pub const fn discarded_messages(self) -> u64 {
        self.discarded_messages
    }

    /// Returns producer or Every factory panics isolated by the supervisor
    #[must_use]
    pub const fn producer_panics(self) -> u64 {
        self.producer_panics
    }

    /// Returns operating-system thread spawn failures for Stream producers
    #[must_use]
    pub const fn spawn_failures(self) -> u64 {
        self.spawn_failures
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SubscriptionTag {
    pub(crate) key: SubscriptionKey,
    pub(crate) generation: u64,
}

pub(crate) struct SubscriptionMessage<Message> {
    pub(crate) tag: SubscriptionTag,
    pub(crate) message: Message,
}

#[derive(Debug)]
pub(crate) struct SubscriptionReconciliation {
    pub(crate) stopped: Vec<SubscriptionTag>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SubscriptionFingerprint {
    Every {
        interval: Duration,
        policy: DeliveryPolicy,
    },
    Stream {
        policy: DeliveryPolicy,
    },
}

enum ActiveProducer<Message> {
    Every {
        interval: Duration,
        next_due: Option<Timestamp>,
        factory: Arc<dyn Fn() -> Message + Send + Sync + 'static>,
    },
    Stream {
        token: CancelToken,
        finished: Arc<AtomicBool>,
    },
}

struct ActiveSubscription<Message> {
    tag: SubscriptionTag,
    fingerprint: SubscriptionFingerprint,
    policy: DeliveryPolicy,
    inbox: Arc<SubscriptionInbox<Message>>,
    producer: ActiveProducer<Message>,
    batch_deadline: Option<Timestamp>,
    batch_ready: bool,
}

pub(crate) struct SubscriptionSupervisor<Message> {
    inbox_capacity: usize,
    active: HashMap<SubscriptionKey, ActiveSubscription<Message>>,
    order: Vec<SubscriptionKey>,
    generations: HashMap<SubscriptionKey, u64>,
    sequence: Arc<AtomicU64>,
    atomic_diagnostics: Arc<SubscriptionAtomicDiagnostics>,
    starts: u64,
    stops: u64,
    batch_flushes: u64,
    spawn_failures: u64,
}

impl<Message: Send + 'static> SubscriptionSupervisor<Message> {
    pub(crate) fn new(inbox_capacity: usize) -> Self {
        Self {
            inbox_capacity,
            active: HashMap::new(),
            order: Vec::new(),
            generations: HashMap::new(),
            sequence: Arc::new(AtomicU64::new(0)),
            atomic_diagnostics: Arc::new(SubscriptionAtomicDiagnostics::default()),
            starts: 0,
            stops: 0,
            batch_flushes: 0,
            spawn_failures: 0,
        }
    }

    pub(crate) fn reconcile(
        &mut self,
        subscription: Subscription<Message>,
        now: Timestamp,
    ) -> Result<SubscriptionReconciliation, SubscriptionKey> {
        let mut sources = Vec::new();
        flatten(subscription, &mut sources);
        let mut keys = HashSet::with_capacity(sources.len());
        for source in &sources {
            if !keys.insert(source.key.clone()) {
                return Err(source.key.clone());
            }
        }

        let mut stopped = Vec::new();
        let mut next_order = Vec::with_capacity(sources.len());
        for source in sources {
            let key = source.key.clone();
            let fingerprint = fingerprint(&source);
            let retained = self
                .active
                .get(&key)
                .is_some_and(|active| active.fingerprint == fingerprint);
            if !retained {
                if let Some(active) = self.active.remove(&key) {
                    stopped.push(self.stop_active(active));
                }
                let active = self.start_source(source, fingerprint, now);
                self.active.insert(key.clone(), active);
            }
            next_order.push(key);
        }

        for key in self.order.clone() {
            if !keys.contains(&key) {
                if let Some(active) = self.active.remove(&key) {
                    stopped.push(self.stop_active(active));
                }
            }
        }
        self.order = next_order;
        Ok(SubscriptionReconciliation { stopped })
    }

    pub(crate) fn poll(&mut self, now: Timestamp) {
        for key in self.order.clone() {
            let Some(active) = self.active.get_mut(&key) else {
                continue;
            };
            poll_every(active, now);
            update_batch_state(active, now, &mut self.batch_flushes);
        }
    }

    pub(crate) fn take_ready(&mut self, maximum: usize) -> Vec<SubscriptionMessage<Message>> {
        let mut ready = Vec::with_capacity(maximum.min(64));
        while ready.len() < maximum {
            let candidate = self
                .order
                .iter()
                .filter_map(|key| {
                    let active = self.active.get(key)?;
                    source_ready(active).then(|| {
                        active
                            .inbox
                            .front_sequence()
                            .map(|sequence| (sequence, key))
                    })?
                })
                .min_by_key(|(sequence, _)| *sequence)
                .map(|(sequence, key)| (sequence, key.clone()));
            let Some((sequence, key)) = candidate else {
                break;
            };
            let Some(active) = self.active.get_mut(&key) else {
                continue;
            };
            let Some(BufferedSubscriptionMessage { message, .. }) =
                active.inbox.pop_if_sequence(sequence)
            else {
                continue;
            };
            ready.push(SubscriptionMessage {
                tag: active.tag.clone(),
                message,
            });
            if active.inbox.is_empty() {
                active.batch_deadline = None;
                active.batch_ready = false;
            }
        }
        ready
    }

    pub(crate) fn active_subscriptions(&self) -> usize {
        self.active.len()
    }

    pub(crate) fn running_streams(&self) -> usize {
        self.active
            .values()
            .filter(|active| {
                matches!(
                    &active.producer,
                    ActiveProducer::Stream { finished, .. }
                        if !finished.load(Ordering::Acquire)
                )
            })
            .count()
    }

    pub(crate) fn pending_messages(&self) -> usize {
        self.active.values().map(|active| active.inbox.len()).sum()
    }

    pub(crate) fn is_active(&self, key: &SubscriptionKey) -> bool {
        self.active.contains_key(key)
    }

    pub(crate) fn generation(&self, key: &SubscriptionKey) -> u64 {
        self.generations.get(key).copied().unwrap_or(0)
    }

    pub(crate) fn diagnostics(&self) -> SubscriptionDiagnostics {
        SubscriptionDiagnostics {
            starts: self.starts,
            stops: self.stops,
            blocked_sends: self
                .atomic_diagnostics
                .blocked_sends
                .load(Ordering::Relaxed),
            latest_replacements: self
                .atomic_diagnostics
                .latest_replacements
                .load(Ordering::Relaxed),
            batch_flushes: self.batch_flushes,
            discarded_messages: self
                .atomic_diagnostics
                .discarded_messages
                .load(Ordering::Relaxed),
            producer_panics: self
                .atomic_diagnostics
                .producer_panics
                .load(Ordering::Relaxed),
            spawn_failures: self.spawn_failures,
        }
    }

    pub(crate) fn note_discarded(&self, count: usize) {
        self.atomic_diagnostics
            .discarded_messages
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    pub(crate) fn time_until_deadline(&self, now: Timestamp) -> Option<Duration> {
        self.active
            .values()
            .flat_map(|active| {
                let every = match active.producer {
                    ActiveProducer::Every { next_due, .. } => next_due,
                    ActiveProducer::Stream { .. } => None,
                };
                let batch = active
                    .batch_deadline
                    .filter(|_| !active.batch_ready && !active.inbox.is_empty());
                [every, batch]
            })
            .flatten()
            .min()
            .map(|deadline| {
                Duration::from_nanos(deadline.as_nanos().saturating_sub(now.as_nanos()))
            })
    }

    fn start_source(
        &mut self,
        source: SubscriptionSource<Message>,
        fingerprint: SubscriptionFingerprint,
        now: Timestamp,
    ) -> ActiveSubscription<Message> {
        let generation = self
            .generations
            .get(&source.key)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        self.generations.insert(source.key.clone(), generation);
        self.starts = self.starts.saturating_add(1);
        let inbox = SubscriptionInbox::new(
            self.inbox_capacity,
            source.policy,
            Arc::clone(&self.sequence),
            Arc::clone(&self.atomic_diagnostics),
        );
        let producer = match source.producer {
            SubscriptionProducer::Every { interval, factory } => ActiveProducer::Every {
                interval,
                next_due: timestamp_after(now, interval),
                factory,
            },
            SubscriptionProducer::Stream(producer) => {
                let token = CancelToken::default();
                let worker_token = token.clone();
                let sink = SubscriptionSink {
                    inbox: Arc::clone(&inbox),
                };
                let finished = Arc::new(AtomicBool::new(false));
                let worker_finished = Arc::clone(&finished);
                let diagnostics = Arc::clone(&self.atomic_diagnostics);
                let spawn = thread::Builder::new()
                    .name(format!("nagi-tui-subscription-{}", source.key))
                    .spawn(move || {
                        if catch_unwind(AssertUnwindSafe(|| producer(worker_token, sink))).is_err()
                        {
                            diagnostics.producer_panics.fetch_add(1, Ordering::Relaxed);
                        }
                        worker_finished.store(true, Ordering::Release);
                    });
                if spawn.is_err() {
                    self.spawn_failures = self.spawn_failures.saturating_add(1);
                    finished.store(true, Ordering::Release);
                }
                ActiveProducer::Stream { token, finished }
            }
        };
        ActiveSubscription {
            tag: SubscriptionTag {
                key: source.key,
                generation,
            },
            fingerprint,
            policy: source.policy,
            inbox,
            producer,
            batch_deadline: None,
            batch_ready: false,
        }
    }

    fn stop_active(&mut self, active: ActiveSubscription<Message>) -> SubscriptionTag {
        if let ActiveProducer::Stream { token, .. } = &active.producer {
            token.cancel();
        }
        active.inbox.stop();
        self.stops = self.stops.saturating_add(1);
        active.tag
    }
}

impl<Message> Drop for SubscriptionSupervisor<Message> {
    fn drop(&mut self) {
        for active in self.active.values() {
            if let ActiveProducer::Stream { token, .. } = &active.producer {
                token.cancel();
            }
            active.inbox.stop();
        }
    }
}

fn flatten<Message>(
    subscription: Subscription<Message>,
    sources: &mut Vec<SubscriptionSource<Message>>,
) {
    match subscription.kind {
        SubscriptionKind::None => {}
        SubscriptionKind::Batch(subscriptions) => {
            for subscription in subscriptions {
                flatten(subscription, sources);
            }
        }
        SubscriptionKind::Source(source) => sources.push(source),
    }
}

fn fingerprint<Message>(source: &SubscriptionSource<Message>) -> SubscriptionFingerprint {
    match &source.producer {
        SubscriptionProducer::Every { interval, .. } => SubscriptionFingerprint::Every {
            interval: *interval,
            policy: source.policy,
        },
        SubscriptionProducer::Stream(_) => SubscriptionFingerprint::Stream {
            policy: source.policy,
        },
    }
}

fn poll_every<Message>(active: &mut ActiveSubscription<Message>, now: Timestamp) {
    let ActiveProducer::Every {
        interval,
        next_due,
        factory,
    } = &mut active.producer
    else {
        return;
    };
    let Some(due) = *next_due else {
        return;
    };
    if due > now {
        return;
    }
    if matches!(active.policy.kind, DeliveryKind::Latest) {
        active.inbox.push_runtime(factory.as_ref());
        *next_due = timestamp_after_now(due, *interval, now);
        return;
    }
    while next_due.is_some_and(|due| due <= now) {
        if !active.inbox.push_runtime(factory.as_ref()) {
            break;
        }
        *next_due = next_due.and_then(|due| timestamp_after(due, *interval));
    }
}

fn update_batch_state<Message>(
    active: &mut ActiveSubscription<Message>,
    now: Timestamp,
    batch_flushes: &mut u64,
) {
    let DeliveryKind::Batch {
        maximum_messages,
        maximum_delay,
    } = active.policy.kind
    else {
        return;
    };
    if active.inbox.is_empty() {
        active.batch_deadline = None;
        active.batch_ready = false;
        return;
    }
    if active.batch_deadline.is_none() {
        active.batch_deadline =
            timestamp_after(now, maximum_delay).or(Some(Timestamp::from_nanos(u64::MAX)));
    }
    let due = active
        .batch_deadline
        .is_some_and(|deadline| deadline <= now);
    if !active.batch_ready && (active.inbox.len() >= maximum_messages || due) {
        active.batch_ready = true;
        *batch_flushes = batch_flushes.saturating_add(1);
    }
}

fn source_ready<Message>(active: &ActiveSubscription<Message>) -> bool {
    !active.inbox.is_empty()
        && (!matches!(active.policy.kind, DeliveryKind::Batch { .. }) || active.batch_ready)
}

fn timestamp_after(timestamp: Timestamp, duration: Duration) -> Option<Timestamp> {
    let delta = duration.as_nanos();
    let next = u128::from(timestamp.as_nanos()).checked_add(delta)?;
    (next <= u128::from(u64::MAX)).then(|| Timestamp::from_nanos(next as u64))
}

fn timestamp_after_now(due: Timestamp, interval: Duration, now: Timestamp) -> Option<Timestamp> {
    let interval = interval.as_nanos();
    let elapsed = u128::from(now.as_nanos().saturating_sub(due.as_nanos()));
    let steps = elapsed / interval + 1;
    let next = u128::from(due.as_nanos()).checked_add(steps.checked_mul(interval)?)?;
    (next <= u128::from(u64::MAX)).then(|| Timestamp::from_nanos(next as u64))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
    use std::sync::{Arc, Mutex};

    use crate::fixture_support;

    use super::*;

    fn stream(
        key: &str,
        policy: DeliveryPolicy,
        started: Option<Arc<Mutex<Option<SubscriptionSink<String>>>>>,
    ) -> Subscription<String> {
        Subscription::stream(key, policy, move |_token, sink| {
            if let Some(started) = started {
                *started.lock().unwrap() = Some(sink.clone());
            }
            sink.wait_closed();
        })
    }

    fn wait_sink(
        started: &Arc<Mutex<Option<SubscriptionSink<String>>>>,
    ) -> SubscriptionSink<String> {
        for _ in 0..10_000 {
            if let Some(sink) = started.lock().unwrap().clone() {
                return sink;
            }
            thread::yield_now();
        }
        panic!("subscription stream did not start")
    }

    #[test]
    fn lifecycle_matches_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "subscriptions/lifecycle.txt",
            "subscription-lifecycle",
            &["frames", "starts", "stops", "active", "generations"],
        ) else {
            return;
        };
        for record in records {
            let mut supervisor = SubscriptionSupervisor::new(8);
            let mut actual_stops = Vec::new();
            for frame in record.field("frames").split('|') {
                let subscriptions = if frame == "-" {
                    Subscription::none()
                } else {
                    Subscription::batch(frame.split(',').map(parse_declaration))
                };
                let before = supervisor.diagnostics().starts();
                let reconciliation = supervisor
                    .reconcile(subscriptions, Timestamp::default())
                    .unwrap();
                actual_stops.extend(
                    reconciliation
                        .stopped
                        .into_iter()
                        .map(|tag| format!("{}:{}", tag.key, tag.generation)),
                );
                assert!(supervisor.diagnostics().starts() >= before);
            }
            let expected_starts = list(record.field("starts"));
            let actual_starts: Vec<_> = supervisor
                .generations
                .iter()
                .flat_map(|(key, generation)| {
                    (1..=*generation).map(move |generation| format!("{key}:{generation}"))
                })
                .collect();
            assert_same_unordered(&actual_starts, &expected_starts, &record.id);
            assert_eq!(
                actual_stops,
                list(record.field("stops")),
                "case {}",
                record.id
            );
            let actual_active: Vec<_> = supervisor.order.iter().map(ToString::to_string).collect();
            assert_eq!(
                actual_active,
                list(record.field("active")),
                "case {}",
                record.id
            );
            let actual_generations: Vec<_> = supervisor
                .generations
                .iter()
                .map(|(key, generation)| format!("{key}:{generation}"))
                .collect();
            assert_same_unordered(
                &actual_generations,
                &list(record.field("generations")),
                &record.id,
            );
        }
    }

    #[test]
    fn backpressure_matches_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "subscriptions/backpressure.txt",
            "subscription-backpressure",
            &[
                "policy",
                "capacity",
                "actions",
                "checkpoints",
                "replacements",
            ],
        ) else {
            return;
        };
        for record in records {
            let capacity: usize = record.field("capacity").parse().unwrap();
            let policy = parse_policy(record.field("policy"));
            let started = Arc::new(Mutex::new(None));
            let mut supervisor = SubscriptionSupervisor::new(capacity);
            supervisor
                .reconcile(
                    stream("source", policy, Some(Arc::clone(&started))),
                    Timestamp::default(),
                )
                .unwrap();
            let sink = wait_sink(&started);
            let mut now = Timestamp::default();
            let mut checkpoints = Vec::new();
            for action in record.field("actions").split(';') {
                let (kind, value) = action.split_once(':').unwrap_or((action, ""));
                match kind {
                    "send" => sink.send(value.to_owned()).unwrap(),
                    "advance" => {
                        now = now.saturating_add(Duration::from_millis(value.parse().unwrap()));
                    }
                    "poll" => {
                        supervisor.poll(now);
                        checkpoints.push(join_messages(supervisor.take_ready(usize::MAX)));
                    }
                    _ => panic!("unknown action {action}"),
                }
            }
            assert_eq!(
                checkpoints.join("|"),
                record.field("checkpoints"),
                "case {}",
                record.id
            );
            assert_eq!(
                supervisor.diagnostics().latest_replacements().to_string(),
                record.field("replacements"),
                "case {}",
                record.id
            );
        }
    }

    #[test]
    fn every_matches_shared_virtual_time_fixtures() {
        let Some(records) = fixture_support::load(
            "subscriptions/every.txt",
            "subscription-every",
            &["interval", "policy", "capacity", "advances", "checkpoints"],
        ) else {
            return;
        };
        for record in records {
            let counter = Arc::new(AtomicU64::new(0));
            let factory_counter = Arc::clone(&counter);
            let mut supervisor =
                SubscriptionSupervisor::new(record.field("capacity").parse().unwrap());
            supervisor
                .reconcile(
                    Subscription::every(
                        "clock",
                        Duration::from_millis(record.field("interval").parse().unwrap()),
                        parse_policy(record.field("policy")),
                        move || factory_counter.fetch_add(1, Ordering::Relaxed) + 1,
                    ),
                    Timestamp::default(),
                )
                .unwrap();
            let mut now = Timestamp::default();
            let mut checkpoints = Vec::new();
            for advance in record.field("advances").split(',') {
                now = now.saturating_add(Duration::from_millis(advance.parse().unwrap()));
                supervisor.poll(now);
                checkpoints.push(
                    supervisor
                        .take_ready(usize::MAX)
                        .into_iter()
                        .map(|value| value.message.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
            let actual = checkpoints
                .into_iter()
                .map(|value| {
                    if value.is_empty() {
                        "-".to_owned()
                    } else {
                        value
                    }
                })
                .collect::<Vec<_>>()
                .join("|");
            assert_eq!(actual, record.field("checkpoints"), "case {}", record.id);
        }
    }

    #[test]
    fn reliable_stream_blocks_at_capacity_and_wakes_on_stop() {
        let started = Arc::new(Mutex::new(None));
        let mut supervisor = SubscriptionSupervisor::new(1);
        supervisor
            .reconcile(
                stream(
                    "source",
                    DeliveryPolicy::reliable(),
                    Some(Arc::clone(&started)),
                ),
                Timestamp::default(),
            )
            .unwrap();
        let sink = wait_sink(&started);
        sink.send("first".to_owned()).unwrap();
        let blocked_sink = sink.clone();
        let (returned_sender, returned): (_, Receiver<_>) = sync_channel(1);
        thread::spawn(move || {
            returned_sender
                .send(blocked_sink.send("second".to_owned()))
                .unwrap();
        });
        for _ in 0..10_000 {
            if supervisor.diagnostics().blocked_sends() == 1 {
                break;
            }
            thread::yield_now();
        }
        assert!(matches!(returned.try_recv(), Err(TryRecvError::Empty)));
        supervisor.poll(Timestamp::default());
        assert_eq!(join_messages(supervisor.take_ready(1)), "first");
        assert!(returned.recv().unwrap().is_ok());
        let reconciliation = supervisor
            .reconcile(Subscription::none(), Timestamp::default())
            .unwrap();
        assert_eq!(reconciliation.stopped.len(), 1);
    }

    #[test]
    fn latest_burst_remains_bounded() {
        let started = Arc::new(Mutex::new(None));
        let mut supervisor = SubscriptionSupervisor::new(2);
        supervisor
            .reconcile(
                stream("logs", DeliveryPolicy::latest(), Some(Arc::clone(&started))),
                Timestamp::default(),
            )
            .unwrap();
        let sink = wait_sink(&started);
        for value in 0..10_000 {
            sink.send(value.to_string()).unwrap();
        }
        assert_eq!(supervisor.pending_messages(), 1);
        assert_eq!(supervisor.diagnostics().latest_replacements(), 9_999);
        supervisor.poll(Timestamp::default());
        assert_eq!(join_messages(supervisor.take_ready(10)), "9999");
    }

    #[test]
    fn multiple_sources_preserve_global_arrival_order() {
        let first_started = Arc::new(Mutex::new(None));
        let second_started = Arc::new(Mutex::new(None));
        let mut supervisor = SubscriptionSupervisor::new(4);
        supervisor
            .reconcile(
                Subscription::batch([
                    stream(
                        "first",
                        DeliveryPolicy::reliable(),
                        Some(Arc::clone(&first_started)),
                    ),
                    stream(
                        "second",
                        DeliveryPolicy::reliable(),
                        Some(Arc::clone(&second_started)),
                    ),
                ]),
                Timestamp::default(),
            )
            .unwrap();
        let first = wait_sink(&first_started);
        let second = wait_sink(&second_started);
        first.send("first-1".to_owned()).unwrap();
        second.send("second-1".to_owned()).unwrap();
        first.send("first-2".to_owned()).unwrap();
        supervisor.poll(Timestamp::default());
        assert_eq!(
            join_messages(supervisor.take_ready(10)),
            "first-1,second-1,first-2"
        );
    }

    #[test]
    fn stream_panic_is_isolated_and_diagnosed() {
        let mut supervisor = SubscriptionSupervisor::<String>::new(2);
        supervisor
            .reconcile(
                Subscription::stream("panic", DeliveryPolicy::reliable(), |_token, _sink| {
                    panic!("producer failure")
                }),
                Timestamp::default(),
            )
            .unwrap();
        for _ in 0..10_000 {
            if supervisor.running_streams() == 0 {
                break;
            }
            thread::yield_now();
        }
        assert_eq!(supervisor.running_streams(), 0);
        assert_eq!(supervisor.active_subscriptions(), 1);
        assert_eq!(supervisor.diagnostics().producer_panics(), 1);
    }

    #[test]
    fn duplicate_keys_are_rejected_before_mutation() {
        let mut supervisor = SubscriptionSupervisor::new(4);
        let error = supervisor
            .reconcile(
                Subscription::batch([
                    Subscription::every(
                        "duplicate",
                        Duration::from_millis(1),
                        DeliveryPolicy::reliable(),
                        || "a",
                    ),
                    Subscription::every(
                        "duplicate",
                        Duration::from_millis(1),
                        DeliveryPolicy::reliable(),
                        || "b",
                    ),
                ]),
                Timestamp::default(),
            )
            .unwrap_err();
        assert_eq!(error.as_str(), "duplicate");
        assert_eq!(supervisor.active_subscriptions(), 0);
    }

    fn parse_declaration(value: &str) -> Subscription<String> {
        let parts: Vec<_> = value.split(':').collect();
        match parts.as_slice() {
            ["every", key, interval, "reliable"] => Subscription::every(
                *key,
                Duration::from_millis(interval.parse().unwrap()),
                DeliveryPolicy::reliable(),
                String::new,
            ),
            ["stream", key, "reliable"] => stream(key, DeliveryPolicy::reliable(), None),
            ["stream", key, "latest"] => stream(key, DeliveryPolicy::latest(), None),
            _ => panic!("invalid declaration {value}"),
        }
    }

    fn parse_policy(value: &str) -> DeliveryPolicy {
        let parts: Vec<_> = value.split(':').collect();
        match parts.as_slice() {
            ["reliable"] => DeliveryPolicy::reliable(),
            ["latest"] => DeliveryPolicy::latest(),
            ["batch", maximum, delay] => DeliveryPolicy::batch(
                maximum.parse().unwrap(),
                Duration::from_millis(delay.parse().unwrap()),
            ),
            _ => panic!("invalid policy {value}"),
        }
    }

    fn join_messages(messages: Vec<SubscriptionMessage<String>>) -> String {
        let joined = messages
            .into_iter()
            .map(|message| message.message)
            .collect::<Vec<_>>()
            .join(",");
        if joined.is_empty() {
            "-".to_owned()
        } else {
            joined
        }
    }

    fn list(value: &str) -> Vec<String> {
        if value == "-" {
            Vec::new()
        } else {
            value.split(',').map(str::to_owned).collect()
        }
    }

    fn assert_same_unordered(actual: &[String], expected: &[String], case: &str) {
        let counts = |values: &[String]| {
            let mut counts = HashMap::new();
            for value in values {
                *counts.entry(value.clone()).or_insert(0) += 1;
            }
            counts
        };
        assert_eq!(counts(actual), counts(expected), "case {case}");
    }
}
