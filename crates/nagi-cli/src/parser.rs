use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::os::unix::ffi::{OsStrExt, OsStringExt};

use crate::command::{
    Argument, Command, OptionGroupKind, OptionKind, OptionSpec, PresenceBasis, option_display,
    quote_value,
};
use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticTarget};
use crate::value::{ParsedValue, ValueSource};

#[derive(Clone, Debug)]
enum InvocationValue {
    Flag,
    Count(u64),
    Values {
        values: Vec<ParsedValue>,
        supplied: bool,
    },
}

#[derive(Clone, Copy, Debug)]
struct InvocationDefinition {
    kind: OptionKind,
    repeated: bool,
}

#[derive(Clone, Debug)]
struct InvocationScopeData {
    definitions: BTreeMap<String, InvocationDefinition>,
    values: BTreeMap<String, InvocationValue>,
}

impl InvocationScopeData {
    fn new(command: &Command) -> Self {
        let mut definitions = BTreeMap::new();
        for option in &command.options {
            definitions.insert(
                option.id.clone(),
                InvocationDefinition {
                    kind: option.kind,
                    repeated: option.repeated,
                },
            );
        }
        for argument in &command.arguments {
            definitions.insert(
                argument.id.clone(),
                InvocationDefinition {
                    kind: OptionKind::Value,
                    repeated: argument.repeated,
                },
            );
        }
        Self {
            definitions,
            values: BTreeMap::new(),
        }
    }
}

/// One exact command-local value scope in an Invocation
#[derive(Clone, Copy, Debug)]
pub struct InvocationScope<'invocation> {
    invocation: &'invocation Invocation,
    index: usize,
}

/// Distinguishes missing required values and parser-result type mismatches
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueAccessErrorKind {
    /// No resolved value exists for the requested ID
    Missing,
    /// The parser result does not have the requested dynamic type
    TypeMismatch,
}

/// Reports why required typed Invocation access failed
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueAccessError {
    kind: ValueAccessErrorKind,
    command_id_path: Vec<String>,
    value_id: String,
}

impl ValueAccessError {
    /// Returns whether the value was missing or had another dynamic type
    pub const fn kind(&self) -> ValueAccessErrorKind {
        self.kind
    }

    /// Returns the stable command-ID path used for lookup
    pub fn command_id_path(&self) -> &[String] {
        &self.command_id_path
    }

    /// Returns the requested command-local value ID
    pub fn value_id(&self) -> &str {
        &self.value_id
    }
}

impl fmt::Display for ValueAccessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self.kind {
            ValueAccessErrorKind::Missing => "is missing",
            ValueAccessErrorKind::TypeMismatch => "has an unexpected parser result type",
        };
        write!(
            formatter,
            "value '{}' in command scope '{}' {reason}",
            self.value_id,
            self.command_id_path.join("/")
        )
    }
}

impl std::error::Error for ValueAccessError {}

/// A canonical parsed command path and its command-local typed value scopes
///
/// Unqualified access starts at the current scope and searches ancestors. The
/// nearest declaration shadows an ancestor even when it has no resolved value.
/// Validators temporarily use their defining Command as current; handlers and
/// returned Invocations use the selected leaf
#[derive(Clone, Debug)]
pub struct Invocation {
    command_path: Vec<String>,
    command_id_path: Vec<String>,
    scopes: Vec<InvocationScopeData>,
    current_scope: usize,
}

impl Invocation {
    /// Returns the canonical root-to-leaf command path
    pub fn command_path(&self) -> &[String] {
        &self.command_path
    }

    /// Returns the stable root-to-leaf command-ID path
    pub fn command_id_path(&self) -> &[String] {
        &self.command_id_path
    }

    /// Returns the current stable path where unqualified lookup starts
    pub fn value_scope_id_path(&self) -> &[String] {
        &self.command_id_path[..=self.current_scope]
    }

