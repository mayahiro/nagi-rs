use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use nagi_surface::SurfaceError;
use nagi_vt::{Event, KeyAction, KeyCode, MouseButton, MouseKind, TerminalOp};

use crate::effect::RuntimeCommand;
use crate::renderer::operations;
use crate::routing::{FocusChange, InteractiveKind, PointerChange, TreeIndex};
use crate::subscription_supervisor::{
    SubscriptionDiagnostics, SubscriptionReconciliation, SubscriptionSupervisor, SubscriptionTag,
};
use crate::supervisor::{EffectDiagnostics, EffectSupervisor};
use crate::text_edit::{TextEdit, apply_text_edit, normalize_cursor};
use crate::{
    App, Clock, EventDispatch, EventResult, InteractionState, Node, NodeId, Point, ScrollOffset,
    Size, SubscriptionKey, Surface, SystemClock, TaskKey, Timestamp,
};

/// The default maximum number of messages waiting in a runtime queue
pub const DEFAULT_QUEUE_CAPACITY: usize = 4_096;

/// The default maximum number of effect tasks executing concurrently
pub const DEFAULT_TASK_LIMIT: usize = 64;

/// The default per-source subscription inbox capacity
pub const DEFAULT_SUBSCRIPTION_CAPACITY: usize = 256;

/// Runtime construction settings
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    /// Initial terminal cell size
    pub size: Size,
    /// Maximum number of messages waiting for sequential processing
    pub queue_capacity: usize,
    /// Maximum number of effect tasks executing concurrently
    pub task_limit: usize,
    /// Maximum pending values retained by each subscription source
    pub subscription_capacity: usize,
    /// Smallest interval between non-urgent rendered frames
    pub minimum_frame_interval: Duration,
}

impl RuntimeConfig {
    /// Creates settings with the default bounded queue capacity
    #[must_use]
    pub const fn new(size: Size) -> Self {
        Self {
            size,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            task_limit: DEFAULT_TASK_LIMIT,
            subscription_capacity: DEFAULT_SUBSCRIPTION_CAPACITY,
            minimum_frame_interval: Duration::ZERO,
        }
    }
}

/// An error while constructing or rendering a runtime
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    /// Queue capacity must be greater than zero
    ZeroQueueCapacity,
    /// Concurrent task limit must be greater than zero
    ZeroTaskLimit,
    /// Per-source subscription capacity must be greater than zero
    ZeroSubscriptionCapacity,
    /// A frame surface could not be constructed
    Surface(SurfaceError),
    /// Two nodes in one semantic tree used the same stable ID
    DuplicateNodeId(NodeId),
    /// Two subscription sources used the same stable key
    DuplicateSubscriptionKey(SubscriptionKey),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroQueueCapacity => {
                formatter.write_str("runtime queue capacity must be positive")
            }
            Self::ZeroTaskLimit => formatter.write_str("runtime task limit must be positive"),
            Self::ZeroSubscriptionCapacity => {
                formatter.write_str("runtime subscription capacity must be positive")
            }
            Self::Surface(error) => write!(formatter, "construct runtime surface: {error}"),
            Self::DuplicateNodeId(id) => write!(formatter, "duplicate NodeId {id}"),
            Self::DuplicateSubscriptionKey(key) => {
                write!(formatter, "duplicate SubscriptionKey {key}")
            }
        }
    }
}

impl Error for RuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Surface(error) => Some(error),
            Self::ZeroQueueCapacity
            | Self::ZeroTaskLimit
            | Self::ZeroSubscriptionCapacity
            | Self::DuplicateNodeId(_)
            | Self::DuplicateSubscriptionKey(_) => None,
        }
    }
}

impl From<SurfaceError> for RuntimeError {
    fn from(error: SurfaceError) -> Self {
        Self::Surface(error)
    }
}

/// An error returned when the bounded message queue is full
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueFull;

impl fmt::Display for QueueFull {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("runtime message queue is full")
    }
}

impl Error for QueueFull {}

/// An error while routing a normalized event through a runtime tree
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeEventError {
    /// The semantic tree could not be reconciled
    Runtime(RuntimeError),
    /// A handler could not enqueue its application message
    QueueFull,
}

impl fmt::Display for RuntimeEventError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => error.fmt(formatter),
            Self::QueueFull => QueueFull.fmt(formatter),
        }
    }
}

impl Error for RuntimeEventError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Runtime(error) => Some(error),
            Self::QueueFull => None,
        }
    }
}

impl From<RuntimeError> for RuntimeEventError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl From<QueueFull> for RuntimeEventError {
    fn from(_: QueueFull) -> Self {
        Self::QueueFull
    }
}

/// One coalesced rendered frame
#[derive(Clone, Debug)]
pub struct Frame {
    timestamp: Timestamp,
    surface: Arc<Surface>,
    operations: Vec<TerminalOp>,
}

impl Frame {
    /// Returns the monotonic time at which the frame was produced
    #[must_use]
    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Returns the normalized rendered surface
    #[must_use]
    pub fn surface(&self) -> &Surface {
        self.surface.as_ref()
    }

    /// Returns typed VT operations relative to the previous frame
    #[must_use]
    pub fn operations(&self) -> &[TerminalOp] {
        &self.operations
    }
}

/// A single-threaded application runtime with a bounded FIFO message queue
pub struct Runtime<Application: App, C: Clock = SystemClock> {
    app: Application,
    clock: C,
    size: Size,
    queue: VecDeque<QueuedMessage<Application::Message>>,
    queue_capacity: usize,
    dirty: bool,
    urgent_frame: bool,
    minimum_frame_interval: Duration,
    last_frame: Option<Timestamp>,
    previous_surface: Option<Arc<Surface>>,
    interaction: InteractionState,
    view_tree: Option<Node<Application::Message>>,
    tree_index: TreeIndex,
    effects: EffectSupervisor<Application::Message>,
    subscriptions: SubscriptionSupervisor<Application::Message>,
    subscriptions_dirty: bool,
    exit_requested: bool,
    pending_focus: Option<NodeId>,
    pending_scroll: Vec<(NodeId, ScrollOffset)>,
}

struct QueuedMessage<Message> {
    message: Message,
    subscription: Option<SubscriptionTag>,
}

impl<Application: App> Runtime<Application, SystemClock> {
    /// Creates a runtime using a production monotonic clock
    pub fn new(app: Application, size: Size) -> Result<Self, RuntimeError> {
        Self::with_clock(app, RuntimeConfig::new(size), SystemClock::new())
    }
}

impl<Application: App, C: Clock> Runtime<Application, C> {
    /// Creates a runtime using explicit settings and clock
    pub fn with_clock(
        mut app: Application,
        config: RuntimeConfig,
        clock: C,
    ) -> Result<Self, RuntimeError> {
        if config.queue_capacity == 0 {
            return Err(RuntimeError::ZeroQueueCapacity);
        }
        if config.task_limit == 0 {
            return Err(RuntimeError::ZeroTaskLimit);
        }
        if config.subscription_capacity == 0 {
            return Err(RuntimeError::ZeroSubscriptionCapacity);
        }
        let startup = app.init();
        let declared_subscriptions = app.subscriptions();
        let mut effects = EffectSupervisor::new(config.task_limit);
        effects.schedule(startup, clock.now());
        let mut subscriptions = SubscriptionSupervisor::new(config.subscription_capacity);
        subscriptions
            .reconcile(declared_subscriptions, clock.now())
            .map_err(RuntimeError::DuplicateSubscriptionKey)?;
        let mut runtime = Self {
            app,
            clock,
            size: config.size,
            queue: VecDeque::with_capacity(config.queue_capacity.min(64)),
            queue_capacity: config.queue_capacity,
            dirty: true,
            urgent_frame: true,
            minimum_frame_interval: config.minimum_frame_interval,
            last_frame: None,
            previous_surface: None,
            interaction: InteractionState::new(),
            view_tree: None,
            tree_index: TreeIndex::default(),
            effects,
            subscriptions,
            subscriptions_dirty: false,
            exit_requested: false,
            pending_focus: None,
            pending_scroll: Vec::new(),
        };
        runtime.apply_effect_commands();
        Ok(runtime)
    }

