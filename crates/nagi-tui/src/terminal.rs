use std::error::Error;
use std::fmt;
use std::io;
use std::time::Duration;

use nagi_text::WidthProfile;

use crate::terminal_unix::{TerminalError, TerminalSession};
use crate::{
    App, Capabilities, Event, EventAction, MouseTracking, QueueFull, Runtime, RuntimeConfig,
    RuntimeError, RuntimeEventError, RuntimeNotice, Size, SystemClock, TerminalOp,
    TimedInputDecoder,
};

/// Error returned when an inline terminal viewport has no rows
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidInlineViewportHeight;

impl fmt::Display for InvalidInlineViewportHeight {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an inline terminal viewport requires a positive height")
    }
}

impl Error for InvalidInlineViewportHeight {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum TerminalViewportKind {
    #[default]
    Fullscreen,
    Inline,
}

/// Screen region owned by the standard terminal runner
///
/// The default is a full-screen alternate-screen viewport.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TerminalViewport {
    kind: TerminalViewportKind,
    inline_height: u16,
}

impl TerminalViewport {
    /// Full-screen alternate-screen ownership
    pub const FULLSCREEN: Self = Self {
        kind: TerminalViewportKind::Fullscreen,
        inline_height: 0,
    };

    /// Creates a main-screen viewport with a positive requested row count
    ///
    /// The terminal runner clamps this height to the current terminal height.
    pub const fn inline(height: u16) -> Result<Self, InvalidInlineViewportHeight> {
        if height == 0 {
            return Err(InvalidInlineViewportHeight);
        }
        Ok(Self {
            kind: TerminalViewportKind::Inline,
            inline_height: height,
        })
    }

    /// Returns the requested row count for an inline viewport
    #[must_use]
    pub const fn inline_height(self) -> Option<u16> {
        match self.kind {
            TerminalViewportKind::Fullscreen => None,
            TerminalViewportKind::Inline => Some(self.inline_height),
        }
    }
}

/// Standard terminal handling for application clipboard requests
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerminalClipboard {
    /// Drop clipboard requests without terminal output
    #[default]
    Disabled,
    /// Write clipboard requests through direct, write-only OSC 52 sequences
    ///
    /// The terminal runner does not detect support or add multiplexer wrapping
    Osc52,
}

/// Settings for [`run_terminal`]
#[derive(Clone, Copy, Debug)]
pub struct TerminalOptions {
    /// Optional output capabilities used by the VT encoder
    pub capabilities: Capabilities,
    /// SGR mouse tracking policy, or `None` to preserve terminal text selection
    pub mouse_tracking: Option<MouseTracking>,
    /// Clipboard output policy, disabled by default
    pub clipboard: TerminalClipboard,
    /// Terminal screen region owned by the runner
    pub viewport: TerminalViewport,
    /// Maximum wait for an inline viewport cursor-position report
    ///
    /// Zero performs an immediate query check.
    pub cursor_query_timeout: Duration,
    /// Whether to focus the first focusable node before the initial frame
    pub focus_first: bool,
    /// Maximum time to disambiguate a lone ESC from an escape sequence
    pub escape_timeout: Duration,
    /// Maximum number of messages waiting in the runtime queue
    pub queue_capacity: usize,
    /// Maximum number of effect tasks executing concurrently
    pub task_limit: usize,
    /// Maximum pending values retained by each subscription source
    pub subscription_capacity: usize,
    /// Maximum retained asynchronous lifecycle notices
    pub runtime_notice_capacity: usize,
    /// Smallest interval between non-urgent rendered frames
    ///
    /// The default limits rendering to 120 frames per second. Zero disables
    /// the limit.
    pub minimum_frame_interval: Duration,
    /// Terminal cell-width policy used by the complete view
    ///
    /// A Custom override must return stable widths for this Runtime's lifetime
    pub width_profile: WidthProfile<'static>,
}