    /// Returns the exact current scope where unqualified lookup starts
    pub fn current_scope(&self) -> InvocationScope<'_> {
        InvocationScope {
            invocation: self,
            index: self.current_scope,
        }
    }

    /// Returns an exact local scope selected by stable command-ID path
    pub fn scope<I, S>(&self, command_id_path: I) -> Option<InvocationScope<'_>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let path: Vec<S> = command_id_path.into_iter().collect();
        self.scopes
            .iter()
            .enumerate()
            .find(|(index, _)| {
                path.len() == index + 1
                    && self.command_id_path[..=*index]
                        .iter()
                        .zip(&path)
                        .all(|(left, right)| left == right.as_ref())
            })
            .map(|(index, _)| InvocationScope {
                invocation: self,
                index,
            })
    }

    /// Iterates exact scopes in root-to-leaf order
    pub fn scopes(&self) -> impl Iterator<Item = InvocationScope<'_>> {
        (0..self.scopes.len()).map(|index| InvocationScope {
            invocation: self,
            index,
        })
    }

    /// Reports whether the nearest visible declaration has a value
    pub fn contains(&self, id: &str) -> bool {
        self.lookup(id).is_some_and(|(_, value)| value.is_some())
    }

    /// Reports whether the nearest visible declaration was present in argv
    pub fn supplied(&self, id: &str) -> bool {
        match self.lookup(id).and_then(|(_, value)| value) {
            Some(InvocationValue::Flag | InvocationValue::Count(_)) => true,
            Some(InvocationValue::Values { supplied, .. }) => *supplied,
            None => false,
        }
    }

    /// Returns Boolean flag presence when the ID denotes a flag
    pub fn flag(&self, id: &str) -> Option<bool> {
        match self.lookup(id) {
            Some((definition, Some(InvocationValue::Flag)))
                if definition.kind == OptionKind::Flag =>
            {
                Some(true)
            }
            _ => None,
        }
    }

    /// Returns an occurrence count when the ID denotes a count option
    pub fn count(&self, id: &str) -> Option<u64> {
        match self.lookup(id) {
            Some((definition, Some(InvocationValue::Count(count))))
                if definition.kind == OptionKind::Count =>
            {
                Some(*count)
            }
            _ => None,
        }
    }

    /// Returns all parsed values and their sources for a value ID
    pub fn parsed_values(&self, id: &str) -> Option<&[ParsedValue]> {
        match self.lookup(id) {
            Some((definition, Some(InvocationValue::Values { values, .. })))
                if definition.kind == OptionKind::Value =>
            {
                Some(values)
            }
            _ => None,
        }
    }

    /// Reports whether a value ID was declared as repeatable
    pub fn is_repeated(&self, id: &str) -> bool {
        self.lookup(id).is_some_and(|(definition, _)| {
            definition.kind == OptionKind::Value && definition.repeated
        })
    }

    /// Returns the first platform-native raw value
    pub fn raw_value(&self, id: &str) -> Option<&OsStr> {
        self.parsed_values(id)?.first().map(ParsedValue::raw)
    }

    /// Returns the first typed value when it has type `T`
    pub fn value<T: Any>(&self, id: &str) -> Option<&T> {
        self.parsed_values(id)?.first()?.downcast_ref()
    }

    /// Returns all typed values when every value has type `T`
    pub fn values<T: Any>(&self, id: &str) -> Option<Vec<&T>> {
        self.parsed_values(id)?
            .iter()
            .map(ParsedValue::downcast_ref)
            .collect()
    }

    /// Returns the first typed value or a structured access error
    ///
    /// Environment and default values are already resolved before this lookup
    pub fn require_value<T: Any>(&self, id: &str) -> Result<&T, ValueAccessError> {
        required_value(self.value_scope_id_path(), self.parsed_values(id), id)
    }

    /// Returns visible IDs that have a value in sorted order
    pub fn value_ids(&self) -> Vec<&str> {
        let mut seen = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for scope in self.scopes[..=self.current_scope].iter().rev() {
            for id in scope.definitions.keys() {
                if seen.insert(id.as_str()) && scope.values.contains_key(id) {
                    ids.insert(id.as_str());
                }
            }
        }
        ids.into_iter().collect()
    }

    fn lookup(&self, id: &str) -> Option<(InvocationDefinition, Option<&InvocationValue>)> {
        for scope in self.scopes[..=self.current_scope].iter().rev() {
            if let Some(definition) = scope.definitions.get(id) {
                return Some((*definition, scope.values.get(id)));
            }
        }
        None
    }
}

