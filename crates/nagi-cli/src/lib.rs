//! Command graph, scoped typed Invocations, structured Help and Diagnostics,
//! parser, and runtime primitives for Nagi CLI
//!
//! Nagi CLI validates a declarative command graph, preserves platform-native
//! argument values, produces and validates typed invocations, and executes
//! handlers through an injected process context and Runtime Policy
//!
//! Parent and child Commands may reuse local value IDs. [`Invocation`] access
//! starts at a documented current scope, while [`Invocation::scope`] selects
//! one exact stable command-ID path and [`Invocation::require_value`] provides
//! fallible schema-required typed access
//!
//! Help-only Usage Variants and [`SubcommandUsageMode`] control presentation
//! without changing parsing. [`InvocationValidator`] returns a structured
//! [`Diagnostic`] with application codes, option or argument targets, and
//! remediation hints
//!
//! [`Command::parse`], [`Command::run_parsed_with_policy`],
//! [`Command::run_invocation_with_policy`], and the pure [`RuntimePolicy`]
//! helpers support command-by-command adoption in an existing CLI.
//! [`Command::run_process`] remains the complete process integration

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
    PresenceBasis, SubcommandUsageMode,
};
pub use diagnostic::{
    Diagnostic, DiagnosticCategory, DiagnosticCode, DiagnosticTarget, DiagnosticTargetKind,
    ExitStatus,
};
pub use help::{
    HelpBlock, HelpDocument, HelpEntry, HelpExample, HelpLink, HelpOptionGroup, HelpOptionRelation,
    HelpOptionRelationKind, HelpRenderer, HelpSection, HelpUsageVariant, PlainHelpRenderer,
};
pub use parser::{
    Invocation, InvocationScope, ParseResult, ValueAccessError, ValueAccessErrorKind,
};
pub use policy::{DiagnosticRenderer, ExitCodePolicy, PlainDiagnosticRenderer, RuntimePolicy};
pub use runtime::{
    CancellationHandle, CancellationToken, Context, Handler, Outcome, cancellation_pair,
};
pub use value::{
    ParsedValue, ValueParser, ValueSource, integer_parser, possible_values_parser, raw_parser,
    string_parser, value_parser,
};
