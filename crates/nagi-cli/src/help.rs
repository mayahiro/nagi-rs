use nagi_text::{WidthProfile, text_width};

use crate::command::{
    Command, OptionGroupKind, PresenceBasis, SubcommandUsageMode, argument_label,
    generated_usage_syntax, option_description, option_display, option_label, usage_command_line,
};
use crate::diagnostic::{Diagnostic, DiagnosticCode};

#[derive(Clone)]
pub(crate) struct UsageVariantDefinition {
    pub(crate) id: String,
    pub(crate) syntax: String,
}

impl UsageVariantDefinition {
    pub(crate) fn new(id: impl Into<String>, syntax: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            syntax: syntax.into(),
        }
    }
}

/// One structured invocation syntax in a Help Document
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpUsageVariant {
    command_id_path: Vec<String>,
    id: String,
    syntax: String,
    command_line: String,
}

impl HelpUsageVariant {
    fn new(
        command_id_path: Vec<String>,
        id: impl Into<String>,
        syntax: impl Into<String>,
        help_path: &[String],
    ) -> Self {
        let syntax = syntax.into();
        Self {
            command_id_path,
            id: id.into(),
            command_line: usage_command_line(help_path, &syntax),
            syntax,
        }
    }

    /// Returns the source stable command-ID path
    pub fn command_id_path(&self) -> &[String] {
        &self.command_id_path
    }

    /// Returns the stable variant identifier
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the command-path-relative syntax suffix
    pub fn syntax(&self) -> &str {
        &self.syntax
    }

    /// Returns the complete canonical usage line
    pub fn command_line(&self) -> &str {
        &self.command_line
    }
}

/// One labeled description in a Help Document
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpEntry {
    id: String,
    label: String,
    description: String,
}

impl HelpEntry {
    pub(crate) fn identified(
        id: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: description.into(),
        }
    }

    /// Returns the stable command, argument, option, or generated entry identifier
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the cell-aligned entry label
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the entry description
    pub fn description(&self) -> &str {
        &self.description
    }
}

/// One named command invocation
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpExample {
    name: String,
    invocation: String,
}

impl HelpExample {
    pub(crate) fn new(name: impl Into<String>, invocation: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            invocation: invocation.into(),
        }
    }

    /// Returns the example purpose
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the complete example command line
    pub fn invocation(&self) -> &str {
        &self.invocation
    }
}

/// One labeled documentation link
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpLink {
    label: String,
    url: String,
}

impl HelpLink {
    pub(crate) fn new(label: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            url: url.into(),
        }
    }

    /// Returns the resource label
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the link target
    pub fn url(&self) -> &str {
        &self.url
    }
}

/// One ordered block in a custom Help section
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HelpBlock {
    /// One indented text paragraph
    Paragraph(String),
    /// One cell-aligned labeled description
    Entry {
        /// The display label
        label: String,
        /// The label description
        description: String,
    },
}

/// One application-defined structured Help section
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpSection {
    id: String,
    heading: String,
    blocks: Vec<HelpBlock>,
}

impl HelpSection {
    /// Constructs an empty custom Help section
    pub fn new(id: impl Into<String>, heading: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            heading: heading.into(),
            blocks: Vec::new(),
        }
    }

    /// Appends one text paragraph
    pub fn paragraph(mut self, text: impl Into<String>) -> Self {
        self.blocks.push(HelpBlock::Paragraph(text.into()));
        self
    }

    /// Appends one labeled description
    pub fn entry(mut self, label: impl Into<String>, description: impl Into<String>) -> Self {
        self.blocks.push(HelpBlock::Entry {
            label: label.into(),
            description: description.into(),
        });
        self
    }

    /// Returns the stable section identifier
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the rendered section heading
    pub fn heading(&self) -> &str {
        &self.heading
    }

    /// Returns ordered section blocks
    pub fn blocks(&self) -> &[HelpBlock] {
        &self.blocks
    }
}

/// Structured metadata for one portable option-group constraint
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpOptionGroup {
    id: String,
    kind: OptionGroupKind,
    presence: PresenceBasis,
    option_ids: Vec<String>,
    option_labels: Vec<String>,
}

impl HelpOptionGroup {
    /// Returns the stable group identifier
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the group cardinality rule
    pub fn kind(&self) -> OptionGroupKind {
        self.kind
    }

    /// Returns the resolved or command-line presence basis
    pub fn presence(&self) -> PresenceBasis {
        self.presence
    }

    /// Returns stable member IDs in definition order
    pub fn option_ids(&self) -> &[String] {
        &self.option_ids
    }