impl InvocationScope<'_> {
    /// Returns the canonical command path prefix for this exact scope
    pub fn command_path(&self) -> &[String] {
        &self.invocation.command_path[..=self.index]
    }

    /// Returns the stable command-ID path for this exact scope
    pub fn command_id_path(&self) -> &[String] {
        &self.invocation.command_id_path[..=self.index]
    }

    /// Reports whether one local declaration has a value
    pub fn contains(&self, id: &str) -> bool {
        self.data().values.contains_key(id)
    }

    /// Reports whether one local declaration was present in argv
    pub fn supplied(&self, id: &str) -> bool {
        match self.data().values.get(id) {
            Some(InvocationValue::Flag | InvocationValue::Count(_)) => true,
            Some(InvocationValue::Values { supplied, .. }) => *supplied,
            None => false,
        }
    }

    /// Returns Boolean presence for one local flag declaration
    pub fn flag(&self, id: &str) -> Option<bool> {
        match (self.data().definitions.get(id), self.data().values.get(id)) {
            (
                Some(InvocationDefinition {
                    kind: OptionKind::Flag,
                    ..
                }),
                Some(InvocationValue::Flag),
            ) => Some(true),
            _ => None,
        }
    }

    /// Returns occurrences for one local count declaration
    pub fn count(&self, id: &str) -> Option<u64> {
        match (self.data().definitions.get(id), self.data().values.get(id)) {
            (
                Some(InvocationDefinition {
                    kind: OptionKind::Count,
                    ..
                }),
                Some(InvocationValue::Count(count)),
            ) => Some(*count),
            _ => None,
        }
    }

    /// Returns local parsed values and their sources
    pub fn parsed_values(&self, id: &str) -> Option<&[ParsedValue]> {
        match (self.data().definitions.get(id), self.data().values.get(id)) {
            (
                Some(InvocationDefinition {
                    kind: OptionKind::Value,
                    ..
                }),
                Some(InvocationValue::Values { values, .. }),
            ) => Some(values),
            _ => None,
        }
    }

    /// Returns the first local platform-native raw value
    pub fn raw_value(&self, id: &str) -> Option<&OsStr> {
        self.parsed_values(id)?.first().map(ParsedValue::raw)
    }

    /// Returns the first local typed value when it has type `T`
    pub fn value<T: Any>(&self, id: &str) -> Option<&T> {
        self.parsed_values(id)?.first()?.downcast_ref()
    }

    /// Returns all local typed values when every value has type `T`
    pub fn values<T: Any>(&self, id: &str) -> Option<Vec<&T>> {
        self.parsed_values(id)?
            .iter()
            .map(ParsedValue::downcast_ref)
            .collect()
    }

    /// Returns the first local typed value or a structured access error
    pub fn require_value<T: Any>(&self, id: &str) -> Result<&T, ValueAccessError> {
        required_value(self.command_id_path(), self.parsed_values(id), id)
    }

    /// Reports whether one local Value declaration is repeatable
    pub fn is_repeated(&self, id: &str) -> bool {
        self.data()
            .definitions
            .get(id)
            .is_some_and(|definition| definition.kind == OptionKind::Value && definition.repeated)
    }

    /// Iterates local IDs that have a value in sorted order
    pub fn value_ids(&self) -> impl Iterator<Item = &str> {
        self.data().values.keys().map(String::as_str)
    }

    fn data(&self) -> &InvocationScopeData {
        &self.invocation.scopes[self.index]
    }
}

fn required_value<'invocation, T: Any>(
    command_id_path: &[String],
    parsed: Option<&'invocation [ParsedValue]>,
    id: &str,
) -> Result<&'invocation T, ValueAccessError> {
    let Some(value) = parsed.and_then(|values| values.first()) else {
        return Err(ValueAccessError {
            kind: ValueAccessErrorKind::Missing,
            command_id_path: command_id_path.to_vec(),
            value_id: id.to_owned(),
        });
    };
    value.downcast_ref().ok_or_else(|| ValueAccessError {
        kind: ValueAccessErrorKind::TypeMismatch,
        command_id_path: command_id_path.to_vec(),
        value_id: id.to_owned(),
    })
}

