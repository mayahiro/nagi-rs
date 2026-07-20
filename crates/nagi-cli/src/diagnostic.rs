use std::ffi::OsStr;
use std::fmt;
use std::os::unix::ffi::OsStrExt;
use std::process::ExitCode;

/// A stable machine-readable diagnostic category
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticCode {
    /// The command definition is internally inconsistent
    InvalidSpecification,
    /// An option spelling is unknown
    UnknownOption,
    /// A flag or count option received a value
    UnexpectedOptionValue,
    /// A value option has no following value
    MissingOptionValue,
    /// A non-repeatable option appeared more than once
    DuplicateOption,
    /// A command name or alias is unknown
    UnknownCommand,
    /// A required subcommand was not selected
    MissingSubcommand,
    /// No positional slot accepts an argument
    UnexpectedArgument,
    /// A required option or positional is absent
    MissingRequired,
    /// A Value Parser rejected a value
    InvalidValue,
    /// An option requirement is not satisfied
    Requires,
    /// Conflicting options are both present
    Conflicts,
    /// The selected command has no handler
    MissingHandler,
    /// A handler reported an application failure
    HandlerError,
    /// Execution was cooperatively cancelled
    Cancelled,
    /// An injected I/O operation failed
    IoError,
}

impl DiagnosticCode {
    /// Returns the stable lowercase code
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSpecification => "invalid-specification",
            Self::UnknownOption => "unknown-option",
            Self::UnexpectedOptionValue => "unexpected-option-value",
            Self::MissingOptionValue => "missing-option-value",
            Self::DuplicateOption => "duplicate-option",
            Self::UnknownCommand => "unknown-command",
            Self::MissingSubcommand => "missing-subcommand",
            Self::UnexpectedArgument => "unexpected-argument",
            Self::MissingRequired => "missing-required",
            Self::InvalidValue => "invalid-value",
            Self::Requires => "requires",
            Self::Conflicts => "conflicts",
            Self::MissingHandler => "missing-handler",
            Self::HandlerError => "handler-error",
            Self::Cancelled => "cancelled",
            Self::IoError => "io-error",
        }
    }
}

/// A portable process exit status in the range 0 through 255
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExitStatus(u8);

impl ExitStatus {
    /// Successful execution
    pub const SUCCESS: Self = Self(0);
    /// General application failure
    pub const FAILURE: Self = Self(1);
    /// Command syntax or usage failure
    pub const USAGE: Self = Self(2);
    /// SIGINT-compatible cooperative cancellation
    pub const CANCELLED: Self = Self(130);

    /// Constructs a custom portable status
    pub const fn new(code: u8) -> Self {
        Self(code)
    }

    /// Returns the numeric status
    pub const fn code(self) -> u8 {
        self.0
    }
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        Self::from(status.code())
    }
}

/// A structured command-definition, parsing, or handler failure
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    code: DiagnosticCode,
    message: String,
    command_path: Vec<String>,
    usage: Option<String>,
    status: ExitStatus,
}

impl Diagnostic {
    /// Constructs a diagnostic with the default status for its code
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        let status = match code {
            DiagnosticCode::InvalidSpecification
            | DiagnosticCode::UnknownOption
            | DiagnosticCode::UnexpectedOptionValue
            | DiagnosticCode::MissingOptionValue
            | DiagnosticCode::DuplicateOption
            | DiagnosticCode::UnknownCommand
            | DiagnosticCode::MissingSubcommand
            | DiagnosticCode::UnexpectedArgument
            | DiagnosticCode::MissingRequired
            | DiagnosticCode::InvalidValue
            | DiagnosticCode::Requires
            | DiagnosticCode::Conflicts => ExitStatus::USAGE,
            DiagnosticCode::Cancelled => ExitStatus::CANCELLED,
            _ => ExitStatus::FAILURE,
        };
        Self {
            code,
            message: message.into(),
            command_path: Vec::new(),
            usage: None,
            status,
        }
    }

    /// Adds a canonical command path
    pub fn with_command_path(mut self, path: Vec<String>) -> Self {
        self.command_path = path;
        self
    }

    /// Adds one usage line without the `usage:` prefix
    pub fn with_usage(mut self, usage: impl Into<String>) -> Self {
        self.usage = Some(usage.into());
        self
    }

    /// Overrides the exit status
    pub fn with_status(mut self, status: ExitStatus) -> Self {
        self.status = status;
        self
    }

    /// Returns the stable diagnostic code
    pub fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// Returns the human-readable message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the canonical command path
    pub fn command_path(&self) -> &[String] {
        &self.command_path
    }

    /// Returns the optional usage line
    pub fn usage(&self) -> Option<&str> {
        self.usage.as_deref()
    }

    /// Returns the outcome status associated with this diagnostic
    pub fn status(&self) -> ExitStatus {
        self.status
    }

    /// Renders deterministic plain text with one final newline
    pub fn render(&self) -> String {
        let mut output = format!("error[{}]: {}\n", self.code.as_str(), self.message);
        if let Some(usage) = &self.usage {
            output.push_str("usage: ");
            output.push_str(usage);
            output.push('\n');
        }
        output
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.render().trim_end_matches('\n'))
    }
}

impl std::error::Error for Diagnostic {}

pub(crate) fn display_os(value: &OsStr) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut index = 0;
    while index < bytes.len() {
        match std::str::from_utf8(&bytes[index..]) {
            Ok(text) => {
                push_safe_text(&mut output, text);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if valid != 0 {
                    let text = std::str::from_utf8(&bytes[index..index + valid])
                        .expect("valid_up_to identifies valid UTF-8");
                    push_safe_text(&mut output, text);
                    index += valid;
                }
                let invalid = error.error_len().unwrap_or(bytes.len() - index);
                for byte in &bytes[index..index + invalid] {
                    output.push_str(&format!("\\x{byte:02X}"));
                }
                index += invalid;
            }
        }
    }
    output
}

fn push_safe_text(output: &mut String, text: &str) {
    for character in text.chars() {
        if character.is_control() {
            let mut encoded = [0; 4];
            for byte in character.encode_utf8(&mut encoded).as_bytes() {
                output.push_str(&format!("\\x{byte:02X}"));
            }
        } else {
            output.push(character);
        }
    }
}