    /// Returns option display spellings in definition order
    pub fn option_labels(&self) -> &[String] {
        &self.option_labels
    }
}

/// Requires or conflicts behavior for one pairwise Help constraint
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpOptionRelationKind {
    /// The target is required when the source is present
    Requires,
    /// The source and target cannot be present together
    Conflicts,
}

/// Structured metadata for one portable pairwise option constraint
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpOptionRelation {
    kind: HelpOptionRelationKind,
    source_id: String,
    source_label: String,
    target_id: String,
    target_label: String,
    presence: PresenceBasis,
}

impl HelpOptionRelation {
    /// Returns requires or conflicts behavior
    pub const fn kind(&self) -> HelpOptionRelationKind {
        self.kind
    }

    /// Returns the stable source option ID
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Returns the source option display spelling
    pub fn source_label(&self) -> &str {
        &self.source_label
    }

    /// Returns the stable target option ID
    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    /// Returns the target option display spelling
    pub fn target_label(&self) -> &str {
        &self.target_label
    }

    /// Returns the resolved or command-line presence basis
    pub const fn presence(&self) -> PresenceBasis {
        self.presence
    }
}

/// The structured, renderer-independent Help representation
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpDocument {
    command_path: Vec<String>,
    description: String,
    usage: Vec<String>,
    usage_variants: Vec<HelpUsageVariant>,
    commands: Vec<HelpEntry>,
    arguments: Vec<HelpEntry>,
    options: Vec<HelpEntry>,
    option_relations: Vec<HelpOptionRelation>,
    option_groups: Vec<HelpOptionGroup>,
    examples: Vec<HelpExample>,
    notes: Vec<String>,
    links: Vec<HelpLink>,
    sections: Vec<HelpSection>,
}

impl HelpDocument {
    /// Returns the canonical root-to-target path
    pub fn command_path(&self) -> &[String] {
        &self.command_path
    }

    /// Returns the command description
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Returns rendered usage lines
    pub fn usage(&self) -> &[String] {
        &self.usage
    }

    /// Returns structured usage metadata
    pub fn usage_variants(&self) -> &[HelpUsageVariant] {
        &self.usage_variants
    }

    /// Returns child-command entries
    pub fn commands(&self) -> &[HelpEntry] {
        &self.commands
    }

    /// Returns positional-argument entries
    pub fn arguments(&self) -> &[HelpEntry] {
        &self.arguments
    }

    /// Returns option entries
    pub fn options(&self) -> &[HelpEntry] {
        &self.options
    }

    /// Returns pairwise option constraints
    pub fn option_relations(&self) -> &[HelpOptionRelation] {
        &self.option_relations
    }

    /// Returns option-group metadata
    pub fn option_groups(&self) -> &[HelpOptionGroup] {
        &self.option_groups
    }

    /// Returns named examples
    pub fn examples(&self) -> &[HelpExample] {
        &self.examples
    }

    /// Returns Help notes
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// Returns documentation links
    pub fn links(&self) -> &[HelpLink] {
        &self.links
    }

    /// Returns custom Help sections
    pub fn sections(&self) -> &[HelpSection] {
        &self.sections
    }

    /// Renders this document with the standard plain renderer
    pub fn render(&self) -> String {
        PlainHelpRenderer.render_help(self)
    }
}

/// Renders one structured Help Document
pub trait HelpRenderer: Send + Sync {
    /// Returns deterministic text with one final newline
    fn render_help(&self, document: &HelpDocument) -> String;
}

/// The standard cell-aware plain Help renderer
#[derive(Clone, Copy, Debug, Default)]
pub struct PlainHelpRenderer;

impl HelpRenderer for PlainHelpRenderer {
    fn render_help(&self, document: &HelpDocument) -> String {
        let mut output = String::new();
        if !document.description.is_empty() {
            output.push_str(&document.description);
            output.push_str("\n\n");
        }

        output.push_str("Usage:\n");
        for usage in &document.usage {
            output.push_str("  ");
            output.push_str(usage);
            output.push('\n');
        }
        render_entry_section(&mut output, "Commands", &document.commands);
        render_entry_section(&mut output, "Arguments", &document.arguments);
        render_entry_section(&mut output, "Options", &document.options);

        let mut constraints = document
            .option_relations
            .iter()
            .map(|relation| {
                HelpEntry::identified(
                    &relation.source_id,
                    &relation.source_label,
                    option_relation_description(relation),
                )
            })
            .collect::<Vec<_>>();
        constraints.extend(document.option_groups.iter().map(|group| {
            HelpEntry::identified(&group.id, &group.id, option_group_description(group))
        }));
        render_entry_section(&mut output, "Constraints", &constraints);

        let examples = document
            .examples
            .iter()
            .map(|example| HelpEntry::identified(&example.name, &example.name, &example.invocation))
            .collect::<Vec<_>>();
        render_entry_section(&mut output, "Examples", &examples);

        if !document.notes.is_empty() {
            output.push_str("\nNotes:\n");
            for note in &document.notes {
                render_paragraph(&mut output, note);
            }
        }

        let links = document
            .links
            .iter()
            .map(|link| HelpEntry::identified(&link.label, &link.label, &link.url))
            .collect::<Vec<_>>();
        render_entry_section(&mut output, "Links", &links);

        for section in &document.sections {
            output.push('\n');
            output.push_str(&section.heading);
            output.push_str(":\n");
            render_blocks(&mut output, &section.blocks);
        }
        output
    }
}

