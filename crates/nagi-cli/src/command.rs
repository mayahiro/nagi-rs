use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::sync::Arc;

use nagi_text::{WidthProfile, text_width};

use crate::diagnostic::{Diagnostic, DiagnosticCode, display_os};
use crate::runtime::Handler;
use crate::value::{ValueParser, raw_parser};

/// The storage and parsing behavior of an option
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionKind {
    /// Boolean presence
    Flag,
    /// An occurrence count
    Count,
    /// One or more parser-produced values
    Value,
}

/// One named option in a command definition
#[derive(Clone)]
pub struct OptionSpec {
    pub(crate) id: String,
    pub(crate) long: Option<String>,
    pub(crate) short: Option<char>,
    pub(crate) kind: OptionKind,
    pub(crate) parser: Arc<dyn ValueParser>,
    pub(crate) help: String,
    pub(crate) required: bool,
    pub(crate) repeated: bool,
    pub(crate) environment: Option<String>,
    pub(crate) default: Option<OsString>,
    pub(crate) requires: Vec<String>,
    pub(crate) conflicts: Vec<String>,
}

impl OptionSpec {
    /// Constructs a Boolean flag
    pub fn flag(id: impl Into<String>) -> Self {
        Self::new(id, OptionKind::Flag)
    }

    /// Constructs an occurrence counter
    pub fn count(id: impl Into<String>) -> Self {
        Self::new(id, OptionKind::Count)
    }

    /// Constructs a raw platform-value option
    pub fn value(id: impl Into<String>) -> Self {
        Self::new(id, OptionKind::Value)
    }

    fn new(id: impl Into<String>, kind: OptionKind) -> Self {
        Self {
            id: id.into(),
            long: None,
            short: None,
            kind,
            parser: raw_parser(),
            help: String::new(),
            required: false,
            repeated: false,
            environment: None,
            default: None,
            requires: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    /// Sets the long option name without leading hyphens
    pub fn long(mut self, name: impl Into<String>) -> Self {
        self.long = Some(name.into());
        self
    }

    /// Sets the one-character short option name
    pub fn short(mut self, name: char) -> Self {
        self.short = Some(name);
        self
    }

    /// Sets the help description
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = help.into();
        self
    }

    /// Requires this option after source resolution
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Allows a Value option to appear multiple times
    pub fn repeated(mut self) -> Self {
        self.repeated = true;
        self
    }

    /// Sets the typed parser used by a Value option
    pub fn parser(mut self, parser: Arc<dyn ValueParser>) -> Self {
        self.parser = parser;
        self
    }

    /// Sets an environment fallback for a Value option
    pub fn environment(mut self, name: impl Into<String>) -> Self {
        self.environment = Some(name.into());
        self
    }

    /// Sets a default raw value for a Value option
    pub fn default_value(mut self, value: impl Into<OsString>) -> Self {
        self.default = Some(value.into());
        self
    }

    /// Requires another option when this option is present
    pub fn requires(mut self, id: impl Into<String>) -> Self {
        self.requires.push(id.into());
        self
    }

    /// Conflicts with another option when both are present
    pub fn conflicts(mut self, id: impl Into<String>) -> Self {
        self.conflicts.push(id.into());
        self
    }

    /// Returns the stable value identifier
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the option kind
    pub fn kind(&self) -> OptionKind {
        self.kind
    }
}

/// One positional argument in a command definition
#[derive(Clone)]
pub struct Argument {
    pub(crate) id: String,
    pub(crate) parser: Arc<dyn ValueParser>,
    pub(crate) help: String,
    pub(crate) required: bool,
    pub(crate) repeated: bool,
}

impl Argument {
    /// Constructs a raw platform-value positional argument
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            parser: raw_parser(),
            help: String::new(),
            required: false,
            repeated: false,
        }
    }

    /// Sets the typed parser
    pub fn parser(mut self, parser: Arc<dyn ValueParser>) -> Self {
        self.parser = parser;
        self
    }

    /// Sets the help description
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = help.into();
        self
    }

    /// Requires this positional argument
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Allows this final positional argument to consume all remaining values
    pub fn repeated(mut self) -> Self {
        self.repeated = true;
        self
    }

    /// Returns the stable value identifier
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// A validated node in the command graph
#[derive(Clone)]
pub struct Command {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) about: String,
    pub(crate) version: Option<String>,
    pub(crate) options: Vec<OptionSpec>,
    pub(crate) arguments: Vec<Argument>,
    pub(crate) subcommands: Vec<Command>,
    pub(crate) subcommand_required: bool,
    pub(crate) handler: Option<Arc<dyn Handler>>,
}

