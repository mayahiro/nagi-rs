//! TTY-aware status reporting layered above Nagi CLI Core
//!
//! [`Reporter`] renders application-owned [`Snapshot`] values through an
//! injected [`StatusIo`] boundary. Terminal output uses one transient line,
//! while non-terminal output falls back to newline-delimited plain logs

#![deny(missing_docs)]
#![deny(unsafe_code)]

use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::io::{self, Write};

use nagi_text::{WidthProfile, truncate};

#[allow(unsafe_code)]
mod process_unix;

pub use process_unix::ProcessIo;

/// Default maximum UTF-8 bytes in one status or log message
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 65_536;

/// Default width used when a terminal does not report its current columns
pub const DEFAULT_FALLBACK_TERMINAL_WIDTH: usize = 80;

/// Default number of cells inside a determinate progress bar
pub const DEFAULT_PROGRESS_WIDTH: usize = 20;

/// Maximum configurable number of cells inside a progress bar
pub const MAX_PROGRESS_WIDTH: usize = 1_024;

/// Number of stable ASCII spinner frames
pub const SPINNER_FRAME_COUNT: usize = 4;

const CLEAR_LINE: &[u8] = b"\r\x1B[2K";
const SPINNER_FRAMES: [&str; SPINNER_FRAME_COUNT] = ["-", "\\", "|", "/"];

/// The semantic kind of one [`Snapshot`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotKind {
    /// A plain status message
    Status,
    /// An indeterminate spinner driven by an application tick
    Spinner,
    /// Determinate current and total progress
    Progress,
}

/// One immutable status-line value supplied by an application
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Snapshot<'message> {
    /// A plain status message
    Status(&'message str),
    /// An indeterminate spinner with an application-owned tick
    Spinner {
        /// Monotonically increasing or otherwise application-defined tick
        tick: u64,
        /// Optional human-readable label
        message: &'message str,
    },
    /// Determinate progress with an optional human-readable label
    Progress {
        /// Completed units before clamping to `total`
        current: u64,
        /// Total units, where zero produces zero completed cells
        total: u64,
        /// Optional human-readable label
        message: &'message str,
    },
}

impl<'message> Snapshot<'message> {
    /// Constructs a plain status message
    #[must_use]
    pub const fn status(message: &'message str) -> Self {
        Self::Status(message)
    }

    /// Constructs an application-driven spinner snapshot
    #[must_use]
    pub const fn spinner(tick: u64, message: &'message str) -> Self {
        Self::Spinner { tick, message }
    }

    /// Constructs a determinate progress snapshot
    #[must_use]
    pub const fn progress(current: u64, total: u64, message: &'message str) -> Self {
        Self::Progress {
            current,
            total,
            message,
        }
    }

    /// Returns the semantic snapshot kind
    #[must_use]
    pub const fn kind(self) -> SnapshotKind {
        match self {
            Self::Status(_) => SnapshotKind::Status,
            Self::Spinner { .. } => SnapshotKind::Spinner,
            Self::Progress { .. } => SnapshotKind::Progress,
        }
    }

    /// Returns the human-readable message
    #[must_use]
    pub const fn message(self) -> &'message str {
        match self {
            Self::Status(message)
            | Self::Spinner { message, .. }
            | Self::Progress { message, .. } => message,
        }
    }

    /// Returns the spinner tick when this is a Spinner snapshot
    #[must_use]
    pub const fn tick(self) -> Option<u64> {
        match self {
            Self::Spinner { tick, .. } => Some(tick),
            _ => None,
        }
    }

    /// Returns current and total units when this is a Progress snapshot
    #[must_use]
    pub const fn progress_values(self) -> Option<(u64, u64)> {
        match self {
            Self::Progress { current, total, .. } => Some((current, total)),
            _ => None,
        }
    }
}

/// Injected terminal and stream operations used by [`Reporter`]
///
/// `terminal_width` returns current columns or `None` when they are unknown.
/// A concrete width must be positive. Terminal availability should remain
/// stable for one Reporter lifetime. Implementations must not retain buffers
/// passed to [`Write::write`]
pub trait StatusIo: Write {
    /// Reports whether output supports a transient ANSI status line
    fn is_terminal(&self) -> bool;