    /// Returns immutable application state
    #[must_use]
    pub const fn app(&self) -> &Application {
        &self.app
    }

    /// Returns mutable application state and schedules a frame
    pub fn app_mut(&mut self) -> &mut Application {
        self.dirty = true;
        self.subscriptions_dirty = true;
        &mut self.app
    }

    /// Consumes the runtime and returns its application state
    #[must_use]
    pub fn into_app(self) -> Application {
        self.app
    }

    /// Returns runtime-owned Interaction State
    #[must_use]
    pub const fn interaction(&self) -> &InteractionState {
        &self.interaction
    }

    /// Returns the current terminal cell size
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Changes terminal size and schedules a frame when it differs
    pub fn resize(&mut self, size: Size) {
        if self.size != size {
            self.size = size;
            self.dirty = true;
            self.urgent_frame = true;
        }
    }

    /// Adds one message to the FIFO queue
    pub fn enqueue(&mut self, message: Application::Message) -> Result<(), QueueFull> {
        if self.queue.len() >= self.queue_capacity {
            return Err(QueueFull);
        }
        self.queue.push_back(QueuedMessage {
            message,
            subscription: None,
        });
        Ok(())
    }

    /// Returns the number of queued messages
    #[must_use]
    pub fn queued_messages(&self) -> usize {
        self.queue.len()
    }

    /// Polls due timers and completed tasks into the bounded message queue
    pub fn poll_effects(&mut self) -> usize {
        self.effects.poll(self.clock.now());
        self.apply_effect_commands();
        let available = self.queue_capacity.saturating_sub(self.queue.len());
        let messages = self.effects.take_ready(available);
        let count = messages.len();
        self.queue
            .extend(messages.into_iter().map(|message| QueuedMessage {
                message,
                subscription: None,
            }));
        count
    }

    /// Reports whether the application requested normal exit
    #[must_use]
    pub const fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    /// Polls subscriptions and moves ready values into the bounded queue
    pub fn poll_subscriptions(&mut self) -> usize {
        self.subscriptions.poll(self.clock.now());
        let available = self.queue_capacity.saturating_sub(self.queue.len());
        let messages = self.subscriptions.take_ready(available);
        let count = messages.len();
        self.queue
            .extend(messages.into_iter().map(|delivery| QueuedMessage {
                message: delivery.message,
                subscription: Some(delivery.tag),
            }));
        count
    }

    /// Returns the number of supervised tasks that have not fully finished
    #[must_use]
    pub fn active_tasks(&self) -> usize {
        self.effects.active_tasks()
    }

    /// Returns the number of supervised tasks occupying worker slots
    #[must_use]
    pub fn running_tasks(&self) -> usize {
        self.effects.running_tasks()
    }

    /// Returns the number of supervised tasks waiting for a worker slot
    #[must_use]
    pub fn pending_tasks(&self) -> usize {
        self.effects.pending_tasks()
    }

    /// Returns completed effect messages waiting for queue capacity
    #[must_use]
    pub fn pending_effect_messages(&self) -> usize {
        self.effects.ready_messages()
    }

    /// Returns counters for cancellation, stale suppression, and task failure
    #[must_use]
    pub const fn effect_diagnostics(&self) -> EffectDiagnostics {
        self.effects.diagnostics()
    }

    /// Returns the latest generation started for one task key
    #[must_use]
    pub fn task_generation(&self, key: &TaskKey) -> u64 {
        self.effects.generation(key)
    }

    /// Returns the number of currently declared subscription sources
    #[must_use]
    pub fn active_subscriptions(&self) -> usize {
        self.subscriptions.active_subscriptions()
    }

    /// Returns Stream producers that have not returned
    #[must_use]
    pub fn running_subscription_streams(&self) -> usize {
        self.subscriptions.running_streams()
    }

    /// Returns subscription values waiting before the application queue
    #[must_use]
    pub fn pending_subscription_messages(&self) -> usize {
        self.subscriptions.pending_messages()
    }

    /// Reports whether one subscription key is currently declared
    #[must_use]
    pub fn subscription_active(&self, key: &SubscriptionKey) -> bool {
        self.subscriptions.is_active(key)
    }

    /// Returns the latest started generation for one subscription key
    #[must_use]
    pub fn subscription_generation(&self, key: &SubscriptionKey) -> u64 {
        self.subscriptions.generation(key)
    }

    /// Returns subscription lifecycle and backpressure counters
    #[must_use]
    pub fn subscription_diagnostics(&self) -> SubscriptionDiagnostics {
        self.subscriptions.diagnostics()
    }

    /// Returns time until the next clock-driven effect deadline
    #[must_use]
    pub fn time_until_effect_deadline(&self) -> Option<Duration> {
        self.effects.time_until_deadline(self.clock.now())
    }

    /// Returns time until the next clock-driven subscription deadline
    #[must_use]
    pub fn time_until_subscription_deadline(&self) -> Option<Duration> {
        self.subscriptions.time_until_deadline(self.clock.now())
    }

    /// Returns time until a pending rate-limited frame may be rendered
    #[must_use]
    pub fn time_until_frame_deadline(&self) -> Option<Duration> {
        if !self.dirty || self.urgent_frame || self.minimum_frame_interval.is_zero() {
            return None;
        }
        self.last_frame.map(|last_frame| {
            let deadline = last_frame.saturating_add(self.minimum_frame_interval);
            Duration::from_nanos(
                deadline
                    .as_nanos()
                    .saturating_sub(self.clock.now().as_nanos()),
            )
        })
    }

    /// Applies every queued message in FIFO order without rendering between
    /// messages
    pub fn process_pending(&mut self) -> Result<usize, RuntimeError> {
        self.process_pending_with(|_| {})
    }

    /// Applies queued messages and observes each immediately before update
    pub fn process_pending_with(
        &mut self,
        mut observe: impl FnMut(&Application::Message),
    ) -> Result<usize, RuntimeError> {
        self.reconcile_subscriptions()?;
        self.poll_effects();
        self.poll_subscriptions();
        let mut processed = 0;
        while let Some(queued) = self.queue.pop_front() {
            observe(&queued.message);
            let effect = self.app.update(queued.message);
            self.effects.schedule(effect, self.clock.now());
            self.apply_effect_commands();
            self.dirty = true;
            self.subscriptions_dirty = true;
            self.reconcile_subscriptions()?;
            processed += 1;
        }
        Ok(processed)
    }

    fn apply_effect_commands(&mut self) {
        for command in self.effects.take_commands() {
            match command {
                RuntimeCommand::Exit => self.exit_requested = true,
                RuntimeCommand::Focus(id) => self.pending_focus = Some(id),
                RuntimeCommand::ScrollTo { id, offset } => {
                    self.pending_scroll.push((id, offset));
                }
            }
            self.dirty = true;
            self.urgent_frame = true;
        }
    }

