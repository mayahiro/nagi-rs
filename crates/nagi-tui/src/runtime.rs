use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use nagi_surface::SurfaceError;
use nagi_text::WidthProfile;
use nagi_vt::{Event, KeyAction, KeyCode, MouseButton, MouseKind, TerminalOp};

use crate::action_routing::{
    ActionIndex, CoreActionMatch, ResolvedActionRoute, route_needs_action_resolution,
    validate_action_owners,
};
use crate::core_action::{CoreAction, default_focus_action};
use crate::effect::{ClipboardRequest, RuntimeCommand};
use crate::renderer::operations;
use crate::routing::{
    FocusChange, InteractiveKind, PointerChange, PointerEventContext, PointerViewport, TreeIndex,
};
use crate::runtime_notice::{RuntimeNotice, RuntimeNoticeDiagnostics, RuntimeNoticeQueue};
use crate::subscription_supervisor::{
    SubscriptionDiagnostics, SubscriptionReconciliation, SubscriptionSupervisor, SubscriptionTag,
};
use crate::supervisor::{EffectDiagnostics, EffectSupervisor};
use crate::text_edit::{TextEdit, apply_text_edit, normalize_cursor};
use crate::wake::WakeHandle;
use crate::{
    App, BindingConflict, Clock, EventDispatch, EventResult, InteractionState, Node, NodeId, Point,
    Rect, ResolvedActions, ScrollOffset, Size, SubscriptionKey, Surface, SystemClock, TaskKey,
    Timestamp,
};

/// The default maximum number of messages waiting in a runtime queue
pub const DEFAULT_QUEUE_CAPACITY: usize = 4_096;

/// The default maximum number of effect tasks executing concurrently
pub const DEFAULT_TASK_LIMIT: usize = 64;

/// The default per-source subscription inbox capacity
pub const DEFAULT_SUBSCRIPTION_CAPACITY: usize = 256;

/// The default maximum number of retained asynchronous lifecycle notices
pub const DEFAULT_RUNTIME_NOTICE_CAPACITY: usize = 256;

/// Runtime construction settings
#[derive(Clone, Copy, Debug)]
pub struct RuntimeConfig {
    /// Initial terminal cell size
    pub size: Size,
    /// Maximum number of messages waiting for sequential processing
    pub queue_capacity: usize,
    /// Maximum number of effect tasks executing concurrently
    pub task_limit: usize,
    /// Maximum pending values retained by each subscription source
    pub subscription_capacity: usize,
    /// Maximum retained asynchronous lifecycle notices
    pub runtime_notice_capacity: usize,
    /// Smallest interval between non-urgent rendered frames
    pub minimum_frame_interval: Duration,
    /// Terminal cell-width policy used by the complete view
    ///
    /// A Custom override must return stable widths for this Runtime's lifetime
    pub width_profile: WidthProfile<'static>,
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
            runtime_notice_capacity: DEFAULT_RUNTIME_NOTICE_CAPACITY,
            minimum_frame_interval: Duration::ZERO,
            width_profile: WidthProfile::MODERN,
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
    /// Runtime notice capacity must be greater than zero
    ZeroRuntimeNoticeCapacity,
    /// A frame surface could not be constructed
    Surface(SurfaceError),
    /// Two nodes in one semantic tree used the same stable ID
    DuplicateNodeId(NodeId),
    /// Two subscription sources used the same stable key
    DuplicateSubscriptionKey(SubscriptionKey),
    /// One active semantic action group has conflicting key bindings
    BindingConflict(BindingConflict),
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
            Self::ZeroRuntimeNoticeCapacity => {
                formatter.write_str("runtime notice capacity must be positive")
            }
            Self::Surface(error) => write!(formatter, "construct runtime surface: {error}"),
            Self::DuplicateNodeId(id) => write!(formatter, "duplicate NodeId {id}"),
            Self::DuplicateSubscriptionKey(key) => {
                write!(formatter, "duplicate SubscriptionKey {key}")
            }
            Self::BindingConflict(error) => error.fmt(formatter),
        }
    }
}

impl Error for RuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Surface(error) => Some(error),
            Self::BindingConflict(error) => Some(error),
            Self::ZeroQueueCapacity
            | Self::ZeroTaskLimit
            | Self::ZeroSubscriptionCapacity
            | Self::ZeroRuntimeNoticeCapacity
            | Self::DuplicateNodeId(_)
            | Self::DuplicateSubscriptionKey(_) => None,
        }
    }
}

impl From<BindingConflict> for RuntimeError {
    fn from(error: BindingConflict) -> Self {
        Self::BindingConflict(error)
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
    width_profile: WidthProfile<'static>,
    last_frame: Option<Timestamp>,
    previous_surface: Option<Arc<Surface>>,
    spare_surface: Option<Surface>,
    interaction: InteractionState,
    view_tree: Option<Node<Application::Message>>,
    tree_index: TreeIndex,
    next_tree_index: TreeIndex,
    action_index: ActionIndex<Application::Message>,
    next_action_index: ActionIndex<Application::Message>,
    resolved_action_route: Option<ResolvedActionRoute<Application::Message>>,
    next_resolved_action_route: ResolvedActionRoute<Application::Message>,
    event_route: Vec<NodeId>,
    effects: EffectSupervisor<Application::Message>,
    subscriptions: SubscriptionSupervisor<Application::Message>,
    notices: Arc<RuntimeNoticeQueue>,
    subscriptions_dirty: bool,
    exit_requested: bool,
    pending_focus: Option<NodeId>,
    pending_scroll: Vec<(NodeId, ScrollOffset)>,
    pending_clipboard: Option<ClipboardRequest>,
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
        app: Application,
        config: RuntimeConfig,
        clock: C,
    ) -> Result<Self, RuntimeError> {
        Self::with_clock_and_wake(app, config, clock, WakeHandle::default())
    }