    /// Returns current terminal columns when available
    fn terminal_width(&self) -> Option<usize>;
}

impl<T: StatusIo + ?Sized> StatusIo for &mut T {
    fn is_terminal(&self) -> bool {
        (**self).is_terminal()
    }

    fn terminal_width(&self) -> Option<usize> {
        (**self).terminal_width()
    }
}

/// Immutable rendering and resource options for one [`Reporter`]
#[derive(Clone, Copy, Debug)]
pub struct Options {
    max_message_bytes: usize,
    fallback_terminal_width: usize,
    progress_width: usize,
    width_profile: WidthProfile<'static>,
    width_profile_explicit: bool,
}

impl Options {
    /// Returns the maximum UTF-8 bytes accepted in one message
    #[must_use]
    pub const fn max_message_bytes(self) -> usize {
        self.max_message_bytes
    }

    /// Returns columns used when terminal width is unavailable
    #[must_use]
    pub const fn fallback_terminal_width(self) -> usize {
        self.fallback_terminal_width
    }

    /// Returns the configured progress-bar width
    #[must_use]
    pub const fn progress_width(self) -> usize {
        self.progress_width
    }

    /// Returns the terminal cell-width policy
    #[must_use]
    pub const fn width_profile(self) -> WidthProfile<'static> {
        self.width_profile
    }

    /// Replaces the maximum message byte count
    ///
    /// A zero value is rejected by [`Reporter::with_options`]
    #[must_use]
    pub const fn with_max_message_bytes(mut self, value: usize) -> Self {
        self.max_message_bytes = value;
        self
    }

    /// Replaces columns used when terminal width is unavailable
    ///
    /// A zero value is rejected by [`Reporter::with_options`]
    #[must_use]
    pub const fn with_fallback_terminal_width(mut self, value: usize) -> Self {
        self.fallback_terminal_width = value;
        self
    }

    /// Replaces the progress-bar width
    ///
    /// Values outside `1..=`[`MAX_PROGRESS_WIDTH`] are rejected by
    /// [`Reporter::with_options`]
    #[must_use]
    pub const fn with_progress_width(mut self, value: usize) -> Self {
        self.progress_width = value;
        self
    }

    /// Replaces the terminal cell-width policy
    #[must_use]
    pub const fn with_width_profile(mut self, value: WidthProfile<'static>) -> Self {
        self.width_profile = value;
        self.width_profile_explicit = true;
        self
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
            fallback_terminal_width: DEFAULT_FALLBACK_TERMINAL_WIDTH,
            progress_width: DEFAULT_PROGRESS_WIDTH,
            width_profile: WidthProfile::MODERN,
            width_profile_explicit: false,
        }
    }
}

/// Classifies a status-reporting failure independently of display text
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusErrorKind {
    /// Options or message metadata violate the status contract
    InvalidStatus,
    /// Injected or process output failed
    Io,
}

/// A structured status-reporting failure
#[derive(Debug)]
pub struct StatusError {
    kind: StatusErrorKind,
    message: String,
    source: Option<io::Error>,
}

impl StatusError {
    /// Returns the stable failure kind
    #[must_use]
    pub const fn kind(&self) -> StatusErrorKind {
        self.kind
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self {
            kind: StatusErrorKind::InvalidStatus,
            message: message.into(),
            source: None,
        }
    }

    fn io(context: &'static str, source: io::Error) -> Self {
        Self {
            kind: StatusErrorKind::Io,
            message: context.to_owned(),
            source: Some(source),
        }
    }
}

impl fmt::Display for StatusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "status reporting failed: {}", self.message)
    }
}

impl Error for StatusError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

