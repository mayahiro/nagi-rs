//! Lightweight line-oriented prompts layered above Nagi CLI Core
//!
//! [`Prompter`] supports Confirm, Select, Input, and Secret requests through
//! an injected [`PromptIo`] boundary. [`ProcessIo`] provides the Unix process
//! implementation while tests can supply deterministic in-memory I/O

#![deny(missing_docs)]
#![deny(unsafe_code)]

use std::fmt;
use std::io::{self, Write};

use nagi_cli::CancellationToken;

#[allow(unsafe_code)]
mod process_unix;

pub use process_unix::ProcessIo;

/// The default maximum number of response bytes before a line ending
pub const DEFAULT_MAX_INPUT_BYTES: usize = 65_536;

/// The default maximum number of choices in one Select request
pub const DEFAULT_MAX_CHOICES: usize = 1_000;

/// Whether an injected line read is visible or must hide terminal echo
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputMode {
    /// Ordinary terminal input with echo unchanged
    Visible,
    /// Sensitive terminal input with echo disabled for the read
    Secret,
}

/// The outcome of one bounded line read from injected prompt I/O
#[derive(Debug, Eq, PartialEq)]
pub enum ReadResult {
    /// A complete line without its LF or immediately preceding CR
    Line(Vec<u8>),
    /// Input ended before another byte was available
    EndOfFile,
    /// The line exceeded the requested byte limit and was drained
    InputTooLong,
    /// Caller or terminal cancellation interrupted the read
    Cancelled,
}

/// Injected terminal and stream operations used by [`Prompter`]
///
/// Implementations remove one trailing LF and an immediately preceding CR
/// from [`ReadResult::Line`]. They must not retain the cancellation reference
/// after `read_line` returns. [`Prompter`] always passes a non-zero byte limit
pub trait PromptIo: Write {
    /// Reports whether both prompt input and output are terminals
    fn is_terminal(&self) -> bool;

    /// Reads one line with the requested visibility and byte limit
    fn read_line(
        &mut self,
        cancellation: &CancellationToken,
        mode: InputMode,
        max_bytes: usize,
    ) -> io::Result<ReadResult>;
}

/// Controls whether visible prompts may use non-terminal I/O
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerminalPolicy {
    /// Require terminal input and output before writing a prompt
    #[default]
    RequireTerminal,
    /// Permit Confirm, Select, and Input on explicitly injected streams
    AllowNonTerminal,
}

/// Resource limits applied by one [`Prompter`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    max_input_bytes: usize,
    max_choices: usize,
}

impl Limits {
    /// Returns the maximum response bytes before a line ending
    pub const fn max_input_bytes(self) -> usize {
        self.max_input_bytes
    }

    /// Returns the maximum Select choice count
    pub const fn max_choices(self) -> usize {
        self.max_choices
    }

    /// Replaces the maximum response byte count
    ///
    /// A zero value is rejected when a request is executed
    pub const fn with_max_input_bytes(mut self, max_input_bytes: usize) -> Self {
        self.max_input_bytes = max_input_bytes;
        self
    }

    /// Replaces the maximum Select choice count
    ///
    /// A zero value is rejected when a request is executed
    pub const fn with_max_choices(mut self, max_choices: usize) -> Self {
        self.max_choices = max_choices;
        self
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            max_choices: DEFAULT_MAX_CHOICES,
        }
    }
}

/// A yes-or-no prompt request
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Confirm {
    message: String,
    default: Option<bool>,
}

impl Confirm {
    /// Constructs a Confirm request without a default answer
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            default: None,
        }
    }

    /// Sets the answer selected by an empty response
    pub const fn with_default(mut self, value: bool) -> Self {
        self.default = Some(value);
        self
    }

    /// Returns the prompt message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the explicit default answer
    pub const fn default_answer(&self) -> Option<bool> {
        self.default
    }
}

/// A numbered choice prompt request
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Select {
    message: String,
    choices: Vec<String>,
    default: Option<usize>,
}