    fn apply_pending_interaction(&mut self, index: &TreeIndex) {
        if let Some(id) = self.pending_focus.take() {
            if index.allows_focus(&id) {
                self.interaction.focused = Some(id);
            }
        }
        for (id, offset) in self.pending_scroll.drain(..) {
            if index
                .record(&id)
                .is_some_and(|record| record.kind == InteractiveKind::ScrollViewport)
            {
                self.interaction.request_scroll(&id, offset);
            }
        }
    }

    /// Schedules a frame even when application state has not changed
    pub fn request_frame(&mut self) {
        self.dirty = true;
        self.urgent_frame = true;
        self.subscriptions_dirty = true;
    }

    /// Requests focus for a focusable ID in the current semantic tree
    pub fn request_focus(&mut self, id: &NodeId) -> Result<bool, RuntimeError> {
        self.ensure_tree()?;
        if !self.tree_index.allows_focus(id) {
            return Ok(false);
        }
        if self.interaction.focused.as_ref() != Some(id) {
            self.interaction.focused = Some(id.clone());
            self.dirty = true;
            self.urgent_frame = true;
        }
        Ok(true)
    }

    pub(crate) fn focus_first(&mut self) -> Result<bool, RuntimeError> {
        self.ensure_tree()?;
        let Some(first) = self.tree_index.focus_scope().into_iter().next() else {
            return Ok(false);
        };
        self.request_focus(&first)
    }

    /// Clears node focus and schedules a frame when focus existed
    pub fn clear_focus(&mut self) {
        if self.interaction.focused.take().is_some() {
            self.dirty = true;
            self.urgent_frame = true;
        }
    }

    /// Sets a TextInput UTF-8 byte cursor, normalizing to a grapheme boundary
    pub fn set_text_cursor(&mut self, id: &NodeId, cursor: usize) -> bool {
        let Some(state) = self.interaction.text_inputs.get_mut(id) else {
            return false;
        };
        let normalized = normalize_cursor(&state.draft, cursor);
        if state.cursor != normalized {
            state.cursor = normalized;
            self.dirty = true;
            self.urgent_frame = true;
        }
        true
    }

    /// Requests a ScrollViewport offset that is clamped during layout
    pub fn set_scroll_offset(&mut self, id: &NodeId, offset: ScrollOffset) -> bool {
        if self.interaction.scroll_state(id).is_none() {
            return false;
        }
        if self
            .interaction
            .request_scroll(id, offset)
            .is_some_and(|(_, changed)| changed)
        {
            self.dirty = true;
            self.urgent_frame = true;
        }
        true
    }

    /// Routes one normalized event through focus, hit testing, and ancestors
    pub fn dispatch_event(&mut self, event: &Event) -> Result<EventDispatch, RuntimeEventError> {
        self.ensure_tree()?;
        if let Event::Key(key) = event {
            if key.action != KeyAction::Release
                && key.code == KeyCode::Tab
                && !key.modifiers.alt
                && !key.modifiers.control
                && !key.modifiers.meta
            {
                self.interaction.focused = crate::interaction::traverse_focus(
                    &self.tree_index.focus_scope(),
                    self.interaction.focused.as_ref(),
                    !key.modifiers.shift,
                );
                self.dirty = true;
                self.urgent_frame = true;
                return Ok(EventDispatch {
                    consumed: true,
                    messages: 0,
                    redraw: true,
                });
            }
        }

        let target = match event {
            Event::Mouse(mouse) => self.interaction.pointer_capture.clone().or_else(|| {
                self.tree_index.hit_test(Point::new(
                    i32::try_from(mouse.x).unwrap_or(i32::MAX),
                    i32::try_from(mouse.y).unwrap_or(i32::MAX),
                ))
            }),
            _ => self.interaction.focused.clone(),
        };
        if matches!(event, Event::Mouse(mouse) if mouse.kind == MouseKind::Press) {
            if let Some(id) = &target {
                if self.tree_index.allows_focus(id) {
                    self.interaction.focused = Some(id.clone());
                    self.dirty = true;
                    self.urgent_frame = true;
                }
            }
        }

        let route = self.tree_index.route(target.as_ref());
        let mut dispatch = EventDispatch::default();
        for (index, id) in route.into_iter().enumerate() {
            let kind = self
                .tree_index
                .record(&id)
                .map_or(InteractiveKind::Generic, |record| record.kind);
            let special = match kind {
                InteractiveKind::TextInput if index == 0 => self.handle_text_input(&id, event),
                InteractiveKind::ScrollViewport => self.handle_scroll(&id, event),
                InteractiveKind::Generic | InteractiveKind::TextInput | InteractiveKind::Modal => {
                    None
                }
            };
            if let Some(result) = special {
                self.apply_event_result(result, &mut dispatch)?;
                if dispatch.consumed {
                    break;
                }
            }
            let result = self
                .view_tree
                .as_ref()
                .and_then(|view| view.handle_event(&id, event));
            if let Some(result) = result {
                self.apply_event_result(result, &mut dispatch)?;
                if dispatch.consumed {
                    break;
                }
            }
        }
        Ok(dispatch)
    }

    fn handle_text_input(
        &mut self,
        id: &NodeId,
        event: &Event,
    ) -> Option<EventResult<Application::Message>> {
        let edit = match event {
            Event::Text(text) => TextEdit::Insert(text),
            Event::Paste(text) => TextEdit::Paste(text),
            Event::Key(key) if key.action != KeyAction::Release => match key.code {
                KeyCode::Left => TextEdit::Left,
                KeyCode::Right => TextEdit::Right,
                KeyCode::Home => TextEdit::Home,
                KeyCode::End => TextEdit::End,
                KeyCode::Backspace => TextEdit::Backspace,
                KeyCode::Delete => TextEdit::Delete,
                KeyCode::Character(_) if !key.modifiers.control && !key.modifiers.meta => {
                    TextEdit::Insert(key.text.as_deref()?)
                }
                _ => return None,
            },
            _ => return None,
        };
        let (changed, value) = {
            let state = self.interaction.text_inputs.get_mut(id)?;
            let previous = state.draft.clone();
            let (value, cursor) = apply_text_edit(&previous, state.cursor, edit);
            state.cursor = cursor;
            state.draft = value.clone();
            (value != previous, value)
        };
        self.dirty = true;
        self.urgent_frame = true;
        let mut result = EventResult::consumed().redraw();
        if changed {
            let message = self
                .view_tree
                .as_ref()
                .and_then(|view| view.text_input_message(id, value))?;
            result = result.emit(message);
        }
        Some(result)
    }

