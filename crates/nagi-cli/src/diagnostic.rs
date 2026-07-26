use std::ffi::OsStr;
use std::fmt;
use std::os::unix::ffi::OsStrExt;
use std::process::ExitCode;

/// A stable machine-readable framework or application diagnostic code
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticCode(&'static str);

#[allow(non_upper_case_globals)]
impl DiagnosticCode {
    /// The command definition is internally inconsistent
    pub const InvalidSpecification: Self = Self("invalid-specification");
    /// An option spelling is unknown
    pub const UnknownOption: Self = Self("unknown-option");
    /// A flag or count option received a value
    pub const UnexpectedOptionValue: Self = Self("unexpected-option-value");
    /// A value option has no following value
    pub const MissingOptionValue: Self = Self("missing-option-value");
    /// A non-repeatable option appeared more than once
    pub const DuplicateOption: Self = Self("duplicate-option");
    /// A command name or alias is unknown
    pub const UnknownCommand: Self = Self("unknown-command");
    /// A required subcommand was not selected
    pub const MissingSubcommand: Self = Self("missing-subcommand");
    /// No positional slot accepts an argument
    pub const UnexpectedArgument: Self = Self("unexpected-argument");
    /// A required option or positional is absent
    pub const MissingRequired: Self = Self("missing-required");
    /// A Value Parser rejected a value
    pub const InvalidValue: Self = Self("invalid-value");
    /// An option requirement is not satisfied
    pub const Requires: Self = Self("requires");
    /// Conflicting options are both present
    pub const Conflicts: Self = Self("conflicts");
    /// An option-group cardinality rule is not satisfied
    pub const OptionGroup: Self = Self("option-group");
    /// A language-native Invocation validator rejected a value combination
    pub const Validation: Self = Self("validation");
    /// The selected command has no handler
    pub const MissingHandler: Self = Self("missing-handler");
    /// A handler reported an application failure
    pub const HandlerError: Self = Self("handler-error");
    /// Execution was cooperatively cancelled
    pub const Cancelled: Self = Self("cancelled");
    /// An injected I/O operation failed
    pub const IoError: Self = Self("io-error");

    /// Constructs an application code using the stable identifier grammar
    ///
    /// # Panics
    ///
    /// Panics when `code` is not a valid stable identifier
    pub fn application(code: &'static str) -> Self {
        assert!(valid_diagnostic_code(code), "invalid Diagnostic code");
        Self(code)
    }

    /// Returns the stable lowercase code
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

fn valid_diagnostic_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_alphabetic()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// Identifies the kind of value referenced by a Diagnostic target
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticTargetKind {
    /// A command option
    Option,
    /// A positional argument
    Argument,
}

/// Identifies one option or argument in a structured Diagnostic
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticTarget {
    kind: DiagnosticTargetKind,
    command_id_path: Vec<String>,
    value_id: String,
}

impl DiagnosticTarget {
    /// Constructs a target for an option in the current Invocation scope
    pub fn option(value_id: impl Into<String>) -> Self {
        Self {
            kind: DiagnosticTargetKind::Option,
            command_id_path: Vec::new(),
            value_id: value_id.into(),
        }
    }

    /// Constructs a target for an argument in the current Invocation scope
    pub fn argument(value_id: impl Into<String>) -> Self {
        Self {
            kind: DiagnosticTargetKind::Argument,
            command_id_path: Vec::new(),
            value_id: value_id.into(),
        }
    }

    /// Returns a copy with an explicit stable command-ID path
    pub fn with_command_id_path(mut self, path: Vec<String>) -> Self {
        self.command_id_path = path;
        self
    }

    /// Returns whether this target identifies an option or argument
    pub const fn kind(&self) -> DiagnosticTargetKind {
        self.kind
    }

    /// Returns the stable command-ID path
    pub fn command_id_path(&self) -> &[String] {
        &self.command_id_path
    }

    /// Returns the command-local value ID
    pub fn value_id(&self) -> &str {
        &self.value_id
    }

    pub(crate) fn set_default_path(&mut self, path: &[String]) {
        if self.command_id_path.is_empty() {
            self.command_id_path = path.to_vec();
        }
    }
}

/// A stable semantic failure category
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticCategory {
    /// An invalid Command Graph
    Specification,
    /// Invalid command-line usage
    Usage,
    /// Application execution failure
    Execution,
    /// Cooperative cancellation
    Cancellation,
    /// Injected I/O failure
    Io,
}

impl DiagnosticCategory {
    /// Returns the stable lowercase category
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Specification => "specification",
            Self::Usage => "usage",
            Self::Execution => "execution",
            Self::Cancellation => "cancellation",
            Self::Io => "io",
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
    category: DiagnosticCategory,
    message: String,
    metadata: Option<Box<DiagnosticMetadata>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct DiagnosticMetadata {
    command_path: Vec<String>,
    usage: Option<String>,
    targets: Vec<DiagnosticTarget>,
    hints: Vec<String>,
}

impl Diagnostic {
    /// Constructs a diagnostic with the semantic category for its code
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        let category = category_for_code(code);
        Self {
            code,
            category,
            message: message.into(),
            metadata: None,
        }
    }

    /// Adds a canonical command path
    pub fn with_command_path(mut self, path: Vec<String>) -> Self {
        self.metadata_mut().command_path = path;
        self
    }

    /// Adds one usage line without the `usage:` prefix
    pub fn with_usage(mut self, usage: impl Into<String>) -> Self {
        self.metadata_mut().usage = Some(usage.into());
        self
    }

    /// Overrides the semantic category
    pub const fn with_category(mut self, category: DiagnosticCategory) -> Self {
        self.category = category;
        self
    }

    /// Appends one structured option or argument target
    pub fn with_target(mut self, target: DiagnosticTarget) -> Self {
        self.metadata_mut().targets.push(target);
        self
    }

    /// Appends one human-readable remediation hint
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.metadata_mut().hints.push(hint.into());
        self
    }

    /// Returns the stable diagnostic code
    pub fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// Returns the semantic failure category
    pub const fn category(&self) -> DiagnosticCategory {
        self.category
    }

    /// Returns the human-readable message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the canonical command path
    pub fn command_path(&self) -> &[String] {
        self.metadata
            .as_deref()
            .map_or(&[], |metadata| metadata.command_path.as_slice())
    }

    /// Returns the optional usage line
    pub fn usage(&self) -> Option<&str> {
        self.metadata
            .as_deref()
            .and_then(|metadata| metadata.usage.as_deref())
    }

    /// Returns structured option and argument targets in insertion order
    pub fn targets(&self) -> &[DiagnosticTarget] {
        self.metadata
            .as_deref()
            .map_or(&[], |metadata| metadata.targets.as_slice())
    }

    /// Returns human-readable remediation hints in insertion order
    pub fn hints(&self) -> &[String] {
        self.metadata
            .as_deref()
            .map_or(&[], |metadata| metadata.hints.as_slice())
    }

    /// Renders deterministic plain text with one final newline
    pub fn render(&self) -> String {
        crate::policy::DiagnosticRenderer::render_diagnostic(
            &crate::policy::PlainDiagnosticRenderer::default(),
            self,
        )
    }

    pub(crate) fn with_default_target_path(mut self, path: &[String]) -> Self {
        if let Some(metadata) = self.metadata.as_deref_mut() {
            for target in &mut metadata.targets {
                target.set_default_path(path);
            }
        }
        self
    }

    fn metadata_mut(&mut self) -> &mut DiagnosticMetadata {
        self.metadata
            .get_or_insert_with(|| Box::new(DiagnosticMetadata::default()))
    }
}

fn category_for_code(code: DiagnosticCode) -> DiagnosticCategory {
    match code.as_str() {
        "invalid-specification" => DiagnosticCategory::Specification,
        "unknown-option"
        | "unexpected-option-value"
        | "missing-option-value"
        | "duplicate-option"
        | "unknown-command"
        | "missing-subcommand"
        | "unexpected-argument"
        | "missing-required"
        | "invalid-value"
        | "requires"
        | "conflicts"
        | "option-group"
        | "validation" => DiagnosticCategory::Usage,
        "cancelled" => DiagnosticCategory::Cancellation,
        "io-error" => DiagnosticCategory::Io,
        _ => DiagnosticCategory::Execution,
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
