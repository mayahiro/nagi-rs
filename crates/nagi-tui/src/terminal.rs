use std::error::Error;
use std::fmt;
use std::io;
use std::time::Duration;

use crate::terminal_unix::{TerminalError, TerminalSession};
use crate::{
    App, Capabilities, Event, EventAction, MouseTracking, QueueFull, Runtime, RuntimeConfig,
    RuntimeError, RuntimeEventError, Size, SystemClock, TimedInputDecoder,
};

/// Settings for [`run_terminal`]
#[derive(Clone, Copy, Debug)]
pub struct TerminalOptions {
    /// Optional output capabilities used by the VT encoder
    pub capabilities: Capabilities,
    /// SGR mouse tracking policy, or `None` to preserve terminal text selection
    pub mouse_tracking: Option<MouseTracking>,
    /// Whether to focus the first focusable node before the initial frame
    pub focus_first: bool,
    /// Maximum time to disambiguate a lone ESC from an escape sequence
    pub escape_timeout: Duration,
    /// Longest wait before checking resize and scheduler state
    pub maximum_idle_wait: Duration,
    /// Maximum number of messages waiting in the runtime queue
    pub queue_capacity: usize,
    /// Maximum number of effect tasks executing concurrently
    pub task_limit: usize,
    /// Maximum pending values retained by each subscription source
    pub subscription_capacity: usize,
    /// Smallest interval between non-urgent rendered frames
    pub minimum_frame_interval: Duration,
}

impl Default for TerminalOptions {
    fn default() -> Self {
        Self {
            capabilities: Capabilities::BASELINE,
            mouse_tracking: None,
            focus_first: false,
            escape_timeout: Duration::from_millis(25),
            maximum_idle_wait: Duration::from_millis(50),
            queue_capacity: crate::DEFAULT_QUEUE_CAPACITY,
            task_limit: crate::DEFAULT_TASK_LIMIT,
            subscription_capacity: crate::DEFAULT_SUBSCRIPTION_CAPACITY,
            minimum_frame_interval: Duration::ZERO,
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
    mut map_event: Mapper,
) -> Result<Application, RunError>
where
    Application: App,
    Mapper: FnMut(Event) -> EventAction<Application::Message>,
{
    let mut session = TerminalSession::open(options.mouse_tracking).map_err(run_terminal_error)?;
    let result = run_terminal_session(&mut session, app, options, &mut map_event);
    let restoration = session.finish().map_err(run_terminal_error);
    match (result, restoration) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(app), Ok(())) => Ok(app),
    }
}

fn run_terminal_session<Application, Mapper>(
    session: &mut TerminalSession,
    app: Application,
    options: TerminalOptions,
    map_event: &mut Mapper,
) -> Result<Application, RunError>
where
    Application: App,
    Mapper: FnMut(Event) -> EventAction<Application::Message>,
{
    let (columns, rows) = session.size().map_err(run_terminal_error)?;
    let clock = SystemClock::new();
    let mut config = RuntimeConfig::new(Size::new(u32::from(columns), u32::from(rows)));
    config.queue_capacity = options.queue_capacity;
    config.task_limit = options.task_limit;
    config.subscription_capacity = options.subscription_capacity;
    config.minimum_frame_interval = options.minimum_frame_interval;
    let mut runtime = Runtime::with_clock(app, config, clock)?;
    let mut focus_first = options.focus_first;
    let mut decoder = TimedInputDecoder::new(clock, options.escape_timeout);
    let mut input = [0_u8; 8_192];

    loop {
        if session.take_resize() {
            let (columns, rows) = session.size().map_err(run_terminal_error)?;
            runtime.resize(Size::new(u32::from(columns), u32::from(rows)));
        }
        runtime.process_pending()?;
        if focus_first {
            focus_first = false;
            runtime.focus_first()?;
        }
        write_pending_frame(session, &mut runtime, options.capabilities)?;
        if runtime.exit_requested() {
            break;
        }

        let mut timeout = decoder
            .time_until_deadline()
            .map_or(options.maximum_idle_wait, |deadline| {
                deadline.min(options.maximum_idle_wait)
            });
        if let Some(deadline) = runtime.time_until_effect_deadline() {
            timeout = timeout.min(deadline);
        }
        if let Some(deadline) = runtime.time_until_subscription_deadline() {
            timeout = timeout.min(deadline);
        }
        if let Some(deadline) = runtime.time_until_frame_deadline() {
            timeout = timeout.min(deadline);
        }
        let readable = session.wait_readable(timeout).map_err(run_terminal_error)?;
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
            if runtime.dispatch_event(&event)?.consumed() {
                continue;
            }
            match map_event(event) {
                EventAction::Message(message) => runtime.enqueue(message)?,
                EventAction::Exit => {
                    exit = true;
                    break;
                }
                EventAction::Ignore => {}
            }
        }
        runtime.process_pending()?;
        write_pending_frame(session, &mut runtime, options.capabilities)?;
        if exit || runtime.exit_requested() {
            break;
        }
    }
    Ok(runtime.into_app())
}

fn write_pending_frame<Application: App>(
    session: &mut TerminalSession,
    runtime: &mut Runtime<Application, SystemClock>,
    capabilities: Capabilities,
) -> Result<(), RunError> {
    if let Some(frame) = runtime.render_if_dirty()? {
        session
            .write_operations(frame.operations(), capabilities)
            .map_err(run_terminal_error)?;
    }
    Ok(())
}

fn run_terminal_error(error: TerminalError) -> RunError {
    let (operation, source) = error.into_parts();
    RunError::Terminal { operation, source }
}

#[cfg(test)]
mod tests {
    use super::TerminalOptions;

    #[test]
    fn defaults_preserve_unfocused_non_mouse_behavior() {
        let options = TerminalOptions::default();
        assert_eq!(options.mouse_tracking, None);
        assert!(!options.focus_first);
    }
}
