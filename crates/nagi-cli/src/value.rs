use std::any::Any;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::sync::Arc;

use crate::diagnostic::Diagnostic;

/// Stable marker used when a framework projection hides a Sensitive value
pub const REDACTED_VALUE: &str = "<redacted>";

/// The source that supplied a parsed command value
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueSource {
    /// The value appeared in argv
    CommandLine,
    /// The value came from the injected environment
    Environment,
    /// The value came from the command definition
    Default,
    /// The value came from an application-provided resolver
    External,
}

/// Identifies one resolved value source and its optional portable identity
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueOrigin {
    source: ValueSource,
    identity: Option<Arc<str>>,
}

impl ValueOrigin {
    pub(crate) const fn command_line() -> Self {
        Self {
            source: ValueSource::CommandLine,
            identity: None,
        }
    }

    pub(crate) fn environment(identity: impl Into<String>) -> Self {
        Self {
            source: ValueSource::Environment,
            identity: Some(Arc::from(identity.into())),
        }
    }

    pub(crate) const fn default_value() -> Self {
        Self {
            source: ValueSource::Default,
            identity: None,
        }
    }

    pub(crate) fn external(identity: Arc<str>) -> Self {
        Self {
            source: ValueSource::External,
            identity: Some(identity),
        }
    }

    /// Returns the source category
    pub const fn source(&self) -> ValueSource {
        self.source
    }

    /// Returns the environment name or non-secret external identity when present
    pub fn identity(&self) -> Option<&str> {
        self.identity.as_deref()
    }
}

/// Selects whether external values replace or merge with a lower Default
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ValueResolutionMode {
    /// External values suppress the configured Default
    #[default]
    Replace,
    /// External values are followed by the configured Default
    Merge,
}

/// One borrowed request passed to an application Value Resolver
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueResolutionRequest<'a> {
    selected_command_path: &'a [String],
    selected_command_id_path: &'a [String],
    command_path: &'a [String],
    command_id_path: &'a [String],
    value_id: &'a str,
    repeated: bool,
    sensitive: bool,
}

impl<'a> ValueResolutionRequest<'a> {
    pub(crate) fn new(
        selected_command_path: &'a [String],
        selected_command_id_path: &'a [String],
        command_path: &'a [String],
        command_id_path: &'a [String],
        value_id: &'a str,
        repeated: bool,
        sensitive: bool,
    ) -> Self {
        Self {
            selected_command_path,
            selected_command_id_path,
            command_path,
            command_id_path,
            value_id,
            repeated,
            sensitive,
        }
    }

    /// Returns the complete selected canonical command path
    pub fn selected_command_path(&self) -> &[String] {
        self.selected_command_path
    }

    /// Returns the complete selected stable command-ID path
    pub fn selected_command_id_path(&self) -> &[String] {
        self.selected_command_id_path
    }

    /// Returns the canonical path of the command declaring this Value Option
    pub fn command_path(&self) -> &[String] {
        self.command_path
    }

    /// Returns the stable path of the command declaring this Value Option
    pub fn command_id_path(&self) -> &[String] {
        self.command_id_path
    }

    /// Returns the command-local Value Option ID
    pub const fn value_id(&self) -> &str {
        self.value_id
    }

    /// Reports whether the Value Option accepts multiple values
    pub const fn is_repeated(&self) -> bool {
        self.repeated
    }

    /// Reports whether the Value Option is Sensitive
    pub const fn is_sensitive(&self) -> bool {
        self.sensitive
    }
}

/// Raw values returned by an application Value Resolver
#[derive(Clone, Default, Eq, PartialEq)]
pub struct ValueResolution {
    source_identity: Option<Arc<str>>,
    values: Vec<OsString>,
    mode: ValueResolutionMode,
}

impl fmt::Debug for ValueResolution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ValueResolution")
            .field("resolved", &self.is_resolved())
            .field("value_count", &self.values.len())
            .field("mode", &self.mode)
            .finish()
    }
}

