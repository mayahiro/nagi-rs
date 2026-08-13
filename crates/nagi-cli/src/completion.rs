use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::sync::Arc;

use crate::command::{Argument, Command, OptionKind, OptionSpec};
use crate::diagnostic::Diagnostic;
use crate::lifecycle::Deprecation;
use crate::runtime::CancellationToken;

/// Identifies the syntax target being completed
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompletionTargetKind {
    /// A child command name or command-level syntax
    Command,
    /// A named option value
    Option,
    /// A positional argument value
    Argument,
}

/// Identifies one completion target by stable Command Graph identity
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CompletionTarget {
    kind: CompletionTargetKind,
    command_id_path: Vec<String>,
    value_id: Option<String>,
}

impl CompletionTarget {
    fn command(command_id_path: Vec<String>) -> Self {
        Self {
            kind: CompletionTargetKind::Command,
            command_id_path,
            value_id: None,
        }
    }

    fn option(command_id_path: Vec<String>, value_id: String) -> Self {
        Self {
            kind: CompletionTargetKind::Option,
            command_id_path,
            value_id: Some(value_id),
        }
    }

    fn argument(command_id_path: Vec<String>, value_id: String) -> Self {
        Self {
            kind: CompletionTargetKind::Argument,
            command_id_path,
            value_id: Some(value_id),
        }
    }

    /// Returns the target category
    pub const fn kind(&self) -> CompletionTargetKind {
        self.kind
    }

    /// Returns the stable path of the command that owns this target
    pub fn command_id_path(&self) -> &[String] {
        &self.command_id_path
    }

    /// Returns the command-local value ID for an Option or Argument target
    pub fn value_id(&self) -> Option<&str> {
        self.value_id.as_deref()
    }
}

/// Identifies how one partial argv occurrence was represented
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionOccurrenceKind {
    /// A Boolean Flag option occurrence
    Flag,
    /// A Count option occurrence
    Count,
    /// A Value option or positional occurrence
    Value,
}

/// One recognized raw occurrence before the active completion token
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionOccurrence {
    target: CompletionTarget,
    kind: CompletionOccurrenceKind,
    raw: Option<OsString>,
}

impl CompletionOccurrence {
    /// Returns the stable option or argument target
    pub fn target(&self) -> &CompletionTarget {
        &self.target
    }

    /// Returns whether this occurrence is a Flag, Count, or raw Value
    pub const fn kind(&self) -> CompletionOccurrenceKind {
        self.kind
    }

    /// Returns the raw value for Value occurrences
    pub fn raw(&self) -> Option<&OsStr> {
        self.raw.as_deref()
    }
}

/// Tokenized shell input for one completion request
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompletionInput {
    arguments: Vec<OsString>,
    current: OsString,
}

impl CompletionInput {
    /// Constructs input from completed arguments and the token prefix at the cursor
    ///
    /// Arguments exclude the program name and the current token
    pub fn new<I, S>(arguments: I, current: impl Into<OsString>) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        Self {
            arguments: arguments.into_iter().map(Into::into).collect(),
            current: current.into(),
        }
    }

    /// Returns completed arguments before the token at the cursor
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    /// Returns the token prefix at the cursor
    pub fn current(&self) -> &OsStr {
        &self.current
    }
}

/// One normalized request passed to a dynamic completion provider
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionRequest {
    arguments: Vec<OsString>,
    current: OsString,
    command_path: Vec<String>,
    command_id_path: Vec<String>,
    target: CompletionTarget,
    prefix: OsString,
    partial: Vec<CompletionOccurrence>,
}

impl CompletionRequest {
    /// Returns completed arguments exactly as supplied by the shell adapter
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    /// Returns the complete token prefix at the cursor
    pub fn current(&self) -> &OsStr {
        &self.current
    }

    /// Returns the selected canonical command path
    pub fn command_path(&self) -> &[String] {
        &self.command_path
    }

    /// Returns the selected stable command-ID path
    pub fn command_id_path(&self) -> &[String] {
        &self.command_id_path
    }

    /// Returns the active completion target
    pub fn target(&self) -> &CompletionTarget {
        &self.target
    }

    /// Returns the target-local prefix being completed
    ///
    /// Attached long and short option syntax is removed from this prefix
    pub fn prefix(&self) -> &OsStr {
        &self.prefix
    }