impl Default for TerminalOptions {
    fn default() -> Self {
        Self {
            capabilities: Capabilities::BASELINE,
            mouse_tracking: None,
            clipboard: TerminalClipboard::Disabled,
            viewport: TerminalViewport::FULLSCREEN,
            cursor_query_timeout: Duration::from_millis(100),
            focus_first: false,
            escape_timeout: Duration::from_millis(25),
            queue_capacity: crate::DEFAULT_QUEUE_CAPACITY,
            task_limit: crate::DEFAULT_TASK_LIMIT,
            subscription_capacity: crate::DEFAULT_SUBSCRIPTION_CAPACITY,
            runtime_notice_capacity: crate::DEFAULT_RUNTIME_NOTICE_CAPACITY,
            minimum_frame_interval: Duration::from_nanos(8_333_334),
            width_profile: WidthProfile::MODERN,
        }
    }
}

/// An error from the application terminal loop
#[derive(Debug)]
pub enum RunError {
    /// Terminal setup, I/O, or restoration failed
    Terminal {
        /// Operation that failed
        operation: &'static str,
        /// Underlying operating-system I/O error
        source: io::Error,
    },
    /// Runtime construction or rendering failed
    Runtime(RuntimeError),
    /// The bounded application message queue filled
    QueueFull,
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Terminal { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::Runtime(error) => error.fmt(formatter),
            Self::QueueFull => QueueFull.fmt(formatter),
        }
    }
}

impl Error for RunError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Terminal { source, .. } => Some(source),
            Self::Runtime(error) => Some(error),
            Self::QueueFull => None,
        }
    }
}

impl From<RuntimeError> for RunError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl From<QueueFull> for RunError {
    fn from(_: QueueFull) -> Self {
        Self::QueueFull
    }
}

impl From<RuntimeEventError> for RunError {
    fn from(error: RuntimeEventError) -> Self {
        match error {
            RuntimeEventError::Runtime(error) => Self::Runtime(error),
            RuntimeEventError::QueueFull => Self::QueueFull,
        }
    }
}

/// Runs an application in the process terminal until the application or mapper
/// requests exit, or terminal input reaches EOF
///
/// The terminal session restores raw mode and screen state on normal, error,
/// and panic exits. The returned application contains its final state.
pub fn run_terminal<Application, Mapper>(
    app: Application,
    options: TerminalOptions,
    map_event: Mapper,
) -> Result<Application, RunError>
where
    Application: App,
    Mapper: FnMut(Event) -> EventAction<Application::Message>,
{
    run_terminal_with_notice_handler(app, options, map_event, |_| {})
}

/// Runs an application while synchronously observing recovered failures and
/// unexpected asynchronous lifecycle transitions
///
/// The terminal session restores raw mode and screen state on normal, error,
/// and panic exits. The returned application contains its final state
pub fn run_terminal_with_notice_handler<Application, Mapper, Handler>(
    app: Application,
    options: TerminalOptions,
    mut map_event: Mapper,
    mut handle_notice: Handler,
) -> Result<Application, RunError>
where
    Application: App,
    Mapper: FnMut(Event) -> EventAction<Application::Message>,
    Handler: FnMut(&RuntimeNotice),
{
    let mut session = TerminalSession::open(
        options.mouse_tracking,
        options.viewport,
        options.cursor_query_timeout,
    )
    .map_err(run_terminal_error)?;
    let result = run_terminal_session(
        &mut session,
        app,
        options,
        &mut map_event,
        &mut handle_notice,
    );
    let restoration = session.finish().map_err(run_terminal_error);
    match (result, restoration) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(app), Ok(())) => Ok(app),
    }
}