impl ValueResolution {
    /// Returns an unresolved result that permits the configured Default
    pub const fn unresolved() -> Self {
        Self {
            source_identity: None,
            values: Vec::new(),
            mode: ValueResolutionMode::Replace,
        }
    }

    /// Returns external values that suppress the configured Default
    ///
    /// The source identity is validated during parsing and must use the stable
    /// ASCII identifier grammar
    pub fn replace<I, S>(source_identity: impl Into<String>, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        Self::resolved(source_identity, values, ValueResolutionMode::Replace)
    }

    /// Returns external values followed by the configured Default
    ///
    /// Merge is valid only for a repeated Value Option. The source identity is
    /// validated during parsing and must use the stable ASCII identifier grammar
    pub fn merge<I, S>(source_identity: impl Into<String>, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        Self::resolved(source_identity, values, ValueResolutionMode::Merge)
    }

    fn resolved<I, S>(
        source_identity: impl Into<String>,
        values: I,
        mode: ValueResolutionMode,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        Self {
            source_identity: Some(Arc::from(source_identity.into())),
            values: values.into_iter().map(Into::into).collect(),
            mode,
        }
    }

    /// Reports whether the resolver supplied a result
    pub const fn is_resolved(&self) -> bool {
        self.source_identity.is_some()
    }

    /// Returns the external source identity when resolved
    pub fn source_identity(&self) -> Option<&str> {
        self.source_identity.as_deref()
    }

    /// Returns raw values in resolver order
    pub fn values(&self) -> &[OsString] {
        &self.values
    }

    /// Returns whether the result replaces or merges with Default
    pub const fn mode(&self) -> ValueResolutionMode {
        self.mode
    }

    pub(crate) fn into_resolved_parts(
        self,
    ) -> Option<(Arc<str>, Vec<OsString>, ValueResolutionMode)> {
        self.source_identity
            .map(|identity| (identity, self.values, self.mode))
    }
}

/// Resolves already loaded application configuration into raw Value Option fallbacks
///
/// Resolution is synchronous during parsing. Implementations should project
/// application state rather than start file or network I/O
pub trait ValueResolver: Send + Sync {
    /// Resolves one selected Value Option or returns a structured failure
    fn resolve(&self, request: &ValueResolutionRequest<'_>) -> Result<ValueResolution, Diagnostic>;
}

impl<F> ValueResolver for F
where
    F: Fn(&ValueResolutionRequest<'_>) -> Result<ValueResolution, Diagnostic> + Send + Sync,
{
    fn resolve(&self, request: &ValueResolutionRequest<'_>) -> Result<ValueResolution, Diagnostic> {
        self(request)
    }
}

/// One raw and typed value stored in an invocation
#[derive(Clone)]
pub struct ParsedValue {
    raw: OsString,
    origin: ValueOrigin,
    typed: Arc<dyn Any + Send + Sync>,
    sensitive: bool,
}

impl ParsedValue {
    pub(crate) fn new(
        raw: OsString,
        origin: ValueOrigin,
        typed: Arc<dyn Any + Send + Sync>,
        sensitive: bool,
    ) -> Self {
        Self {
            raw,
            origin,
            typed,
            sensitive,
        }
    }

    /// Returns the platform-native value before typed parsing
    pub fn raw(&self) -> &OsStr {
        &self.raw
    }

    /// Returns where this value came from
    pub fn source(&self) -> ValueSource {
        self.origin.source()
    }

    /// Returns the source category and optional source identity
    pub const fn origin(&self) -> &ValueOrigin {
        &self.origin
    }

    /// Reports whether framework-controlled display must redact this value
    pub const fn is_sensitive(&self) -> bool {
        self.sensitive
    }

    /// Returns the typed parser result when it has type `T`
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.typed.downcast_ref()
    }
}

impl fmt::Debug for ParsedValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("ParsedValue");
        if self.sensitive {
            debug.field("raw", &REDACTED_VALUE);
        } else {
            debug.field("raw", &self.raw);
        }
        debug
            .field("source", &self.origin.source())
            .finish_non_exhaustive()
    }
}