    /// Returns recognized argv occurrences in original order
    ///
    /// Completion does not run Value Parsers, fallbacks, or validators
    pub fn partial_occurrences(&self) -> &[CompletionOccurrence] {
        &self.partial
    }
}

/// Classifies a completion candidate for presentation adapters
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum CompletionCandidateKind {
    /// A child command name or alias
    Command,
    /// A long or short option spelling
    Option,
    /// An option or positional value
    #[default]
    Value,
}

/// One portable completion candidate
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionCandidate {
    value: Arc<str>,
    display_label: Option<Arc<str>>,
    description: Option<Arc<str>>,
    deprecation: Option<Deprecation>,
    kind: CompletionCandidateKind,
    append_space: bool,
}

impl CompletionCandidate {
    /// Constructs a Value candidate that appends a space when selected
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: Arc::from(value.into()),
            display_label: None,
            description: None,
            deprecation: None,
            kind: CompletionCandidateKind::Value,
            append_space: true,
        }
    }

    /// Sets the text displayed separately from the inserted value
    ///
    /// An empty label restores the inserted value as the display label
    pub fn with_display_label(mut self, label: impl Into<String>) -> Self {
        let label = label.into();
        self.display_label = (!label.is_empty()).then(|| Arc::from(label));
        self
    }

    /// Sets a short human-readable candidate description
    ///
    /// An empty description is treated as absent
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        let description = description.into();
        self.description = (!description.is_empty()).then(|| Arc::from(description));
        self
    }

    /// Sets the semantic candidate kind
    pub const fn with_kind(mut self, kind: CompletionCandidateKind) -> Self {
        self.kind = kind;
        self
    }

    /// Controls whether adapters append a space after this candidate
    pub const fn with_append_space(mut self, append: bool) -> Self {
        self.append_space = append;
        self
    }

    /// Returns the complete text inserted for this candidate
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns the display label, defaulting to the inserted value
    pub fn display_label(&self) -> &str {
        self.display_label.as_deref().unwrap_or(&self.value)
    }

    /// Returns the optional short description
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns replacement metadata for a deprecated static candidate
    pub fn deprecation(&self) -> Option<&Deprecation> {
        self.deprecation.as_ref()
    }

    /// Returns the semantic candidate kind
    pub const fn kind(&self) -> CompletionCandidateKind {
        self.kind
    }

    /// Reports whether adapters should append a space after insertion
    pub const fn append_space(&self) -> bool {
        self.append_space
    }

    fn with_value_prefix(mut self, prefix: &str) -> Self {
        if prefix.is_empty() {
            return self;
        }
        let mut value = String::with_capacity(prefix.len() + self.value.len());
        value.push_str(prefix);
        value.push_str(&self.value);
        self.value = Arc::from(value);
        self
    }

    fn with_deprecation(mut self, deprecation: Option<Deprecation>) -> Self {
        self.deprecation = deprecation;
        self
    }
}

/// An application-defined dynamic completion failure
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionProviderError {
    message: String,
}

impl CompletionProviderError {
    /// Constructs an error with an application-readable message
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the provider's message
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for CompletionProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CompletionProviderError {}

/// Supplies runtime candidates for one active Option or Argument target
pub trait CompletionProvider: Send + Sync {
    /// Returns ordered candidates without running a command handler
    fn complete(
        &self,
        cancellation: &CancellationToken,
        request: &CompletionRequest,
    ) -> Result<Vec<CompletionCandidate>, CompletionProviderError>;
}

impl<F> CompletionProvider for F
where
    F: Fn(
            &CancellationToken,
            &CompletionRequest,
        ) -> Result<Vec<CompletionCandidate>, CompletionProviderError>
        + Send
        + Sync,
{
    fn complete(
        &self,
        cancellation: &CancellationToken,
        request: &CompletionRequest,
    ) -> Result<Vec<CompletionCandidate>, CompletionProviderError> {
        self(cancellation, request)
    }
}

/// Classifies a completion resolution failure
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionErrorKind {
    /// Cancellation was requested before completion finished
    Cancelled,
    /// The active dynamic provider returned an error
    Provider,
    /// A candidate could not be represented safely by shell adapters
    InvalidCandidate,
}

/// A completion-specific failure separate from parsing and handler Diagnostics
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionError {
    kind: CompletionErrorKind,
    target: Option<CompletionTarget>,
    message: String,
}

impl CompletionError {
    fn cancelled(target: Option<CompletionTarget>) -> Self {
        Self {
            kind: CompletionErrorKind::Cancelled,
            target,
            message: "completion was cancelled".to_owned(),
        }
    }