fn run_terminal_session<Application, Mapper, Handler>(
    session: &mut TerminalSession,
    app: Application,
    options: TerminalOptions,
    map_event: &mut Mapper,
    handle_notice: &mut Handler,
) -> Result<Application, RunError>
where
    Application: App,
    Mapper: FnMut(Event) -> EventAction<Application::Message>,
    Handler: FnMut(&RuntimeNotice),
{
    let (columns, rows) = session.viewport_size().map_err(run_terminal_error)?;
    let clock = SystemClock::new();
    let mut config = RuntimeConfig::new(Size::new(u32::from(columns), u32::from(rows)));
    config.queue_capacity = options.queue_capacity;
    config.task_limit = options.task_limit;
    config.subscription_capacity = options.subscription_capacity;
    config.runtime_notice_capacity = options.runtime_notice_capacity;
    config.minimum_frame_interval = options.minimum_frame_interval;
    config.width_profile = options.width_profile;
    let mut runtime = Runtime::with_clock_and_wake(app, config, clock, session.wake_handle())?;
    let mut decoder = TimedInputDecoder::new(clock, options.escape_timeout);
    let mut input = [0_u8; 8_192];

    if session.take_resize() {
        let (columns, rows) = session.refresh_viewport().map_err(run_terminal_error)?;
        runtime.resize(Size::new(u32::from(columns), u32::from(rows)));
    }
    runtime.process_pending()?;
    handle_runtime_notices(&mut runtime, handle_notice);
    if !runtime.exit_requested() {
        run_pending_terminal_tasks(session, &mut runtime, &mut decoder, handle_notice)?;
    }
    if options.focus_first {
        runtime.focus_first()?;
    }
    write_pending_output(
        session,
        &mut runtime,
        options.capabilities,
        options.clipboard,
    )?;

    while !runtime.exit_requested() {
        let timeout = nearest_terminal_deadline([
            decoder.time_until_deadline(),
            runtime.time_until_effect_deadline(),
            runtime.time_until_subscription_deadline(),
            runtime.time_until_frame_deadline(),
        ]);
        let readable = session.wait(timeout).map_err(run_terminal_error)?;
        if session.take_resize() {
            let (columns, rows) = session.refresh_viewport().map_err(run_terminal_error)?;
            runtime.resize(Size::new(u32::from(columns), u32::from(rows)));
        }
        let mut events = if readable {
            let read = session.read(&mut input).map_err(run_terminal_error)?;
            if read == 0 {
                break;
            }
            decoder.feed(&input[..read])
        } else {
            Vec::new()
        };
        events.extend(decoder.poll());

        let mut exit = false;
        for event in events {
            let Some(event) = session.localize_event(event) else {
                continue;
            };
            let dispatch = runtime.dispatch_event(&event)?;
            if !dispatch.consumed() {
                match map_event(event) {
                    EventAction::Message(message) => runtime.enqueue(message)?,
                    EventAction::Exit => exit = true,
                    EventAction::Ignore => {}
                }
            }
            runtime.process_queued()?;
            if exit || runtime.exit_requested() {
                break;
            }
            if run_pending_terminal_tasks(session, &mut runtime, &mut decoder, handle_notice)? {
                break;
            }
        }
        runtime.process_pending()?;
        handle_runtime_notices(&mut runtime, handle_notice);
        if !exit && !runtime.exit_requested() {
            run_pending_terminal_tasks(session, &mut runtime, &mut decoder, handle_notice)?;
        }
        write_pending_output(
            session,
            &mut runtime,
            options.capabilities,
            options.clipboard,
        )?;
        if exit || runtime.exit_requested() {
            break;
        }
    }
    Ok(runtime.into_app())
}

fn run_pending_terminal_tasks<Application, Handler>(
    session: &mut TerminalSession,
    runtime: &mut Runtime<Application, SystemClock>,
    decoder: &mut TimedInputDecoder<SystemClock>,
    handle_notice: &mut Handler,
) -> Result<bool, RunError>
where
    Application: App,
    Handler: FnMut(&RuntimeNotice),
{
    let mut ran = false;
    while !runtime.exit_requested() && runtime.pending_terminal_tasks() > 0 {
        session.suspend().map_err(run_terminal_error)?;
        let ran_task = runtime.run_terminal_task();
        session.resume().map_err(run_terminal_error)?;
        debug_assert!(ran_task);
        if !ran_task {
            break;
        }

        decoder.reset();
        runtime.invalidate_terminal_surface();
        let (columns, rows) = session.viewport_size().map_err(run_terminal_error)?;
        runtime.resize(Size::new(u32::from(columns), u32::from(rows)));
        runtime.process_pending()?;
        handle_runtime_notices(runtime, handle_notice);
        ran = true;
    }
    Ok(ran)
}

fn handle_runtime_notices<Application, C, Handler>(
    runtime: &mut Runtime<Application, C>,
    handler: &mut Handler,
) where
    Application: App,
    C: crate::Clock,
    Handler: FnMut(&RuntimeNotice),
{
    for notice in runtime.drain_runtime_notices() {
        handler(&notice);
    }
}