/// The result of parsing argv before handler execution
#[derive(Clone, Debug)]
pub enum ParseResult {
    /// A validated invocation ready for execution
    Invocation(Invocation),
    /// A request to render help for a canonical command path
    Help {
        /// The canonical root-to-leaf command path
        command_path: Vec<String>,
        /// The stable root-to-leaf command-ID path
        command_id_path: Vec<String>,
    },
    /// A request to render the configured root version
    Version {
        /// The configured version without the command name
        version: String,
        /// The canonical root command path
        command_path: Vec<String>,
        /// The stable root command-ID path
        command_id_path: Vec<String>,
    },
}

impl ParseResult {
    /// Returns the selected canonical command path
    pub fn command_path(&self) -> &[String] {
        match self {
            Self::Invocation(invocation) => invocation.command_path(),
            Self::Help { command_path, .. } | Self::Version { command_path, .. } => command_path,
        }
    }

    /// Returns the selected stable command-ID path
    pub fn command_id_path(&self) -> &[String] {
        match self {
            Self::Invocation(invocation) => invocation.command_id_path(),
            Self::Help {
                command_id_path, ..
            }
            | Self::Version {
                command_id_path, ..
            } => command_id_path,
        }
    }
}

impl Command {
    /// Parses arguments after the program name with an empty environment
    pub fn parse<I, S>(&self, arguments: I) -> Result<ParseResult, Diagnostic>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.parse_with_environment(arguments, std::iter::empty::<(OsString, OsString)>())
    }

    /// Parses arguments and injected environment values
    pub fn parse_with_environment<I, S, E, K, V>(
        &self,
        arguments: I,
        environment: E,
    ) -> Result<ParseResult, Diagnostic>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
        E: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        self.validate()?;
        let arguments: Vec<OsString> = arguments.into_iter().map(Into::into).collect();
        let environment: BTreeMap<OsString, OsString> = environment
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        Parser::new(self, arguments, environment).parse()
    }
}

struct Parser<'command> {
    root: &'command Command,
    arguments: Vec<OsString>,
    environment: BTreeMap<OsString, OsString>,
    index: usize,
    commands: Vec<&'command Command>,
    command_path: Vec<String>,
    scopes: Vec<InvocationScopeData>,
    positional_index: usize,
    positional_started: bool,
    options_enabled: bool,
}

impl<'command> Parser<'command> {
    fn new(
        root: &'command Command,
        arguments: Vec<OsString>,
        environment: BTreeMap<OsString, OsString>,
    ) -> Self {
        Self {
            root,
            arguments,
            environment,
            index: 0,
            commands: vec![root],
            command_path: vec![root.name.clone()],
            scopes: vec![InvocationScopeData::new(root)],
            positional_index: 0,
            positional_started: false,
            options_enabled: true,
        }
    }

    fn parse(mut self) -> Result<ParseResult, Diagnostic> {
        if let Some(result) = self.parse_help_command()? {
            return Ok(result);
        }
        while self.index < self.arguments.len() {
            let argument = self.arguments[self.index].clone();
            let bytes = argument.as_bytes();
            if self.options_enabled && bytes == b"--" {
                self.options_enabled = false;
                self.index += 1;
                continue;
            }
            if self.options_enabled && bytes.starts_with(b"--") && bytes.len() > 2 {
                if let Some(action) = self.parse_long(&argument)? {
                    return Ok(action);
                }
                continue;
            }
            if self.options_enabled
                && bytes.starts_with(b"-")
                && bytes.len() > 1
                && !bytes.starts_with(b"--")
            {
                if let Some(action) = self.parse_short(&argument)? {
                    return Ok(action);
                }
                continue;
            }
            if self.options_enabled && !self.positional_started && self.select_subcommand(&argument)
            {
                self.index += 1;
                continue;
            }
            self.parse_positional(argument)?;
            self.index += 1;
        }

        self.resolve_fallbacks()?;
        self.validate_values()?;
        let mut invocation = Invocation {
            command_path: self.command_path.clone(),
            command_id_path: self
                .commands
                .iter()
                .map(|command| command.id.clone())
                .collect(),
            current_scope: self.scopes.len() - 1,
            scopes: std::mem::take(&mut self.scopes),
        };
        self.run_validators(&mut invocation)?;
        Ok(ParseResult::Invocation(invocation))
    }