    fn provider(target: CompletionTarget, error: CompletionProviderError) -> Self {
        Self {
            kind: CompletionErrorKind::Provider,
            target: Some(target),
            message: error.message,
        }
    }

    fn invalid_candidate(target: CompletionTarget, reason: impl Into<String>) -> Self {
        Self {
            kind: CompletionErrorKind::InvalidCandidate,
            target: Some(target),
            message: reason.into(),
        }
    }

    /// Returns the failure category
    pub const fn kind(&self) -> CompletionErrorKind {
        self.kind
    }

    /// Returns the active target when resolution reached one
    pub fn target(&self) -> Option<&CompletionTarget> {
        self.target.as_ref()
    }

    /// Returns the human-readable failure message
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for CompletionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "completion failed: {}", self.message)
    }
}

impl std::error::Error for CompletionError {}

/// The normalized request and deterministic candidates for one completion
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionResult {
    request: CompletionRequest,
    candidates: Vec<CompletionCandidate>,
}

impl CompletionResult {
    /// Returns the normalized request used for static and dynamic candidates
    pub fn request(&self) -> &CompletionRequest {
        &self.request
    }

    /// Returns candidates in deterministic source order
    pub fn candidates(&self) -> &[CompletionCandidate] {
        &self.candidates
    }

    /// Consumes the result and returns its candidate storage
    pub fn into_candidates(self) -> Vec<CompletionCandidate> {
        self.candidates
    }
}

#[derive(Clone)]
struct EngineOption {
    id: String,
    long: Option<String>,
    short: Option<char>,
    kind: OptionKind,
    help: String,
    hidden: bool,
    deprecation: Option<Deprecation>,
    inherited: bool,
    repeated: bool,
    possible_values: Vec<String>,
    provider: Option<Arc<dyn CompletionProvider>>,
}

#[derive(Clone)]
struct EngineArgument {
    id: String,
    repeated: bool,
    possible_values: Vec<String>,
    provider: Option<Arc<dyn CompletionProvider>>,
}

#[derive(Clone)]
struct EngineCommand {
    id: String,
    name: String,
    aliases: Vec<String>,
    description: String,
    hidden: bool,
    deprecation: Option<Deprecation>,
    options: Vec<EngineOption>,
    arguments: Vec<EngineArgument>,
    subcommands: Vec<EngineCommand>,
    child_spellings: HashMap<String, usize>,
    has_version: bool,
}

impl EngineCommand {
    fn snapshot(command: &Command, root_has_version: bool) -> Self {
        let subcommands: Vec<_> = command
            .subcommands
            .iter()
            .map(|child| Self::snapshot(child, root_has_version))
            .collect();
        let mut child_spellings = HashMap::new();
        for (index, child) in subcommands.iter().enumerate() {
            child_spellings.insert(child.name.clone(), index);
            for alias in &child.aliases {
                child_spellings.insert(alias.clone(), index);
            }
        }
        Self {
            id: command.id.clone(),
            name: command.name.clone(),
            aliases: command.aliases.clone(),
            description: command.about.clone(),
            hidden: command.hidden,
            deprecation: command.deprecation.clone(),
            options: command.options.iter().map(EngineOption::snapshot).collect(),
            arguments: command
                .arguments
                .iter()
                .map(EngineArgument::snapshot)
                .collect(),
            subcommands,
            child_spellings,
            has_version: root_has_version,
        }
    }
}

impl EngineOption {
    fn snapshot(option: &OptionSpec) -> Self {
        Self {
            id: option.id.clone(),
            long: option.long.clone(),
            short: option.short,
            kind: option.kind,
            help: option.help.clone(),
            hidden: option.hidden,
            deprecation: option.deprecation.clone(),
            inherited: option.inherited,
            repeated: option.repeated,
            possible_values: option.parser.possible_values().to_vec(),
            provider: option.completion_provider.clone(),
        }
    }
}