impl Select {
    /// Constructs a Select request without a default choice
    pub fn new<I, S>(message: impl Into<String>, choices: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            message: message.into(),
            choices: choices.into_iter().map(Into::into).collect(),
            default: None,
        }
    }

    /// Sets the zero-based choice selected by an empty response
    pub const fn with_default(mut self, index: usize) -> Self {
        self.default = Some(index);
        self
    }

    /// Returns the prompt message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the ordered choice labels
    pub fn choices(&self) -> &[String] {
        &self.choices
    }

    /// Returns the explicit zero-based default choice
    pub const fn default_choice(&self) -> Option<usize> {
        self.default
    }
}

/// A visible text input request
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Input {
    message: String,
    default: Option<String>,
    required: bool,
}

impl Input {
    /// Constructs an Input request that permits an empty value
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            default: None,
            required: false,
        }
    }

    /// Sets the value selected by an empty response
    pub fn with_default(mut self, value: impl Into<String>) -> Self {
        self.default = Some(value.into());
        self
    }

    /// Requires a non-empty response when no default is configured
    pub const fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Returns the prompt message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the explicit default value
    pub fn default_value(&self) -> Option<&str> {
        self.default.as_deref()
    }

    /// Reports whether an empty response must be retried
    pub const fn is_required(&self) -> bool {
        self.required
    }
}

/// A text input request whose process read hides terminal echo
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Secret {
    message: String,
    required: bool,
}

impl Secret {
    /// Constructs a Secret request that permits an empty value
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            required: false,
        }
    }

    /// Requires a non-empty response
    pub const fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Returns the prompt message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Reports whether an empty response must be retried
    pub const fn is_required(&self) -> bool {
        self.required
    }
}

/// Classifies a prompt failure independently of its display text
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptErrorKind {
    /// The caller, terminal, or end of input cancelled the request
    Cancelled,
    /// Required input or output is not a terminal
    NotTerminal,
    /// Request metadata or limits violate the prompt contract
    InvalidRequest,
    /// A response exceeded the configured byte limit
    InputTooLong,
    /// An injected or process I/O operation failed
    Io,
}

/// A structured prompt failure
#[derive(Debug)]
pub struct PromptError {
    kind: PromptErrorKind,
    message: String,
    source: Option<io::Error>,
}

impl PromptError {
    /// Returns the stable failure category
    pub const fn kind(&self) -> PromptErrorKind {
        self.kind
    }

    fn new(kind: PromptErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }

    fn io(operation: &str, error: io::Error) -> Self {
        Self {
            kind: PromptErrorKind::Io,
            message: format!("{operation}: {error}"),
            source: Some(error),
        }
    }
}

impl fmt::Display for PromptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "prompt failed: {}", self.message)
    }
}

impl std::error::Error for PromptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

/// Executes prompt requests through one injected I/O implementation
pub struct Prompter<'a> {
    io: &'a mut dyn PromptIo,
    limits: Limits,
    terminal_policy: TerminalPolicy,
}

impl<'a> Prompter<'a> {
    /// Constructs a Prompter with default resource and terminal policies
    pub fn new(io: &'a mut dyn PromptIo) -> Self {
        Self {
            io,
            limits: Limits::default(),
            terminal_policy: TerminalPolicy::default(),
        }
    }

    /// Replaces the resource limits
    pub const fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Replaces the visible-prompt terminal policy
    pub const fn with_terminal_policy(mut self, policy: TerminalPolicy) -> Self {
        self.terminal_policy = policy;
        self
    }