    pub(crate) fn with_clock_and_wake(
        mut app: Application,
        config: RuntimeConfig,
        clock: C,
        wake: WakeHandle,
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
        if config.runtime_notice_capacity == 0 {
            return Err(RuntimeError::ZeroRuntimeNoticeCapacity);
        }
        let startup = app.init();
        let declared_subscriptions = app.subscriptions();
        let notices = Arc::new(RuntimeNoticeQueue::new(config.runtime_notice_capacity));
        let mut effects = EffectSupervisor::new(config.task_limit);
        effects.set_notices(Arc::clone(&notices));
        effects.set_wake(wake.clone());
        effects.schedule(startup, clock.now());
        let mut subscriptions = SubscriptionSupervisor::new(config.subscription_capacity);
        subscriptions.set_notices(Arc::clone(&notices));
        subscriptions.set_wake(wake);
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
            width_profile: config.width_profile,
            last_frame: None,
            previous_surface: None,
            spare_surface: None,
            interaction: InteractionState::new(),
            view_tree: None,
            tree_index: TreeIndex::default(),
            next_tree_index: TreeIndex::default(),
            action_index: ActionIndex::default(),
            next_action_index: ActionIndex::default(),
            resolved_action_route: None,
            next_resolved_action_route: ResolvedActionRoute::default(),
            event_route: Vec::new(),
            effects,
            subscriptions,
            notices,
            subscriptions_dirty: false,
            exit_requested: false,
            pending_focus: None,
            pending_scroll: Vec::new(),
            pending_clipboard: None,
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
        self.view_tree = None;
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

    /// Returns the latest clipboard request without clearing it
    #[must_use]
    pub const fn pending_clipboard_request(&self) -> Option<&ClipboardRequest> {
        self.pending_clipboard.as_ref()
    }

    /// Takes and clears the latest pending clipboard request
    pub fn take_clipboard_request(&mut self) -> Option<ClipboardRequest> {
        self.pending_clipboard.take()
    }

    /// Changes terminal size and schedules a frame when it differs
    pub fn resize(&mut self, size: Size) {
        if self.size != size {
            self.size = size;
            self.dirty = true;
            self.urgent_frame = true;
            self.view_tree = None;
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

    /// Returns retained asynchronous lifecycle notices
    #[must_use]
    pub fn pending_runtime_notices(&self) -> usize {
        self.notices.pending()
    }

    /// Removes and returns retained notices in occurrence order
    pub fn drain_runtime_notices(&mut self) -> Vec<RuntimeNotice> {
        self.notices.drain()
    }

    /// Returns bounded notice queue counters
    #[must_use]
    pub fn runtime_notice_diagnostics(&self) -> RuntimeNoticeDiagnostics {
        self.notices.diagnostics()
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
        observe: impl FnMut(&Application::Message),
    ) -> Result<usize, RuntimeError> {
        self.reconcile_subscriptions()?;
        self.poll_effects();
        self.poll_subscriptions();
        self.process_queued_with_inner(observe)
    }

    /// Applies messages already in the application queue without polling
    /// asynchronous Effects or Subscriptions
    pub fn process_queued(&mut self) -> Result<usize, RuntimeError> {
        self.process_queued_with(|_| {})
    }

    /// Applies messages already in the application queue and observes each
    /// immediately before update without polling asynchronous sources
    pub fn process_queued_with(
        &mut self,
        observe: impl FnMut(&Application::Message),
    ) -> Result<usize, RuntimeError> {
        self.reconcile_subscriptions()?;
        self.process_queued_with_inner(observe)
    }

    fn process_queued_with_inner(
        &mut self,
        mut observe: impl FnMut(&Application::Message),
    ) -> Result<usize, RuntimeError> {
        let mut processed = 0;
        while let Some(queued) = self.queue.pop_front() {
            observe(&queued.message);
            let effect = self.app.update(queued.message);
            let without_redraw = effect.without_redraw;
            self.effects.schedule(effect, self.clock.now());
            self.apply_effect_commands();
            if !without_redraw {
                self.dirty = true;
                self.view_tree = None;
            }
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
                RuntimeCommand::SetClipboard(request) => {
                    self.pending_clipboard = Some(request);
                    continue;
                }
            }
            self.dirty = true;
            self.urgent_frame = true;
            self.view_tree = None;
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
                .is_some_and(|record| record.kind.is_scroll_viewport())
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
        let Some(first) = self.tree_index.focus_scope().first().cloned() else {
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
            self.view_tree = None;
        }
        true
    }

    /// Returns resolved semantic action groups on the active target-to-root route
    ///
    /// Groups outside the nearest StopAtScope boundary are omitted. Each
    /// projection contains the complete active root-to-target scope path.
    /// At one Node, a Node-declared group precedes a Core semantic group, so
    /// the same owner may occur twice
    pub fn active_action_groups(&mut self) -> Result<Vec<ResolvedActions>, RuntimeError> {
        self.ensure_tree()?;
        let route = self.tree_index.route(self.interaction.focused.as_ref());
        let focus_owner = self
            .tree_index
            .focus_action_owner(self.interaction.focused.as_ref());
        self.ensure_action_route(&route, focus_owner.as_ref())?;
        Ok(self
            .resolved_action_route
            .as_ref()
            .map_or_else(Vec::new, ResolvedActionRoute::groups))
    }

    /// Routes one normalized event through focus, hit testing, and ancestors
    pub fn dispatch_event(&mut self, event: &Event) -> Result<EventDispatch, RuntimeEventError> {
        self.ensure_tree()?;
        if self
            .tree_index
            .focus_action_owner(self.interaction.focused.as_ref())
            .is_none()
        {
            if let Some(action) = default_focus_action(event) {
                return Ok(self.dispatch_legacy_focus_action(action));
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

        let mut route = std::mem::take(&mut self.event_route);
        self.tree_index.route_into(target.as_ref(), &mut route);
        let result = self.dispatch_event_route(event, &route);
        route.clear();
        self.event_route = route;
        result
    }

    fn dispatch_event_route(
        &mut self,
        event: &Event,
        route: &[NodeId],
    ) -> Result<EventDispatch, RuntimeEventError> {
        let focus_owner = self
            .tree_index
            .focus_action_owner(self.interaction.focused.as_ref());
        self.ensure_action_route(route, focus_owner.as_ref())?;
        let mut dispatch = EventDispatch::default();
        for (index, id) in route.iter().enumerate() {
            let action_result = self
                .resolved_action_route
                .as_ref()
                .and_then(|resolved| resolved.match_declared_event(index, event).invoke());
            if let Some(result) = action_result {
                self.apply_event_result(result, &mut dispatch)?;
                if dispatch.consumed {
                    break;
                }
            }
            let core_match = self
                .resolved_action_route
                .as_ref()
                .map_or(CoreActionMatch::None, |resolved| {
                    resolved.match_core_event(index, event)
                });
            let core_result = match core_match {
                CoreActionMatch::None => None,
                CoreActionMatch::Consume => Some(EventResult::consumed()),
                CoreActionMatch::Invoke(action) => self.handle_core_action(id, action),
            };
            if let Some(result) = core_result {
                self.apply_event_result(result, &mut dispatch)?;
                if dispatch.consumed {
                    break;
                }
            }
            let kind = self
                .tree_index
                .record(id)
                .map_or(InteractiveKind::Generic, |record| record.kind);
            let special = match kind {
                InteractiveKind::TextInput if index == 0 => self.handle_text_input(id, event),
                InteractiveKind::ScrollViewportVertical
                | InteractiveKind::ScrollViewportHorizontal => self.handle_scroll_mouse(id, event),
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
            let pointer_context = self.pointer_event_context(id, route, index, event);
            let pointer_result = pointer_context.and_then(|context| {
                self.view_tree
                    .as_ref()
                    .and_then(|view| view.handle_pointer_event(id, context))
            });
            if let Some(result) = pointer_result {
                self.apply_event_result(result, &mut dispatch)?;
                if dispatch.consumed {
                    break;
                }
            }
            let result = self
                .view_tree
                .as_ref()
                .and_then(|view| view.handle_event(id, event));
            if let Some(result) = result {
                self.apply_event_result(result, &mut dispatch)?;
                if dispatch.consumed {
                    break;
                }
            }
            if self
                .tree_index
                .record(id)
                .is_some_and(|record| record.blocks_unhandled_events)
            {
                dispatch.consumed = true;
                break;
            }
        }
        Ok(dispatch)
    }

    fn pointer_event_context(
        &self,
        id: &NodeId,
        route: &[NodeId],
        index: usize,
        event: &Event,
    ) -> Option<PointerEventContext> {
        let Event::Mouse(mouse) = event else {
            return None;
        };
        let record = self.tree_index.record(id)?;
        let visible = record.rect.intersection(record.clip);
        let local_position = Point::new(
            local_pointer_coordinate(mouse.x, record.rect.x),
            local_pointer_coordinate(mouse.y, record.rect.y),
        );
        let visible_bounds = Rect::new(
            local_geometry_coordinate(visible.x, record.rect.x),
            local_geometry_coordinate(visible.y, record.rect.y),
            visible.width,
            visible.height,
        );
        let viewport = route[index.saturating_add(1)..]
            .iter()
            .find_map(|viewport_id| {
                let viewport_record = self.tree_index.record(viewport_id)?;
                let axis = viewport_record.kind.scroll_axis()?;
                let state = self.interaction.scroll_state(viewport_id)?;
                Some(PointerViewport::new(
                    viewport_id.clone(),
                    axis,
                    state,
                    viewport_record.rect.intersection(viewport_record.clip),
                ))
            });
        Some(PointerEventContext::new(
            *mouse,
            local_position,
            record.rect.size(),
            visible_bounds,
            self.width_profile,
            self.interaction.pointer_capture() == Some(id),
            viewport,
        ))
    }

    fn ensure_action_route(
        &mut self,
        route: &[NodeId],
        focus_owner: Option<&NodeId>,
    ) -> Result<(), RuntimeError> {
        if !route_needs_action_resolution(route, &self.action_index, &self.tree_index, focus_owner)
        {
            self.publish_resolved_action_route(false);
            return Ok(());
        }
        if self
            .resolved_action_route
            .as_ref()
            .is_some_and(|resolved| resolved.matches_route(route, focus_owner))
        {
            return Ok(());
        }
        self.next_resolved_action_route.resolve_into(
            route,
            &self.action_index,
            &self.tree_index,
            focus_owner,
        )?;
        self.publish_resolved_action_route(true);
        Ok(())
    }

    fn publish_resolved_action_route(&mut self, active: bool) {
        if active {
            let previous = self.resolved_action_route.take().unwrap_or_default();
            let current = std::mem::replace(&mut self.next_resolved_action_route, previous);
            self.resolved_action_route = Some(current);
        } else if let Some(previous) = self.resolved_action_route.take() {
            self.next_resolved_action_route = previous;
        }
    }

    fn dispatch_legacy_focus_action(&mut self, action: CoreAction) -> EventDispatch {
        let forward = matches!(action, CoreAction::FocusNext);
        let focus_scope = self.tree_index.focus_scope();
        self.interaction.focused = crate::interaction::traverse_focus(
            focus_scope.as_ref(),
            self.interaction.focused.as_ref(),
            forward,
        );
        self.dirty = true;
        self.urgent_frame = true;
        EventDispatch {
            consumed: true,
            messages: 0,
            redraw: true,
        }
    }

    fn handle_core_action(
        &mut self,
        id: &NodeId,
        action: CoreAction,
    ) -> Option<EventResult<Application::Message>> {
        match action {
            CoreAction::FocusNext | CoreAction::FocusPrevious => {
                let focus_scope = self.tree_index.focus_scope();
                self.interaction.focused = crate::interaction::traverse_focus(
                    focus_scope.as_ref(),
                    self.interaction.focused.as_ref(),
                    action == CoreAction::FocusNext,
                );
                self.dirty = true;
                self.urgent_frame = true;
                Some(EventResult::consumed().redraw())
            }
            CoreAction::ScrollPageUp
            | CoreAction::ScrollPageDown
            | CoreAction::ScrollStart
            | CoreAction::ScrollEnd => self.handle_scroll_action(id, action),
        }
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

    fn handle_scroll_mouse(
        &mut self,
        id: &NodeId,
        event: &Event,
    ) -> Option<EventResult<Application::Message>> {
        let state = self.interaction.scroll_state(id)?;
        let axis = self.view_tree.as_ref()?.scroll_options(id)?.axis;
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

    fn handle_scroll_action(
        &mut self,
        id: &NodeId,
        action: CoreAction,
    ) -> Option<EventResult<Application::Message>> {
        let state = self.interaction.scroll_state(id)?;
        let axis = self.view_tree.as_ref()?.scroll_options(id)?.axis;
        let viewport = self.tree_index.record(id)?.rect;
        let current = state.offset;
        let next = match action {
            CoreAction::ScrollPageUp if axis.allows_vertical() => {
                ScrollOffset::new(current.x, current.y.saturating_sub(viewport.height.max(1)))
            }
            CoreAction::ScrollPageDown if axis.allows_vertical() => {
                ScrollOffset::new(current.x, current.y.saturating_add(viewport.height.max(1)))
            }
            CoreAction::ScrollStart if axis.allows_vertical() => ScrollOffset::new(current.x, 0),
            CoreAction::ScrollEnd if axis.allows_vertical() => {
                ScrollOffset::new(current.x, state.maximum.y)
            }
            CoreAction::ScrollStart if axis.allows_horizontal() => ScrollOffset::new(0, current.y),
            CoreAction::ScrollEnd if axis.allows_horizontal() => {
                ScrollOffset::new(state.maximum.x, current.y)
            }
            CoreAction::FocusNext
            | CoreAction::FocusPrevious
            | CoreAction::ScrollPageUp
            | CoreAction::ScrollPageDown
            | CoreAction::ScrollStart
            | CoreAction::ScrollEnd => return None,
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
        if let Some((id, offset)) = result.scroll {
            let is_available = self
                .tree_index
                .record(&id)
                .is_some_and(|record| record.kind.is_scroll_viewport())
                && self.tree_index.allows_interaction(&id);
            if is_available {
                if let Some((state, true)) = self.interaction.request_scroll(&id, offset) {
                    self.dirty = true;
                    self.urgent_frame = true;
                    dispatch.redraw = true;
                    if let Some(message) = self
                        .view_tree
                        .as_ref()
                        .and_then(|view| view.scroll_message(&id, state))
                    {
                        self.enqueue(message)?;
                        dispatch.messages += 1;
                    }
                }
            }
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

    fn ensure_reveal_targets_visible(
        &mut self,
        view: &Node<Application::Message>,
        index: &mut TreeIndex,
        actions: &mut ActionIndex<Application::Message>,
    ) -> Result<(), RuntimeError> {
        if let Some(focused) = self.interaction.focused.clone() {
            let scrolls: Vec<_> = index
                .route(Some(&focused))
                .into_iter()
                .filter(|id| {
                    index
                        .reveal_targets
                        .iter()
                        .all(|(viewport, _)| viewport != id)
                        && view
                            .scroll_options(id)
                            .is_some_and(|options| options.ensure_focused_visible)
                })
                .collect();
            for viewport in scrolls {
                self.ensure_target_visible(view, index, actions, &viewport, &focused)?;
            }
        }

        let mut remaining = index.reveal_targets.len();
        while remaining > 0 {
            remaining = remaining.min(index.reveal_targets.len());
            if remaining == 0 {
                break;
            }
            remaining -= 1;
            let (viewport, target) = index.reveal_targets[remaining].clone();
            self.ensure_target_visible(view, index, actions, &viewport, &target)?;
        }
        Ok(())
    }

    fn ensure_target_visible(
        &mut self,
        view: &Node<Application::Message>,
        index: &mut TreeIndex,
        actions: &mut ActionIndex<Application::Message>,
        viewport_id: &NodeId,
        target_id: &NodeId,
    ) -> Result<(), RuntimeError> {
        if !index.is_within(target_id, viewport_id) {
            return Ok(());
        }
        let Some(target) = index.record(target_id).cloned() else {
            return Ok(());
        };
        let Some(viewport) = index.record(viewport_id).cloned() else {
            return Ok(());
        };
        let Some(axis) = view.scroll_options(viewport_id).map(|options| options.axis) else {
            return Ok(());
        };
        let Some(state) = self.interaction.scroll_state(viewport_id) else {
            return Ok(());
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
            return Ok(());
        }
        self.interaction.request_scroll(viewport_id, next);
        view.prepare_interaction(self.size, &mut self.interaction, self.width_profile);
        view.build_tree_index_into(
            self.size,
            &self.interaction,
            index,
            actions,
            self.width_profile,
        )
        .map_err(RuntimeError::DuplicateNodeId)
    }

    fn ensure_tree(&mut self) -> Result<(), RuntimeError> {
        if self.view_tree.is_some() {
            return Ok(());
        }
        let view = self.app.view(crate::ViewContext::with_width_profile(
            self.size,
            self.width_profile,
        ));
        view.prepare_virtual_flows(self.size, &mut self.interaction, self.width_profile);
        let mut tree_index = std::mem::take(&mut self.next_tree_index);
        let mut action_index = std::mem::take(&mut self.next_action_index);
        if let Err(id) = view.build_tree_index_into(
            self.size,
            &self.interaction,
            &mut tree_index,
            &mut action_index,
            self.width_profile,
        ) {
            self.next_tree_index = tree_index;
            self.next_action_index = action_index;
            return Err(RuntimeError::DuplicateNodeId(id));
        }
        let previous_focus_order = self.tree_index.focus_scope();
        let current_focus_order = tree_index.focus_scope();
        let focus_fallback = self
            .interaction
            .focused
            .as_ref()
            .and_then(|focused| self.tree_index.focus_fallback(focused))
            .cloned();
        self.interaction.reconcile(
            &tree_index.active,
            previous_focus_order.as_ref(),
            current_focus_order.as_ref(),
            tree_index
                .active_modal
                .as_ref()
                .map(|modal| (modal, &tree_index.active_modal_focus)),
            focus_fallback.as_ref(),
        );
        if self
            .interaction
            .pointer_capture
            .as_ref()
            .is_some_and(|id| !tree_index.allows_interaction(id))
        {
            self.interaction.pointer_capture = None;
        }
        self.apply_pending_interaction(&tree_index);
        if view.prepare_interaction(self.size, &mut self.interaction, self.width_profile) {
            if let Err(id) = view.build_tree_index_into(
                self.size,
                &self.interaction,
                &mut tree_index,
                &mut action_index,
                self.width_profile,
            ) {
                self.next_tree_index = tree_index;
                self.next_action_index = action_index;
                return Err(RuntimeError::DuplicateNodeId(id));
            }
        }
        if let Err(error) =
            self.ensure_reveal_targets_visible(&view, &mut tree_index, &mut action_index)
        {
            self.next_tree_index = tree_index;
            self.next_action_index = action_index;
            return Err(error);
        }
        let has_resolved_action_route = match resolve_frame_actions_into(
            &tree_index,
            &action_index,
            self.interaction.focused.as_ref(),
            &mut self.next_resolved_action_route,
        ) {
            Ok(resolved) => resolved,
            Err(error) => {
                self.next_tree_index = tree_index;
                self.next_action_index = action_index;
                return Err(error);
            }
        };
        let previous = std::mem::replace(&mut self.tree_index, tree_index);
        self.next_tree_index = previous;
        let previous = std::mem::replace(&mut self.action_index, action_index);
        self.next_action_index = previous;
        self.publish_resolved_action_route(has_resolved_action_route);
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
        let view = self.app.view(crate::ViewContext::with_width_profile(
            self.size,
            self.width_profile,
        ));
        view.prepare_virtual_flows(self.size, &mut self.interaction, self.width_profile);
        let mut tree_index = std::mem::take(&mut self.next_tree_index);
        let mut action_index = std::mem::take(&mut self.next_action_index);
        if let Err(id) = view.build_tree_index_into(
            self.size,
            &self.interaction,
            &mut tree_index,
            &mut action_index,
            self.width_profile,
        ) {
            self.next_tree_index = tree_index;
            self.next_action_index = action_index;
            return Err(RuntimeError::DuplicateNodeId(id));
        }
        {
            let previous_focus_order = self.tree_index.focus_scope();
            let current_focus_order = tree_index.focus_scope();
            let focus_fallback = self
                .interaction
                .focused
                .as_ref()
                .and_then(|focused| self.tree_index.focus_fallback(focused))
                .cloned();
            self.interaction.reconcile(
                &tree_index.active,
                previous_focus_order.as_ref(),
                current_focus_order.as_ref(),
                tree_index
                    .active_modal
                    .as_ref()
                    .map(|modal| (modal, &tree_index.active_modal_focus)),
                focus_fallback.as_ref(),
            );
        }
        if self
            .interaction
            .pointer_capture
            .as_ref()
            .is_some_and(|id| !tree_index.allows_interaction(id))
        {
            self.interaction.pointer_capture = None;
        }
        self.apply_pending_interaction(&tree_index);
        if view.prepare_interaction(self.size, &mut self.interaction, self.width_profile) {
            if let Err(id) = view.build_tree_index_into(
                self.size,
                &self.interaction,
                &mut tree_index,
                &mut action_index,
                self.width_profile,
            ) {
                self.next_tree_index = tree_index;
                self.next_action_index = action_index;
                return Err(RuntimeError::DuplicateNodeId(id));
            }
        }
        if let Err(error) =
            self.ensure_reveal_targets_visible(&view, &mut tree_index, &mut action_index)
        {
            self.next_tree_index = tree_index;
            self.next_action_index = action_index;
            return Err(error);
        }
        let has_resolved_action_route = match resolve_frame_actions_into(
            &tree_index,
            &action_index,
            self.interaction.focused.as_ref(),
            &mut self.next_resolved_action_route,
        ) {
            Ok(resolved) => resolved,
            Err(error) => {
                self.next_tree_index = tree_index;
                self.next_action_index = action_index;
                return Err(error);
            }
        };
        let mut surface = match self.spare_surface.take() {
            Some(mut surface)
                if surface.width() == self.size.width && surface.height() == self.size.height =>
            {
                surface.clear();
                surface
            }
            _ => match Surface::new(self.size.width, self.size.height) {
                Ok(surface) => surface,
                Err(error) => {
                    self.next_tree_index = tree_index;
                    self.next_action_index = action_index;
                    return Err(error.into());
                }
            },
        };
        view.render_to_profile(&mut surface, &self.interaction, self.width_profile);
        let previous_surface = self.previous_surface.take();
        let frame_operations = operations(previous_surface.as_deref(), &surface);
        if let Some(previous_surface) = previous_surface {
            if let Ok(surface) = Arc::try_unwrap(previous_surface) {
                self.spare_surface = Some(surface);
            }
        }
        let surface = Arc::new(surface);
        self.previous_surface = Some(Arc::clone(&surface));
        self.view_tree = Some(view);
        let previous = std::mem::replace(&mut self.tree_index, tree_index);
        self.next_tree_index = previous;
        let previous = std::mem::replace(&mut self.action_index, action_index);
        self.next_action_index = previous;
        self.publish_resolved_action_route(has_resolved_action_route);
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

fn local_pointer_coordinate(value: u32, origin: i32) -> i32 {
    clamp_pointer_coordinate(i64::from(value).saturating_sub(i64::from(origin)))
}

fn local_geometry_coordinate(value: i32, origin: i32) -> i32 {
    clamp_pointer_coordinate(i64::from(value).saturating_sub(i64::from(origin)))
}

fn clamp_pointer_coordinate(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn resolve_frame_actions_into<Message>(
    tree: &TreeIndex,
    actions: &ActionIndex<Message>,
    target: Option<&NodeId>,
    resolved: &mut ResolvedActionRoute<Message>,
) -> Result<bool, RuntimeError> {
    validate_action_owners(tree, actions)?;
    let focus_owner = tree.focus_action_owner(target);
    Ok(resolved.resolve_tree_route_into(target, actions, tree, focus_owner.as_ref())?)
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use crate::fixture_support;
    use crate::{
        Action, ActionDescriptor, CancelToken, DeliveryPolicy, Effect, EventResult, Insets,
        KeyBinding, KeyEvent, KeyProtocol, KeyStroke, Modifiers, Node, NodeId, ScrollOffset,
        Subscription, SubscriptionKey, Task, TaskKey, VirtualClock,
    };

    use super::*;

    enum Message {
        Add(u32),
        WithoutRedraw,
        ExitWithoutRedraw,
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
                Message::WithoutRedraw => return Effect::none().without_redraw(),
                Message::ExitWithoutRedraw => return Effect::exit().without_redraw(),
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
    fn effect_without_redraw_skips_unchanged_frame() {
        let mut runtime = Runtime::new(
            Counter {
                value: 0,
                updates: Vec::new(),
            },
            Size::new(3, 1),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap().unwrap();
        runtime.enqueue(Message::WithoutRedraw).unwrap();

        assert_eq!(runtime.process_pending().unwrap(), 1);
        assert!(runtime.render_if_dirty().unwrap().is_none());
    }

    #[test]
    fn effect_without_redraw_does_not_suppress_synchronous_command_frame() {
        let mut runtime = Runtime::new(
            Counter {
                value: 0,
                updates: Vec::new(),
            },
            Size::new(3, 1),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap().unwrap();
        runtime.enqueue(Message::ExitWithoutRedraw).unwrap();

        runtime.process_pending().unwrap();
        assert!(runtime.render_if_dirty().unwrap().is_some());
    }

    #[test]
    fn rendering_reclaims_unretained_surface_storage() {
        let mut runtime = Runtime::new(
            Counter {
                value: 0,
                updates: Vec::new(),
            },
            Size::new(3, 1),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap().unwrap();
        runtime.request_frame();
        runtime.render_if_dirty().unwrap().unwrap();

        let spare = runtime.spare_surface.as_ref().expect("reclaimed surface");
        assert_eq!(spare.width(), 3);
        assert_eq!(spare.height(), 1);
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
                SubscriptionMessage::Toggle => {
                    self.running = !self.running;
                    return Effect::none().without_redraw();
                }
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

    struct CachedActionApp;

    impl App for CachedActionApp {
        type Message = ();

        fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            Node::text("target").focusable("target").on_actions(
                "target",
                [Action::new(
                    ActionDescriptor::new(
                        "app.action",
                        "Action",
                        [KeyBinding::new(KeyStroke::character('x', Modifiers::NONE))],
                    ),
                    |_| EventResult::ignored(),
                )],
            )
        }
    }

    #[test]
    fn dispatch_reuses_the_resolved_action_route_for_unchanged_focus() {
        let mut runtime = Runtime::with_clock(
            CachedActionApp,
            RuntimeConfig::new(Size::new(8, 1)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        runtime.request_focus(&NodeId::from("target")).unwrap();
        runtime.active_action_groups().unwrap();
        let before = std::ptr::from_ref(runtime.resolved_action_route.as_ref().unwrap());
        let event = Event::Key(KeyEvent {
            code: KeyCode::Character('x'),
            modifiers: Modifiers::NONE,
            action: KeyAction::Press,
            text: Some("x".to_owned()),
            protocol: KeyProtocol::Legacy,
        });

        runtime.dispatch_event(&event).unwrap();
        runtime.dispatch_event(&event).unwrap();

        let after = std::ptr::from_ref(runtime.resolved_action_route.as_ref().unwrap());
        assert_eq!(before, after);
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

    struct PointerHandlerOrderApp {
        visits: Vec<&'static str>,
    }

    impl App for PointerHandlerOrderApp {
        type Message = FocusMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                FocusMessage::Visit(id) => self.visits.push(id),
            }
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            Node::rich_text([crate::TextSpan::new("ab", crate::Style::default())])
                .on_pointer_event("target", |context| {
                    let hit = context.text_hit().expect("paragraph text hit");
                    assert_eq!((hit.start(), hit.end()), (0, 1));
                    assert_eq!(context.local_position(), Point::new(0, 0));
                    assert_eq!(context.bounds(), Size::new(2, 1));
                    assert_eq!(context.visible_bounds(), Rect::new(0, 0, 2, 1));
                    assert!(!context.is_captured());
                    assert!(context.viewport().is_none());
                    EventResult::ignored().emit(FocusMessage::Visit("pointer"))
                })
                .on_event("target", |_| {
                    EventResult::message(FocusMessage::Visit("raw"))
                })
        }
    }

    #[test]
    fn geometry_pointer_handler_precedes_raw_handler() {
        let mut runtime = Runtime::with_clock(
            PointerHandlerOrderApp { visits: Vec::new() },
            RuntimeConfig::new(Size::new(2, 1)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        let dispatch = runtime
            .dispatch_event(&Event::Mouse(crate::MouseEvent {
                kind: MouseKind::Press,
                button: MouseButton::Left,
                x: 0,
                y: 0,
                modifiers: Modifiers::NONE,
            }))
            .unwrap();

        assert!(dispatch.consumed());
        assert_eq!(dispatch.messages(), 2);
        runtime.process_pending().unwrap();
        assert_eq!(runtime.app().visits, ["pointer", "raw"]);
    }

    struct ModalApp {
        visits: Vec<&'static str>,
        hard: bool,
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
            let mut modal = Node::modal("modal", input).on_event("modal", |_| {
                EventResult::ignored().emit(FocusMessage::Visit("modal"))
            });
            if self.hard {
                modal = modal.block_unhandled_events();
            }
            Node::stack([background, modal]).on_event("root", |_| {
                EventResult::message(FocusMessage::Visit("root"))
            })
        }
    }

    #[test]
    fn modal_restricts_focus_and_routes_through_modal_ancestors() {
        let mut runtime = Runtime::with_clock(
            ModalApp {
                visits: Vec::new(),
                hard: false,
            },
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

    #[test]
    fn event_boundary_stops_unhandled_event_at_modal() {
        let mut runtime = Runtime::with_clock(
            ModalApp {
                visits: Vec::new(),
                hard: true,
            },
            RuntimeConfig::new(Size::new(12, 1)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        assert!(runtime.request_focus(&NodeId::from("input")).unwrap());

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
        assert_eq!(routed.messages(), 2);
        runtime.step().unwrap();
        assert_eq!(runtime.app().visits, ["input", "modal"]);
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

    #[derive(Clone)]
    struct VirtualFlowEntry {
        key: &'static str,
        label: &'static str,
        height: u32,
    }

    struct VirtualFlowApp {
        entries: Vec<VirtualFlowEntry>,
        items: crate::VirtualFlowItems,
        update: crate::VirtualFlowUpdate,
        stick_to_end: bool,
        builds: Arc<Mutex<Vec<String>>>,
    }

    impl VirtualFlowApp {
        fn new(entries: Vec<VirtualFlowEntry>, stick_to_end: bool) -> Self {
            let items = virtual_flow_items(&entries);
            Self {
                entries,
                items,
                update: crate::VirtualFlowUpdate::reset(1),
                stick_to_end,
                builds: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn replace_entries(
            &mut self,
            entries: Vec<VirtualFlowEntry>,
            update: crate::VirtualFlowUpdate,
        ) {
            self.items = virtual_flow_items(&entries);
            self.entries = entries;
            self.update = update;
        }
    }

    impl App for VirtualFlowApp {
        type Message = ();

        fn update(&mut self, (): ()) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            let estimates = self.entries.clone();
            let entries = self.entries.clone();
            let builds = Arc::clone(&self.builds);
            let source = crate::VirtualFlowSource::new(self.items.clone(), move |context| {
                builds
                    .lock()
                    .unwrap()
                    .push(context.key().as_str().to_owned());
                let entry = entries
                    .iter()
                    .find(|entry| entry.key == context.key().as_str())
                    .expect("built virtual flow item exists");
                virtual_flow_item_node(entry)
            })
            .estimated_height(move |context| {
                estimates
                    .iter()
                    .find(|entry| entry.key == context.key().as_str())
                    .expect("estimated virtual flow item exists")
                    .height
            })
            .update(self.update.clone());
            Node::virtual_flow_with_options(
                "virtual-flow",
                source,
                crate::VirtualFlowOptions {
                    overscan: 1,
                    stick_to_end: self.stick_to_end,
                    ..crate::VirtualFlowOptions::default()
                },
            )
        }
    }

    fn virtual_flow_items(entries: &[VirtualFlowEntry]) -> crate::VirtualFlowItems {
        crate::VirtualFlowItems::new(
            entries
                .iter()
                .map(|entry| crate::VirtualFlowItem::new(entry.key)),
        )
        .unwrap()
    }

    fn virtual_flow_item_node(entry: &VirtualFlowEntry) -> Node<()> {
        Node::column((0..entry.height).map(|_| Node::text(entry.label)))
            .with_id(format!("item-{}", entry.key))
    }

    #[test]
    fn virtual_flow_measures_once_and_preserves_prepend_anchor() {
        let mut runtime = Runtime::with_clock(
            VirtualFlowApp::new(
                vec![
                    VirtualFlowEntry {
                        key: "a",
                        label: "A",
                        height: 2,
                    },
                    VirtualFlowEntry {
                        key: "b",
                        label: "B",
                        height: 3,
                    },
                    VirtualFlowEntry {
                        key: "c",
                        label: "C",
                        height: 1,
                    },
                    VirtualFlowEntry {
                        key: "d",
                        label: "D",
                        height: 2,
                    },
                ],
                false,
            ),
            RuntimeConfig::new(Size::new(4, 3)),
            VirtualClock::new(),
        )
        .unwrap();

        let initial = runtime.render_if_dirty().unwrap().unwrap();
        assert_eq!(initial.surface().cell(0, 0).unwrap().content(), "A");
        assert_eq!(initial.surface().cell(0, 2).unwrap().content(), "B");
        assert_eq!(&*runtime.app().builds.lock().unwrap(), &["a", "b"]);
        let state = runtime
            .interaction()
            .virtual_flow_state(&NodeId::from("virtual-flow"))
            .unwrap();
        assert_eq!(state.visible_range(), 0..2);

        runtime.app().builds.lock().unwrap().clear();
        assert!(runtime.set_scroll_offset(&NodeId::from("virtual-flow"), ScrollOffset::new(0, 2),));
        runtime.render_if_dirty().unwrap().unwrap();
        assert_eq!(&*runtime.app().builds.lock().unwrap(), &["a", "b", "c"]);

        runtime.app().builds.lock().unwrap().clear();
        runtime.app_mut().replace_entries(
            vec![
                VirtualFlowEntry {
                    key: "x",
                    label: "X",
                    height: 4,
                },
                VirtualFlowEntry {
                    key: "a",
                    label: "A",
                    height: 2,
                },
                VirtualFlowEntry {
                    key: "b",
                    label: "B",
                    height: 3,
                },
                VirtualFlowEntry {
                    key: "c",
                    label: "C",
                    height: 1,
                },
                VirtualFlowEntry {
                    key: "d",
                    label: "D",
                    height: 2,
                },
            ],
            crate::VirtualFlowUpdate::changed(2, 1, 0..1),
        );
        runtime.request_frame();
        let prepended = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(prepended.surface().cell(0, 0).unwrap().content(), "B");
        let state = runtime
            .interaction()
            .virtual_flow_state(&NodeId::from("virtual-flow"))
            .unwrap();
        assert_eq!(state.scroll().offset, ScrollOffset::new(0, 6));
        assert_eq!(state.anchor().unwrap().key().as_str(), "b");
        assert_eq!(&*runtime.app().builds.lock().unwrap(), &["a", "b", "c"]);
    }

    #[test]
    fn virtual_flow_follows_streaming_tail_growth() {
        let mut runtime = Runtime::with_clock(
            VirtualFlowApp::new(
                vec![
                    VirtualFlowEntry {
                        key: "a",
                        label: "A",
                        height: 2,
                    },
                    VirtualFlowEntry {
                        key: "b",
                        label: "B",
                        height: 2,
                    },
                    VirtualFlowEntry {
                        key: "c",
                        label: "C",
                        height: 1,
                    },
                ],
                true,
            ),
            RuntimeConfig::new(Size::new(4, 3)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap().unwrap();

        runtime.app_mut().replace_entries(
            vec![
                VirtualFlowEntry {
                    key: "a",
                    label: "A",
                    height: 2,
                },
                VirtualFlowEntry {
                    key: "b",
                    label: "B",
                    height: 2,
                },
                VirtualFlowEntry {
                    key: "c",
                    label: "C2",
                    height: 4,
                },
            ],
            crate::VirtualFlowUpdate::changed(2, 1, 2..3),
        );
        runtime.request_frame();
        let streamed = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(streamed.surface().cell(0, 0).unwrap().content(), "C");
        assert_eq!(streamed.surface().cell(1, 0).unwrap().content(), "2");
        let state = runtime
            .interaction()
            .virtual_flow_state(&NodeId::from("virtual-flow"))
            .unwrap();
        assert_eq!(state.scroll().offset, ScrollOffset::new(0, 5));
        assert_eq!(state.scroll().maximum, ScrollOffset::new(0, 5));
        assert!(state.scroll().at_end);
    }

    enum VirtualFlowScrollMessage {
        Scrolled(crate::ScrollState),
    }

    struct VirtualFlowScrollApp {
        items: crate::VirtualFlowItems,
        observed: Vec<crate::ScrollState>,
    }

    impl App for VirtualFlowScrollApp {
        type Message = VirtualFlowScrollMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                VirtualFlowScrollMessage::Scrolled(state) => self.observed.push(state),
            }
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            let source = crate::VirtualFlowSource::new(self.items.clone(), |context| {
                Node::text(context.key().as_str().to_owned())
            });
            Node::virtual_flow_with_options(
                "virtual-flow",
                source,
                crate::VirtualFlowOptions {
                    on_scroll: Some(Box::new(VirtualFlowScrollMessage::Scrolled)),
                    ..crate::VirtualFlowOptions::default()
                },
            )
        }
    }

    #[test]
    fn virtual_flow_routes_core_scroll_actions_and_user_callback() {
        let items = crate::VirtualFlowItems::new(
            ["a", "b", "c", "d", "e"]
                .into_iter()
                .map(crate::VirtualFlowItem::new),
        )
        .unwrap();
        let mut runtime = Runtime::with_clock(
            VirtualFlowScrollApp {
                items,
                observed: Vec::new(),
            },
            RuntimeConfig::new(Size::new(4, 2)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        runtime
            .request_focus(&NodeId::from("virtual-flow"))
            .unwrap();
        runtime.render_if_dirty().unwrap();

        let dispatched = runtime
            .dispatch_event(&Event::Key(KeyEvent {
                code: KeyCode::PageDown,
                modifiers: Modifiers::NONE,
                action: KeyAction::Unknown,
                text: None,
                protocol: KeyProtocol::Legacy,
            }))
            .unwrap();
        assert!(dispatched.consumed());
        runtime.process_pending().unwrap();
        runtime.render_if_dirty().unwrap();

        assert_eq!(runtime.app().observed.len(), 1);
        assert_eq!(runtime.app().observed[0].offset, ScrollOffset::new(0, 2));
        assert_eq!(
            runtime
                .interaction()
                .virtual_flow_state(&NodeId::from("virtual-flow"))
                .unwrap()
                .scroll()
                .offset,
            ScrollOffset::new(0, 2)
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

    struct GrowingVirtualScrollApp {
        content_height: u32,
        builds: Arc<AtomicUsize>,
        rows: Arc<AtomicUsize>,
    }

    impl App for GrowingVirtualScrollApp {
        type Message = ();

        fn update(&mut self, (): ()) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            let builds = Arc::clone(&self.builds);
            let rows = Arc::clone(&self.rows);
            Node::virtual_scroll_viewport_with_options(
                "growing-virtual-scroll",
                Size::new(3, self.content_height),
                crate::ScrollViewportOptions {
                    axis: crate::ScrollAxis::Vertical,
                    stick_to_end: true,
                    ..crate::ScrollViewportOptions::default()
                },
                move |viewport| {
                    builds.fetch_add(1, Ordering::Relaxed);
                    rows.fetch_add(viewport.size.height as usize, Ordering::Relaxed);
                    let start = viewport.offset.y;
                    let end = start.saturating_add(viewport.size.height);
                    let visible = (start..end).map(|row| Node::text(row.to_string()));
                    crate::VirtualFragment::new(ScrollOffset::new(0, start), Node::column(visible))
                },
            )
        }
    }

    #[test]
    fn virtual_scroll_viewport_builds_once_when_following_growing_end() {
        let builds = Arc::new(AtomicUsize::new(0));
        let rows = Arc::new(AtomicUsize::new(0));
        let mut runtime = Runtime::with_clock(
            GrowingVirtualScrollApp {
                content_height: 4,
                builds: Arc::clone(&builds),
                rows: Arc::clone(&rows),
            },
            RuntimeConfig::new(Size::new(3, 2)),
            VirtualClock::new(),
        )
        .unwrap();

        let initial = runtime.render_if_dirty().unwrap().unwrap();
        assert_eq!(builds.load(Ordering::Relaxed), 1);
        assert_eq!(rows.load(Ordering::Relaxed), 2);
        assert_eq!(initial.surface().cell(0, 0).unwrap().content(), "2");
        assert_eq!(initial.surface().cell(0, 1).unwrap().content(), "3");

        runtime.app_mut().content_height = 5;
        let grown = runtime.render_if_dirty().unwrap().unwrap();
        assert_eq!(builds.load(Ordering::Relaxed), 2);
        assert_eq!(rows.load(Ordering::Relaxed), 4);
        assert_eq!(grown.surface().cell(0, 0).unwrap().content(), "3");
        assert_eq!(grown.surface().cell(0, 1).unwrap().content(), "4");
        let state = runtime
            .interaction()
            .scroll_state(&NodeId::from("growing-virtual-scroll"))
            .unwrap();
        assert_eq!(state.offset, ScrollOffset::new(0, 3));
        assert_eq!(state.maximum, ScrollOffset::new(0, 3));
        assert!(state.at_end);

        assert!(runtime.set_scroll_offset(
            &NodeId::from("growing-virtual-scroll"),
            ScrollOffset::new(0, 1),
        ));
        runtime.render_if_dirty().unwrap().unwrap();
        runtime.app_mut().content_height = 6;
        let away = runtime.render_if_dirty().unwrap().unwrap();
        assert_eq!(builds.load(Ordering::Relaxed), 4);
        assert_eq!(rows.load(Ordering::Relaxed), 8);
        assert_eq!(away.surface().cell(0, 0).unwrap().content(), "1");
        assert_eq!(away.surface().cell(0, 1).unwrap().content(), "2");
        let state = runtime
            .interaction()
            .scroll_state(&NodeId::from("growing-virtual-scroll"))
            .unwrap();
        assert_eq!(state.offset, ScrollOffset::new(0, 1));
        assert_eq!(state.maximum, ScrollOffset::new(0, 4));
        assert!(!state.at_end);
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

    struct NestedRevealApp;

    impl App for NestedRevealApp {
        type Message = ();

        fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: crate::ViewContext) -> Node<Self::Message> {
            let rows = (0..5).map(|index| {
                let row = Node::text(index.to_string());
                let row = if index == 4 {
                    row.with_id("target")
                } else {
                    row
                };
                row.with_length(crate::Length::Fixed(1))
            });
            let inner = Node::scroll_viewport_with_options(
                "inner",
                Node::column(rows),
                crate::ScrollViewportOptions {
                    axis: crate::ScrollAxis::Vertical,
                    ..crate::ScrollViewportOptions::default()
                },
            )
            .reveal_descendant("target")
            .with_length(crate::Length::Fixed(2));
            Node::scroll_viewport_with_options(
                "outer",
                Node::column([
                    Node::text("header").with_length(crate::Length::Fixed(2)),
                    inner,
                ]),
                crate::ScrollViewportOptions {
                    axis: crate::ScrollAxis::Vertical,
                    ..crate::ScrollViewportOptions::default()
                },
            )
            .reveal_descendant("target")
        }
    }

    #[test]
    fn nested_explicit_reveal_adjusts_inner_before_outer() {
        let mut runtime = Runtime::with_clock(
            NestedRevealApp,
            RuntimeConfig::new(Size::new(8, 3)),
            VirtualClock::new(),
        )
        .unwrap();

        let frame = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(
            runtime.interaction().scroll_offset(&NodeId::from("inner")),
            ScrollOffset::new(0, 3)
        );
        assert_eq!(
            runtime.interaction().scroll_offset(&NodeId::from("outer")),
            ScrollOffset::new(0, 1)
        );
        assert_eq!(frame.surface().cell(0, 2).unwrap().content(), "4");
    }
}
