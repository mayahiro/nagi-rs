use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, DiagnosticCode, display_os};
use crate::help::{HelpBlock, HelpExample, HelpLink, HelpSection, UsageVariantDefinition};
use crate::parser::Invocation;
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

/// Resolved or command-line presence used by validation
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresenceBasis {
    /// Values from command line, environment, or default
    Resolved,
    /// Values supplied in argv only
    CommandLine,
}

/// The cardinality rule for one option group
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionGroupKind {
    /// Accepts zero or one present option
    AtMostOne,
    /// Accepts exactly one present option
    ExactlyOne,
    /// Accepts one or more present options
    AtLeastOne,
    /// Accepts zero options or every option
    AllOrNone,
}

#[derive(Clone)]
pub(crate) struct OptionRelation {
    pub(crate) id: String,
    pub(crate) presence: PresenceBasis,
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
    pub(crate) requires: Vec<OptionRelation>,
    pub(crate) conflicts: Vec<OptionRelation>,
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
        self.requires.push(OptionRelation {
            id: id.into(),
            presence: PresenceBasis::Resolved,
        });
        self
    }

    /// Requires another command-line-supplied option
    pub fn requires_supplied(mut self, id: impl Into<String>) -> Self {
        self.requires.push(OptionRelation {
            id: id.into(),
            presence: PresenceBasis::CommandLine,
        });
        self
    }

    /// Conflicts with another option when both are present
    pub fn conflicts(mut self, id: impl Into<String>) -> Self {
        self.conflicts.push(OptionRelation {
            id: id.into(),
            presence: PresenceBasis::Resolved,
        });
        self
    }

    /// Conflicts with another command-line-supplied option
    pub fn conflicts_supplied(mut self, id: impl Into<String>) -> Self {
        self.conflicts.push(OptionRelation {
            id: id.into(),
            presence: PresenceBasis::CommandLine,
        });
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

/// One portable cardinality rule over local command options
#[derive(Clone)]
pub struct OptionGroup {
    pub(crate) id: String,
    pub(crate) kind: OptionGroupKind,
    pub(crate) presence: PresenceBasis,
    pub(crate) options: Vec<String>,
}

impl OptionGroup {
    /// Constructs an optional mutually exclusive option group
    pub fn at_most_one<I, S>(id: impl Into<String>, options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::new(id, OptionGroupKind::AtMostOne, options)
    }

    /// Constructs a required mutually exclusive option group
    pub fn exactly_one<I, S>(id: impl Into<String>, options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::new(id, OptionGroupKind::ExactlyOne, options)
    }

    /// Constructs a group requiring one or more options
    pub fn at_least_one<I, S>(id: impl Into<String>, options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::new(id, OptionGroupKind::AtLeastOne, options)
    }

    /// Constructs a group whose options must occur together
    pub fn all_or_none<I, S>(id: impl Into<String>, options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::new(id, OptionGroupKind::AllOrNone, options)
    }

    fn new<I, S>(id: impl Into<String>, kind: OptionGroupKind, options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            id: id.into(),
            kind,
            presence: PresenceBasis::CommandLine,
            options: options.into_iter().map(Into::into).collect(),
        }
    }

    /// Changes how this group determines whether an option is present
    pub const fn presence(mut self, presence: PresenceBasis) -> Self {
        self.presence = presence;
        self
    }

    /// Returns the stable group identifier
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the group cardinality rule
    pub const fn kind(&self) -> OptionGroupKind {
        self.kind
    }

    /// Returns the group's presence basis
    pub const fn presence_basis(&self) -> PresenceBasis {
        self.presence
    }

    /// Returns group members in definition order
    pub fn option_ids(&self) -> &[String] {
        &self.options
    }
}

/// Performs application-specific validation over a typed Invocation
///
/// During validation, unqualified value access starts at the Command that
/// declared the validator. Returning a Diagnostic preserves application codes,
/// semantic categories, value targets, and hints
pub trait InvocationValidator: Send + Sync {
    /// Returns a structured Diagnostic when validation fails
    fn validate(&self, invocation: &Invocation) -> Result<(), Diagnostic>;
}

impl<F> InvocationValidator for F
where
    F: Fn(&Invocation) -> Result<(), Diagnostic> + Send + Sync,
{
    fn validate(&self, invocation: &Invocation) -> Result<(), Diagnostic> {
        self(invocation)
    }
}