fn nearest_terminal_deadline(deadlines: [Option<Duration>; 4]) -> Option<Duration> {
    deadlines.into_iter().flatten().min()
}

fn write_pending_output<Application: App>(
    session: &mut TerminalSession,
    runtime: &mut Runtime<Application, SystemClock>,
    capabilities: Capabilities,
    clipboard: TerminalClipboard,
) -> Result<(), RunError> {
    let frame = runtime.render_if_dirty()?;
    let clipboard_operation = take_clipboard_operation(runtime, clipboard);
    match (frame.as_ref(), clipboard_operation.as_ref()) {
        (Some(frame), extra) => session
            .write_viewport_operations_with_extra(
                frame.operations(),
                frame.surface().cursor().map(|cursor| (cursor.x, cursor.y)),
                extra,
                capabilities,
            )
            .map_err(run_terminal_error)?,
        (None, Some(operation)) => session
            .write_operations(std::slice::from_ref(operation), capabilities)
            .map_err(run_terminal_error)?,
        (None, None) => {}
    }
    Ok(())
}

fn take_clipboard_operation<Application: App, C: crate::Clock>(
    runtime: &mut Runtime<Application, C>,
    clipboard: TerminalClipboard,
) -> Option<TerminalOp> {
    let request = runtime.take_clipboard_request()?;
    (clipboard == TerminalClipboard::Osc52).then(|| TerminalOp::SetClipboard(request.into_text()))
}

fn run_terminal_error(error: TerminalError) -> RunError {
    let (operation, source) = error.into_parts();
    RunError::Terminal { operation, source }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{App, Effect, Node, Runtime, RuntimeConfig, Size, ViewContext, VirtualClock};

    use super::{
        TerminalClipboard, TerminalOptions, TerminalViewport, nearest_terminal_deadline,
        take_clipboard_operation,
    };

    struct ClipboardApp;

    impl App for ClipboardApp {
        type Message = &'static str;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            Effect::set_clipboard(message).without_redraw()
        }

        fn view(&self, _context: ViewContext) -> Node<Self::Message> {
            Node::text("view")
        }
    }

    #[test]
    fn defaults_preserve_unfocused_non_mouse_behavior() {
        let options = TerminalOptions::default();
        assert_eq!(options.mouse_tracking, None);
        assert_eq!(options.clipboard, TerminalClipboard::Disabled);
        assert_eq!(options.viewport, TerminalViewport::FULLSCREEN);
        assert_eq!(options.cursor_query_timeout, Duration::from_millis(100));
        assert!(!options.focus_first);
        assert_eq!(
            options.minimum_frame_interval,
            std::time::Duration::from_nanos(8_333_334)
        );
    }

    #[test]
    fn inline_viewport_requires_positive_height() {
        assert_eq!(
            TerminalViewport::inline(0),
            Err(super::InvalidInlineViewportHeight)
        );
        let viewport = TerminalViewport::inline(4).unwrap();
        assert_eq!(viewport.inline_height(), Some(4));
        assert_eq!(TerminalViewport::default().inline_height(), None);
    }

    #[test]
    fn terminal_wait_uses_no_timeout_without_a_deadline() {
        assert_eq!(nearest_terminal_deadline([None, None, None, None]), None);
        assert_eq!(
            nearest_terminal_deadline([
                Some(Duration::from_millis(25)),
                None,
                Some(Duration::from_secs(1)),
                Some(Duration::from_millis(8)),
            ]),
            Some(Duration::from_millis(8))
        );
    }

    #[test]
    fn clipboard_output_is_explicit_and_does_not_require_a_frame() {
        let mut runtime = Runtime::with_clock(
            ClipboardApp,
            RuntimeConfig::new(Size::new(8, 1)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        runtime.enqueue("copy").unwrap();
        runtime.process_pending().unwrap();
        assert_eq!(
            take_clipboard_operation(&mut runtime, TerminalClipboard::Disabled),
            None
        );

        runtime.enqueue("copy").unwrap();
        runtime.process_pending().unwrap();
        assert_eq!(
            take_clipboard_operation(&mut runtime, TerminalClipboard::Osc52),
            Some(crate::TerminalOp::SetClipboard("copy".to_owned()))
        );
    }
}
