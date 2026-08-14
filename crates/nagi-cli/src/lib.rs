//! Command graph, scoped typed Invocations, structured Help and Diagnostics,
//! parser, and runtime primitives for Nagi CLI
//!
//! Nagi CLI validates a declarative command graph, preserves platform-native
//! argument values, produces and validates typed invocations, and executes
//! handlers through an injected process context and Runtime Policy
//!
//! Options are local unless [`OptionSpec::inherited`] makes them visible in
//! selected descendants. Every value remains in its declaration scope. Parent
//! and child Commands may reuse local value IDs. [`Invocation`] access starts
//! at a documented current scope, while [`Invocation::scope`] selects one
//! exact stable command-ID path and [`Invocation::require_value`] provides
//! fallible schema-required typed access
//!
//! Help-only Usage Variants and [`SubcommandUsageMode`] control presentation
//! without changing parsing. [`InvocationValidator`] returns a structured
//! [`Diagnostic`] with application codes, option or argument targets, and
//! remediation hints
//!
//! [`Command::visit_help_documents`] validates once and streams structured
//! Help for every visible command. Deterministic Markdown and man rendering
//! remains in the optional `nagi-cli-document` crate
//!
//! [`OptionSpec::sensitive`] and [`Argument::sensitive`] attach generic
//! presentation metadata. Framework Help, parser Diagnostics, Debug output,
//! and completion redact or suppress those values while explicit Invocation
//! access preserves the original raw and typed data
//!
//! [`ValueResolver`] adapts already loaded application configuration into
//! selected Value Option fallbacks without giving Nagi ownership of its schema
//! or I/O. Fixed precedence remains command line, environment, external
//! resolver, then command-definition default
//!
//! [`expand_response_files`] provides an opt-in, resource-bounded lexical
//! layer for `@file` arguments without shell, variable, glob, tilde, or
//! environment expansion. Ordinary parser and runtime entry points preserve
//! leading `@` literally unless Response Files are enabled through [`Context`]
//! or [`ProcessOptions`]
//!
//! [`CompletionEngine`] snapshots the validated graph without handlers and
//! resolves static candidates plus only the active Option or Argument
//! [`CompletionProvider`]. Shell-specific generation remains in the optional
//! `nagi-cli-completion` crate
//!
//! [`Command::parse`], [`Command::run_parsed_with_policy`],
//! [`Command::run_invocation_with_policy`], and the pure [`RuntimePolicy`]
//! helpers support command-by-command adoption in an existing CLI.
//! [`Command::run_process`] remains the complete process integration

#![deny(missing_docs)]
#![deny(unsafe_code)]

mod command;
mod completion;
mod diagnostic;
mod diagnostic_json;
mod help;
mod lifecycle;
mod parser;
mod policy;
mod response_file;
mod runtime;
#[allow(unsafe_code)]
mod signal_unix;
mod value;

pub use command::{
    Argument, Command, InvocationValidator, OptionGroup, OptionGroupKind, OptionKind, OptionSpec,
    PresenceBasis, SubcommandUsageMode,
};
pub use completion::{
    CompletionCandidate, CompletionCandidateKind, CompletionEngine, CompletionError,
    CompletionErrorKind, CompletionInput, CompletionOccurrence, CompletionOccurrenceKind,
    CompletionProvider, CompletionProviderError, CompletionRequest, CompletionResult,
    CompletionTarget, CompletionTargetKind,
};
pub use diagnostic::{
    Diagnostic, DiagnosticCategory, DiagnosticCode, DiagnosticTarget, DiagnosticTargetKind,
    ExitStatus,
};
pub use diagnostic_json::{JSON_DIAGNOSTIC_SCHEMA, JsonDiagnosticRenderer};
pub use help::{
    HelpBlock, HelpDocument, HelpEntry, HelpExample, HelpInheritedOption, HelpLink,
    HelpOptionGroup, HelpOptionRelation, HelpOptionRelationKind, HelpRenderer, HelpSection,
    HelpUsageVariant, PlainHelpRenderer,
};
pub use lifecycle::{
    Deprecation, DeprecationNotice, DeprecationNoticeRenderer, DeprecationTargetKind,
    PlainDeprecationNoticeRenderer,
};
pub use parser::{
    Invocation, InvocationScope, ParseResult, ValueAccessError, ValueAccessErrorKind,
};
pub use policy::{DiagnosticRenderer, ExitCodePolicy, PlainDiagnosticRenderer, RuntimePolicy};
pub use response_file::{
    FilesystemResponseFileReader, ResponseFileLimits, ResponseFileOptions, ResponseFileReadRequest,
    ResponseFileReader, expand_response_files,
};
pub use runtime::{
    CancellationHandle, CancellationToken, Context, Handler, Outcome, ProcessOptions,
    cancellation_pair,
};
pub use value::{
    ParsedValue, REDACTED_VALUE, ValueOrigin, ValueParser, ValueResolution, ValueResolutionMode,
    ValueResolutionRequest, ValueResolver, ValueSource, integer_parser, possible_values_parser,
    raw_parser, string_parser, value_parser,
};