impl Command {
    /// Constructs a command whose stable ID initially matches its name
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: name.clone(),
            name,
            aliases: Vec::new(),
            about: String::new(),
            version: None,
            options: Vec::new(),
            arguments: Vec::new(),
            subcommands: Vec::new(),
            subcommand_required: false,
            handler: None,
        }
    }

    /// Sets the stable command identity independently of its display name
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Adds a command alias
    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    /// Sets the short command description
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = about.into();
        self
    }

    /// Sets the root version used by the built-in version action
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Appends an option in help-definition order
    pub fn option(mut self, option: OptionSpec) -> Self {
        self.options.push(option);
        self
    }

    /// Appends a positional argument in consumption order
    pub fn argument(mut self, argument: Argument) -> Self {
        self.arguments.push(argument);
        self
    }

    /// Appends a child command in help-definition order
    pub fn subcommand(mut self, command: Command) -> Self {
        self.subcommands.push(command);
        self
    }

    /// Requires one child command to be selected
    pub fn require_subcommand(mut self) -> Self {
        self.subcommand_required = true;
        self
    }

    /// Sets the handler for this command
    pub fn handler<H>(mut self, handler: H) -> Self
    where
        H: Handler + 'static,
    {
        self.handler = Some(Arc::new(handler));
        self
    }

    /// Returns the stable command identity
    pub fn stable_id(&self) -> &str {
        &self.id
    }

    /// Returns the canonical command name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the short description
    pub fn description(&self) -> &str {
        &self.about
    }

    /// Validates the entire graph without consuming argv
    pub fn validate(&self) -> Result<(), Diagnostic> {
        let mut path_ids = BTreeSet::new();
        validate_command(self, true, &mut path_ids)
    }

    /// Renders deterministic help for a canonical path
    pub fn render_help(&self, path: &[String]) -> Result<String, Diagnostic> {
        self.validate()?;
        let command = self.command_at_path(path).ok_or_else(|| {
            Diagnostic::new(
                DiagnosticCode::InvalidSpecification,
                "help path does not identify a command",
            )
        })?;
        Ok(self.render_command_help(command, path))
    }

    pub(crate) fn command_at_path(&self, path: &[String]) -> Option<&Command> {
        if path.first().map(String::as_str) != Some(self.name.as_str()) {
            return None;
        }
        let mut command = self;
        for name in &path[1..] {
            command = command
                .subcommands
                .iter()
                .find(|candidate| candidate.name == *name)?;
        }
        Some(command)
    }

    pub(crate) fn usage_for_path(&self, path: &[String]) -> String {
        let command = self
            .command_at_path(path)
            .expect("validated parser paths identify commands");
        usage_line(command, path)
    }

    fn render_command_help(&self, command: &Command, path: &[String]) -> String {
        let mut output = String::new();
        if !command.about.is_empty() {
            output.push_str(&command.about);
            output.push_str("\n\n");
        }

        output.push_str("Usage:\n  ");
        output.push_str(&usage_line(command, path));
        output.push('\n');
        if !command.subcommands.is_empty() && !command.subcommand_required {
            output.push_str("  ");
            output.push_str(&path.join(" "));
            output.push_str(" [OPTIONS] <COMMAND>\n");
        }

        if !command.subcommands.is_empty() {
            output.push_str("\nCommands:\n");
            let entries = command
                .subcommands
                .iter()
                .map(|child| (child.name.clone(), child.about.clone()))
                .collect();
            render_entries(&mut output, entries);
        }

        if !command.arguments.is_empty() {
            output.push_str("\nArguments:\n");
            let entries = command
                .arguments
                .iter()
                .map(|argument| (argument_label(argument), argument.help.clone()))
                .collect();
            render_entries(&mut output, entries);
        }

        output.push_str("\nOptions:\n");
        let mut entries: Vec<(String, String)> = command
            .options
            .iter()
            .map(|option| (option_label(option), option_description(option)))
            .collect();
        entries.push(("-h, --help".to_owned(), "Print help".to_owned()));
        if self.version.is_some() {
            entries.push(("-V, --version".to_owned(), "Print version".to_owned()));
        }
        render_entries(&mut output, entries);
        output
    }
}

