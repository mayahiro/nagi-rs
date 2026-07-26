//! Command graph, structured Help and Diagnostics, parser, and runtime
//! primitives for Nagi CLI
//!
//! Nagi CLI validates a declarative command graph, preserves platform-native
//! argument values, produces and validates typed invocations, and executes
//! handlers through an injected process context and Runtime Policy

#![deny(missing_docs)]
#![deny(unsafe_code)]

mod command;
mod diagnostic;
mod help;
mod parser;
mod policy;
mod runtime;
#[allow(unsafe_code)]
mod signal_unix;
mod value;

pub use command::{
    Argument, Command, InvocationValidator, OptionGroup, OptionGroupKind, OptionKind, OptionSpec,
    PresenceBasis,
};
pub use diagnostic::{Diagnostic, DiagnosticCategory, DiagnosticCode, ExitStatus};
pub use help::{
    HelpBlock, HelpDocument, HelpEntry, HelpExample, HelpLink, HelpOptionGroup, HelpOptionRelation,
    HelpOptionRelationKind, HelpRenderer, HelpSection, HelpUsageVariant, PlainHelpRenderer,
};
pub use parser::{Invocation, ParseResult};
pub use policy::{DiagnosticRenderer, ExitCodePolicy, PlainDiagnosticRenderer, RuntimePolicy};
pub use runtime::{
    CancellationHandle, CancellationToken, Context, Handler, Outcome, cancellation_pair,
};
pub use value::{
    ParsedValue, ValueParser, ValueSource, integer_parser, possible_values_parser, raw_parser,
    string_parser, value_parser,
};