impl EngineArgument {
    fn snapshot(argument: &Argument) -> Self {
        Self {
            id: argument.id.clone(),
            repeated: argument.repeated,
            possible_values: argument.parser.possible_values().to_vec(),
            provider: argument.completion_provider.clone(),
        }
    }
}

/// An immutable handler-free projection compiled from a validated Command Graph
#[derive(Clone)]
pub struct CompletionEngine {
    root: Arc<EngineCommand>,
}

impl CompletionEngine {
    /// Validates and snapshots a Command Graph for repeated completion requests
    pub fn new(command: &Command) -> Result<Self, Diagnostic> {
        command.validate()?;
        Ok(Self {
            root: Arc::new(EngineCommand::snapshot(command, command.version.is_some())),
        })
    }

    /// Returns the canonical program name used by shell generators
    pub fn root_name(&self) -> &str {
        &self.root.name
    }

    /// Resolves static candidates and only the active target's dynamic provider
    ///
    /// This does not run Value Parsers, fallbacks, validators, or handlers
    pub fn complete(
        &self,
        cancellation: &CancellationToken,
        input: CompletionInput,
    ) -> Result<CompletionResult, CompletionError> {
        if cancellation.is_cancelled() {
            return Err(CompletionError::cancelled(None));
        }

        let resolution = CompletionState::new(&self.root, &input.arguments).resolve(&input.current);
        let request = CompletionRequest {
            arguments: input.arguments,
            current: input.current,
            command_path: resolution.command_path,
            command_id_path: resolution.command_id_path,
            target: resolution.target.clone(),
            prefix: resolution.prefix,
            partial: resolution.partial,
        };
        let mut candidates = resolution.candidates;

        if let Some(provider) = resolution.provider {
            if cancellation.is_cancelled() {
                return Err(CompletionError::cancelled(Some(request.target.clone())));
            }
            let provided = provider
                .complete(cancellation, &request)
                .map_err(|error| CompletionError::provider(request.target.clone(), error))?;
            if cancellation.is_cancelled() {
                return Err(CompletionError::cancelled(Some(request.target.clone())));
            }
            for candidate in provided {
                validate_candidate(&candidate).map_err(|reason| {
                    CompletionError::invalid_candidate(request.target.clone(), reason)
                })?;
                candidates.push(candidate.with_value_prefix(&resolution.value_prefix));
            }
        }

        let current = request.current.as_bytes();
        let mut seen = HashSet::<Arc<str>>::new();
        let mut filtered = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            validate_candidate(&candidate).map_err(|reason| {
                CompletionError::invalid_candidate(request.target.clone(), reason)
            })?;
            if !candidate.value.as_bytes().starts_with(current) {
                continue;
            }
            if seen.insert(Arc::clone(&candidate.value)) {
                filtered.push(candidate);
            }
        }

        Ok(CompletionResult {
            request,
            candidates: filtered,
        })
    }
}