/// A synchronous TTY-aware reporter with bounded retained rendering buffers
///
/// Reporter creates no thread or timer. The application owns spinner ticks,
/// progress updates, and serialization with any other writer using the same
/// output. Reporter is not safe for concurrent use
pub struct Reporter<I> {
    io: I,
    options: Options,
    active: bool,
    last_terminal: String,
    last_log: String,
    scratch: String,
}

impl<I: StatusIo> Reporter<I> {
    /// Constructs a reporter with portable default options
    #[must_use]
    pub fn new(io: I) -> Self {
        Self::build(io, Options::default())
    }

    /// Constructs a reporter after validating explicit options
    pub fn with_options(io: I, options: Options) -> Result<Self, StatusError> {
        validate_options(options)?;
        Ok(Self::build(io, options))
    }

    fn build(io: I, options: Options) -> Self {
        Self {
            io,
            options,
            active: false,
            last_terminal: String::new(),
            last_log: String::new(),
            scratch: String::new(),
        }
    }

    /// Returns immutable rendering options
    #[must_use]
    pub const fn options(&self) -> Options {
        self.options
    }

    /// Returns whether a transient terminal line is currently active
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    /// Returns shared access to injected I/O
    #[must_use]
    pub const fn io(&self) -> &I {
        &self.io
    }

    /// Consumes the reporter and returns injected I/O without implicit output
    ///
    /// Call [`Reporter::clear`] or [`Reporter::finish`] first when a terminal
    /// line is active
    #[must_use]
    pub fn into_inner(self) -> I {
        self.io
    }

    /// Updates the transient line or emits a plain non-terminal log record
    ///
    /// The result is `true` only when bytes were written. Repeated identical
    /// terminal lines and repeated identical non-terminal fallback records are
    /// coalesced
    pub fn update(&mut self, snapshot: Snapshot<'_>) -> Result<bool, StatusError> {
        self.validate_message(snapshot.message())?;
        if self.io.is_terminal() {
            self.update_terminal(snapshot)
        } else {
            self.update_log(snapshot)
        }
    }

    /// Commits a final terminal line or final plain fallback record
    ///
    /// Finishing resets coalescing state so a later identical update starts a
    /// new reporting lifecycle
    pub fn finish(&mut self, snapshot: Snapshot<'_>) -> Result<bool, StatusError> {
        self.validate_message(snapshot.message())?;
        if self.io.is_terminal() {
            self.finish_terminal(snapshot)
        } else {
            let emitted = self.update_log(snapshot)?;
            self.last_log.clear();
            self.active = false;
            Ok(emitted)
        }
    }

    /// Removes an active terminal line and resets coalescing state
    ///
    /// Non-terminal output is never erased. The result reports whether bytes
    /// were written
    pub fn clear(&mut self) -> Result<bool, StatusError> {
        let emitted = if self.active && self.io.is_terminal() {
            write_and_flush(&mut self.io, &[CLEAR_LINE])?;
            true
        } else {
            false
        };
        self.active = false;
        self.last_terminal.clear();
        self.last_log.clear();
        Ok(emitted)
    }

    /// Writes one permanent plain log line without losing an active status
    ///
    /// On a terminal the transient line is erased, the log is written, and
    /// the previous status line is repainted in one flushed operation
    pub fn log(&mut self, message: &str) -> Result<(), StatusError> {
        self.validate_message(message)?;
        if self.active && self.io.is_terminal() {
            write_and_flush(
                &mut self.io,
                &[
                    CLEAR_LINE,
                    message.as_bytes(),
                    b"\n",
                    CLEAR_LINE,
                    self.last_terminal.as_bytes(),
                ],
            )
        } else {
            write_and_flush(&mut self.io, &[message.as_bytes(), b"\n"])
        }
    }

    fn update_terminal(&mut self, snapshot: Snapshot<'_>) -> Result<bool, StatusError> {
        self.render_terminal(snapshot);
        if self.active && self.last_terminal == self.scratch {
            return Ok(false);
        }
        write_and_flush(&mut self.io, &[CLEAR_LINE, self.scratch.as_bytes()])?;
        std::mem::swap(&mut self.last_terminal, &mut self.scratch);
        self.active = true;
        Ok(true)
    }