    /// Prompts until a valid yes-or-no answer is read
    pub fn confirm(
        &mut self,
        cancellation: &CancellationToken,
        request: &Confirm,
    ) -> Result<bool, PromptError> {
        self.validate_common(request.message())?;
        self.prepare(cancellation, false)?;
        let suffix = match request.default_answer() {
            Some(true) => " [Y/n] ",
            Some(false) => " [y/N] ",
            None => " [y/n] ",
        };
        loop {
            self.write_prompt(
                cancellation,
                format!("{}{suffix}", request.message()).as_bytes(),
            )?;
            let line = self.read_value(cancellation, InputMode::Visible)?;
            let answer = trim_ascii_space(&line);
            if answer.is_empty() {
                if let Some(default) = request.default_answer() {
                    return Ok(default);
                }
            } else if answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes") {
                return Ok(true);
            } else if answer.eq_ignore_ascii_case("n") || answer.eq_ignore_ascii_case("no") {
                return Ok(false);
            }
            self.write_prompt(cancellation, b"Enter yes or no\n")?;
        }
    }

    /// Prompts until a valid zero-based choice index is read
    pub fn select(
        &mut self,
        cancellation: &CancellationToken,
        request: &Select,
    ) -> Result<usize, PromptError> {
        self.validate_common(request.message())?;
        self.validate_limits()?;
        if request.choices().is_empty() {
            return Err(PromptError::new(
                PromptErrorKind::InvalidRequest,
                "Select has no choices",
            ));
        }
        if request.choices().len() > self.limits.max_choices() {
            return Err(PromptError::new(
                PromptErrorKind::InvalidRequest,
                "Select exceeds the configured choice limit",
            ));
        }
        for choice in request.choices() {
            validate_metadata(choice, "Select choice")?;
        }
        if request
            .default_choice()
            .is_some_and(|index| index >= request.choices().len())
        {
            return Err(PromptError::new(
                PromptErrorKind::InvalidRequest,
                "Select default is outside the choice list",
            ));
        }
        self.prepare(cancellation, false)?;
        let mut menu = format!("{}\n", request.message());
        for (index, choice) in request.choices().iter().enumerate() {
            use fmt::Write as _;
            writeln!(menu, "  {}) {choice}", index + 1).expect("writing to String cannot fail");
        }
        self.write_prompt(cancellation, menu.as_bytes())?;
        loop {
            let prompt = match request.default_choice() {
                Some(default) => format!(
                    "Select [1-{}, default {}]: ",
                    request.choices().len(),
                    default + 1
                ),
                None => format!("Select [1-{}]: ", request.choices().len()),
            };
            self.write_prompt(cancellation, prompt.as_bytes())?;
            let line = self.read_value(cancellation, InputMode::Visible)?;
            let answer = trim_ascii_space(&line);
            if answer.is_empty() {
                if let Some(default) = request.default_choice() {
                    return Ok(default);
                }
            } else if answer.bytes().all(|byte| byte.is_ascii_digit()) {
                if let Ok(index) = answer.parse::<usize>() {
                    if (1..=request.choices().len()).contains(&index) {
                        return Ok(index - 1);
                    }
                }
            }
            self.write_prompt(
                cancellation,
                format!("Enter a number from 1 to {}\n", request.choices().len()).as_bytes(),
            )?;
        }
    }

    /// Reads one visible text value
    pub fn input(
        &mut self,
        cancellation: &CancellationToken,
        request: &Input,
    ) -> Result<String, PromptError> {
        self.validate_common(request.message())?;
        if let Some(default) = request.default_value() {
            validate_metadata(default, "Input default")?;
        }
        self.prepare(cancellation, false)?;
        loop {
            let prompt = match request.default_value() {
                Some(default) => format!("{} [{default}]: ", request.message()),
                None => format!("{}: ", request.message()),
            };
            self.write_prompt(cancellation, prompt.as_bytes())?;
            let value = self.read_value(cancellation, InputMode::Visible)?;
            if value.is_empty() {
                if let Some(default) = request.default_value() {
                    return Ok(default.to_owned());
                }
                if request.is_required() {
                    self.write_prompt(cancellation, b"A value is required\n")?;
                    continue;
                }
            }
            return Ok(value);
        }
    }

    /// Reads one text value while process terminal echo is disabled
    pub fn secret(
        &mut self,
        cancellation: &CancellationToken,
        request: &Secret,
    ) -> Result<String, PromptError> {
        self.validate_common(request.message())?;
        self.prepare(cancellation, true)?;
        loop {
            self.write_prompt(cancellation, format!("{}: ", request.message()).as_bytes())?;
            let value = self.read_value(cancellation, InputMode::Secret)?;
            if value.is_empty() && request.is_required() {
                self.write_prompt(cancellation, b"A value is required\n")?;
                continue;
            }
            return Ok(value);
        }
    }

    fn validate_common(&self, message: &str) -> Result<(), PromptError> {
        self.validate_limits()?;
        validate_metadata(message, "prompt message")
    }

    fn validate_limits(&self) -> Result<(), PromptError> {
        if self.limits.max_input_bytes() == 0 || self.limits.max_choices() == 0 {
            return Err(PromptError::new(
                PromptErrorKind::InvalidRequest,
                "prompt limits must be non-zero",
            ));
        }
        Ok(())
    }

    fn prepare(
        &mut self,
        cancellation: &CancellationToken,
        secret: bool,
    ) -> Result<(), PromptError> {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        if !self.io.is_terminal()
            && (secret || self.terminal_policy == TerminalPolicy::RequireTerminal)
        {
            return Err(PromptError::new(
                PromptErrorKind::NotTerminal,
                "input and output are not interactive terminals",
            ));
        }
        Ok(())
    }

    fn write_prompt(
        &mut self,
        cancellation: &CancellationToken,
        bytes: &[u8],
    ) -> Result<(), PromptError> {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        self.io
            .write_all(bytes)
            .map_err(|error| PromptError::io("write", error))?;
        self.io
            .flush()
            .map_err(|error| PromptError::io("flush", error))?;
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        Ok(())
    }

    fn read_value(
        &mut self,
        cancellation: &CancellationToken,
        mode: InputMode,
    ) -> Result<String, PromptError> {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let result = self
            .io
            .read_line(cancellation, mode, self.limits.max_input_bytes())
            .map_err(|error| PromptError::io("read", error));
        if mode == InputMode::Secret {
            self.io
                .write_all(b"\n")
                .map_err(|error| PromptError::io("write secret line ending", error))?;
            self.io
                .flush()
                .map_err(|error| PromptError::io("flush secret line ending", error))?;
        }
        let result = result?;
        if cancellation.is_cancelled() || result == ReadResult::Cancelled {
            return Err(cancelled());
        }
        match result {
            ReadResult::Line(bytes) => {
                if bytes.len() > self.limits.max_input_bytes() {
                    return Err(too_long(self.limits.max_input_bytes()));
                }
                if matches!(bytes.as_slice(), [0x03] | [0x1b]) {
                    return Err(cancelled());
                }
                Ok(nagi_text::normalize_utf8(&bytes).into_owned())
            }
            ReadResult::EndOfFile | ReadResult::Cancelled => Err(cancelled()),
            ReadResult::InputTooLong => Err(too_long(self.limits.max_input_bytes())),
        }
    }
}

fn validate_metadata(value: &str, field: &str) -> Result<(), PromptError> {
    if value.is_empty() {
        return Err(PromptError::new(
            PromptErrorKind::InvalidRequest,
            format!("{field} is empty"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(PromptError::new(
            PromptErrorKind::InvalidRequest,
            format!("{field} contains a control character"),
        ));
    }
    Ok(())
}

fn trim_ascii_space(value: &str) -> &str {
    value.trim_matches([' ', '\t'])
}

fn cancelled() -> PromptError {
    PromptError::new(PromptErrorKind::Cancelled, "cancelled")
}

fn too_long(limit: usize) -> PromptError {
    PromptError::new(
        PromptErrorKind::InputTooLong,
        format!("input exceeds {limit} bytes"),
    )
}
