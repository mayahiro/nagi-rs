//! Virtual terminal and deterministic application test support for Nagi TUI

use std::error::Error;
use std::fmt;
use std::time::Duration;

use nagi_tui::{
    App, EffectDiagnostics, Event, EventAction, Frame, InteractionState, NodeId, QueueFull,
    ResolvedActions, Runtime, RuntimeConfig, RuntimeError, RuntimeEventError, RuntimeNotice,
    RuntimeNoticeDiagnostics, ScrollOffset, ScrollState, Size, SubscriptionDiagnostics,
    SubscriptionKey, TaskKey, TimedInputDecoder, VirtualClock,
};

mod manual_subscription;
mod manual_task;

pub use manual_subscription::{
    ManualSubscription, ManualSubscriptionSendError, ManualSubscriptionSendErrorKind,
    manual_subscription,
};
pub use manual_task::{ManualTask, ManualTaskError, manual_task};

/// An error while driving a test application
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HarnessError {
    /// Runtime construction or rendering failed
    Runtime(RuntimeError),
    /// The bounded runtime queue filled
    QueueFull,
}

impl fmt::Display for HarnessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => error.fmt(formatter),
            Self::QueueFull => QueueFull.fmt(formatter),
        }
    }
}

impl Error for HarnessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Runtime(error) => Some(error),
            Self::QueueFull => None,
        }
    }
}

impl From<RuntimeError> for HarnessError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl From<QueueFull> for HarnessError {
    fn from(_: QueueFull) -> Self {
        Self::QueueFull
    }
}

impl From<RuntimeEventError> for HarnessError {
    fn from(error: RuntimeEventError) -> Self {
        match error {
            RuntimeEventError::Runtime(error) => Self::Runtime(error),
            RuntimeEventError::QueueFull => Self::QueueFull,
        }
    }
}

/// A deterministic application driver with virtual terminal input and time
pub struct Harness<Application, Mapper>
where
    Application: App,
    Application::Message: Clone,
    Mapper: FnMut(Event) -> EventAction<Application::Message>,
{
    runtime: Runtime<Application, VirtualClock>,
    decoder: TimedInputDecoder<VirtualClock>,
    clock: VirtualClock,
    map_event: Mapper,
    frames: Vec<Frame>,
    messages: Vec<Application::Message>,
    exit_requested: bool,
}