    fn finish_terminal(&mut self, snapshot: Snapshot<'_>) -> Result<bool, StatusError> {
        self.render_terminal(snapshot);
        if self.active && self.last_terminal == self.scratch {
            write_and_flush(&mut self.io, &[b"\n"])?;
        } else {
            write_and_flush(&mut self.io, &[CLEAR_LINE, self.scratch.as_bytes(), b"\n"])?;
        }
        self.active = false;
        self.last_terminal.clear();
        self.last_log.clear();
        Ok(true)
    }

    fn update_log(&mut self, snapshot: Snapshot<'_>) -> Result<bool, StatusError> {
        render_fallback(snapshot, &mut self.scratch);
        if self.scratch.is_empty() || self.last_log == self.scratch {
            return Ok(false);
        }
        write_and_flush(&mut self.io, &[self.scratch.as_bytes(), b"\n"])?;
        std::mem::swap(&mut self.last_log, &mut self.scratch);
        self.active = false;
        Ok(true)
    }

    fn render_terminal(&mut self, snapshot: Snapshot<'_>) {
        let terminal_width = self
            .io
            .terminal_width()
            .filter(|width| *width != 0)
            .unwrap_or(self.options.fallback_terminal_width);
        let usable_width = terminal_width.saturating_sub(1);
        render_terminal(snapshot, self.options, usable_width, &mut self.scratch);
    }

    fn validate_message(&self, message: &str) -> Result<(), StatusError> {
        if message.len() > self.options.max_message_bytes {
            return Err(StatusError::invalid("message exceeds its byte limit"));
        }
        if message.chars().any(char::is_control) {
            return Err(StatusError::invalid("message contains a control character"));
        }
        Ok(())
    }
}

fn validate_options(options: Options) -> Result<(), StatusError> {
    if options.max_message_bytes == 0 {
        return Err(StatusError::invalid(
            "maximum message bytes must be non-zero",
        ));
    }
    if options.fallback_terminal_width == 0 {
        return Err(StatusError::invalid(
            "fallback terminal width must be non-zero",
        ));
    }
    if options.progress_width == 0 || options.progress_width > MAX_PROGRESS_WIDTH {
        return Err(StatusError::invalid(format!(
            "progress width must be between 1 and {MAX_PROGRESS_WIDTH}",
        )));
    }
    Ok(())
}

fn render_terminal(
    snapshot: Snapshot<'_>,
    options: Options,
    usable_width: usize,
    output: &mut String,
) {
    if options.width_profile_explicit {
        render_terminal_profiled(snapshot, options, usable_width, output);
        return;
    }
    output.clear();
    match snapshot {
        Snapshot::Status(message) => {
            append_text(output, message, usable_width, options.width_profile);
        }
        Snapshot::Spinner { tick, message } => {
            append_ascii(
                output,
                SPINNER_FRAMES[(tick % SPINNER_FRAMES.len() as u64) as usize],
                usable_width,
            );
            append_message(output, message, usable_width, options.width_profile);
        }
        Snapshot::Progress {
            current,
            total,
            message,
        } => {
            let current = normalized_current(current, total);
            let complete = completed_cells(current, total, options.progress_width);
            append_ascii(output, "[", usable_width);
            append_repeated(output, '#', complete, usable_width);
            append_repeated(
                output,
                '-',
                options.progress_width.saturating_sub(complete),
                usable_width,
            );
            append_ascii(output, "] ", usable_width);
            append_u64(output, current, usable_width);
            append_ascii(output, "/", usable_width);
            append_u64(output, total, usable_width);
            append_message(output, message, usable_width, options.width_profile);
        }
    }
}

