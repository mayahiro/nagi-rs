use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread;
use std::time::Duration;

use crate::effect::{EffectKind, RuntimeCommand, Task};
use crate::{CancelToken, Effect, ScopeId, TaskKey, Timestamp};

/// Counters describing supervised task behavior
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EffectDiagnostics {
    cancellations: u64,
    stale_results: u64,
    task_panics: u64,
    spawn_failures: u64,
}

impl EffectDiagnostics {
    /// Returns the number of cooperative cancellation requests
    #[must_use]
    pub const fn cancellations(self) -> u64 {
        self.cancellations
    }

    /// Returns the number of completed task results suppressed as stale
    #[must_use]
    pub const fn stale_results(self) -> u64 {
        self.stale_results
    }

    /// Returns the number of task panics caught at the worker boundary
    #[must_use]
    pub const fn task_panics(self) -> u64 {
        self.task_panics
    }

    /// Returns the number of operating-system thread spawn failures
    #[must_use]
    pub const fn spawn_failures(self) -> u64 {
        self.spawn_failures
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ScopeTag {
    id: ScopeId,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Continuation {
    #[default]
    None,
    Sequence(u64),
    Barrier(u64),
}

enum TaskStatus<Message> {
    Pending(Task<Message>),
    Running,
}

struct TaskState<Message> {
    status: TaskStatus<Message>,
    token: CancelToken,
    scopes: Vec<ScopeTag>,
    continuation: Option<Continuation>,
    latest: Option<(TaskKey, u64)>,
    cancelled: bool,
}

enum TaskResult<Message> {
    Message(Message),
    Panicked,
}

struct TaskOutcome<Message> {
    id: u64,
    result: TaskResult<Message>,
}

struct TimerState<Message> {
    deadline: Timestamp,
    order: u64,
    message: Message,
    scopes: Vec<ScopeTag>,
    continuation: Continuation,
}

struct SequenceState<Message> {
    remaining: VecDeque<Effect<Message>>,
    scopes: Vec<ScopeTag>,
    parent: Continuation,
}

struct BarrierState {
    remaining: usize,
    parent: Continuation,
}

pub(crate) struct EffectSupervisor<Message> {
    task_limit: usize,
    sender: SyncSender<TaskOutcome<Message>>,
    receiver: Receiver<TaskOutcome<Message>>,
    tasks: HashMap<u64, TaskState<Message>>,
    pending: VecDeque<u64>,
    running: usize,
    latest: HashMap<TaskKey, (u64, u64)>,
    generations: HashMap<TaskKey, u64>,
    scope_generations: HashMap<ScopeId, u64>,
    timers: Vec<TimerState<Message>>,
    sequences: HashMap<u64, SequenceState<Message>>,
    barriers: HashMap<u64, BarrierState>,
    ready: VecDeque<Message>,
    commands: VecDeque<RuntimeCommand>,
    next_identifier: u64,
    next_order: u64,
    diagnostics: EffectDiagnostics,
}

impl<Message: Send + 'static> EffectSupervisor<Message> {
    pub(crate) fn new(task_limit: usize) -> Self {
        let (sender, receiver) = sync_channel(task_limit);
        Self {
            task_limit,
            sender,
            receiver,
            tasks: HashMap::new(),
            pending: VecDeque::new(),
            running: 0,
            latest: HashMap::new(),
            generations: HashMap::new(),
            scope_generations: HashMap::new(),
            timers: Vec::new(),
            sequences: HashMap::new(),
            barriers: HashMap::new(),
            ready: VecDeque::new(),
            commands: VecDeque::new(),
            next_identifier: 1,
            next_order: 0,
            diagnostics: EffectDiagnostics::default(),
        }
    }

    pub(crate) fn schedule(&mut self, effect: Effect<Message>, now: Timestamp) {
        self.start_effect(effect, Vec::new(), Continuation::None, now);
    }

    pub(crate) fn poll(&mut self, now: Timestamp) {
        self.poll_timers(now);
        while let Ok(outcome) = self.receiver.try_recv() {
            self.finish_task(outcome, now);
        }
        self.spawn_available(now);
    }

    pub(crate) fn take_ready(&mut self, maximum: usize) -> Vec<Message> {
        let count = maximum.min(self.ready.len());
        self.ready.drain(..count).collect()
    }

    pub(crate) fn take_commands(&mut self) -> Vec<RuntimeCommand> {
        self.commands.drain(..).collect()
    }

    pub(crate) fn ready_messages(&self) -> usize {
        self.ready.len()
    }

    pub(crate) fn active_tasks(&self) -> usize {
        self.tasks.len()
    }

    pub(crate) fn running_tasks(&self) -> usize {
        self.running
    }

    pub(crate) fn pending_tasks(&self) -> usize {
        self.tasks
            .values()
            .filter(|task| matches!(task.status, TaskStatus::Pending(_)))
            .count()
    }

    pub(crate) const fn diagnostics(&self) -> EffectDiagnostics {
        self.diagnostics
    }

    pub(crate) fn generation(&self, key: &TaskKey) -> u64 {
        self.generations.get(key).copied().unwrap_or(0)
    }

    pub(crate) fn time_until_deadline(&self, now: Timestamp) -> Option<Duration> {
        self.timers
            .iter()
            .map(|timer| timer.deadline)
            .min()
            .map(|deadline| {
                Duration::from_nanos(deadline.as_nanos().saturating_sub(now.as_nanos()))
            })
    }

    fn start_effect(
        &mut self,
        effect: Effect<Message>,
        mut scopes: Vec<ScopeTag>,
        continuation: Continuation,
        now: Timestamp,
    ) {
        if !self.scopes_active(&scopes) {
            self.complete(continuation, now);
            return;
        }
        match effect.kind {
            EffectKind::None => self.complete(continuation, now),
            EffectKind::Exit => {
                self.commands.push_back(RuntimeCommand::Exit);
                self.complete(continuation, now);
            }
            EffectKind::Focus(id) => {
                self.commands.push_back(RuntimeCommand::Focus(id));
                self.complete(continuation, now);
            }
            EffectKind::ScrollTo { id, offset } => {
                self.commands
                    .push_back(RuntimeCommand::ScrollTo { id, offset });
                self.complete(continuation, now);
            }
            EffectKind::Run(task) => {
                self.start_task(task, scopes, continuation, None, now);
            }
            EffectKind::Latest { key, task } => {
                self.cancel_latest(&key, now);
                let generation = self
                    .generations
                    .get(&key)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(1);
                self.generations.insert(key.clone(), generation);
                let id = self.start_task(
                    task,
                    scopes,
                    continuation,
                    Some((key.clone(), generation)),
                    now,
                );
                self.latest.insert(key, (id, generation));
            }
            EffectKind::Cancel(key) => {
                self.cancel_latest(&key, now);
                self.complete(continuation, now);
            }
            EffectKind::Scoped { scope, effect } => {
                let generation = self.scope_generations.get(&scope).copied().unwrap_or(0);
                if !scopes.iter().any(|tag| tag.id == scope) {
                    scopes.push(ScopeTag {
                        id: scope,
                        generation,
                    });
                }
                self.start_effect(*effect, scopes, continuation, now);
            }
            EffectKind::CancelScope(scope) => {
                self.cancel_scope(&scope, now);
                self.complete(continuation, now);
            }
            EffectKind::After { delay, message } => {
                let order = self.next_order;
                self.next_order = self.next_order.saturating_add(1);
                self.timers.push(TimerState {
                    deadline: now.saturating_add(delay),
                    order,
                    message,
                    scopes,
                    continuation,
                });
            }
            EffectKind::Batch(effects) => {
                if effects.is_empty() {
                    self.complete(continuation, now);
                    return;
                }
                let id = self.identifier();
                self.barriers.insert(
                    id,
                    BarrierState {
                        remaining: effects.len(),
                        parent: continuation,
                    },
                );
                for effect in effects {
                    self.start_effect(effect, scopes.clone(), Continuation::Barrier(id), now);
                }
            }
            EffectKind::Sequence(effects) => {
                let mut remaining: VecDeque<_> = effects.into();
                let Some(first) = remaining.pop_front() else {
                    self.complete(continuation, now);
                    return;
                };
                let id = self.identifier();
                self.sequences.insert(
                    id,
                    SequenceState {
                        remaining,
                        scopes: scopes.clone(),
                        parent: continuation,
                    },
                );
                self.start_effect(first, scopes, Continuation::Sequence(id), now);
            }
        }
    }

    fn start_task(
        &mut self,
        task: Task<Message>,
        scopes: Vec<ScopeTag>,
        continuation: Continuation,
        latest: Option<(TaskKey, u64)>,
        now: Timestamp,
    ) -> u64 {
        let id = self.identifier();
        self.tasks.insert(
            id,
            TaskState {
                status: TaskStatus::Pending(task),
                token: CancelToken::default(),
                scopes,
                continuation: Some(continuation),
                latest,
                cancelled: false,
            },
        );
        self.pending.push_back(id);
        self.spawn_available(now);
        id
    }

    fn spawn_available(&mut self, now: Timestamp) {
        while self.running < self.task_limit {
            let Some(id) = self.pending.pop_front() else {
                break;
            };
            let Some(state) = self.tasks.get_mut(&id) else {
                continue;
            };
            if state.cancelled {
                continue;
            }
            let TaskStatus::Pending(task) =
                std::mem::replace(&mut state.status, TaskStatus::Running)
            else {
                continue;
            };
            let token = state.token.clone();
            let sender = self.sender.clone();
            self.running += 1;
            let spawned = thread::Builder::new()
                .name(format!("nagi-tui-effect-{id}"))
                .spawn(move || {
                    let result = catch_unwind(AssertUnwindSafe(|| task(token)))
                        .map_or(TaskResult::Panicked, TaskResult::Message);
                    let _ = sender.send(TaskOutcome { id, result });
                });
            if spawned.is_err() {
                self.diagnostics.spawn_failures = self.diagnostics.spawn_failures.saturating_add(1);
                self.finish_without_message(id, now);
            }
        }
    }

    fn finish_task(&mut self, outcome: TaskOutcome<Message>, now: Timestamp) {
        let Some(mut state) = self.tasks.remove(&outcome.id) else {
            return;
        };
        if matches!(state.status, TaskStatus::Running) {
            self.running = self.running.saturating_sub(1);
        }
        let latest_matches = state.latest.as_ref().is_none_or(|(key, generation)| {
            self.latest.get(key) == Some(&(outcome.id, *generation))
        });
        if let Some((key, generation)) = &state.latest {
            if self.latest.get(key) == Some(&(outcome.id, *generation)) {
                self.latest.remove(key);
            }
        }
        match outcome.result {
            TaskResult::Message(message) if !state.cancelled && latest_matches => {
                self.ready.push_back(message);
            }
            TaskResult::Message(_) => {
                self.diagnostics.stale_results = self.diagnostics.stale_results.saturating_add(1);
            }
            TaskResult::Panicked => {
                self.diagnostics.task_panics = self.diagnostics.task_panics.saturating_add(1);
            }
        }
        if let Some(continuation) = state.continuation.take() {
            self.complete(continuation, now);
        }
        self.spawn_available(now);
    }

    fn finish_without_message(&mut self, id: u64, now: Timestamp) {
        let Some(mut state) = self.tasks.remove(&id) else {
            return;
        };
        if matches!(state.status, TaskStatus::Running) {
            self.running = self.running.saturating_sub(1);
        }
        if let Some((key, generation)) = &state.latest {
            if self.latest.get(key) == Some(&(id, *generation)) {
                self.latest.remove(key);
            }
        }
        if let Some(continuation) = state.continuation.take() {
            self.complete(continuation, now);
        }
    }

    fn cancel_latest(&mut self, key: &TaskKey, now: Timestamp) {
        if let Some((id, _)) = self.latest.remove(key) {
            self.cancel_task(id, now);
        }
    }

    fn cancel_task(&mut self, id: u64, now: Timestamp) {
        let Some(state) = self.tasks.get_mut(&id) else {
            return;
        };
        if state.cancelled {
            return;
        }
        state.cancelled = true;
        state.token.cancel();
        self.diagnostics.cancellations = self.diagnostics.cancellations.saturating_add(1);
        let continuation = state.continuation.take();
        let pending = matches!(state.status, TaskStatus::Pending(_));
        if let Some((key, generation)) = &state.latest {
            if self.latest.get(key) == Some(&(id, *generation)) {
                self.latest.remove(key);
            }
        }
        if pending {
            self.tasks.remove(&id);
        }
        if let Some(continuation) = continuation {
            self.complete(continuation, now);
        }
        self.spawn_available(now);
    }

    fn cancel_scope(&mut self, scope: &ScopeId, now: Timestamp) {
        let generation = self.scope_generations.get(scope).copied().unwrap_or(0);
        self.scope_generations
            .insert(scope.clone(), generation.saturating_add(1));
        let tasks: Vec<_> = self
            .tasks
            .iter()
            .filter(|(_, task)| {
                task.scopes
                    .iter()
                    .any(|tag| tag.id == *scope && tag.generation == generation)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in tasks {
            self.cancel_task(id, now);
        }

        let mut retained = Vec::with_capacity(self.timers.len());
        let mut cancelled = Vec::new();
        for timer in self.timers.drain(..) {
            if timer
                .scopes
                .iter()
                .any(|tag| tag.id == *scope && tag.generation == generation)
            {
                cancelled.push(timer.continuation);
                self.diagnostics.cancellations = self.diagnostics.cancellations.saturating_add(1);
            } else {
                retained.push(timer);
            }
        }
        self.timers = retained;
        for continuation in cancelled {
            self.complete(continuation, now);
        }
    }

    fn poll_timers(&mut self, now: Timestamp) {
        loop {
            let mut retained = Vec::with_capacity(self.timers.len());
            let mut due = Vec::new();
            for timer in self.timers.drain(..) {
                if timer.deadline <= now {
                    due.push(timer);
                } else {
                    retained.push(timer);
                }
            }
            self.timers = retained;
            if due.is_empty() {
                return;
            }
            due.sort_by_key(|timer| (timer.deadline, timer.order));
            for timer in due {
                if self.scopes_active(&timer.scopes) {
                    self.ready.push_back(timer.message);
                }
                self.complete(timer.continuation, now);
            }
        }
    }

    fn complete(&mut self, mut continuation: Continuation, now: Timestamp) {
        loop {
            match continuation {
                Continuation::None => return,
                Continuation::Barrier(id) => {
                    let Some(barrier) = self.barriers.get_mut(&id) else {
                        return;
                    };
                    barrier.remaining = barrier.remaining.saturating_sub(1);
                    if barrier.remaining != 0 {
                        return;
                    }
                    continuation = self
                        .barriers
                        .remove(&id)
                        .map_or(Continuation::None, |barrier| barrier.parent);
                }
                Continuation::Sequence(id) => {
                    let Some(mut sequence) = self.sequences.remove(&id) else {
                        return;
                    };
                    let Some(next) = sequence.remaining.pop_front() else {
                        continuation = sequence.parent;
                        continue;
                    };
                    let scopes = sequence.scopes.clone();
                    self.sequences.insert(id, sequence);
                    self.start_effect(next, scopes, Continuation::Sequence(id), now);
                    return;
                }
            }
        }
    }

    fn scopes_active(&self, scopes: &[ScopeTag]) -> bool {
        scopes
            .iter()
            .all(|tag| self.scope_generations.get(&tag.id).copied().unwrap_or(0) == tag.generation)
    }

    fn identifier(&mut self) -> u64 {
        let id = self.next_identifier;
        self.next_identifier = self.next_identifier.saturating_add(1);
        id
    }
}

impl<Message> Drop for EffectSupervisor<Message> {
    fn drop(&mut self) {
        for task in self.tasks.values() {
            task.token.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};

    use crate::{Clock, VirtualClock, fixture_support};

    use super::*;

    struct TaskControl {
        started: Receiver<CancelToken>,
        complete: SyncSender<String>,
        returned: Receiver<()>,
    }

    fn controlled_task() -> (Task<String>, TaskControl) {
        let (started_sender, started) = sync_channel(1);
        let (complete, complete_receiver) = sync_channel(1);
        let (returned_sender, returned) = sync_channel(1);
        let task = Box::new(move |token: CancelToken| {
            started_sender.send(token).unwrap();
            let message = complete_receiver.recv().unwrap();
            returned_sender.send(()).unwrap();
            message
        });
        (
            task,
            TaskControl {
                started,
                complete,
                returned,
            },
        )
    }

    fn wait_started(control: &TaskControl) -> CancelToken {
        for _ in 0..10_000 {
            match control.started.try_recv() {
                Ok(token) => return token,
                Err(TryRecvError::Empty) => thread::yield_now(),
                Err(TryRecvError::Disconnected) => panic!("controlled task did not start"),
            }
        }
        panic!("controlled task did not start")
    }

    fn complete(control: &TaskControl, message: &str) {
        control.complete.send(message.to_owned()).unwrap();
        control.returned.recv().unwrap();
    }

    fn poll_until<Message: Send + 'static>(
        supervisor: &mut EffectSupervisor<Message>,
        predicate: impl Fn(&EffectSupervisor<Message>) -> bool,
    ) {
        for _ in 0..10_000 {
            supervisor.poll(Timestamp::default());
            if predicate(supervisor) {
                return;
            }
            thread::yield_now();
        }
        panic!("effect supervisor did not reach expected state")
    }

    #[test]
    fn latest_cancels_old_task_and_suppresses_stale_result() {
        let mut supervisor = EffectSupervisor::new(2);
        let (old_task, old) = controlled_task();
        let (new_task, new) = controlled_task();
        supervisor.schedule(Effect::latest("search", old_task), Timestamp::default());
        let old_token = wait_started(&old);

        supervisor.schedule(Effect::latest("search", new_task), Timestamp::default());
        let _new_token = wait_started(&new);
        assert!(old_token.is_cancelled());
        assert_eq!(supervisor.generation(&TaskKey::from("search")), 2);

        complete(&old, "old");
        complete(&new, "new");
        poll_until(&mut supervisor, |supervisor| supervisor.active_tasks() == 0);

        assert_eq!(supervisor.take_ready(usize::MAX), ["new"]);
        assert_eq!(supervisor.diagnostics().cancellations(), 1);
        assert_eq!(supervisor.diagnostics().stale_results(), 1);
    }

    #[test]
    fn scope_cancellation_propagates_without_affecting_other_scopes() {
        let mut supervisor = EffectSupervisor::new(2);
        let (cancelled_task, cancelled) = controlled_task();
        let (retained_task, retained) = controlled_task();
        supervisor.schedule(
            Effect::scoped("screen-a", Effect::run(cancelled_task)),
            Timestamp::default(),
        );
        supervisor.schedule(
            Effect::scoped("screen-b", Effect::run(retained_task)),
            Timestamp::default(),
        );
        let cancelled_token = wait_started(&cancelled);
        let retained_token = wait_started(&retained);

        supervisor.schedule(Effect::cancel_scope("screen-a"), Timestamp::default());
        assert!(cancelled_token.is_cancelled());
        assert!(!retained_token.is_cancelled());
        complete(&cancelled, "cancelled");
        complete(&retained, "retained");
        poll_until(&mut supervisor, |supervisor| supervisor.active_tasks() == 0);

        assert_eq!(supervisor.take_ready(usize::MAX), ["retained"]);
    }

    #[test]
    fn virtual_timers_preserve_batch_and_sequence_semantics() {
        let mut supervisor = EffectSupervisor::new(1);
        supervisor.schedule(
            Effect::batch([
                Effect::after(Duration::from_millis(10), "late"),
                Effect::after(Duration::from_millis(5), "early"),
            ]),
            Timestamp::default(),
        );
        assert_eq!(
            supervisor.time_until_deadline(Timestamp::default()),
            Some(Duration::from_millis(5))
        );
        supervisor.poll(Timestamp::from_nanos(4_999_999));
        assert!(supervisor.take_ready(usize::MAX).is_empty());
        supervisor.poll(Timestamp::from_nanos(10_000_000));
        assert_eq!(supervisor.take_ready(usize::MAX), ["early", "late"]);

        supervisor.schedule(
            Effect::sequence([
                Effect::after(Duration::from_millis(5), "first"),
                Effect::after(Duration::ZERO, "second"),
            ]),
            Timestamp::from_nanos(10_000_000),
        );
        supervisor.poll(Timestamp::from_nanos(15_000_000));
        assert_eq!(supervisor.take_ready(usize::MAX), ["first", "second"]);
    }

    #[test]
    fn task_limit_defers_batch_workers() {
        let mut supervisor = EffectSupervisor::new(1);
        let (first_task, first) = controlled_task();
        let (second_task, second) = controlled_task();
        supervisor.schedule(
            Effect::batch([Effect::run(first_task), Effect::run(second_task)]),
            Timestamp::default(),
        );
        let _first_token = wait_started(&first);
        assert!(matches!(
            second.started.try_recv(),
            Err(TryRecvError::Empty)
        ));
        assert_eq!(supervisor.running_tasks(), 1);
        assert_eq!(supervisor.pending_tasks(), 1);

        complete(&first, "first");
        poll_until(&mut supervisor, |supervisor| {
            supervisor.pending_tasks() == 0
        });
        let _second_token = wait_started(&second);
        complete(&second, "second");
        poll_until(&mut supervisor, |supervisor| supervisor.active_tasks() == 0);
        assert_eq!(supervisor.take_ready(usize::MAX), ["first", "second"]);
    }

    #[test]
    fn panics_become_diagnostics_and_do_not_stall_sequences() {
        let mut supervisor = EffectSupervisor::new(1);
        supervisor.schedule(
            Effect::sequence([
                Effect::run(|_| -> &'static str { panic!("task failure") }),
                Effect::after(Duration::ZERO, "recovered"),
            ]),
            Timestamp::default(),
        );
        poll_until(&mut supervisor, |supervisor| {
            supervisor.diagnostics().task_panics() == 1
        });
        supervisor.poll(Timestamp::default());
        assert_eq!(supervisor.take_ready(usize::MAX), ["recovered"]);
    }

    #[test]
    fn latest_generations_match_shared_fixtures() {
        run_controlled_fixtures(
            "effects/generation.txt",
            "effect-generation",
            &["actions", "expected", "cancelled", "generations", "stale"],
            true,
        );
    }

    #[test]
    fn scope_cancellation_matches_shared_fixtures() {
        run_controlled_fixtures(
            "effects/cancellation.txt",
            "effect-cancellation",
            &["actions", "expected", "cancelled", "stale"],
            false,
        );
    }

    #[test]
    fn virtual_time_matches_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "effects/virtual-time.txt",
            "effect-virtual-time",
            &["mode", "effects", "advances", "checkpoints"],
        ) else {
            return;
        };
        for record in records {
            let clock = VirtualClock::new();
            let effects: Vec<_> = record
                .field("effects")
                .split(',')
                .map(|item| {
                    let (delay, message) = item
                        .split_once(':')
                        .unwrap_or_else(|| panic!("invalid timer {item}"));
                    Effect::after(Duration::from_millis(number(delay)), message.to_owned())
                })
                .collect();
            let effect = match record.field("mode") {
                "batch" => Effect::batch(effects),
                "sequence" => Effect::sequence(effects),
                mode => panic!("invalid effect mode {mode}"),
            };
            let mut supervisor = EffectSupervisor::new(1);
            supervisor.schedule(effect, clock.now());
            let checkpoints: Vec<_> = record.field("checkpoints").split('|').collect();
            let advances: Vec<_> = record.field("advances").split(',').collect();
            assert_eq!(advances.len(), checkpoints.len(), "case {}", record.id);
            let mut delivered = Vec::new();
            for (advance, expected) in advances.into_iter().zip(checkpoints) {
                clock.advance(Duration::from_millis(number(advance)));
                supervisor.poll(clock.now());
                delivered.extend(supervisor.take_ready(usize::MAX));
                assert_eq!(
                    list(&delivered.join(",")),
                    list(expected),
                    "case {} after {advance} ms",
                    record.id
                );
            }
        }
    }

    fn run_controlled_fixtures(path: &str, suite: &str, fields: &[&str], check_generations: bool) {
        let Some(records) = fixture_support::load(path, suite, fields) else {
            return;
        };
        for record in records {
            let mut supervisor = EffectSupervisor::new(16);
            let mut controls: HashMap<String, (TaskControl, CancelToken)> = HashMap::new();
            for action in record.field("actions").split(';') {
                let parts: Vec<_> = action.split(':').collect();
                match parts.as_slice() {
                    ["latest", key, name] => {
                        let (task, control) = controlled_task();
                        supervisor.schedule(Effect::latest(*key, task), Timestamp::default());
                        let token = wait_started(&control);
                        controls.insert((*name).to_owned(), (control, token));
                    }
                    ["scoped", scope, name] => {
                        let (task, control) = controlled_task();
                        supervisor.schedule(
                            Effect::scoped(*scope, Effect::run(task)),
                            Timestamp::default(),
                        );
                        let token = wait_started(&control);
                        controls.insert((*name).to_owned(), (control, token));
                    }
                    ["complete", name, message] => {
                        let active = supervisor.active_tasks();
                        complete(&controls[*name].0, message);
                        poll_until(&mut supervisor, |supervisor| {
                            supervisor.active_tasks() < active
                        });
                    }
                    ["cancel", key] => {
                        supervisor.schedule(Effect::cancel(*key), Timestamp::default())
                    }
                    ["cancel-scope", scope] => {
                        supervisor.schedule(Effect::cancel_scope(*scope), Timestamp::default())
                    }
                    _ => panic!("invalid action {action}"),
                }
            }
            let delivered = supervisor.take_ready(usize::MAX);
            assert_eq!(
                delivered,
                list(record.field("expected")),
                "case {} messages",
                record.id
            );
            let mut cancelled: Vec<_> = controls
                .iter()
                .filter(|(_, (_, token))| token.is_cancelled())
                .map(|(name, _)| name.clone())
                .collect();
            cancelled.sort();
            assert_eq!(
                cancelled,
                list(record.field("cancelled")),
                "case {} cancellation",
                record.id
            );
            assert_eq!(
                supervisor.diagnostics().stale_results(),
                number(record.field("stale")),
                "case {} stale results",
                record.id
            );
            if check_generations {
                for item in record.field("generations").split(',') {
                    let (key, generation) = item
                        .split_once(':')
                        .unwrap_or_else(|| panic!("invalid generation {item}"));
                    assert_eq!(
                        supervisor.generation(&TaskKey::from(key)),
                        number(generation),
                        "case {} generation {key}",
                        record.id
                    );
                }
            }
        }
    }

    fn list(value: &str) -> Vec<String> {
        if value.is_empty() || value == "-" {
            Vec::new()
        } else {
            value.split(',').map(str::to_owned).collect()
        }
    }

    fn number(value: &str) -> u64 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
    }
}