fn validate_command(
    command: &Command,
    root: bool,
    path_ids: &mut BTreeSet<String>,
) -> Result<(), Diagnostic> {
    if !valid_id(&command.id) {
        return invalid(format!("invalid command ID '{}'", command.id));
    }
    if !valid_name(&command.name) || reserved_long(&command.name) {
        return invalid(format!(
            "invalid or reserved command name '{}'",
            command.name
        ));
    }
    if !root && command.version.is_some() {
        return invalid(format!(
            "child command '{}' declares a version",
            command.name
        ));
    }
    for alias in &command.aliases {
        if !valid_name(alias) || reserved_long(alias) {
            return invalid(format!("invalid or reserved command alias '{alias}'"));
        }
    }
    if command
        .arguments
        .iter()
        .enumerate()
        .any(|(index, argument)| argument.repeated && index + 1 != command.arguments.len())
    {
        return invalid(format!(
            "command '{}' has a non-final repeated positional",
            command.name
        ));
    }

    let mut ids = path_ids.clone();
    let mut local_ids = BTreeSet::new();
    let mut longs = BTreeSet::new();
    let mut shorts = BTreeSet::new();
    for option in &command.options {
        if !valid_id(&option.id) || !ids.insert(option.id.clone()) {
            return invalid(format!("duplicate or invalid value ID '{}'", option.id));
        }
        local_ids.insert(option.id.clone());
        if option.long.is_none() && option.short.is_none() {
            return invalid(format!("option '{}' has no spelling", option.id));
        }
        if let Some(long) = &option.long {
            if !valid_name(long) || reserved_long(long) || !longs.insert(long.clone()) {
                return invalid(format!(
                    "duplicate, invalid, or reserved long option '{long}'"
                ));
            }
        }
        if let Some(short) = option.short {
            if !short.is_ascii_alphanumeric() || reserved_short(short) || !shorts.insert(short) {
                return invalid(format!(
                    "duplicate, invalid, or reserved short option '{short}'"
                ));
            }
        }
        if option.kind != OptionKind::Value
            && (option.repeated || option.environment.is_some() || option.default.is_some())
        {
            return invalid(format!(
                "non-value option '{}' has value-only configuration",
                option.id
            ));
        }
    }
    for argument in &command.arguments {
        if !valid_id(&argument.id) || !ids.insert(argument.id.clone()) {
            return invalid(format!("duplicate or invalid value ID '{}'", argument.id));
        }
    }
    for option in &command.options {
        for relation in option.requires.iter().chain(&option.conflicts) {
            if !local_ids.contains(relation) {
                return invalid(format!(
                    "option '{}' references unknown option '{relation}'",
                    option.id
                ));
            }
        }
    }

    let mut child_spellings = BTreeSet::new();
    for child in &command.subcommands {
        for spelling in std::iter::once(&child.name).chain(&child.aliases) {
            if !child_spellings.insert(spelling.clone()) {
                return invalid(format!(
                    "command '{}' has duplicate child spelling '{spelling}'",
                    command.name
                ));
            }
        }
        validate_command(child, false, &mut ids.clone())?;
    }
    *path_ids = ids;
    Ok(())
}

fn invalid<T>(message: String) -> Result<T, Diagnostic> {
    Err(Diagnostic::new(
        DiagnosticCode::InvalidSpecification,
        message,
    ))
}

fn valid_id(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(byte) if byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn valid_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(byte) if byte.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn reserved_long(value: &str) -> bool {
    matches!(value, "help" | "version")
}

fn reserved_short(value: char) -> bool {
    matches!(value, 'h' | 'V')
}

fn usage_line(command: &Command, path: &[String]) -> String {
    let mut usage = format!("{} [OPTIONS]", path.join(" "));
    for argument in &command.arguments {
        usage.push(' ');
        usage.push_str(&argument_label(argument));
    }
    if command.subcommand_required {
        usage.push_str(" <COMMAND>");
    }
    usage
}

fn argument_label(argument: &Argument) -> String {
    let name = argument.id.replace('_', "-").to_ascii_uppercase();
    let mut label = if argument.required {
        format!("<{name}>")
    } else {
        format!("[{name}]")
    };
    if argument.repeated {
        label.push_str("...");
    }
    label
}

fn option_label(option: &OptionSpec) -> String {
    let mut label = match (option.short, &option.long) {
        (Some(short), Some(long)) => format!("-{short}, --{long}"),
        (Some(short), None) => format!("-{short}"),
        (None, Some(long)) => format!("    --{long}"),
        (None, None) => String::new(),
    };
    if option.kind == OptionKind::Value {
        label.push(' ');
        label.push('<');
        label.push_str(option.parser.metavar());
        label.push('>');
        if option.repeated {
            label.push_str("...");
        }
    }
    label
}

fn option_description(option: &OptionSpec) -> String {
    let mut description = option.help.clone();
    if option.required {
        append_note(&mut description, "required");
    }
    if let Some(environment) = &option.environment {
        append_note(&mut description, &format!("env: {environment}"));
    }
    if let Some(default) = &option.default {
        append_note(
            &mut description,
            &format!("default: {}", display_os(default)),
        );
    }
    if !option.parser.possible_values().is_empty() {
        append_note(
            &mut description,
            &format!("possible: {}", option.parser.possible_values().join(", ")),
        );
    }
    description
}

fn append_note(description: &mut String, note: &str) {
    if !description.is_empty() {
        description.push(' ');
    }
    description.push('[');
    description.push_str(note);
    description.push(']');
}

fn render_entries(output: &mut String, entries: Vec<(String, String)>) {
    let width = entries
        .iter()
        .map(|(label, _)| text_width(label, WidthProfile::MODERN))
        .max()
        .unwrap_or(0);
    for (label, description) in entries {
        output.push_str("  ");
        output.push_str(&label);
        let label_width = text_width(&label, WidthProfile::MODERN);
        for _ in 0..width.saturating_sub(label_width).saturating_add(2) {
            output.push(' ');
        }
        output.push_str(&description);
        output.push('\n');
    }
}

pub(crate) fn option_display(option: &OptionSpec) -> String {
    if let Some(long) = &option.long {
        format!("--{long}")
    } else if let Some(short) = option.short {
        format!("-{short}")
    } else {
        option.id.clone()
    }
}

pub(crate) fn quote_value(value: &OsStr) -> String {
    format!("'{}'", display_os(value))
}