fn render_terminal_profiled(
    snapshot: Snapshot<'_>,
    options: Options,
    usable_width: usize,
    output: &mut String,
) {
    output.clear();
    match snapshot {
        Snapshot::Status(message) => output.push_str(message),
        Snapshot::Spinner { tick, message } => {
            output.push_str(SPINNER_FRAMES[(tick % SPINNER_FRAMES.len() as u64) as usize]);
            if !message.is_empty() {
                output.push(' ');
                output.push_str(message);
            }
        }
        Snapshot::Progress {
            current,
            total,
            message,
        } => {
            let current = normalized_current(current, total);
            let complete = completed_cells(current, total, options.progress_width);
            output.push('[');
            output.extend(std::iter::repeat_n('#', complete));
            output.extend(std::iter::repeat_n(
                '-',
                options.progress_width.saturating_sub(complete),
            ));
            write!(output, "] {current}/{total}").expect("writing to String cannot fail");
            if !message.is_empty() {
                output.push(' ');
                output.push_str(message);
            }
        }
    }
    let end = truncate(output, usable_width, options.width_profile).len();
    output.truncate(end);
}

fn append_message(
    output: &mut String,
    message: &str,
    usable_width: usize,
    profile: WidthProfile<'_>,
) {
    if message.is_empty() {
        return;
    }
    let occupied = output.len();
    if occupied >= usable_width {
        return;
    }
    output.push(' ');
    append_text(
        output,
        message,
        usable_width.saturating_sub(occupied.saturating_add(1)),
        profile,
    );
}

fn append_text(
    output: &mut String,
    message: &str,
    remaining_width: usize,
    profile: WidthProfile<'_>,
) {
    if message.is_ascii() {
        output.push_str(&message[..message.len().min(remaining_width)]);
    } else {
        output.push_str(truncate(message, remaining_width, profile));
    }
}

fn append_ascii(output: &mut String, value: &str, usable_width: usize) {
    let remaining = usable_width.saturating_sub(output.len());
    output.push_str(&value[..value.len().min(remaining)]);
}

fn append_repeated(output: &mut String, value: char, count: usize, usable_width: usize) {
    output.extend(std::iter::repeat_n(
        value,
        count.min(usable_width.saturating_sub(output.len())),
    ));
}

fn append_u64(output: &mut String, value: u64, usable_width: usize) {
    let start = output.len();
    write!(output, "{value}").expect("writing to String cannot fail");
    if output.len() > usable_width {
        output.truncate(start.max(usable_width));
    }
}

fn render_fallback(snapshot: Snapshot<'_>, output: &mut String) {
    output.clear();
    match snapshot {
        Snapshot::Status(message) | Snapshot::Spinner { message, .. } => {
            output.push_str(message);
        }
        Snapshot::Progress {
            current,
            total,
            message,
        } => {
            let current = normalized_current(current, total);
            write!(output, "{current}/{total}").expect("writing to String cannot fail");
            if !message.is_empty() {
                output.push(' ');
                output.push_str(message);
            }
        }
    }
}

fn normalized_current(current: u64, total: u64) -> u64 {
    if total == 0 { 0 } else { current.min(total) }
}

fn completed_cells(current: u64, total: u64, width: usize) -> usize {
    if total == 0 {
        return 0;
    }
    ((u128::from(current) * width as u128) / u128::from(total)) as usize
}