    fn active(&self) -> &'command Command {
        self.commands
            .last()
            .copied()
            .expect("parser always has a root command")
    }

    fn active_values(&self) -> &BTreeMap<String, InvocationValue> {
        &self
            .scopes
            .last()
            .expect("parser always has a root value scope")
            .values
    }

    fn active_values_mut(&mut self) -> &mut BTreeMap<String, InvocationValue> {
        &mut self
            .scopes
            .last_mut()
            .expect("parser always has a root value scope")
            .values
    }

    fn current_command_id_path(&self) -> Vec<String> {
        self.commands
            .iter()
            .map(|command| command.id.clone())
            .collect()
    }

    fn parse_help_command(&mut self) -> Result<Option<ParseResult>, Diagnostic> {
        if self
            .arguments
            .first()
            .map(|argument| argument.as_os_str().as_bytes())
            != Some(b"help")
        {
            return Ok(None);
        }
        let mut command = self.root;
        let mut path = vec![self.root.name.clone()];
        let mut id_path = vec![self.root.id.clone()];
        for target in &self.arguments[1..] {
            let Some(name) = target.to_str() else {
                self.command_path = path;
                return Err(self.error(
                    DiagnosticCode::UnknownCommand,
                    format!("unknown command {}", quote_value(target)),
                ));
            };
            let Some(selected) = command.subcommands.iter().find(|child| {
                child.name == name || child.aliases.iter().any(|alias| alias == name)
            }) else {
                self.command_path = path;
                return Err(self.error(
                    DiagnosticCode::UnknownCommand,
                    format!("unknown command {}", quote_value(target)),
                ));
            };
            command = selected;
            path.push(command.name.clone());
            id_path.push(command.id.clone());
        }
        Ok(Some(ParseResult::Help {
            command_path: path,
            command_id_path: id_path,
        }))
    }

    fn parse_long(&mut self, argument: &OsStr) -> Result<Option<ParseResult>, Diagnostic> {
        let bytes = argument.as_bytes();
        let body = &bytes[2..];
        let (name_bytes, attached) = match body.iter().position(|byte| *byte == b'=') {
            Some(separator) => (
                &body[..separator],
                Some(OsString::from_vec(body[separator + 1..].to_vec())),
            ),
            None => (body, None),
        };
        let Ok(name) = std::str::from_utf8(name_bytes) else {
            return Err(self.error(
                DiagnosticCode::UnknownOption,
                format!("unknown option {}", quote_value(argument)),
            ));
        };

        if name == "help" {
            if attached.is_some() {
                return Err(self.error(
                    DiagnosticCode::UnexpectedOptionValue,
                    "option '--help' does not take a value",
                ));
            }
            return Ok(Some(ParseResult::Help {
                command_path: self.command_path.clone(),
                command_id_path: self.current_command_id_path(),
            }));
        }
        if name == "version" && self.root.version.is_some() {
            if attached.is_some() {
                return Err(self.error(
                    DiagnosticCode::UnexpectedOptionValue,
                    "option '--version' does not take a value",
                ));
            }
            return Ok(Some(ParseResult::Version {
                version: self.root.version.clone().expect("version was checked"),
                command_path: vec![self.root.name.clone()],
                command_id_path: vec![self.root.id.clone()],
            }));
        }

        let Some(option) = self
            .active()
            .options
            .iter()
            .find(|option| option.long.as_deref() == Some(name))
            .cloned()
        else {
            return Err(self.error(
                DiagnosticCode::UnknownOption,
                format!("unknown option {}", quote_value(argument)),
            ));
        };
        self.index += 1;
        self.apply_option(&option, attached, argument)?;
        Ok(None)
    }

    fn parse_short(&mut self, argument: &OsStr) -> Result<Option<ParseResult>, Diagnostic> {
        let bytes = argument.as_bytes();
        let mut offset = 1;
        self.index += 1;
        while offset < bytes.len() {
            let byte = bytes[offset];
            if !byte.is_ascii_alphanumeric() {
                return Err(self.error(
                    DiagnosticCode::UnknownOption,
                    format!("unknown option {}", quote_value(argument)),
                ));
            }
            let short = char::from(byte);
            if short == 'h' {
                return Ok(Some(ParseResult::Help {
                    command_path: self.command_path.clone(),
                    command_id_path: self.current_command_id_path(),
                }));
            }
            if short == 'V' && self.root.version.is_some() {
                return Ok(Some(ParseResult::Version {
                    version: self.root.version.clone().expect("version was checked"),
                    command_path: vec![self.root.name.clone()],
                    command_id_path: vec![self.root.id.clone()],
                }));
            }
            let Some(option) = self
                .active()
                .options
                .iter()
                .find(|option| option.short == Some(short))
                .cloned()
            else {
                return Err(self.error(
                    DiagnosticCode::UnknownOption,
                    format!("unknown option '-{short}'"),
                ));
            };
            if option.kind == OptionKind::Value {
                let attached = (offset + 1 < bytes.len())
                    .then(|| OsString::from_vec(bytes[offset + 1..].to_vec()));
                self.apply_option(&option, attached, argument)?;
                return Ok(None);
            }
            self.apply_option(&option, None, argument)?;
            offset += 1;
        }
        Ok(None)
    }

    fn apply_option(
        &mut self,
        option: &OptionSpec,
        attached: Option<OsString>,
        _spelling: &OsStr,
    ) -> Result<(), Diagnostic> {
        match option.kind {
            OptionKind::Flag => {
                if attached.is_some() {
                    return Err(self.error_with_targets(
                        DiagnosticCode::UnexpectedOptionValue,
                        format!("option '{}' does not take a value", option_display(option)),
                        [DiagnosticTarget::option(&option.id)],
                    ));
                }
                if self.active_values().contains_key(&option.id) {
                    return Err(self.error_with_targets(
                        DiagnosticCode::DuplicateOption,
                        format!(
                            "option '{}' was provided more than once",
                            option_display(option)
                        ),
                        [DiagnosticTarget::option(&option.id)],
                    ));
                }
                self.active_values_mut()
                    .insert(option.id.clone(), InvocationValue::Flag);
            }
            OptionKind::Count => {
                if attached.is_some() {
                    return Err(self.error_with_targets(
                        DiagnosticCode::UnexpectedOptionValue,
                        format!("option '{}' does not take a value", option_display(option)),
                        [DiagnosticTarget::option(&option.id)],
                    ));
                }
                match self.active_values_mut().entry(option.id.clone()) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(InvocationValue::Count(1));
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        if let InvocationValue::Count(count) = entry.get_mut() {
                            *count = count.saturating_add(1);
                        }
                    }
                }
            }
            OptionKind::Value => {
                let raw = match attached {
                    Some(value) => value,
                    None => {
                        let Some(value) = self.arguments.get(self.index).cloned() else {
                            return Err(self.error_with_targets(
                                DiagnosticCode::MissingOptionValue,
                                format!("option '{}' requires a value", option_display(option)),
                                [DiagnosticTarget::option(&option.id)],
                            ));
                        };
                        self.index += 1;
                        value
                    }
                };
                if !option.repeated && self.active_values().contains_key(&option.id) {
                    return Err(self.error_with_targets(
                        DiagnosticCode::DuplicateOption,
                        format!(
                            "option '{}' was provided more than once",
                            option_display(option)
                        ),
                        [DiagnosticTarget::option(&option.id)],
                    ));
                }
                let parsed = self.parse_value(
                    &option.id,
                    &option.parser,
                    raw,
                    ValueSource::CommandLine,
                    DiagnosticTarget::option(&option.id),
                )?;
                self.push_value(self.scopes.len() - 1, &option.id, parsed);
            }
        }
        Ok(())
    }

    fn select_subcommand(&mut self, argument: &OsStr) -> bool {
        let Some(name) = argument.to_str() else {
            return false;
        };
        let Some(command) = self.active().subcommands.iter().find(|command| {
            command.name == name || command.aliases.iter().any(|alias| alias == name)
        }) else {
            return false;
        };
        self.commands.push(command);
        self.command_path.push(command.name.clone());
        self.scopes.push(InvocationScopeData::new(command));
        self.positional_index = 0;
        self.positional_started = false;
        true
    }

    fn parse_positional(&mut self, raw: OsString) -> Result<(), Diagnostic> {
        let command = self.active();
        let Some(argument) = command.arguments.get(self.positional_index).cloned() else {
            let (code, message) = if !self.positional_started
                && command.arguments.is_empty()
                && !command.subcommands.is_empty()
            {
                (
                    DiagnosticCode::UnknownCommand,
                    format!("unknown command {}", quote_value(&raw)),
                )
            } else {
                (
                    DiagnosticCode::UnexpectedArgument,
                    format!("unexpected argument {}", quote_value(&raw)),
                )
            };
            return Err(self.error(code, message));
        };
        self.positional_started = true;
        let parsed = self.parse_value(
            &argument.id,
            &argument.parser,
            raw,
            ValueSource::CommandLine,
            DiagnosticTarget::argument(&argument.id),
        )?;
        self.push_value(self.scopes.len() - 1, &argument.id, parsed);
        if !argument.repeated {
            self.positional_index += 1;
        }
        Ok(())
    }

    fn resolve_fallbacks(&mut self) -> Result<(), Diagnostic> {
        let commands = self.commands.clone();
        for (command_index, command) in commands.into_iter().enumerate() {
            for option in &command.options {
                if option.kind != OptionKind::Value
                    || self.scopes[command_index].values.contains_key(&option.id)
                {
                    continue;
                }
                let fallback = option
                    .environment
                    .as_ref()
                    .and_then(|name| self.environment.get(OsStr::new(name)))
                    .cloned()
                    .map(|value| (value, ValueSource::Environment))
                    .or_else(|| {
                        option
                            .default
                            .clone()
                            .map(|value| (value, ValueSource::Default))
                    });
                if let Some((raw, source)) = fallback {
                    let parsed = self.parse_value(
                        &option.id,
                        &option.parser,
                        raw,
                        source,
                        DiagnosticTarget::option(&option.id)
                            .with_command_id_path(self.command_id_path(command_index)),
                    )?;
                    self.push_value(command_index, &option.id, parsed);
                }
            }
        }
        Ok(())
    }

    fn validate_values(&self) -> Result<(), Diagnostic> {
        for (command_index, command) in self.commands.iter().enumerate() {
            let values = &self.scopes[command_index].values;
            if command.subcommand_required && command_index + 1 == self.commands.len() {
                return Err(self.error(
                    DiagnosticCode::MissingSubcommand,
                    format!("command '{}' requires a subcommand", command.name),
                ));
            }
            for option in &command.options {
                if option.required && !values.contains_key(&option.id) {
                    return Err(self.error_with_targets(
                        DiagnosticCode::MissingRequired,
                        format!("required option '{}' is missing", option_display(option)),
                        [DiagnosticTarget::option(&option.id)
                            .with_command_id_path(self.command_id_path(command_index))],
                    ));
                }
                if !values.contains_key(&option.id) {
                    continue;
                }
                for required in &option.requires {
                    if self.present(command_index, &option.id, required.presence)
                        && !self.present(command_index, &required.id, required.presence)
                    {
                        return Err(self.error_with_targets(
                            DiagnosticCode::Requires,
                            format!(
                                "option '{}' requires '{}'",
                                option_display(option),
                                required.id
                            ),
                            [
                                DiagnosticTarget::option(&option.id)
                                    .with_command_id_path(self.command_id_path(command_index)),
                                DiagnosticTarget::option(&required.id)
                                    .with_command_id_path(self.command_id_path(command_index)),
                            ],
                        ));
                    }
                }
                for conflict in &option.conflicts {
                    if self.present(command_index, &option.id, conflict.presence)
                        && self.present(command_index, &conflict.id, conflict.presence)
                    {
                        return Err(self.error_with_targets(
                            DiagnosticCode::Conflicts,
                            format!(
                                "option '{}' conflicts with '{}'",
                                option_display(option),
                                conflict.id
                            ),
                            [
                                DiagnosticTarget::option(&option.id)
                                    .with_command_id_path(self.command_id_path(command_index)),
                                DiagnosticTarget::option(&conflict.id)
                                    .with_command_id_path(self.command_id_path(command_index)),
                            ],
                        ));
                    }
                }
            }
            for argument in &command.arguments {
                if argument.required && !values.contains_key(&argument.id) {
                    return Err(self.error_with_targets(
                        DiagnosticCode::MissingRequired,
                        format!("required argument '{}' is missing", argument.id),
                        [DiagnosticTarget::argument(&argument.id)
                            .with_command_id_path(self.command_id_path(command_index))],
                    ));
                }
            }
            for group in &command.option_groups {
                let count = group
                    .options
                    .iter()
                    .filter(|id| self.present(command_index, id, group.presence))
                    .count();
                let valid = match group.kind {
                    OptionGroupKind::AtMostOne => count <= 1,
                    OptionGroupKind::ExactlyOne => count == 1,
                    OptionGroupKind::AtLeastOne => count >= 1,
                    OptionGroupKind::AllOrNone => count == 0 || count == group.options.len(),
                };
                if !valid {
                    return Err(self.error_with_targets(
                        DiagnosticCode::OptionGroup,
                        format!("option group '{}' is not satisfied", group.id),
                        group.options.iter().map(|id| {
                            DiagnosticTarget::option(id)
                                .with_command_id_path(self.command_id_path(command_index))
                        }),
                    ));
                }
            }
        }
        Ok(())
    }

    fn present(&self, scope_index: usize, id: &str, basis: PresenceBasis) -> bool {
        let Some(value) = self.scopes[scope_index].values.get(id) else {
            return false;
        };
        basis == PresenceBasis::Resolved
            || match value {
                InvocationValue::Flag | InvocationValue::Count(_) => true,
                InvocationValue::Values { supplied, .. } => *supplied,
            }
    }

    fn run_validators(&self, invocation: &mut Invocation) -> Result<(), Diagnostic> {
        for (command_index, command) in self.commands.iter().enumerate() {
            invocation.current_scope = command_index;
            for validator in &command.validators {
                if let Err(diagnostic) = validator.validate(invocation) {
                    invocation.current_scope = invocation.scopes.len() - 1;
                    return Err(diagnostic
                        .with_default_target_path(&invocation.command_id_path[..=command_index])
                        .with_command_path(self.command_path.clone())
                        .with_usage(self.root.usage_for_path(&self.command_path)));
                }
            }
        }
        invocation.current_scope = invocation.scopes.len() - 1;
        Ok(())
    }

    fn parse_value(
        &self,
        id: &str,
        parser: &std::sync::Arc<dyn crate::value::ValueParser>,
        raw: OsString,
        source: ValueSource,
        target: DiagnosticTarget,
    ) -> Result<ParsedValue, Diagnostic> {
        let typed = parser.parse(&raw).map_err(|reason| {
            self.error_with_targets(
                DiagnosticCode::InvalidValue,
                format!("invalid value {} for '{id}': {reason}", quote_value(&raw)),
                [target],
            )
        })?;
        Ok(ParsedValue::new(raw, source, typed))
    }

    fn push_value(&mut self, scope_index: usize, id: &str, value: ParsedValue) {
        let supplied = value.source() == ValueSource::CommandLine;
        match self.scopes[scope_index].values.entry(id.to_owned()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(InvocationValue::Values {
                    values: vec![value],
                    supplied,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                if let InvocationValue::Values { values, .. } = entry.get_mut() {
                    values.push(value);
                }
            }
        }
    }

    fn error(&self, code: DiagnosticCode, message: impl Into<String>) -> Diagnostic {
        self.error_with_targets(code, message, std::iter::empty())
    }

    fn error_with_targets<I>(
        &self,
        code: DiagnosticCode,
        message: impl Into<String>,
        targets: I,
    ) -> Diagnostic
    where
        I: IntoIterator<Item = DiagnosticTarget>,
    {
        let mut diagnostic = Diagnostic::new(code, message);
        for target in targets {
            diagnostic = diagnostic.with_target(target);
        }
        diagnostic
            .with_default_target_path(&self.current_command_id_path())
            .with_command_path(self.command_path.clone())
            .with_usage(self.root.usage_for_path(&self.command_path))
    }

    fn command_id_path(&self, index: usize) -> Vec<String> {
        self.commands[..=index]
            .iter()
            .map(|command| command.id.clone())
            .collect()
    }
}

#[allow(dead_code)]
fn _argument_is_public(_: &Argument) {}