impl<Application, Mapper> Harness<Application, Mapper>
where
    Application: App,
    Application::Message: Clone,
    Mapper: FnMut(Event) -> EventAction<Application::Message>,
{
    /// Creates a harness with default runtime settings and a 25 ms ESC timeout
    pub fn new(app: Application, size: Size, map_event: Mapper) -> Result<Self, HarnessError> {
        Self::with_config(
            app,
            RuntimeConfig::new(size),
            Duration::from_millis(25),
            map_event,
        )
    }

    /// Creates a harness with explicit runtime settings and ESC timeout
    pub fn with_config(
        app: Application,
        config: RuntimeConfig,
        escape_timeout: Duration,
        map_event: Mapper,
    ) -> Result<Self, HarnessError> {
        let clock = VirtualClock::new();
        let runtime = Runtime::with_clock(app, config, clock.clone())?;
        let decoder = TimedInputDecoder::new(clock.clone(), escape_timeout);
        let mut harness = Self {
            runtime,
            decoder,
            clock,
            map_event,
            frames: Vec::new(),
            messages: Vec::new(),
            exit_requested: false,
        };
        harness.capture_frame()?;
        Ok(harness)
    }

    /// Returns immutable application state
    #[must_use]
    pub fn app(&self) -> &Application {
        self.runtime.app()
    }

    /// Returns mutable application state and schedules a frame
    pub fn app_mut(&mut self) -> &mut Application {
        self.runtime.app_mut()
    }

    /// Returns runtime-owned Interaction State for assertions
    #[must_use]
    pub const fn interaction(&self) -> &InteractionState {
        self.runtime.interaction()
    }

    /// Requests focus for a focusable ID in the current semantic tree
    pub fn request_focus(&mut self, id: &NodeId) -> Result<bool, HarnessError> {
        Ok(self.runtime.request_focus(id)?)
    }

    /// Clears node focus
    pub fn clear_focus(&mut self) {
        self.runtime.clear_focus();
    }

    /// Sets a TextInput UTF-8 byte cursor at a grapheme boundary
    pub fn set_text_cursor(&mut self, id: &NodeId, cursor: usize) -> bool {
        self.runtime.set_text_cursor(id, cursor)
    }

    /// Requests a ScrollViewport offset that is clamped during layout
    pub fn set_scroll_offset(&mut self, id: &NodeId, offset: ScrollOffset) -> bool {
        self.runtime.set_scroll_offset(id, offset)
    }

    /// Returns resolved ScrollViewport state for assertions
    #[must_use]
    pub fn scroll_state(&self, id: &NodeId) -> Option<ScrollState> {
        self.runtime.interaction().scroll_state(id)
    }

    /// Returns resolved action groups on the active target-to-root route
    pub fn active_action_groups(&mut self) -> Result<Vec<ResolvedActions>, HarnessError> {
        Ok(self.runtime.active_action_groups()?)
    }

    /// Returns the number of supervised tasks that have not fully finished
    #[must_use]
    pub fn active_tasks(&self) -> usize {
        self.runtime.active_tasks()
    }

    /// Returns the number of tasks occupying worker slots
    #[must_use]
    pub fn running_tasks(&self) -> usize {
        self.runtime.running_tasks()
    }

    /// Returns the number of tasks waiting for a worker slot
    #[must_use]
    pub fn pending_tasks(&self) -> usize {
        self.runtime.pending_tasks()
    }

    /// Returns completed effect messages waiting for queue capacity
    #[must_use]
    pub fn pending_effect_messages(&self) -> usize {
        self.runtime.pending_effect_messages()
    }

    /// Returns supervised effect diagnostic counters
    #[must_use]
    pub const fn effect_diagnostics(&self) -> EffectDiagnostics {
        self.runtime.effect_diagnostics()
    }

    /// Returns the latest generation started for one task key
    #[must_use]
    pub fn task_generation(&self, key: &TaskKey) -> u64 {
        self.runtime.task_generation(key)
    }

    /// Returns the number of currently declared subscription sources
    #[must_use]
    pub fn active_subscriptions(&self) -> usize {
        self.runtime.active_subscriptions()
    }

    /// Returns Stream producers that have not returned
    #[must_use]
    pub fn running_subscription_streams(&self) -> usize {
        self.runtime.running_subscription_streams()
    }

    /// Returns subscription values waiting before the application queue
    #[must_use]
    pub fn pending_subscription_messages(&self) -> usize {
        self.runtime.pending_subscription_messages()
    }

    /// Reports whether one subscription key is currently declared
    #[must_use]
    pub fn subscription_active(&self, key: &SubscriptionKey) -> bool {
        self.runtime.subscription_active(key)
    }

    /// Returns the latest generation started for one subscription key
    #[must_use]
    pub fn subscription_generation(&self, key: &SubscriptionKey) -> u64 {
        self.runtime.subscription_generation(key)
    }

    /// Returns subscription lifecycle and backpressure counters
    #[must_use]
    pub fn subscription_diagnostics(&self) -> SubscriptionDiagnostics {
        self.runtime.subscription_diagnostics()
    }

    /// Returns retained asynchronous lifecycle notices
    #[must_use]
    pub fn pending_runtime_notices(&self) -> usize {
        self.runtime.pending_runtime_notices()
    }

    /// Removes and returns retained notices in occurrence order
    pub fn drain_runtime_notices(&mut self) -> Vec<RuntimeNotice> {
        self.runtime.drain_runtime_notices()
    }

    /// Returns bounded Runtime notice queue counters
    #[must_use]
    pub fn runtime_notice_diagnostics(&self) -> RuntimeNoticeDiagnostics {
        self.runtime.runtime_notice_diagnostics()
    }

    /// Injects one application message and completes one coalesced step
    pub fn send(&mut self, message: Application::Message) -> Result<(), HarnessError> {
        self.runtime.enqueue(message)?;
        self.step()
    }

    /// Injects raw terminal bytes and completes one coalesced step
    pub fn input(&mut self, bytes: &[u8]) -> Result<(), HarnessError> {
        let events = self.decoder.feed(bytes);
        self.dispatch(events)?;
        self.step()
    }

    /// Resolves incomplete terminal input and completes one coalesced step
    pub fn flush_input(&mut self) -> Result<(), HarnessError> {
        let events = self.decoder.flush();
        self.dispatch(events)?;
        self.step()
    }

    /// Advances virtual time, resolves due input deadlines, and steps once
    pub fn advance(&mut self, duration: Duration) -> Result<(), HarnessError> {
        self.clock.advance(duration);
        let events = self.decoder.poll();
        self.dispatch(events)?;
        self.step()
    }

    /// Changes virtual terminal size and renders one coalesced frame
    pub fn resize(&mut self, size: Size) -> Result<(), HarnessError> {
        self.runtime.resize(size);
        self.step()
    }

    /// Processes queued messages and captures at most one rendered frame
    pub fn step(&mut self) -> Result<(), HarnessError> {
        let messages = &mut self.messages;
        self.runtime
            .process_pending_with(|message| messages.push(message.clone()))?;
        self.capture_frame()
    }

    /// Returns every captured frame, including the initial frame
    #[must_use]
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// Returns the most recently captured frame
    #[must_use]
    pub fn latest_frame(&self) -> &Frame {
        self.frames
            .last()
            .expect("a successfully constructed harness has an initial frame")
    }

    /// Returns messages delivered to the application in injection order
    #[must_use]
    pub fn message_history(&self) -> &[Application::Message] {
        &self.messages
    }

    /// Reports whether the application or event mapper requested normal exit
    #[must_use]
    pub const fn exit_requested(&self) -> bool {
        self.exit_requested || self.runtime.exit_requested()
    }

    fn dispatch(&mut self, events: Vec<Event>) -> Result<(), HarnessError> {
        for event in events {
            let dispatch = self.runtime.dispatch_event(&event)?;
            if !dispatch.consumed() {
                match (self.map_event)(event) {
                    EventAction::Message(message) => {
                        self.runtime.enqueue(message)?;
                    }
                    EventAction::Exit => self.exit_requested = true,
                    EventAction::Ignore => {}
                }
            }
            let messages = &mut self.messages;
            self.runtime
                .process_queued_with(|message| messages.push(message.clone()))?;
            if self.exit_requested || self.runtime.exit_requested() {
                break;
            }
        }
        Ok(())
    }

    fn capture_frame(&mut self) -> Result<(), HarnessError> {
        if let Some(frame) = self.runtime.render_if_dirty()? {
            self.frames.push(frame);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use nagi_tui::{
        Action, ActionDescriptor, DeliveryPolicy, Effect, EventResult, KeyBinding, KeyCode,
        KeyStroke, Modifiers, Node, Subscription, Task,
    };

    use super::*;

    #[derive(Clone)]
    enum Message {
        Append(String),
    }

    #[derive(Default)]
    struct Echo(String);

    impl App for Echo {
        type Message = Message;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                Message::Append(text) => self.0.push_str(&text),
            }
            Effect::none()
        }

        fn subscriptions(&self) -> Subscription<Self::Message> {
            Subscription::none()
        }

        fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
            Node::text(&self.0)
        }
    }

    #[test]
    fn bytes_messages_frames_and_escape_time_are_observable() {
        let mut harness = Harness::new(Echo::default(), Size::new(4, 1), |event| match event {
            Event::Text(text) => EventAction::Message(Message::Append(text)),
            Event::Key(key) if key.code == KeyCode::Escape => EventAction::Exit,
            _ => EventAction::Ignore,
        })
        .unwrap();

        harness.input("A日".as_bytes()).unwrap();
        harness.input(b"\x1B").unwrap();
        harness.advance(Duration::from_millis(25)).unwrap();

        assert_eq!(harness.app().0, "A日");
        assert_eq!(harness.frames().len(), 2);
        assert_eq!(harness.message_history().len(), 2);
        assert!(harness.exit_requested());
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    struct ControlledInputMessage {
        value: String,
        selection: usize,
    }

    #[derive(Default)]
    struct ControlledInputApp {
        value: String,
        selection: usize,
    }

    impl App for ControlledInputApp {
        type Message = ControlledInputMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            self.value = message.value;
            self.selection = message.selection;
            Effect::none()
        }

        fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
            let value = self.value.clone();
            let selection = self.selection;
            Node::text(&self.value)
                .focusable("controlled")
                .on_event("controlled", move |event| match event {
                    Event::Text(text) => EventResult::message(ControlledInputMessage {
                        value: format!("{value}{text}"),
                        selection,
                    }),
                    Event::Key(key) if key.code == KeyCode::Backspace => {
                        let mut next = value.clone();
                        next.pop();
                        EventResult::message(ControlledInputMessage {
                            value: next,
                            selection,
                        })
                    }
                    Event::Key(key) if key.code == KeyCode::Down => {
                        EventResult::message(ControlledInputMessage {
                            value: value.clone(),
                            selection: selection + 1,
                        })
                    }
                    _ => EventResult::ignored(),
                })
        }
    }

    #[test]
    fn controlled_input_events_are_processed_sequentially() {
        let cases = [
            (
                "unicode text",
                ControlledInputApp::default(),
                "A日".as_bytes(),
                "A日",
                0,
            ),
            (
                "backspace",
                ControlledInputApp {
                    value: "abc".to_owned(),
                    selection: 0,
                },
                b"\x7f\x7f".as_slice(),
                "a",
                0,
            ),
            (
                "down",
                ControlledInputApp::default(),
                b"\x1b[B\x1b[B".as_slice(),
                "",
                2,
            ),
        ];
        for (name, app, input, expected_value, expected_selection) in cases {
            let mut harness = Harness::new(app, Size::new(8, 1), |_| EventAction::Ignore)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(
                harness.request_focus(&NodeId::from("controlled")).unwrap(),
                "{name}"
            );
            harness
                .input(input)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(harness.app().value, expected_value, "{name}");
            assert_eq!(harness.app().selection, expected_selection, "{name}");
            assert_eq!(harness.frames().len(), 2, "{name}");
        }
    }

    struct ActionProjectionApp;

    impl App for ActionProjectionApp {
        type Message = ();

        fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
            Node::text("action").focusable("owner").on_actions(
                "owner",
                [Action::new(
                    ActionDescriptor::new(
                        "app.action",
                        "Action",
                        [KeyBinding::new(KeyStroke::character('x', Modifiers::NONE))],
                    ),
                    |_| EventResult::consumed(),
                )],
            )
        }
    }

    #[test]
    fn active_action_projection_is_observable() {
        let mut harness = Harness::new(ActionProjectionApp, Size::new(8, 1), |_| {
            EventAction::Ignore
        })
        .unwrap();
        harness.request_focus(&NodeId::from("owner")).unwrap();

        let groups = harness.active_action_groups().unwrap();

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].owner().as_str(), "owner");
        assert_eq!(groups[0].actions()[0].id().as_str(), "app.action");
        assert_eq!(groups[1].owner().as_str(), "owner");
        assert_eq!(
            groups[1].actions()[0].id().as_str(),
            nagi_tui::FOCUS_NEXT_ACTION_ID
        );
    }

    struct ManualApp {
        task: Option<Task<Message>>,
        values: Vec<String>,
    }

    impl App for ManualApp {
        type Message = Message;

        fn init(&mut self) -> Effect<Self::Message> {
            let task = self.task.take().unwrap();
            Effect::run(task)
        }

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                Message::Append(value) => self.values.push(value),
            }
            Effect::none()
        }

        fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
            Node::text(self.values.join(","))
        }
    }

    #[test]
    fn manual_tasks_and_supervision_are_observable() {
        let (task, manual) = manual_task();
        let mut harness = Harness::new(
            ManualApp {
                task: Some(task),
                values: Vec::new(),
            },
            Size::new(8, 1),
            |_| EventAction::Ignore,
        )
        .unwrap();
        manual.wait_started();
        assert_eq!(harness.active_tasks(), 1);
        manual.complete(Message::Append("done".to_owned())).unwrap();
        for _ in 0..10_000 {
            harness.step().unwrap();
            if harness.active_tasks() == 0 {
                break;
            }
            thread::yield_now();
        }

        assert_eq!(harness.app().values, ["done"]);
        assert_eq!(harness.active_tasks(), 0);
    }

    #[derive(Clone)]
    enum SubscriptionMessage {
        Value(String),
        Toggle,
    }

    struct ManualSubscriptionApp {
        source: ManualSubscription<SubscriptionMessage>,
        running: bool,
        values: Vec<String>,
    }

    impl App for ManualSubscriptionApp {
        type Message = SubscriptionMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                SubscriptionMessage::Value(value) => self.values.push(value),
                SubscriptionMessage::Toggle => self.running = !self.running,
            }
            Effect::none()
        }

        fn subscriptions(&self) -> Subscription<Self::Message> {
            if self.running {
                self.source
                    .subscription("manual", DeliveryPolicy::reliable())
            } else {
                Subscription::none()
            }
        }

        fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
            Node::text(self.values.join(","))
        }
    }

    #[test]
    fn manual_subscription_lifecycle_and_messages_are_observable() {
        let source = manual_subscription();
        let mut harness = Harness::new(
            ManualSubscriptionApp {
                source: source.clone(),
                running: true,
                values: Vec::new(),
            },
            Size::new(8, 1),
            |_| EventAction::Ignore,
        )
        .unwrap();
        source.wait_started();
        assert!(source.is_active());
        source
            .send(SubscriptionMessage::Value("first".to_owned()))
            .unwrap();
        harness.step().unwrap();
        assert_eq!(harness.app().values, ["first"]);
        assert_eq!(harness.active_subscriptions(), 1);

        harness.send(SubscriptionMessage::Toggle).unwrap();
        source.wait_stopped();
        assert!(!source.is_active());
        assert_eq!(harness.active_subscriptions(), 0);
        assert_eq!(harness.message_history().len(), 2);
    }

    struct CompletedStreamHarnessApp;

    impl App for CompletedStreamHarnessApp {
        type Message = ();

        fn update(&mut self, (): Self::Message) -> Effect<Self::Message> {
            Effect::none()
        }

        fn subscriptions(&self) -> Subscription<Self::Message> {
            Subscription::stream("events", DeliveryPolicy::reliable(), |_, _| {})
        }

        fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
            Node::text("")
        }
    }

    #[test]
    fn runtime_notices_are_observable() {
        let mut harness = Harness::new(CompletedStreamHarnessApp, Size::new(1, 1), |_| {
            EventAction::Ignore
        })
        .unwrap();

        for _ in 0..10_000 {
            if harness.pending_runtime_notices() > 0 {
                break;
            }
            thread::yield_now();
        }
        let notices = harness.drain_runtime_notices();
        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices[0].kind(),
            nagi_tui::RuntimeNoticeKind::SubscriptionStreamCompleted
        );
        assert_eq!(harness.runtime_notice_diagnostics().dropped(), 0);
    }
}