impl Command {
    /// Returns structured Help for a canonical command path
    pub fn help_document(&self, path: &[String]) -> Result<HelpDocument, Diagnostic> {
        self.validate()?;
        let command = self.command_at_path(path).ok_or_else(|| {
            Diagnostic::new(
                DiagnosticCode::InvalidSpecification,
                "help path does not identify a command",
            )
        })?;
        let command_id_path = self
            .command_id_path_at_path(path)
            .expect("the validated Help path was resolved above");

        let usage_variants = help_usage_variants(command, path, &command_id_path);
        let usage = usage_variants
            .iter()
            .map(|variant| variant.command_line.clone())
            .collect();

        let mut commands = command
            .subcommands
            .iter()
            .map(|child| HelpEntry::identified(&child.id, &child.name, &child.about))
            .collect::<Vec<_>>();
        if path.len() == 1 && !command.subcommands.is_empty() {
            commands.push(HelpEntry::identified(
                "help",
                "help",
                "Print this message or the help of the given command",
            ));
        }

        let arguments = command
            .arguments
            .iter()
            .map(|argument| {
                HelpEntry::identified(&argument.id, argument_label(argument), &argument.help)
            })
            .collect();
        let mut options = Vec::with_capacity(command.options.len() + 2);
        let mut option_relations = Vec::new();
        for option in &command.options {
            options.push(HelpEntry::identified(
                &option.id,
                option_label(option),
                option_description(option),
            ));
            option_relations.extend(option.requires.iter().map(|relation| {
                help_option_relation(command, option, relation, HelpOptionRelationKind::Requires)
            }));
            option_relations.extend(option.conflicts.iter().map(|relation| {
                help_option_relation(command, option, relation, HelpOptionRelationKind::Conflicts)
            }));
        }
        options.push(HelpEntry::identified("help", "-h, --help", "Print help"));
        if self.version.is_some() {
            options.push(HelpEntry::identified(
                "version",
                "-V, --version",
                "Print version",
            ));
        }

        let option_groups = command
            .option_groups
            .iter()
            .map(|group| HelpOptionGroup {
                id: group.id.clone(),
                kind: group.kind,
                presence: group.presence,
                option_ids: group.options.clone(),
                option_labels: group
                    .options
                    .iter()
                    .map(|id| {
                        option_display(
                            command
                                .option_by_id(id)
                                .expect("validated option groups reference local options"),
                        )
                    })
                    .collect(),
            })
            .collect();

        Ok(HelpDocument {
            command_path: path.to_vec(),
            description: command.about.clone(),
            usage,
            usage_variants,
            commands,
            arguments,
            options,
            option_relations,
            option_groups,
            examples: command.examples.clone(),
            notes: command.notes.clone(),
            links: command.links.clone(),
            sections: command.help_sections.clone(),
        })
    }

    /// Renders standard Help for a canonical command path
    pub fn render_help(&self, path: &[String]) -> Result<String, Diagnostic> {
        Ok(self.help_document(path)?.render())
    }
}

fn help_usage_variants(
    command: &Command,
    path: &[String],
    command_id_path: &[String],
) -> Vec<HelpUsageVariant> {
    let mut variants = if command.subcommand_usage == SubcommandUsageMode::Expanded
        && command.subcommand_required
        && command.usage_variants.is_empty()
    {
        Vec::new()
    } else {
        direct_usage_variants(command, path, command_id_path, "")
    };
    match command.subcommand_usage {
        SubcommandUsageMode::Auto
            if !command.subcommands.is_empty() && !command.subcommand_required =>
        {
            variants.push(HelpUsageVariant::new(
                command_id_path.to_vec(),
                "subcommand",
                "[OPTIONS] <COMMAND>",
                path,
            ));
        }
        SubcommandUsageMode::Expanded => {
            for child in &command.subcommands {
                let mut child_id_path = command_id_path.to_vec();
                child_id_path.push(child.id.clone());
                variants.extend(direct_usage_variants(
                    child,
                    path,
                    &child_id_path,
                    &child.name,
                ));
            }
        }
        SubcommandUsageMode::Auto | SubcommandUsageMode::Hidden => {}
    }
    variants
}