    fn handle_scroll(
        &mut self,
        id: &NodeId,
        event: &Event,
    ) -> Option<EventResult<Application::Message>> {
        let state = self.interaction.scroll_state(id)?;
        let axis = self.view_tree.as_ref()?.scroll_options(id)?.axis;
        let viewport = self.tree_index.record(id)?.rect;
        let current = state.offset;
        let next = match event {
            Event::Mouse(mouse) if mouse.kind == MouseKind::Scroll => match mouse.button {
                MouseButton::WheelUp if axis.allows_vertical() => {
                    ScrollOffset::new(current.x, current.y.saturating_sub(3))
                }
                MouseButton::WheelDown if axis.allows_vertical() => {
                    ScrollOffset::new(current.x, current.y.saturating_add(3))
                }
                MouseButton::WheelLeft if axis.allows_horizontal() => {
                    ScrollOffset::new(current.x.saturating_sub(3), current.y)
                }
                MouseButton::WheelRight if axis.allows_horizontal() => {
                    ScrollOffset::new(current.x.saturating_add(3), current.y)
                }
                _ => return None,
            },
            Event::Key(key) if key.action != KeyAction::Release => match key.code {
                KeyCode::PageUp if axis.allows_vertical() => {
                    ScrollOffset::new(current.x, current.y.saturating_sub(viewport.height.max(1)))
                }
                KeyCode::PageDown if axis.allows_vertical() => {
                    ScrollOffset::new(current.x, current.y.saturating_add(viewport.height.max(1)))
                }
                KeyCode::Home if axis.allows_vertical() => ScrollOffset::new(current.x, 0),
                KeyCode::End if axis.allows_vertical() => {
                    ScrollOffset::new(current.x, state.maximum.y)
                }
                KeyCode::Home if axis.allows_horizontal() => ScrollOffset::new(0, current.y),
                KeyCode::End if axis.allows_horizontal() => {
                    ScrollOffset::new(state.maximum.x, current.y)
                }
                _ => return None,
            },
            _ => return None,
        };
        let (state, changed) = self.interaction.request_scroll(id, next)?;
        self.dirty = true;
        self.urgent_frame = true;
        let mut result = EventResult::consumed().redraw();
        if changed {
            if let Some(message) = self
                .view_tree
                .as_ref()
                .and_then(|view| view.scroll_message(id, state))
            {
                result = result.emit(message);
            }
        }
        Some(result)
    }

    fn apply_event_result(
        &mut self,
        result: EventResult<Application::Message>,
        dispatch: &mut EventDispatch,
    ) -> Result<(), QueueFull> {
        match result.focus {
            FocusChange::Unchanged => {}
            FocusChange::Focus(id) => {
                if self.tree_index.allows_focus(&id) {
                    self.interaction.focused = Some(id);
                    self.dirty = true;
                    self.urgent_frame = true;
                }
            }
            FocusChange::Release => {
                self.interaction.focused = None;
                self.dirty = true;
                self.urgent_frame = true;
            }
        }
        match result.pointer {
            PointerChange::Unchanged => {}
            PointerChange::Capture(id) => {
                if self.tree_index.allows_interaction(&id) {
                    self.interaction.pointer_capture = Some(id);
                }
            }
            PointerChange::Release => self.interaction.pointer_capture = None,
        }
        for message in result.messages {
            self.enqueue(message)?;
            dispatch.messages += 1;
        }
        dispatch.consumed |= result.consumed;
        dispatch.redraw |= result.redraw;
        if result.redraw {
            self.dirty = true;
            self.urgent_frame = true;
        }
        Ok(())
    }

    fn reconcile_subscriptions(&mut self) -> Result<(), RuntimeError> {
        if !self.subscriptions_dirty {
            return Ok(());
        }
        let declared = self.app.subscriptions();
        let SubscriptionReconciliation { stopped } = self
            .subscriptions
            .reconcile(declared, self.clock.now())
            .map_err(RuntimeError::DuplicateSubscriptionKey)?;
        self.subscriptions_dirty = false;
        if stopped.is_empty() {
            return Ok(());
        }
        let before = self.queue.len();
        self.queue.retain(|queued| {
            !queued
                .subscription
                .as_ref()
                .is_some_and(|tag| stopped.contains(tag))
        });
        self.subscriptions
            .note_discarded(before.saturating_sub(self.queue.len()));
        Ok(())
    }

    fn ensure_focused_visible(
        &mut self,
        view: &Node<Application::Message>,
        mut index: TreeIndex,
    ) -> Result<TreeIndex, RuntimeError> {
        let Some(focused) = self.interaction.focused.clone() else {
            return Ok(index);
        };
        let scrolls: Vec<_> = index
            .route(Some(&focused))
            .into_iter()
            .filter(|id| {
                view.scroll_options(id)
                    .is_some_and(|options| options.ensure_focused_visible)
            })
            .collect();
        for id in scrolls {
            let Some(target) = index.record(&focused).cloned() else {
                break;
            };
            let Some(viewport) = index.record(&id).cloned() else {
                continue;
            };
            let Some(axis) = view.scroll_options(&id).map(|options| options.axis) else {
                continue;
            };
            let Some(state) = self.interaction.scroll_state(&id) else {
                continue;
            };
            let mut next = state.offset;
            if axis.allows_horizontal() {
                next.x = visible_axis_offset(
                    next.x,
                    viewport.rect.x,
                    viewport.rect.width,
                    target.rect.x,
                    target.rect.width,
                );
            }
            if axis.allows_vertical() {
                next.y = visible_axis_offset(
                    next.y,
                    viewport.rect.y,
                    viewport.rect.height,
                    target.rect.y,
                    target.rect.height,
                );
            }
            if next == state.offset {
                continue;
            }
            self.interaction.request_scroll(&id, next);
            view.prepare_interaction(self.size, &mut self.interaction);
            index = view
                .build_tree_index(self.size, &self.interaction)
                .map_err(RuntimeError::DuplicateNodeId)?;
        }
        Ok(index)
    }

    fn ensure_tree(&mut self) -> Result<(), RuntimeError> {
        if self.view_tree.is_some() {
            return Ok(());
        }
        let view = self.app.view(crate::ViewContext::new(self.size));
        let initial_index = view
            .build_tree_index(self.size, &self.interaction)
            .map_err(RuntimeError::DuplicateNodeId)?;
        self.interaction
            .reconcile(&initial_index.active, &[], &initial_index.focus_scope());
        if self
            .interaction
            .pointer_capture
            .as_ref()
            .is_some_and(|id| !initial_index.allows_interaction(id))
        {
            self.interaction.pointer_capture = None;
        }
        self.apply_pending_interaction(&initial_index);
        view.prepare_interaction(self.size, &mut self.interaction);
        let tree_index = view
            .build_tree_index(self.size, &self.interaction)
            .map_err(RuntimeError::DuplicateNodeId)?;
        self.tree_index = self.ensure_focused_visible(&view, tree_index)?;
        self.view_tree = Some(view);
        Ok(())
    }

    /// Renders one frame when requested or state changed
    pub fn render_if_dirty(&mut self) -> Result<Option<Frame>, RuntimeError> {
        self.reconcile_subscriptions()?;
        if !self.dirty {
            return Ok(None);
        }
        let now = self.clock.now();
        if !self.urgent_frame && !self.minimum_frame_interval.is_zero() {
            if let Some(last_frame) = self.last_frame {
                if now < last_frame.saturating_add(self.minimum_frame_interval) {
                    return Ok(None);
                }
            }
        }
        let view = self.app.view(crate::ViewContext::new(self.size));
        let initial_index = view
            .build_tree_index(self.size, &self.interaction)
            .map_err(RuntimeError::DuplicateNodeId)?;
        let previous_focus_order = self.tree_index.focus_scope();
        self.interaction.reconcile(
            &initial_index.active,
            &previous_focus_order,
            &initial_index.focus_scope(),
        );
        if self
            .interaction
            .pointer_capture
            .as_ref()
            .is_some_and(|id| !initial_index.allows_interaction(id))
        {
            self.interaction.pointer_capture = None;
        }
        self.apply_pending_interaction(&initial_index);
        view.prepare_interaction(self.size, &mut self.interaction);
        let tree_index = view
            .build_tree_index(self.size, &self.interaction)
            .map_err(RuntimeError::DuplicateNodeId)?;
        let tree_index = self.ensure_focused_visible(&view, tree_index)?;
        let mut surface = Surface::new(self.size.width, self.size.height)?;
        view.render_to(&mut surface, &self.interaction);
        let frame_operations = operations(self.previous_surface.as_deref(), &surface);
        let surface = Arc::new(surface);
        self.previous_surface = Some(Arc::clone(&surface));
        self.view_tree = Some(view);
        self.tree_index = tree_index;
        self.dirty = false;
        self.urgent_frame = false;
        self.last_frame = Some(now);
        Ok(Some(Frame {
            timestamp: now,
            surface,
            operations: frame_operations,
        }))
    }

