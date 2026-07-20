//! Command graph, parser, and runtime primitives for Nagi CLI
//!
//! Nagi CLI validates a declarative command graph, preserves platform-native
//! argument values, produces typed invocations, and executes handlers through
//! an injected process context

#![deny(missing_docs)]
#![deny(unsafe_code)]

mod command;
mod diagnostic;
mod parser;
mod runtime;
#[allow(unsafe_code)]
mod signal_unix;
mod value;

pub use command::{Argument, Command, OptionKind, OptionSpec};
pub use diagnostic::{Diagnostic, DiagnosticCode, ExitStatus};
pub use parser::{Invocation, ParseResult};
pub use runtime::{
    CancellationHandle, CancellationToken, Context, Handler, Outcome, cancellation_pair,
};
pub use value::{
    ParsedValue, ValueParser, ValueSource, integer_parser, possible_values_parser, raw_parser,
    string_parser, value_parser,
};
