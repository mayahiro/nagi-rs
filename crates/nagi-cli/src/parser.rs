use std::any::Any;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};

use crate::command::{Argument, Command, OptionKind, OptionSpec, option_display, quote_value};
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::value::{ParsedValue, ValueSource};

#[derive(Clone, Debug)]
enum InvocationValue {
    Flag,
    Count(u64),
    Values {
        values: Vec<ParsedValue>,
        repeated: bool,
    },
}

/// A canonical parsed command path and its typed values
#[derive(Clone, Debug)]
pub struct Invocation {
    command_path: Vec<String>,
    values: BTreeMap<String, InvocationValue>,
}

impl Invocation {
    /// Returns the canonical root-to-leaf command path
    pub fn command_path(&self) -> &[String] {
        &self.command_path
    }

    /// Reports whether any value is present for an ID
    pub fn contains(&self, id: &str) -> bool {
        self.values.contains_key(id)
    }

    /// Returns Boolean flag presence when the ID denotes a flag
    pub fn flag(&self, id: &str) -> Option<bool> {
        match self.values.get(id) {
            Some(InvocationValue::Flag) => Some(true),
            _ => None,
        }
    }

    /// Returns an occurrence count when the ID denotes a count option
    pub fn count(&self, id: &str) -> Option<u64> {
        match self.values.get(id) {
            Some(InvocationValue::Count(count)) => Some(*count),
            _ => None,
        }
    }

    /// Returns all parsed values and their sources for a value ID
    pub fn parsed_values(&self, id: &str) -> Option<&[ParsedValue]> {
        match self.values.get(id) {
            Some(InvocationValue::Values { values, .. }) => Some(values),
            _ => None,
        }
    }

    /// Reports whether a value ID was declared as repeatable
    pub fn is_repeated(&self, id: &str) -> bool {
        matches!(
            self.values.get(id),
            Some(InvocationValue::Values { repeated: true, .. })
        )
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

    /// Returns all IDs that have a value in sorted order
    pub fn value_ids(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }
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
    },
    /// A request to render the configured root version
    Version {
        /// The configured version without the command name
        version: String,
    },
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
    values: BTreeMap<String, InvocationValue>,
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
            values: BTreeMap::new(),
            positional_index: 0,
            positional_started: false,
            options_enabled: true,
        }
    }

    fn parse(mut self) -> Result<ParseResult, Diagnostic> {
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
        Ok(ParseResult::Invocation(Invocation {
            command_path: self.command_path,
            values: self.values,
        }))
    }

    fn active(&self) -> &'command Command {
        self.commands
            .last()
            .copied()
            .expect("parser always has a root command")
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
                }));
            }
            if short == 'V' && self.root.version.is_some() {
                return Ok(Some(ParseResult::Version {
                    version: self.root.version.clone().expect("version was checked"),
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
        spelling: &OsStr,
    ) -> Result<(), Diagnostic> {
        match option.kind {
            OptionKind::Flag => {
                if attached.is_some() {
                    return Err(self.error(
                        DiagnosticCode::UnexpectedOptionValue,
                        format!("option '{}' does not take a value", option_display(option)),
                    ));
                }
                if self.values.contains_key(&option.id) {
                    return Err(self.error(
                        DiagnosticCode::DuplicateOption,
                        format!(
                            "option '{}' was provided more than once",
                            option_display(option)
                        ),
                    ));
                }
                self.values.insert(option.id.clone(), InvocationValue::Flag);
            }
            OptionKind::Count => {
                if attached.is_some() {
                    return Err(self.error(
                        DiagnosticCode::UnexpectedOptionValue,
                        format!("option '{}' does not take a value", option_display(option)),
                    ));
                }
                match self.values.entry(option.id.clone()) {
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
                            return Err(self.error(
                                DiagnosticCode::MissingOptionValue,
                                format!("option '{}' requires a value", option_display(option)),
                            ));
                        };
                        self.index += 1;
                        value
                    }
                };
                if !option.repeated && self.values.contains_key(&option.id) {
                    return Err(self.error(
                        DiagnosticCode::DuplicateOption,
                        format!(
                            "option '{}' was provided more than once",
                            option_display(option)
                        ),
                    ));
                }
                let parsed = self.parse_value(
                    &option.id,
                    &option.parser,
                    raw,
                    ValueSource::CommandLine,
                    Some(spelling),
                )?;
                self.push_value(&option.id, parsed, option.repeated);
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
            None,
        )?;
        self.push_value(&argument.id, parsed, argument.repeated);
        if !argument.repeated {
            self.positional_index += 1;
        }
        Ok(())
    }

    fn resolve_fallbacks(&mut self) -> Result<(), Diagnostic> {
        let commands = self.commands.clone();
        for command in commands {
            for option in &command.options {
                if option.kind != OptionKind::Value || self.values.contains_key(&option.id) {
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
                    let parsed = self.parse_value(&option.id, &option.parser, raw, source, None)?;
                    self.push_value(&option.id, parsed, option.repeated);
                }
            }
        }
        Ok(())
    }

    fn validate_values(&self) -> Result<(), Diagnostic> {
        for (command_index, command) in self.commands.iter().enumerate() {
            if command.subcommand_required && command_index + 1 == self.commands.len() {
                return Err(self.error(
                    DiagnosticCode::MissingSubcommand,
                    format!("command '{}' requires a subcommand", command.name),
                ));
            }
            for option in &command.options {
                if option.required && !self.values.contains_key(&option.id) {
                    return Err(self.error(
                        DiagnosticCode::MissingRequired,
                        format!("required option '{}' is missing", option_display(option)),
                    ));
                }
                if !self.values.contains_key(&option.id) {
                    continue;
                }
                for required in &option.requires {
                    if !self.values.contains_key(required) {
                        return Err(self.error(
                            DiagnosticCode::Requires,
                            format!("option '{}' requires '{required}'", option_display(option)),
                        ));
                    }
                }
                for conflict in &option.conflicts {
                    if self.values.contains_key(conflict) {
                        return Err(self.error(
                            DiagnosticCode::Conflicts,
                            format!(
                                "option '{}' conflicts with '{conflict}'",
                                option_display(option)
                            ),
                        ));
                    }
                }
            }
            for argument in &command.arguments {
                if argument.required && !self.values.contains_key(&argument.id) {
                    return Err(self.error(
                        DiagnosticCode::MissingRequired,
                        format!("required argument '{}' is missing", argument.id),
                    ));
                }
            }
        }
        Ok(())
    }

    fn parse_value(
        &self,
        id: &str,
        parser: &std::sync::Arc<dyn crate::value::ValueParser>,
        raw: OsString,
        source: ValueSource,
        _spelling: Option<&OsStr>,
    ) -> Result<ParsedValue, Diagnostic> {
        let typed = parser.parse(&raw).map_err(|reason| {
            self.error(
                DiagnosticCode::InvalidValue,
                format!("invalid value {} for '{id}': {reason}", quote_value(&raw)),
            )
        })?;
        Ok(ParsedValue::new(raw, source, typed))
    }

    fn push_value(&mut self, id: &str, value: ParsedValue, repeated: bool) {
        match self.values.entry(id.to_owned()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(InvocationValue::Values {
                    values: vec![value],
                    repeated,
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
        Diagnostic::new(code, message)
            .with_command_path(self.command_path.clone())
            .with_usage(self.root.usage_for_path(&self.command_path))
    }
}

#[allow(dead_code)]
fn _argument_is_public(_: &Argument) {}