/// Selects how parent Help presents subcommand invocation syntax
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SubcommandUsageMode {
    /// Generates one generic optional-subcommand usage line
    #[default]
    Auto,
    /// Omits generated optional-subcommand usage
    Hidden,
    /// Expands each immediate child's direct usage variants without recursion
    Expanded,
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
    pub(crate) option_groups: Vec<OptionGroup>,
    pub(crate) subcommands: Vec<Command>,
    pub(crate) subcommand_required: bool,
    pub(crate) subcommand_usage: SubcommandUsageMode,
    pub(crate) usage_variants: Vec<UsageVariantDefinition>,
    pub(crate) examples: Vec<HelpExample>,
    pub(crate) notes: Vec<String>,
    pub(crate) links: Vec<HelpLink>,
    pub(crate) help_sections: Vec<HelpSection>,
    pub(crate) validators: Vec<Arc<dyn InvocationValidator>>,
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
            option_groups: Vec::new(),
            subcommands: Vec::new(),
            subcommand_required: false,
            subcommand_usage: SubcommandUsageMode::Auto,
            usage_variants: Vec::new(),
            examples: Vec::new(),
            notes: Vec::new(),
            links: Vec::new(),
            help_sections: Vec::new(),
            validators: Vec::new(),
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

    /// Appends one portable option-group constraint
    pub fn option_group(mut self, group: OptionGroup) -> Self {
        self.option_groups.push(group);
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

    /// Selects generic, hidden, or expanded subcommand usage in parent Help
    ///
    /// This changes Help presentation only and does not change parsing,
    /// validation, Diagnostics, or Invocation values
    pub const fn subcommand_usage(mut self, mode: SubcommandUsageMode) -> Self {
        self.subcommand_usage = mode;
        self
    }

    /// Appends one Help-only invocation syntax with a stable ID
    ///
    /// The syntax is a non-empty suffix relative to the canonical command path
    /// and does not change argv parsing, Invocation validation, Diagnostic
    /// usage, or Invocation values
    pub fn usage_variant(mut self, id: impl Into<String>, syntax: impl Into<String>) -> Self {
        self.usage_variants
            .push(UsageVariantDefinition::new(id, syntax));
        self
    }

    /// Appends one named command-line example
    pub fn example(mut self, name: impl Into<String>, invocation: impl Into<String>) -> Self {
        self.examples.push(HelpExample::new(name, invocation));
        self
    }

    /// Appends one structured Help note
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Appends one labeled documentation link
    pub fn link(mut self, label: impl Into<String>, url: impl Into<String>) -> Self {
        self.links.push(HelpLink::new(label, url));
        self
    }

    /// Appends one application-defined structured Help section
    pub fn help_section(mut self, section: HelpSection) -> Self {
        self.help_sections.push(section);
        self
    }

    /// Appends one language-native typed Invocation validator
    pub fn validator<V>(mut self, validator: V) -> Self
    where
        V: InvocationValidator + 'static,
    {
        self.validators.push(Arc::new(validator));
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
        validate_command(self, true)
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

    pub(crate) fn command_id_path_at_path(&self, path: &[String]) -> Option<Vec<String>> {
        if path.first().map(String::as_str) != Some(self.name.as_str()) {
            return None;
        }
        let mut command = self;
        let mut ids = vec![self.id.clone()];
        for name in &path[1..] {
            command = command
                .subcommands
                .iter()
                .find(|candidate| candidate.name == *name)?;
            ids.push(command.id.clone());
        }
        Some(ids)
    }

    pub(crate) fn usage_for_path(&self, path: &[String]) -> String {
        let command = self
            .command_at_path(path)
            .expect("validated parser paths identify commands");
        usage_line(command, path)
    }

    pub(crate) fn option_by_id(&self, id: &str) -> Option<&OptionSpec> {
        self.options.iter().find(|option| option.id == id)
    }
}

fn validate_command(command: &Command, root: bool) -> Result<(), Diagnostic> {
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

    let mut ids = BTreeSet::new();
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
            if !local_ids.contains(&relation.id) {
                return invalid(format!(
                    "option '{}' references unknown option '{}'",
                    option.id, relation.id
                ));
            }
        }
    }
    let mut group_ids = BTreeSet::new();
    for group in &command.option_groups {
        if !valid_id(&group.id) || !group_ids.insert(group.id.clone()) {
            return invalid(format!(
                "duplicate or invalid option group ID '{}'",
                group.id
            ));
        }
        if group.options.len() < 2 {
            return invalid(format!(
                "option group '{}' has fewer than two options",
                group.id
            ));
        }
        let mut members = BTreeSet::new();
        for id in &group.options {
            if !local_ids.contains(id) || !members.insert(id) {
                return invalid(format!(
                    "option group '{}' references duplicate or unknown option '{id}'",
                    group.id
                ));
            }
        }
    }
    validate_help(command)?;

    let mut child_spellings = BTreeSet::new();
    let mut child_ids = BTreeSet::new();
    for child in &command.subcommands {
        if !child_ids.insert(child.id.clone()) {
            return invalid(format!(
                "command '{}' has duplicate child ID '{}'",
                command.name, child.id
            ));
        }
        for spelling in std::iter::once(&child.name).chain(&child.aliases) {
            if !child_spellings.insert(spelling.clone()) {
                return invalid(format!(
                    "command '{}' has duplicate child spelling '{spelling}'",
                    command.name
                ));
            }
        }
        validate_command(child, false)?;
    }
    Ok(())
}