fn validate_candidate(candidate: &CompletionCandidate) -> Result<(), String> {
    if candidate.value.is_empty() {
        return Err("candidate value is empty".to_owned());
    }
    for (field, value) in [
        ("value", Some(candidate.value())),
        ("display label", candidate.display_label.as_deref()),
        ("description", candidate.description.as_deref()),
    ] {
        if value.is_some_and(|value| value.chars().any(char::is_control)) {
            return Err(format!("candidate {field} contains a control character"));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct PendingValue {
    scope_index: usize,
    option_index: usize,
}

struct Resolution {
    command_path: Vec<String>,
    command_id_path: Vec<String>,
    target: CompletionTarget,
    prefix: OsString,
    value_prefix: String,
    partial: Vec<CompletionOccurrence>,
    candidates: Vec<CompletionCandidate>,
    provider: Option<Arc<dyn CompletionProvider>>,
}

struct CompletionState<'engine> {
    root: &'engine EngineCommand,
    commands: Vec<&'engine EngineCommand>,
    command_path: Vec<String>,
    positional_index: usize,
    positional_started: bool,
    options_enabled: bool,
    pending: Option<PendingValue>,
    partial: Vec<CompletionOccurrence>,
    help_mode: bool,
    blocked: bool,
}

impl<'engine> CompletionState<'engine> {
    fn new(root: &'engine EngineCommand, arguments: &[OsString]) -> Self {
        let mut state = Self {
            root,
            commands: vec![root],
            command_path: vec![root.name.clone()],
            positional_index: 0,
            positional_started: false,
            options_enabled: true,
            pending: None,
            partial: Vec::new(),
            help_mode: false,
            blocked: false,
        };
        state.consume(arguments);
        state
    }

    fn active(&self) -> &'engine EngineCommand {
        self.commands
            .last()
            .copied()
            .expect("completion state always has a root command")
    }

    fn command_id_path(&self) -> Vec<String> {
        self.commands
            .iter()
            .map(|command| command.id.clone())
            .collect()
    }

    fn target_path(&self, scope_index: usize) -> Vec<String> {
        self.commands[..=scope_index]
            .iter()
            .map(|command| command.id.clone())
            .collect()
    }

    fn consume(&mut self, arguments: &[OsString]) {
        if !self.root.subcommands.is_empty()
            && arguments.first().is_some_and(|argument| argument == "help")
        {
            self.help_mode = true;
            for argument in &arguments[1..] {
                if !self.select_help_subcommand(argument) {
                    self.blocked = true;
                    break;
                }
            }
            return;
        }

        let mut index = 0;
        while index < arguments.len() && !self.blocked {
            let argument = &arguments[index];
            let bytes = argument.as_bytes();
            if self.options_enabled && bytes == b"--" {
                self.options_enabled = false;
                index += 1;
                continue;
            }
            if self.options_enabled && bytes.starts_with(b"--") && bytes.len() > 2 {
                index += self.consume_long(arguments, index);
                continue;
            }
            if self.options_enabled
                && bytes.starts_with(b"-")
                && !bytes.starts_with(b"--")
                && bytes.len() > 1
            {
                index += self.consume_short(arguments, index);
                continue;
            }
            if self.options_enabled && !self.positional_started && self.select_subcommand(argument)
            {
                index += 1;
                continue;
            }
            self.consume_positional(argument);
            index += 1;
        }
    }

    fn consume_long(&mut self, arguments: &[OsString], index: usize) -> usize {
        let bytes = arguments[index].as_bytes();
        let body = &bytes[2..];
        let (name, attached) = match body.iter().position(|byte| *byte == b'=') {
            Some(separator) => (&body[..separator], Some(&body[separator + 1..])),
            None => (body, None),
        };
        if matches!(name, b"help" | b"version") {
            self.blocked = true;
            return 1;
        }
        let Ok(name) = std::str::from_utf8(name) else {
            self.blocked = true;
            return 1;
        };
        let Some((scope_index, option_index)) = self.visible_long(name) else {
            self.blocked = true;
            return 1;
        };
        let option = &self.commands[scope_index].options[option_index];
        if attached.is_some() && option.kind != OptionKind::Value {
            self.blocked = true;
            return 1;
        }
        match option.kind {
            OptionKind::Flag => self.push_option_occurrence(
                scope_index,
                option,
                CompletionOccurrenceKind::Flag,
                None,
            ),
            OptionKind::Count => self.push_option_occurrence(
                scope_index,
                option,
                CompletionOccurrenceKind::Count,
                None,
            ),
            OptionKind::Value => {
                if let Some(value) = attached {
                    self.push_option_occurrence(
                        scope_index,
                        option,
                        CompletionOccurrenceKind::Value,
                        Some(OsString::from_vec(value.to_vec())),
                    );
                } else if index + 1 < arguments.len() {
                    self.push_option_occurrence(
                        scope_index,
                        option,
                        CompletionOccurrenceKind::Value,
                        Some(arguments[index + 1].clone()),
                    );
                    return 2;
                } else {
                    self.pending = Some(PendingValue {
                        scope_index,
                        option_index,
                    });
                }
            }
        }
        1
    }

    fn consume_short(&mut self, arguments: &[OsString], index: usize) -> usize {
        let bytes = arguments[index].as_bytes();
        let mut offset = 1;
        while offset < bytes.len() {
            let short = bytes[offset];
            if !short.is_ascii_alphanumeric() || matches!(short, b'h' | b'V') {
                self.blocked = true;
                return 1;
            }
            let Some((scope_index, option_index)) = self.visible_short(char::from(short)) else {
                self.blocked = true;
                return 1;
            };
            let option = &self.commands[scope_index].options[option_index];
            match option.kind {
                OptionKind::Flag => self.push_option_occurrence(
                    scope_index,
                    option,
                    CompletionOccurrenceKind::Flag,
                    None,
                ),
                OptionKind::Count => self.push_option_occurrence(
                    scope_index,
                    option,
                    CompletionOccurrenceKind::Count,
                    None,
                ),
                OptionKind::Value => {
                    if offset + 1 < bytes.len() {
                        self.push_option_occurrence(
                            scope_index,
                            option,
                            CompletionOccurrenceKind::Value,
                            Some(OsString::from_vec(bytes[offset + 1..].to_vec())),
                        );
                    } else if index + 1 < arguments.len() {
                        self.push_option_occurrence(
                            scope_index,
                            option,
                            CompletionOccurrenceKind::Value,
                            Some(arguments[index + 1].clone()),
                        );
                        return 2;
                    } else {
                        self.pending = Some(PendingValue {
                            scope_index,
                            option_index,
                        });
                    }
                    return 1;
                }
            }
            offset += 1;
        }
        1
    }

    fn consume_positional(&mut self, raw: &OsStr) {
        let Some(argument) = self.active().arguments.get(self.positional_index) else {
            self.blocked = true;
            return;
        };
        self.positional_started = true;
        self.partial.push(CompletionOccurrence {
            target: CompletionTarget::argument(self.command_id_path(), argument.id.clone()),
            kind: CompletionOccurrenceKind::Value,
            raw: Some(raw.to_owned()),
        });
        if !argument.repeated {
            self.positional_index += 1;
        }
    }

    fn push_option_occurrence(
        &mut self,
        scope_index: usize,
        option: &EngineOption,
        kind: CompletionOccurrenceKind,
        raw: Option<OsString>,
    ) {
        self.partial.push(CompletionOccurrence {
            target: CompletionTarget::option(self.target_path(scope_index), option.id.clone()),
            kind,
            raw,
        });
    }

    fn select_subcommand(&mut self, argument: &OsStr) -> bool {
        let Some(name) = argument.to_str() else {
            return false;
        };
        let Some(index) = self.active().child_spellings.get(name).copied() else {
            return false;
        };
        let command = &self.active().subcommands[index];
        self.commands.push(command);
        self.command_path.push(command.name.clone());
        self.positional_index = 0;
        self.positional_started = false;
        true
    }

    fn select_help_subcommand(&mut self, argument: &OsStr) -> bool {
        let selected = self.select_subcommand(argument);
        self.positional_started = false;
        selected
    }

    fn visible_long(&self, name: &str) -> Option<(usize, usize)> {
        let active = self.commands.len() - 1;
        (0..self.commands.len()).rev().find_map(|scope_index| {
            self.commands[scope_index]
                .options
                .iter()
                .enumerate()
                .find(|(_, option)| {
                    option.long.as_deref() == Some(name)
                        && (scope_index == active || option.inherited)
                })
                .map(|(option_index, _)| (scope_index, option_index))
        })
    }

    fn visible_short(&self, short: char) -> Option<(usize, usize)> {
        let active = self.commands.len() - 1;
        (0..self.commands.len()).rev().find_map(|scope_index| {
            self.commands[scope_index]
                .options
                .iter()
                .enumerate()
                .find(|(_, option)| {
                    option.short == Some(short) && (scope_index == active || option.inherited)
                })
                .map(|(option_index, _)| (scope_index, option_index))
        })
    }

    fn option_seen(&self, scope_index: usize, option: &EngineOption) -> bool {
        self.partial.iter().any(|occurrence| {
            occurrence.target.kind == CompletionTargetKind::Option
                && occurrence.target.command_id_path.len() == scope_index + 1
                && occurrence
                    .target
                    .command_id_path
                    .iter()
                    .zip(&self.commands[..=scope_index])
                    .all(|(id, command)| id == &command.id)
                && occurrence.target.value_id.as_deref() == Some(&option.id)
        })
    }

    fn resolve(self, current: &OsStr) -> Resolution {
        let command_id_path = self.command_id_path();
        let command_path = self.command_path.clone();
        if self.blocked {
            return Resolution {
                command_path,
                command_id_path: command_id_path.clone(),
                target: CompletionTarget::command(command_id_path),
                prefix: current.to_owned(),
                value_prefix: String::new(),
                partial: self.partial,
                candidates: Vec::new(),
                provider: None,
            };
        }

        if self.help_mode {
            let mut candidates = Vec::new();
            self.push_subcommands(&mut candidates, current.as_bytes());
            return Resolution {
                command_path,
                command_id_path: command_id_path.clone(),
                target: CompletionTarget::command(command_id_path),
                prefix: current.to_owned(),
                value_prefix: String::new(),
                partial: self.partial,
                candidates,
                provider: None,
            };
        }

        if let Some(pending) = self.pending {
            return self.resolve_option_value(
                pending.scope_index,
                pending.option_index,
                current.to_owned(),
                String::new(),
            );
        }

        if self.options_enabled {
            if let Some((scope_index, option_index, prefix, value_prefix)) =
                self.attached_value_context(current)
            {
                return self.resolve_option_value(scope_index, option_index, prefix, value_prefix);
            }
        }

        let current_bytes = current.as_bytes();
        let mut candidates = Vec::new();
        let completing_option = self.options_enabled && current_bytes.starts_with(b"-");
        let completing_word = !completing_option;
        let mut value_target = None;

        if completing_word && self.options_enabled && !self.positional_started {
            self.push_subcommands(&mut candidates, current_bytes);
            if self.commands.len() == 1
                && self.root.subcommands.iter().any(|command| !command.hidden)
                && b"help".starts_with(current_bytes)
            {
                candidates.push(
                    CompletionCandidate::new("help")
                        .with_kind(CompletionCandidateKind::Command)
                        .with_description("Show help for a command"),
                );
            }
        }

        if completing_word {
            if let Some(argument) = self.active().arguments.get(self.positional_index) {
                value_target = Some((argument, command_id_path.clone()));
                self.push_values(&mut candidates, &argument.possible_values, "", current);
            }
        }

        if self.options_enabled && (completing_option || current_bytes.is_empty()) {
            self.push_options(&mut candidates, current_bytes);
        }

        let (target, provider) = match value_target {
            Some((argument, path)) => (
                CompletionTarget::argument(path, argument.id.clone()),
                argument.provider.clone(),
            ),
            None => (CompletionTarget::command(command_id_path.clone()), None),
        };
        Resolution {
            command_path,
            command_id_path,
            target,
            prefix: current.to_owned(),
            value_prefix: String::new(),
            partial: self.partial,
            candidates,
            provider,
        }
    }

    fn attached_value_context(&self, current: &OsStr) -> Option<(usize, usize, OsString, String)> {
        let bytes = current.as_bytes();
        if let Some(body) = bytes.strip_prefix(b"--") {
            let separator = body.iter().position(|byte| *byte == b'=')?;
            let name = std::str::from_utf8(&body[..separator]).ok()?;
            let (scope_index, option_index) = self.visible_long(name)?;
            if self.commands[scope_index].options[option_index].kind != OptionKind::Value {
                return None;
            }
            let value_prefix = format!("--{name}=");
            return Some((
                scope_index,
                option_index,
                OsString::from_vec(body[separator + 1..].to_vec()),
                value_prefix,
            ));
        }
        if !bytes.starts_with(b"-") || bytes.starts_with(b"--") || bytes.len() <= 2 {
            return None;
        }
        let mut offset = 1;
        while offset < bytes.len() {
            let short = bytes[offset];
            if !short.is_ascii_alphanumeric() {
                return None;
            }
            let (scope_index, option_index) = self.visible_short(char::from(short))?;
            let option = &self.commands[scope_index].options[option_index];
            if option.kind == OptionKind::Value {
                let head = String::from_utf8(bytes[..=offset].to_vec()).ok()?;
                return Some((
                    scope_index,
                    option_index,
                    OsString::from_vec(bytes[offset + 1..].to_vec()),
                    head,
                ));
            }
            offset += 1;
        }
        None
    }

    fn resolve_option_value(
        self,
        scope_index: usize,
        option_index: usize,
        prefix: OsString,
        value_prefix: String,
    ) -> Resolution {
        let command_id_path = self.command_id_path();
        let command_path = self.command_path.clone();
        let option = &self.commands[scope_index].options[option_index];
        let target = CompletionTarget::option(self.target_path(scope_index), option.id.clone());
        let mut candidates = Vec::new();
        if !option.hidden {
            self.push_values(
                &mut candidates,
                &option.possible_values,
                &value_prefix,
                &prefix,
            );
        }
        Resolution {
            command_path,
            command_id_path,
            target,
            prefix,
            value_prefix,
            partial: self.partial,
            candidates,
            provider: (!option.hidden).then(|| option.provider.clone()).flatten(),
        }
    }

    fn push_values(
        &self,
        candidates: &mut Vec<CompletionCandidate>,
        values: &[String],
        value_prefix: &str,
        filter_prefix: &OsStr,
    ) {
        candidates.extend(
            values
                .iter()
                .filter(|value| value.as_bytes().starts_with(filter_prefix.as_bytes()))
                .map(|value| {
                    CompletionCandidate::new(value.clone())
                        .with_kind(CompletionCandidateKind::Value)
                        .with_value_prefix(value_prefix)
                }),
        );
    }

    fn push_subcommands(&self, candidates: &mut Vec<CompletionCandidate>, prefix: &[u8]) {
        for command in &self.active().subcommands {
            if command.hidden {
                continue;
            }
            if command.name.as_bytes().starts_with(prefix) {
                candidates.push(
                    CompletionCandidate::new(command.name.clone())
                        .with_kind(CompletionCandidateKind::Command)
                        .with_description(command.description.clone())
                        .with_deprecation(command.deprecation.clone()),
                );
            }
            for alias in &command.aliases {
                if alias.as_bytes().starts_with(prefix) {
                    candidates.push(
                        CompletionCandidate::new(alias.clone())
                            .with_kind(CompletionCandidateKind::Command)
                            .with_description(command.description.clone())
                            .with_deprecation(command.deprecation.clone()),
                    );
                }
            }
        }
    }

    fn push_options(&self, candidates: &mut Vec<CompletionCandidate>, prefix: &[u8]) {
        let active = self.commands.len() - 1;
        for scope_index in 0..self.commands.len() {
            for option in &self.commands[scope_index].options {
                if scope_index != active && !option.inherited {
                    continue;
                }
                if option.hidden {
                    continue;
                }
                if option.kind != OptionKind::Count
                    && !option.repeated
                    && self.option_seen(scope_index, option)
                {
                    continue;
                }
                if let Some(long) = &option.long {
                    if candidate_parts_start_with("--", long, prefix) {
                        candidates.push(
                            CompletionCandidate::new(format!("--{long}"))
                                .with_kind(CompletionCandidateKind::Option)
                                .with_description(option.help.clone())
                                .with_deprecation(option.deprecation.clone()),
                        );
                    }
                }
                if let Some(short) = option.short {
                    let spelling = [b'-', short as u8];
                    if spelling.starts_with(prefix) {
                        candidates.push(
                            CompletionCandidate::new(format!("-{short}"))
                                .with_kind(CompletionCandidateKind::Option)
                                .with_description(option.help.clone())
                                .with_deprecation(option.deprecation.clone()),
                        );
                    }
                }
            }
        }
        if b"--help".starts_with(prefix) {
            candidates.push(
                CompletionCandidate::new("--help")
                    .with_kind(CompletionCandidateKind::Option)
                    .with_description("Show help"),
            );
        }
        if b"-h".starts_with(prefix) {
            candidates.push(
                CompletionCandidate::new("-h")
                    .with_kind(CompletionCandidateKind::Option)
                    .with_description("Show help"),
            );
        }
        if self.root.has_version {
            if b"--version".starts_with(prefix) {
                candidates.push(
                    CompletionCandidate::new("--version")
                        .with_kind(CompletionCandidateKind::Option)
                        .with_description("Show version"),
                );
            }
            if b"-V".starts_with(prefix) {
                candidates.push(
                    CompletionCandidate::new("-V")
                        .with_kind(CompletionCandidateKind::Option)
                        .with_description("Show version"),
                );
            }
        }
    }
}

fn candidate_parts_start_with(head: &str, tail: &str, prefix: &[u8]) -> bool {
    let head = head.as_bytes();
    if prefix.len() <= head.len() {
        return head.starts_with(prefix);
    }
    prefix.starts_with(head) && tail.as_bytes().starts_with(&prefix[head.len()..])
}
