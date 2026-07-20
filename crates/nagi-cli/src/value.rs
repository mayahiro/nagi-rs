use std::any::Any;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::sync::Arc;

/// The source that supplied a parsed command value
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueSource {
    /// The value appeared in argv
    CommandLine,
    /// The value came from the injected environment
    Environment,
    /// The value came from the command definition
    Default,
}

/// One raw and typed value stored in an invocation
#[derive(Clone)]
pub struct ParsedValue {
    raw: OsString,
    source: ValueSource,
    typed: Arc<dyn Any + Send + Sync>,
}

impl ParsedValue {
    pub(crate) fn new(
        raw: OsString,
        source: ValueSource,
        typed: Arc<dyn Any + Send + Sync>,
    ) -> Self {
        Self { raw, source, typed }
    }

    /// Returns the platform-native value before typed parsing
    pub fn raw(&self) -> &OsStr {
        &self.raw
    }

    /// Returns where this value came from
    pub fn source(&self) -> ValueSource {
        self.source
    }

    /// Returns the typed parser result when it has type `T`
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.typed.downcast_ref()
    }
}

impl fmt::Debug for ParsedValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ParsedValue")
            .field("raw", &self.raw)
            .field("source", &self.source)
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