fn validate_help(command: &Command) -> Result<(), Diagnostic> {
    if !command.usage_variants.is_empty() && command.subcommand_required {
        return invalid(format!(
            "command '{}' declares Help usage variants while requiring a subcommand",
            command.name
        ));
    }
    let mut usage_ids = BTreeSet::new();
    for variant in &command.usage_variants {
        if !valid_id(&variant.id)
            || !usage_ids.insert(&variant.id)
            || !valid_usage_syntax(&variant.syntax)
        {
            return invalid(format!(
                "command '{}' has an invalid Help usage variant '{}'",
                command.name, variant.id
            ));
        }
        if command.subcommand_usage == SubcommandUsageMode::Auto
            && !command.subcommands.is_empty()
            && variant.id == "subcommand"
        {
            return invalid(format!(
                "command '{}' Help usage variant '{}' conflicts with a generated variant",
                command.name, variant.id
            ));
        }
    }
    if command
        .examples
        .iter()
        .any(|example| example.name().is_empty() || example.invocation().is_empty())
    {
        return invalid(format!(
            "command '{}' has an invalid Help example",
            command.name
        ));
    }
    if command.notes.iter().any(String::is_empty) {
        return invalid(format!(
            "command '{}' has an invalid Help note",
            command.name
        ));
    }
    if command
        .links
        .iter()
        .any(|link| link.label().is_empty() || link.url().is_empty())
    {
        return invalid(format!(
            "command '{}' has an invalid Help link",
            command.name
        ));
    }
    let mut section_ids = BTreeSet::new();
    for section in &command.help_sections {
        if !valid_id(section.id())
            || !section_ids.insert(section.id())
            || section.heading().is_empty()
            || section.blocks().is_empty()
        {
            return invalid(format!(
                "command '{}' has an invalid Help section '{}'",
                command.name,
                section.id()
            ));
        }
        if section.blocks().iter().any(|block| match block {
            HelpBlock::Paragraph(text) => text.is_empty(),
            HelpBlock::Entry { label, description } => label.is_empty() || description.is_empty(),
        }) {
            return invalid(format!(
                "Help section '{}' has an invalid block",
                section.id()
            ));
        }
    }
    Ok(())
}

fn valid_usage_syntax(syntax: &str) -> bool {
    let bytes = syntax.as_bytes();
    !bytes.is_empty()
        && bytes.first() != Some(&b' ')
        && bytes.last() != Some(&b' ')
        && bytes.iter().all(|byte| *byte >= 0x20 && *byte != 0x7f)
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

pub(crate) fn usage_line(command: &Command, path: &[String]) -> String {
    usage_command_line(path, &generated_usage_syntax(command))
}

pub(crate) fn generated_usage_syntax(command: &Command) -> String {
    let mut syntax = "[OPTIONS]".to_owned();
    for argument in &command.arguments {
        syntax.push(' ');
        syntax.push_str(&argument_label(argument));
    }
    if command.subcommand_required {
        syntax.push_str(" <COMMAND>");
    }
    syntax
}

pub(crate) fn usage_command_line(path: &[String], syntax: &str) -> String {
    format!("{} {syntax}", path.join(" "))
}

pub(crate) fn argument_label(argument: &Argument) -> String {
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

pub(crate) fn option_label(option: &OptionSpec) -> String {
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

pub(crate) fn option_description(option: &OptionSpec) -> String {
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