fn direct_usage_variants(
    command: &Command,
    help_path: &[String],
    command_id_path: &[String],
    prefix: &str,
) -> Vec<HelpUsageVariant> {
    if command.usage_variants.is_empty() {
        let syntax = prefixed_usage(prefix, &generated_usage_syntax(command));
        return vec![HelpUsageVariant::new(
            command_id_path.to_vec(),
            "default",
            syntax,
            help_path,
        )];
    }
    command
        .usage_variants
        .iter()
        .map(|variant| {
            HelpUsageVariant::new(
                command_id_path.to_vec(),
                &variant.id,
                prefixed_usage(prefix, &variant.syntax),
                help_path,
            )
        })
        .collect()
}

fn prefixed_usage(prefix: &str, syntax: &str) -> String {
    if prefix.is_empty() {
        syntax.to_owned()
    } else {
        format!("{prefix} {syntax}")
    }
}

fn render_entry_section(output: &mut String, heading: &str, entries: &[HelpEntry]) {
    if entries.is_empty() {
        return;
    }
    output.push('\n');
    output.push_str(heading);
    output.push_str(":\n");
    render_entries(output, entries);
}

fn render_entries(output: &mut String, entries: &[HelpEntry]) {
    let width = entries
        .iter()
        .map(|entry| text_width(&entry.label, WidthProfile::MODERN))
        .max()
        .unwrap_or(0);
    for entry in entries {
        output.push_str("  ");
        output.push_str(&entry.label);
        let label_width = text_width(&entry.label, WidthProfile::MODERN);
        for _ in 0..width.saturating_sub(label_width).saturating_add(2) {
            output.push(' ');
        }
        output.push_str(&entry.description);
        output.push('\n');
    }
}

fn render_blocks(output: &mut String, blocks: &[HelpBlock]) {
    let width = blocks
        .iter()
        .filter_map(|block| match block {
            HelpBlock::Paragraph(_) => None,
            HelpBlock::Entry { label, .. } => Some(text_width(label, WidthProfile::MODERN)),
        })
        .max()
        .unwrap_or(0);
    for block in blocks {
        match block {
            HelpBlock::Paragraph(text) => render_paragraph(output, text),
            HelpBlock::Entry { label, description } => {
                output.push_str("  ");
                output.push_str(label);
                let label_width = text_width(label, WidthProfile::MODERN);
                for _ in 0..width.saturating_sub(label_width).saturating_add(2) {
                    output.push(' ');
                }
                output.push_str(description);
                output.push('\n');
            }
        }
    }
}

fn render_paragraph(output: &mut String, text: &str) {
    for line in text.split('\n') {
        output.push_str("  ");
        output.push_str(line);
        output.push('\n');
    }
}

fn option_group_description(group: &HelpOptionGroup) -> String {
    let rule = match group.kind {
        OptionGroupKind::AtMostOne => "at most one of ",
        OptionGroupKind::ExactlyOne => "exactly one of ",
        OptionGroupKind::AtLeastOne => "at least one of ",
        OptionGroupKind::AllOrNone => "all or none of ",
    };
    format!(
        "{rule}{}{}",
        group.option_labels.join(", "),
        presence_description(group.presence)
    )
}

fn help_option_relation(
    command: &Command,
    source: &crate::command::OptionSpec,
    relation: &crate::command::OptionRelation,
    kind: HelpOptionRelationKind,
) -> HelpOptionRelation {
    HelpOptionRelation {
        kind,
        source_id: source.id.clone(),
        source_label: option_display(source),
        target_id: relation.id.clone(),
        target_label: option_display(
            command
                .option_by_id(&relation.id)
                .expect("validated relations reference local options"),
        ),
        presence: relation.presence,
    }
}

fn option_relation_description(relation: &HelpOptionRelation) -> String {
    let rule = match relation.kind {
        HelpOptionRelationKind::Requires => "requires ",
        HelpOptionRelationKind::Conflicts => "conflicts with ",
    };
    format!(
        "{rule}{}{}",
        relation.target_label,
        presence_description(relation.presence)
    )
}

fn presence_description(presence: PresenceBasis) -> &'static str {
    match presence {
        PresenceBasis::Resolved => " [resolved]",
        PresenceBasis::CommandLine => " [command line]",
    }
}