    /// Processes all queued messages and produces at most one frame
    pub fn step(&mut self) -> Result<Option<Frame>, RuntimeError> {
        self.process_pending()?;
        self.render_if_dirty()
    }
}

fn visible_axis_offset(
    current: u32,
    viewport_start: i32,
    viewport_size: u32,
    target_start: i32,
    target_size: u32,
) -> u32 {
    if viewport_size == 0 || target_size == 0 {
        return current;
    }
    let viewport_start = i64::from(viewport_start);
    let viewport_end = viewport_start + i64::from(viewport_size);
    let target_start = i64::from(target_start);
    let target_end = target_start + i64::from(target_size);
    if target_start < viewport_start {
        current.saturating_sub((viewport_start - target_start).min(i64::from(u32::MAX)) as u32)
    } else if target_end > viewport_end {
        current.saturating_add((target_end - viewport_end).min(i64::from(u32::MAX)) as u32)
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
    use std::thread;
    use std::time::Duration;

    use crate::fixture_support;
    use crate::{
        CancelToken, DeliveryPolicy, Effect, EventResult, Insets, KeyEvent, KeyProtocol, Modifiers,
        Node, NodeId, ScrollOffset, Subscription, SubscriptionKey, Task, TaskKey, VirtualClock,
    };

    use super::*;

    enum Message {
        Add(u32),
    }

    struct Counter {
        value: u32,
        updates: Vec<u32>,
    }

    impl App for Counter {
        type Message = Message;

        fn init(&mut self) -> Effect<Self::Message> {
            self.value = 1;
            Effect::none()
        }

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                Message::Add(value) => {
                    self.value += value;
                    self.updates.push(value);
                }
            }
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            Node::text(self.value.to_string())
        }
    }

    #[test]
    fn messages_are_fifo_and_rendering_is_coalesced() {
        let clock = VirtualClock::new();
        let mut runtime = Runtime::with_clock(
            Counter {
                value: 0,
                updates: Vec::new(),
            },
            RuntimeConfig::new(Size::new(3, 1)),
            clock.clone(),
        )
        .unwrap();
        runtime.enqueue(Message::Add(2)).unwrap();
        runtime.enqueue(Message::Add(3)).unwrap();
        clock.advance(Duration::from_millis(7));

        let frame = runtime.step().unwrap().unwrap();

        assert_eq!(runtime.app().updates, [2, 3]);
        assert_eq!(frame.surface().cell(0, 0).unwrap().content(), "6");
        assert_eq!(frame.timestamp().as_nanos(), 7_000_000);
        assert!(runtime.render_if_dirty().unwrap().is_none());
    }

    #[test]
    fn frame_rate_and_urgent_coalescing_match_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "subscriptions/frame-coalescing.txt",
            "runtime-frame-coalescing",
            &["interval", "actions", "updates", "frames"],
        ) else {
            return;
        };
        for record in records {
            let clock = VirtualClock::new();
            let mut config = RuntimeConfig::new(Size::new(3, 1));
            config.minimum_frame_interval =
                Duration::from_millis(record.field("interval").parse().unwrap());
            let mut runtime = Runtime::with_clock(
                Counter {
                    value: 0,
                    updates: Vec::new(),
                },
                config,
                clock.clone(),
            )
            .unwrap();
            let mut frames = vec![
                runtime
                    .render_if_dirty()
                    .unwrap()
                    .unwrap()
                    .timestamp()
                    .as_nanos()
                    / 1_000_000,
            ];
            for action in record.field("actions").split(';') {
                let (kind, value) = action.split_once(':').unwrap_or((action, ""));
                match kind {
                    "enqueue" => runtime
                        .enqueue(Message::Add(value.parse().unwrap()))
                        .unwrap(),
                    "advance" => {
                        clock.advance(Duration::from_millis(value.parse().unwrap()));
                    }
                    "urgent" => runtime.request_frame(),
                    "step" => {
                        if let Some(frame) = runtime.step().unwrap() {
                            frames.push(frame.timestamp().as_nanos() / 1_000_000);
                        }
                    }
                    _ => panic!("unknown action {action}"),
                }
            }
            assert_eq!(
                runtime
                    .app()
                    .updates
                    .iter()
                    .copied()
                    .map(u64::from)
                    .collect::<Vec<_>>(),
                decimal_list(record.field("updates")),
                "case {}",
                record.id
            );
            assert_eq!(
                frames,
                decimal_list(record.field("frames")),
                "case {}",
                record.id
            );
        }
    }

    enum SubscriptionMessage {
        Toggle,
        Tick(u64),
    }

    struct SubscriptionApp {
        running: bool,
        counter: std::sync::Arc<std::sync::atomic::AtomicU64>,
        values: Vec<u64>,
    }

    impl App for SubscriptionApp {
        type Message = SubscriptionMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                SubscriptionMessage::Toggle => self.running = !self.running,
                SubscriptionMessage::Tick(value) => self.values.push(value),
            }
            Effect::none()
        }

        fn subscriptions(&self) -> Subscription<Self::Message> {
            if !self.running {
                return Subscription::none();
            }
            let counter = std::sync::Arc::clone(&self.counter);
            Subscription::every(
                "ticks",
                Duration::from_millis(10),
                DeliveryPolicy::latest(),
                move || {
                    SubscriptionMessage::Tick(
                        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1,
                    )
                },
            )
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            Node::text(format!("{}", self.values.len()))
        }
    }

    #[test]
    fn pause_discards_queued_subscription_values_and_resume_restarts_generation() {
        let clock = VirtualClock::new();
        let mut runtime = Runtime::with_clock(
            SubscriptionApp {
                running: true,
                counter: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
                values: Vec::new(),
            },
            RuntimeConfig::new(Size::new(2, 1)),
            clock.clone(),
        )
        .unwrap();
        let key = SubscriptionKey::from("ticks");
        assert_eq!(runtime.subscription_generation(&key), 1);

        clock.advance(Duration::from_millis(10));
        runtime.enqueue(SubscriptionMessage::Toggle).unwrap();
        assert_eq!(runtime.poll_subscriptions(), 1);
        runtime.process_pending().unwrap();
        assert!(runtime.app().values.is_empty());
        assert!(!runtime.subscription_active(&key));
        assert_eq!(runtime.subscription_diagnostics().discarded_messages(), 1);

        clock.advance(Duration::from_millis(10));
        runtime.process_pending().unwrap();
        assert!(runtime.app().values.is_empty());

        runtime.enqueue(SubscriptionMessage::Toggle).unwrap();
        runtime.process_pending().unwrap();
        assert!(runtime.subscription_active(&key));
        assert_eq!(runtime.subscription_generation(&key), 2);
        clock.advance(Duration::from_millis(10));
        runtime.process_pending().unwrap();
        assert_eq!(runtime.app().values, [2]);
    }

    fn decimal_list(value: &str) -> Vec<u64> {
        if value == "-" {
            Vec::new()
        } else {
            value.split(',').map(|part| part.parse().unwrap()).collect()
        }
    }

    #[test]
    fn queue_capacity_is_enforced_before_update() {
        let mut config = RuntimeConfig::new(Size::new(1, 1));
        config.queue_capacity = 1;
        let mut runtime = Runtime::with_clock(
            Counter {
                value: 0,
                updates: Vec::new(),
            },
            config,
            VirtualClock::new(),
        )
        .unwrap();

        runtime.enqueue(Message::Add(1)).unwrap();

        assert_eq!(runtime.enqueue(Message::Add(2)), Err(QueueFull));
        assert_eq!(runtime.queued_messages(), 1);
    }

    enum AsyncMessage {
        StartOld,
        StartNew,
        Result(&'static str),
    }

    struct AsyncApp {
        old: Option<Task<AsyncMessage>>,
        new: Option<Task<AsyncMessage>>,
        results: Vec<&'static str>,
    }

    impl App for AsyncApp {
        type Message = AsyncMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                AsyncMessage::StartOld => {
                    let task = self.old.take().unwrap();
                    Effect::latest("search", task)
                }
                AsyncMessage::StartNew => {
                    let task = self.new.take().unwrap();
                    Effect::latest("search", task)
                }
                AsyncMessage::Result(value) => {
                    self.results.push(value);
                    Effect::none()
                }
            }
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            Node::text(self.results.join(","))
        }
    }

    struct AsyncControl {
        started: Receiver<CancelToken>,
        complete: SyncSender<AsyncMessage>,
        returned: Receiver<()>,
    }

    fn async_task() -> (Task<AsyncMessage>, AsyncControl) {
        let (started_sender, started) = sync_channel(1);
        let (complete, complete_receiver) = sync_channel(1);
        let (returned_sender, returned) = sync_channel(1);
        (
            Box::new(move |token| {
                started_sender.send(token).unwrap();
                let message = complete_receiver.recv().unwrap();
                returned_sender.send(()).unwrap();
                message
            }),
            AsyncControl {
                started,
                complete,
                returned,
            },
        )
    }

    fn wait_async_started(control: &AsyncControl) -> CancelToken {
        for _ in 0..10_000 {
            match control.started.try_recv() {
                Ok(token) => return token,
                Err(TryRecvError::Empty) => thread::yield_now(),
                Err(TryRecvError::Disconnected) => panic!("async task did not start"),
            }
        }
        panic!("async task did not start")
    }

    fn complete_async(control: &AsyncControl, message: AsyncMessage) {
        control.complete.send(message).unwrap();
        control.returned.recv().unwrap();
    }

    #[test]
    fn runtime_never_delivers_stale_latest_results_to_the_app() {
        let (old_task, old) = async_task();
        let (new_task, new) = async_task();
        let mut runtime = Runtime::with_clock(
            AsyncApp {
                old: Some(old_task),
                new: Some(new_task),
                results: Vec::new(),
            },
            RuntimeConfig::new(Size::new(8, 1)),
            VirtualClock::new(),
        )
        .unwrap();

        runtime.enqueue(AsyncMessage::StartOld).unwrap();
        runtime.process_pending().unwrap();
        let old_token = wait_async_started(&old);
        runtime.enqueue(AsyncMessage::StartNew).unwrap();
        runtime.process_pending().unwrap();
        let _new_token = wait_async_started(&new);
        assert!(old_token.is_cancelled());

        complete_async(&old, AsyncMessage::Result("old"));
        complete_async(&new, AsyncMessage::Result("new"));
        for _ in 0..10_000 {
            runtime.poll_effects();
            if runtime.active_tasks() == 0 {
                break;
            }
            thread::yield_now();
        }
        assert_eq!(runtime.active_tasks(), 0);
        runtime.process_pending().unwrap();

        assert_eq!(runtime.app().results, ["new"]);
        assert_eq!(runtime.task_generation(&TaskKey::from("search")), 2);
        assert_eq!(runtime.effect_diagnostics().stale_results(), 1);
    }

    enum InputMessage {
        Change(String),
    }

    #[derive(Default)]
    struct InputApp {
        value: String,
    }

    impl App for InputApp {
        type Message = InputMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                InputMessage::Change(value) => self.value = value,
            }
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            Node::text_input("input", &self.value, InputMessage::Change)
        }
    }

    #[test]
    fn text_input_routes_unicode_edits_and_renders_cursor() {
        let mut runtime = Runtime::with_clock(
            InputApp::default(),
            RuntimeConfig::new(Size::new(5, 1)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap().unwrap();
        assert!(runtime.request_focus(&NodeId::from("input")).unwrap());

        let dispatch = runtime
            .dispatch_event(&Event::Text("日".to_owned()))
            .unwrap();
        assert!(dispatch.consumed());
        assert_eq!(dispatch.messages(), 1);
        let frame = runtime.step().unwrap().unwrap();

        assert_eq!(runtime.app().value, "日");
        assert_eq!(
            runtime
                .interaction()
                .text_input(&NodeId::from("input"))
                .unwrap()
                .cursor(),
            3
        );
        assert_eq!(
            frame.surface().cursor(),
            Some(nagi_surface::Cursor::new(2, 0))
        );

        runtime
            .dispatch_event(&Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                modifiers: Modifiers::NONE,
                action: KeyAction::Unknown,
                text: None,
                protocol: KeyProtocol::Legacy,
            }))
            .unwrap();
        runtime.step().unwrap();
        assert_eq!(runtime.app().value, "");
    }

    enum FocusMessage {
        Visit(&'static str),
    }

    struct FocusApp {
        ids: Vec<&'static str>,
        visits: Vec<&'static str>,
        handlers: bool,
    }

    impl App for FocusApp {
        type Message = FocusMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                FocusMessage::Visit(id) => self.visits.push(id),
            }
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            if self.handlers {
                let input = Node::text("input")
                    .focusable("input")
                    .on_event("input", |_| {
                        EventResult::ignored().emit(FocusMessage::Visit("input"))
                    });
                let panel = Node::padding(input, Insets::all(0)).on_event("panel", |_| {
                    EventResult::message(FocusMessage::Visit("panel"))
                });
                return Node::padding(panel, Insets::all(0)).on_event("root", |_| {
                    EventResult::message(FocusMessage::Visit("root"))
                });
            }
            Node::column(self.ids.iter().map(|id| Node::text(*id).focusable(*id)))
        }
    }

    #[test]
    fn focus_first_selects_first_focusable_node() {
        let mut runtime = Runtime::with_clock(
            FocusApp {
                ids: vec!["a", "b"],
                visits: Vec::new(),
                handlers: false,
            },
            RuntimeConfig::new(Size::new(8, 2)),
            VirtualClock::new(),
        )
        .unwrap();

        assert!(runtime.focus_first().unwrap());
        assert_eq!(runtime.interaction().focused(), Some(&NodeId::from("a")));
    }

    #[test]
    fn focus_falls_forward_and_handlers_route_to_consuming_ancestor() {
        let mut runtime = Runtime::with_clock(
            FocusApp {
                ids: vec!["a", "b", "c"],
                visits: Vec::new(),
                handlers: false,
            },
            RuntimeConfig::new(Size::new(8, 3)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        runtime.request_focus(&NodeId::from("b")).unwrap();
        runtime.app_mut().ids = vec!["a", "c"];
        runtime.render_if_dirty().unwrap();
        assert_eq!(runtime.interaction().focused(), Some(&NodeId::from("c")));

        runtime.app_mut().handlers = true;
        runtime.render_if_dirty().unwrap();
        runtime.request_focus(&NodeId::from("input")).unwrap();
        let dispatch = runtime
            .dispatch_event(&Event::Key(KeyEvent {
                code: KeyCode::Right,
                modifiers: Modifiers::NONE,
                action: KeyAction::Unknown,
                text: None,
                protocol: KeyProtocol::Legacy,
            }))
            .unwrap();
        assert!(dispatch.consumed());
        runtime.step().unwrap();
        assert_eq!(runtime.app().visits, ["input", "panel"]);
    }

    struct ModalApp {
        visits: Vec<&'static str>,
    }

    impl App for ModalApp {
        type Message = FocusMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                FocusMessage::Visit(id) => self.visits.push(id),
            }
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            let background = Node::text("background")
                .focusable("background")
                .on_event("background", |_| {
                    EventResult::ignored().emit(FocusMessage::Visit("background"))
                });
            let input = Node::text("input")
                .focusable("input")
                .on_event("input", |_| {
                    EventResult::ignored().emit(FocusMessage::Visit("input"))
                });
            let modal = Node::modal("modal", input).on_event("modal", |_| {
                EventResult::ignored().emit(FocusMessage::Visit("modal"))
            });
            Node::stack([background, modal]).on_event("root", |_| {
                EventResult::message(FocusMessage::Visit("root"))
            })
        }
    }

    #[test]
    fn modal_restricts_focus_and_routes_through_modal_ancestors() {
        let mut runtime = Runtime::with_clock(
            ModalApp { visits: Vec::new() },
            RuntimeConfig::new(Size::new(12, 1)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        assert!(!runtime.request_focus(&NodeId::from("background")).unwrap());
        let tab = runtime
            .dispatch_event(&Event::Key(KeyEvent {
                code: KeyCode::Tab,
                modifiers: Modifiers::NONE,
                action: KeyAction::Unknown,
                text: None,
                protocol: KeyProtocol::Legacy,
            }))
            .unwrap();
        assert!(tab.consumed());
        assert_eq!(
            runtime.interaction().focused(),
            Some(&NodeId::from("input"))
        );

        let routed = runtime
            .dispatch_event(&Event::Key(KeyEvent {
                code: KeyCode::Right,
                modifiers: Modifiers::NONE,
                action: KeyAction::Unknown,
                text: None,
                protocol: KeyProtocol::Legacy,
            }))
            .unwrap();
        assert!(routed.consumed());
        assert_eq!(routed.messages(), 3);
        runtime.step().unwrap();
        assert_eq!(runtime.app().visits, ["input", "modal", "root"]);
    }

    struct ScrollApp;

    impl App for ScrollApp {
        type Message = FocusMessage;

        fn update(&mut self, _: Self::Message) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            Node::scroll_viewport(
                "scroll",
                Node::column([
                    Node::text("A").with_length(crate::Length::Fixed(1)),
                    Node::text("B").with_length(crate::Length::Fixed(1)),
                    Node::text("C").with_length(crate::Length::Fixed(1)),
                ]),
            )
        }
    }

    #[test]
    fn scroll_viewport_clamps_and_clips_content() {
        let mut runtime = Runtime::with_clock(
            ScrollApp,
            RuntimeConfig::new(Size::new(3, 2)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        assert!(runtime.set_scroll_offset(&NodeId::from("scroll"), ScrollOffset::new(0, 9)));

        let frame = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(frame.surface().cell(0, 0).unwrap().content(), "B");
        assert_eq!(frame.surface().cell(0, 1).unwrap().content(), "C");
        assert_eq!(
            runtime.interaction().scroll_offset(&NodeId::from("scroll")),
            ScrollOffset::new(0, 1)
        );
    }

    struct VirtualScrollApp {
        builds: Arc<AtomicUsize>,
        rows: Arc<AtomicUsize>,
    }

    impl App for VirtualScrollApp {
        type Message = ();

        fn update(&mut self, (): ()) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            let builds = Arc::clone(&self.builds);
            let rows = Arc::clone(&self.rows);
            Node::virtual_scroll_viewport_with_options(
                "virtual-scroll",
                Size::new(3, 1_000_000),
                crate::ScrollViewportOptions {
                    axis: crate::ScrollAxis::Vertical,
                    ..crate::ScrollViewportOptions::default()
                },
                move |viewport| {
                    builds.fetch_add(1, Ordering::Relaxed);
                    rows.fetch_add(viewport.size.height as usize, Ordering::Relaxed);
                    let start = viewport.offset.y;
                    let end = start.saturating_add(viewport.size.height);
                    let visible = (start..end).map(|row| {
                        Node::text((row % 10).to_string()).with_id(format!("row-{row}"))
                    });
                    crate::VirtualFragment::new(ScrollOffset::new(0, start), Node::column(visible))
                },
            )
        }
    }

    #[test]
    fn virtual_scroll_viewport_bounds_construction_to_visible_rows() {
        let builds = Arc::new(AtomicUsize::new(0));
        let rows = Arc::new(AtomicUsize::new(0));
        let mut runtime = Runtime::with_clock(
            VirtualScrollApp {
                builds: Arc::clone(&builds),
                rows: Arc::clone(&rows),
            },
            RuntimeConfig::new(Size::new(3, 2)),
            VirtualClock::new(),
        )
        .unwrap();

        let first = runtime.render_if_dirty().unwrap().unwrap();
        assert_eq!(builds.load(Ordering::Relaxed), 1);
        assert_eq!(rows.load(Ordering::Relaxed), 2);
        assert_eq!(runtime.tree_index.records.len(), 3);
        assert_eq!(first.surface().cell(0, 0).unwrap().content(), "0");
        assert_eq!(first.surface().cell(0, 1).unwrap().content(), "1");

        assert!(runtime.set_scroll_offset(
            &NodeId::from("virtual-scroll"),
            ScrollOffset::new(0, u32::MAX),
        ));
        let last = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(builds.load(Ordering::Relaxed), 2);
        assert_eq!(rows.load(Ordering::Relaxed), 4);
        assert_eq!(runtime.tree_index.records.len(), 3);
        assert_eq!(last.surface().cell(0, 0).unwrap().content(), "8");
        assert_eq!(last.surface().cell(0, 1).unwrap().content(), "9");
        assert_eq!(
            runtime
                .interaction()
                .scroll_state(&NodeId::from("virtual-scroll"))
                .unwrap()
                .maximum,
            ScrollOffset::new(0, 999_998),
        );

        for offset in 0..256 {
            assert!(runtime.set_scroll_offset(
                &NodeId::from("virtual-scroll"),
                ScrollOffset::new(0, offset),
            ));
            assert!(runtime.render_if_dirty().unwrap().is_some());
            assert_eq!(runtime.tree_index.records.len(), 3);
        }
        assert_eq!(builds.load(Ordering::Relaxed), 258);
        assert_eq!(rows.load(Ordering::Relaxed), 516);
    }

    struct OverscannedScrollApp;

    impl App for OverscannedScrollApp {
        type Message = ();

        fn update(&mut self, (): ()) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            Node::virtual_scroll_viewport_with_options(
                "overscanned-scroll",
                Size::new(3, 6),
                crate::ScrollViewportOptions {
                    axis: crate::ScrollAxis::Vertical,
                    ..crate::ScrollViewportOptions::default()
                },
                |viewport| {
                    let start = viewport.offset.y.saturating_sub(1);
                    let end = viewport
                        .offset
                        .y
                        .saturating_add(viewport.size.height)
                        .saturating_add(1)
                        .min(viewport.content_size.height);
                    let rows = (start..end).map(|row| {
                        Node::text(row.to_string()).with_id(format!("overscan-row-{row}"))
                    });
                    crate::VirtualFragment::new(ScrollOffset::new(0, start), Node::column(rows))
                },
            )
        }
    }

    #[test]
    fn virtual_scroll_viewport_positions_bounded_overscan() {
        let mut runtime = Runtime::with_clock(
            OverscannedScrollApp,
            RuntimeConfig::new(Size::new(3, 2)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        assert!(
            runtime
                .set_scroll_offset(&NodeId::from("overscanned-scroll"), ScrollOffset::new(0, 2),)
        );

        let frame = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(frame.surface().cell(0, 0).unwrap().content(), "2");
        assert_eq!(frame.surface().cell(0, 1).unwrap().content(), "3");
        assert_eq!(runtime.tree_index.records.len(), 5);
    }

    enum LifecycleMessage {
        Stop,
    }

    struct LifecycleApp {
        stopped: bool,
    }

    impl App for LifecycleApp {
        type Message = LifecycleMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                LifecycleMessage::Stop => self.stopped = true,
            }
            Effect::exit()
        }

        fn view(&self, context: crate::ViewContext) -> Node<Self::Message> {
            let state = if self.stopped { "stopped" } else { "running" };
            Node::text(format!("{state}:{}", context.size.width))
        }
    }

    #[test]
    fn exit_effect_preserves_the_final_view_and_view_receives_size() {
        let mut runtime = Runtime::with_clock(
            LifecycleApp { stopped: false },
            RuntimeConfig::new(Size::new(12, 1)),
            VirtualClock::new(),
        )
        .unwrap();

        let initial = runtime.render_if_dirty().unwrap().unwrap();
        assert_eq!(initial.surface().cell(0, 0).unwrap().content(), "r");
        assert!(!runtime.exit_requested());

        runtime.enqueue(LifecycleMessage::Stop).unwrap();
        runtime.process_pending().unwrap();
        assert!(runtime.exit_requested());
        let final_frame = runtime.render_if_dirty().unwrap().unwrap();
        assert_eq!(final_frame.surface().cell(0, 0).unwrap().content(), "s");

        runtime.resize(Size::new(9, 1));
        let resized = runtime.render_if_dirty().unwrap().unwrap();
        assert_eq!(resized.surface().cell(8, 0).unwrap().content(), "9");
    }

    enum CommandMessage {
        Apply,
    }

    struct CommandApp;

    impl App for CommandApp {
        type Message = CommandMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                CommandMessage::Apply => Effect::batch([
                    Effect::focus("target"),
                    Effect::scroll_to("scroll", ScrollOffset::new(0, u32::MAX)),
                ]),
            }
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            Node::scroll_viewport(
                "scroll",
                Node::column([
                    Node::text("A").with_length(crate::Length::Fixed(1)),
                    Node::text("B")
                        .focusable("target")
                        .with_length(crate::Length::Fixed(1)),
                    Node::text("C").with_length(crate::Length::Fixed(1)),
                ]),
            )
        }
    }

    #[test]
    fn focus_and_scroll_effects_apply_synchronously_to_the_next_view() {
        let mut runtime = Runtime::with_clock(
            CommandApp,
            RuntimeConfig::new(Size::new(3, 2)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.enqueue(CommandMessage::Apply).unwrap();

        let frame = runtime.step().unwrap().unwrap();

        assert_eq!(
            runtime.interaction().focused(),
            Some(&NodeId::from("target"))
        );
        assert_eq!(
            runtime.interaction().scroll_offset(&NodeId::from("scroll")),
            ScrollOffset::new(0, 1)
        );
        assert_eq!(frame.surface().cell(0, 0).unwrap().content(), "B");
    }

    enum AdvancedScrollMessage {
        Scrolled(crate::ScrollState),
    }

    struct AdvancedScrollApp {
        lines: u32,
        stick_to_end: bool,
        ensure_focused_visible: bool,
        observed: Vec<crate::ScrollState>,
    }

    impl App for AdvancedScrollApp {
        type Message = AdvancedScrollMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                AdvancedScrollMessage::Scrolled(state) => self.observed.push(state),
            }
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            let rows = (0..self.lines).map(|index| {
                let row = Node::text(index.to_string());
                let row = if index + 1 == self.lines {
                    row.focusable("target")
                } else {
                    row
                };
                row.with_length(crate::Length::Fixed(1))
            });
            let viewport = Node::scroll_viewport_with_options(
                "scroll",
                Node::column(rows),
                crate::ScrollViewportOptions {
                    axis: crate::ScrollAxis::Vertical,
                    stick_to_end: self.stick_to_end,
                    ensure_focused_visible: self.ensure_focused_visible,
                    on_scroll: Some(Box::new(AdvancedScrollMessage::Scrolled)),
                },
            )
            .with_length(crate::Length::Fixed(2));
            Node::column([
                viewport,
                Node::text("footer").with_length(crate::Length::Flex(1)),
            ])
        }
    }

    fn key_event(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers::NONE,
            action: KeyAction::Unknown,
            text: None,
            protocol: KeyProtocol::Legacy,
        })
    }

    #[test]
    fn scroll_state_uses_viewport_page_and_stick_to_end_resumes_at_end() {
        let mut runtime = Runtime::with_clock(
            AdvancedScrollApp {
                lines: 6,
                stick_to_end: true,
                ensure_focused_visible: false,
                observed: Vec::new(),
            },
            RuntimeConfig::new(Size::new(8, 5)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        assert_eq!(
            runtime.interaction().scroll_state(&NodeId::from("scroll")),
            Some(crate::ScrollState {
                offset: ScrollOffset::new(0, 4),
                maximum: ScrollOffset::new(0, 4),
                at_start: false,
                at_end: true,
            })
        );
        runtime.request_focus(&NodeId::from("target")).unwrap();
        runtime.render_if_dirty().unwrap();

        let page_up = runtime.dispatch_event(&key_event(KeyCode::PageUp)).unwrap();
        assert!(page_up.consumed());
        runtime.process_pending().unwrap();
        assert_eq!(
            runtime.interaction().scroll_offset(&NodeId::from("scroll")),
            ScrollOffset::new(0, 2)
        );
        assert_eq!(runtime.app().observed.len(), 1);
        assert!(!runtime.app().observed[0].at_end);

        runtime.app_mut().lines = 7;
        runtime.render_if_dirty().unwrap();
        let state = runtime
            .interaction()
            .scroll_state(&NodeId::from("scroll"))
            .unwrap();
        assert_eq!(state.offset, ScrollOffset::new(0, 2));
        assert_eq!(state.maximum, ScrollOffset::new(0, 5));

        runtime.dispatch_event(&key_event(KeyCode::End)).unwrap();
        runtime.process_pending().unwrap();
        runtime.render_if_dirty().unwrap();
        runtime.app_mut().lines = 8;
        runtime.render_if_dirty().unwrap();
        let state = runtime
            .interaction()
            .scroll_state(&NodeId::from("scroll"))
            .unwrap();
        assert_eq!(state.offset, ScrollOffset::new(0, 6));
        assert_eq!(state.maximum, ScrollOffset::new(0, 6));
        assert!(state.at_end);
    }

    #[test]
    fn focused_descendant_is_revealed_when_requested() {
        let mut runtime = Runtime::with_clock(
            AdvancedScrollApp {
                lines: 5,
                stick_to_end: false,
                ensure_focused_visible: true,
                observed: Vec::new(),
            },
            RuntimeConfig::new(Size::new(8, 4)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        runtime.request_focus(&NodeId::from("target")).unwrap();

        let frame = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(
            runtime.interaction().scroll_offset(&NodeId::from("scroll")),
            ScrollOffset::new(0, 3)
        );
        assert_eq!(frame.surface().cell(0, 1).unwrap().content(), "4");
    }
}