/// Parses one platform-native option or positional value
pub trait ValueParser: Send + Sync {
    /// Parses a raw value into a type stored by the invocation
    fn parse(&self, raw: &OsStr) -> Result<Arc<dyn Any + Send + Sync>, String>;

    /// Returns the placeholder used by help output
    fn metavar(&self) -> &str {
        "VALUE"
    }

    /// Returns the documented finite value set when one exists
    fn possible_values(&self) -> &[String] {
        &[]
    }
}

struct RawParser;

impl ValueParser for RawParser {
    fn parse(&self, raw: &OsStr) -> Result<Arc<dyn Any + Send + Sync>, String> {
        Ok(Arc::new(raw.to_owned()))
    }
}

struct StringParser;

impl ValueParser for StringParser {
    fn parse(&self, raw: &OsStr) -> Result<Arc<dyn Any + Send + Sync>, String> {
        raw.to_str()
            .map(|value| Arc::new(value.to_owned()) as Arc<dyn Any + Send + Sync>)
            .ok_or_else(|| "value is not valid UTF-8".to_owned())
    }
}

struct IntegerParser;

impl ValueParser for IntegerParser {
    fn parse(&self, raw: &OsStr) -> Result<Arc<dyn Any + Send + Sync>, String> {
        let value = raw
            .to_str()
            .ok_or_else(|| "integer is not valid UTF-8".to_owned())?
            .parse::<i64>()
            .map_err(|_| "value is not a signed 64-bit integer".to_owned())?;
        Ok(Arc::new(value))
    }

    fn metavar(&self) -> &str {
        "INTEGER"
    }
}

struct PossibleValuesParser {
    values: Vec<String>,
}

impl ValueParser for PossibleValuesParser {
    fn parse(&self, raw: &OsStr) -> Result<Arc<dyn Any + Send + Sync>, String> {
        let value = raw
            .to_str()
            .ok_or_else(|| "value is not valid UTF-8".to_owned())?;
        if !self.values.iter().any(|candidate| candidate == value) {
            return Err(format!("expected one of {}", self.values.join(", ")));
        }
        Ok(Arc::new(value.to_owned()))
    }

    fn possible_values(&self) -> &[String] {
        &self.values
    }
}

struct CustomParser<F> {
    metavar: String,
    parser: F,
}

impl<F, T> ValueParser for CustomParser<F>
where
    F: Fn(&OsStr) -> Result<T, String> + Send + Sync,
    T: Any + Send + Sync,
{
    fn parse(&self, raw: &OsStr) -> Result<Arc<dyn Any + Send + Sync>, String> {
        (self.parser)(raw).map(|value| Arc::new(value) as Arc<dyn Any + Send + Sync>)
    }

    fn metavar(&self) -> &str {
        &self.metavar
    }
}

/// Returns a parser that preserves an arbitrary platform-native value
pub fn raw_parser() -> Arc<dyn ValueParser> {
    Arc::new(RawParser)
}

/// Returns a parser that requires valid UTF-8 and stores a `String`
pub fn string_parser() -> Arc<dyn ValueParser> {
    Arc::new(StringParser)
}

/// Returns a parser that stores a signed 64-bit integer
pub fn integer_parser() -> Arc<dyn ValueParser> {
    Arc::new(IntegerParser)
}

/// Returns a parser that accepts only the given UTF-8 values
pub fn possible_values_parser<I, S>(values: I) -> Arc<dyn ValueParser>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    Arc::new(PossibleValuesParser {
        values: values.into_iter().map(Into::into).collect(),
    })
}

/// Adapts a language-native closure into a typed value parser
pub fn value_parser<F, T>(metavar: impl Into<String>, parser: F) -> Arc<dyn ValueParser>
where
    F: Fn(&OsStr) -> Result<T, String> + Send + Sync + 'static,
    T: Any + Send + Sync,
{
    Arc::new(CustomParser {
        metavar: metavar.into(),
        parser,
    })
}