fn write_and_flush<I: Write>(io: &mut I, parts: &[&[u8]]) -> Result<(), StatusError> {
    for part in parts {
        io.write_all(part)
            .map_err(|error| StatusError::io("could not write output", error))?;
    }
    io.flush()
        .map_err(|error| StatusError::io("could not flush output", error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_accessors_preserve_values() {
        assert_eq!(Snapshot::status("Ready").kind(), SnapshotKind::Status);
        assert_eq!(Snapshot::spinner(7, "Wait").tick(), Some(7));
        assert_eq!(
            Snapshot::progress(3, 9, "Build").progress_values(),
            Some((3, 9))
        );
    }

    #[test]
    fn invalid_options_are_rejected_before_output() {
        for options in [
            Options::default().with_max_message_bytes(0),
            Options::default().with_fallback_terminal_width(0),
            Options::default().with_progress_width(0),
            Options::default().with_progress_width(MAX_PROGRESS_WIDTH + 1),
        ] {
            let error = Reporter::with_options(MemoryIo::default(), options)
                .err()
                .expect("invalid options must fail");
            assert_eq!(error.kind(), StatusErrorKind::InvalidStatus);
        }
    }

    #[test]
    fn terminal_log_preserves_active_status() {
        let mut io = MemoryIo::terminal(40);
        let mut reporter = Reporter::new(&mut io);
        assert!(reporter.update(Snapshot::status("Working")).unwrap());
        reporter.log("downloaded").unwrap();
        assert!(reporter.is_active());
        assert_eq!(
            io.output,
            b"\r\x1B[2KWorking\r\x1B[2Kdownloaded\n\r\x1B[2KWorking"
        );
    }

    #[test]
    fn invalid_messages_do_not_modify_output() {
        let mut io = MemoryIo::terminal(40);
        let mut reporter = Reporter::new(&mut io);
        let error = reporter
            .update(Snapshot::status("bad\nline"))
            .expect_err("controls must fail");
        assert_eq!(error.kind(), StatusErrorKind::InvalidStatus);
        assert!(io.output.is_empty());
    }

    #[test]
    fn write_failure_is_structured_and_retryable() {
        let mut reporter = Reporter::new(FailingIo);
        let error = reporter
            .update(Snapshot::status("Working"))
            .expect_err("write must fail");
        assert_eq!(error.kind(), StatusErrorKind::Io);
        assert!(!reporter.is_active());
        assert!(error.source().is_some());
    }

    #[test]
    fn retained_capacity_stabilizes_across_long_update_runs() {
        let mut reporter = Reporter::new(CountingIo::default());
        for tick in 0..8 {
            reporter.update(Snapshot::spinner(tick, "waiting")).unwrap();
        }
        let capacity = reporter.last_terminal.capacity()
            + reporter.last_log.capacity()
            + reporter.scratch.capacity();
        for tick in 0..10_000 {
            reporter.update(Snapshot::spinner(tick, "waiting")).unwrap();
        }
        assert_eq!(
            reporter.last_terminal.capacity()
                + reporter.last_log.capacity()
                + reporter.scratch.capacity(),
            capacity
        );
    }

    #[test]
    fn explicit_custom_width_applies_to_status_prefixes() {
        fn width(grapheme: &str) -> Option<nagi_text::CellCount> {
            (grapheme == "-").then_some(nagi_text::CellCount::Two)
        }

        let profile = WidthProfile::custom(WidthProfile::MODERN, &width);
        let options = Options::default().with_width_profile(profile);
        let mut io = MemoryIo::terminal(4);
        let mut reporter = Reporter::with_options(&mut io, options).unwrap();
        reporter
            .update(Snapshot::spinner(0, "A"))
            .expect("custom-width update must succeed");
        assert_eq!(io.output, b"\r\x1B[2K- ");
    }

    #[derive(Default)]
    struct MemoryIo {
        output: Vec<u8>,
        terminal: bool,
        width: Option<usize>,
    }

    impl MemoryIo {
        fn terminal(width: usize) -> Self {
            Self {
                output: Vec::new(),
                terminal: true,
                width: Some(width),
            }
        }
    }

    impl Write for MemoryIo {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.output.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl StatusIo for MemoryIo {
        fn is_terminal(&self) -> bool {
            self.terminal
        }

        fn terminal_width(&self) -> Option<usize> {
            self.width
        }
    }

    struct FailingIo;

    impl Write for FailingIo {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("failed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl StatusIo for FailingIo {
        fn is_terminal(&self) -> bool {
            true
        }

        fn terminal_width(&self) -> Option<usize> {
            Some(80)
        }
    }

    #[derive(Default)]
    struct CountingIo {
        bytes: usize,
    }

    impl Write for CountingIo {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.bytes = self.bytes.saturating_add(buffer.len());
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl StatusIo for CountingIo {
        fn is_terminal(&self) -> bool {
            true
        }

        fn terminal_width(&self) -> Option<usize> {
            Some(80)
        }
    }
}
