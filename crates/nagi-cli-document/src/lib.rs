//! Deterministic Markdown and man page renderers for Nagi CLI Help Documents
//!
//! The renderers are pure formatting adapters over [`nagi_cli::HelpDocument`]
//! and perform no filesystem I/O. Application-provided strings are treated as
//! plain text and escaped for the target format

#![deny(missing_docs)]
#![deny(unsafe_code)]

use nagi_cli::{
    HelpBlock, HelpDocument, HelpEntry, HelpOptionGroup, HelpOptionRelation,
    HelpOptionRelationKind, HelpRenderer, OptionGroupKind, PresenceBasis,
};

/// Renders one Help Document as deterministic CommonMark
#[derive(Clone, Copy, Debug, Default)]
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    /// Renders one Help Document with exactly one final newline
    pub fn render(self, document: &HelpDocument) -> String {
        render_markdown(document)
    }
}

impl HelpRenderer for MarkdownRenderer {
    fn render_help(&self, document: &HelpDocument) -> String {
        render_markdown(document)
    }
}

/// Renders one Help Document with the portable `man` macro subset
#[derive(Clone, Copy, Debug, Default)]
pub struct ManRenderer;

impl ManRenderer {
    /// Renders one section 1 man page with exactly one final newline
    pub fn render(self, document: &HelpDocument) -> String {
        render_man(document)
    }
}

impl HelpRenderer for ManRenderer {
    fn render_help(&self, document: &HelpDocument) -> String {
        render_man(document)
    }
}

fn render_markdown(document: &HelpDocument) -> String {
    let mut output = String::new();
    output.push_str("# ");
    push_markdown_inline(&mut output, &document.command_path().join(" "));
    output.push_str("\n\n");

    if !document.description().is_empty() {
        push_markdown_text(&mut output, document.description(), "\n");
        output.push_str("\n\n");
    }
    if let Some(deprecation) = document.deprecation() {
        output.push_str("**Deprecated:** use ");
        push_markdown_inline(&mut output, deprecation.replacement());
        output.push_str("\n\n");
    }

    output.push_str("## Usage\n\n");
    for usage in document.usage() {
        push_markdown_code_block(&mut output, usage);
    }
    push_markdown_entries(&mut output, "Commands", document.commands());
    push_markdown_entries(&mut output, "Arguments", document.arguments());
    push_markdown_entries(&mut output, "Options", document.options());

    if !document.inherited_options().is_empty() {
        output.push_str("## Inherited Options\n\n");
        for option in document.inherited_options() {
            let mut description = option.description().to_owned();
            if !description.is_empty() {
                description.push(' ');
            }
            description.push_str("[from ");
            description.push_str(&option.command_path().join(" "));
            description.push(']');
            push_markdown_entry(
                &mut output,
                option.label(),
                &description,
                option.deprecation().map(|value| value.replacement()),
            );
        }
        output.push('\n');
    }

    if !document.option_relations().is_empty() || !document.option_groups().is_empty() {
        output.push_str("## Constraints\n\n");
        for relation in document.option_relations() {
            push_markdown_entry(
                &mut output,
                relation.source_label(),
                &option_relation_description(relation),
                None,
            );
        }
        for group in document.option_groups() {
            push_markdown_entry(
                &mut output,
                group.id(),
                &option_group_description(group),
                None,
            );
        }
        output.push('\n');
    }

    if !document.examples().is_empty() {
        output.push_str("## Examples\n\n");
        for example in document.examples() {
            output.push_str("### ");
            push_markdown_inline(&mut output, example.name());
            output.push_str("\n\n");
            push_markdown_code_block(&mut output, example.invocation());
        }
    }

    if !document.notes().is_empty() {
        output.push_str("## Notes\n\n");
        for note in document.notes() {
            push_markdown_text(&mut output, note, "\n");
            output.push_str("\n\n");
        }
    }

    if !document.links().is_empty() {
        output.push_str("## Links\n\n");
        for link in document.links() {
            output.push_str("- [");
            push_markdown_inline(&mut output, link.label());
            output.push_str("](<");
            push_markdown_destination(&mut output, link.url());
            output.push_str(">)\n");
        }
        output.push('\n');
    }

    for section in document.sections() {
        output.push_str("## ");
        push_markdown_inline(&mut output, section.heading());
        output.push_str("\n\n");
        for block in section.blocks() {
            match block {
                HelpBlock::Paragraph(text) => {
                    push_markdown_text(&mut output, text, "\n");
                    output.push_str("\n\n");
                }
                HelpBlock::Entry { label, description } => {
                    push_markdown_entry(&mut output, label, description, None);
                }
            }
        }
        output.push('\n');
    }

    finish_output(output)
}

fn push_markdown_entries(output: &mut String, heading: &str, entries: &[HelpEntry]) {
    if entries.is_empty() {
        return;
    }
    output.push_str("## ");
    output.push_str(heading);
    output.push_str("\n\n");
    for entry in entries {
        push_markdown_entry(
            output,
            entry.label(),
            entry.description(),
            entry.deprecation().map(|value| value.replacement()),
        );
    }
    output.push('\n');
}

fn push_markdown_entry(
    output: &mut String,
    label: &str,
    description: &str,
    replacement: Option<&str>,
) {
    output.push_str("- **");
    push_markdown_inline(output, label);
    output.push_str("**");
    if !description.is_empty() {
        output.push_str(": ");
        push_markdown_text(output, description, "  \n  ");
    }
    if let Some(replacement) = replacement {
        output.push_str(" [deprecated: use ");
        push_markdown_inline(output, replacement);
        output.push(']');
    }
    output.push('\n');
}

fn push_markdown_code_block(output: &mut String, text: &str) {
    output.push_str("    ");
    for_normalized_chars(text, |character| {
        if character == '\n' {
            output.push_str("\n    ");
        } else {
            output.push(character);
        }
    });
    output.push_str("\n\n");
}

fn push_markdown_text(output: &mut String, text: &str, newline: &str) {
    let mut line_start = true;
    for_normalized_chars(text, |character| {
        if character == '\n' {
            output.push_str(newline);
            line_start = true;
        } else if line_start && character == ' ' {
            output.push_str("&#32;");
            line_start = false;
        } else {
            push_markdown_character(output, character);
            line_start = false;
        }
    });
}

fn push_markdown_inline(output: &mut String, text: &str) {
    for_normalized_chars(text, |character| {
        push_markdown_character(
            output,
            if character == '\n' {
                '\u{fffd}'
            } else {
                character
            },
        );
    });
}

fn push_markdown_character(output: &mut String, character: char) {
    if character.is_ascii_punctuation() {
        output.push('\\');
    }
    output.push(character);
}

fn push_markdown_destination(output: &mut String, text: &str) {
    for_normalized_chars(text, |character| {
        if character.is_whitespace()
            || character.is_control()
            || matches!(character, '<' | '>' | '\\')
        {
            let mut bytes = [0_u8; 4];
            for byte in character.encode_utf8(&mut bytes).as_bytes() {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                output.push('%');
                output.push(char::from(HEX[usize::from(*byte >> 4)]));
                output.push(char::from(HEX[usize::from(*byte & 0x0f)]));
            }
        } else {
            output.push(character);
        }
    });
}

fn render_man(document: &HelpDocument) -> String {
    let path = document.command_path().join(" ");
    let mut output = String::new();
    output.push_str(".\\\" Generated by Nagi\n.TH \"");
    push_roff_argument(&mut output, &path.to_ascii_uppercase());
    output.push_str("\" \"1\"\n.SH NAME\n");
    push_roff_inline(&mut output, &path);
    if !document.description().is_empty() {
        output.push_str(" \\- ");
        push_roff_inline(&mut output, document.description());
    }
    output.push('\n');

    output.push_str(".SH SYNOPSIS\n.nf\n");
    for usage in document.usage() {
        push_roff_text(&mut output, usage);
        output.push('\n');
    }
    output.push_str(".fi\n");

    if !document.description().is_empty() {
        output.push_str(".SH DESCRIPTION\n");
        push_roff_text(&mut output, document.description());
        output.push('\n');
    }
    if let Some(deprecation) = document.deprecation() {
        output.push_str(".SH DEPRECATED\n");
        push_roff_text(&mut output, "Use ");
        push_roff_text(&mut output, deprecation.replacement());
        output.push_str(" instead\n");
    }

    push_man_entries(&mut output, "COMMANDS", document.commands());
    push_man_entries(&mut output, "ARGUMENTS", document.arguments());
    push_man_entries(&mut output, "OPTIONS", document.options());

    if !document.inherited_options().is_empty() {
        output.push_str(".SH \"INHERITED OPTIONS\"\n");
        for option in document.inherited_options() {
            let mut description = option.description().to_owned();
            if !description.is_empty() {
                description.push(' ');
            }
            description.push_str("[from ");
            description.push_str(&option.command_path().join(" "));
            description.push(']');
            push_man_entry(
                &mut output,
                option.label(),
                &description,
                option.deprecation().map(|value| value.replacement()),
            );
        }
    }

    if !document.option_relations().is_empty() || !document.option_groups().is_empty() {
        output.push_str(".SH CONSTRAINTS\n");
        for relation in document.option_relations() {
            push_man_entry(
                &mut output,
                relation.source_label(),
                &option_relation_description(relation),
                None,
            );
        }
        for group in document.option_groups() {
            push_man_entry(
                &mut output,
                group.id(),
                &option_group_description(group),
                None,
            );
        }
    }

    if !document.examples().is_empty() {
        output.push_str(".SH EXAMPLES\n");
        for (index, example) in document.examples().iter().enumerate() {
            push_roff_paragraph(&mut output, example.name(), index != 0);
            output.push_str(".nf\n");
            push_roff_text(&mut output, example.invocation());
            output.push_str("\n.fi\n");
        }
    }
    if !document.notes().is_empty() {
        output.push_str(".SH NOTES\n");
        for (index, note) in document.notes().iter().enumerate() {
            push_roff_paragraph(&mut output, note, index != 0);
        }
    }
    if !document.links().is_empty() {
        output.push_str(".SH LINKS\n");
        for link in document.links() {
            push_man_entry(&mut output, link.label(), link.url(), None);
        }
    }
    for section in document.sections() {
        output.push_str(".SH \"");
        push_roff_argument(&mut output, section.heading());
        output.push_str("\"\n");
        for (index, block) in section.blocks().iter().enumerate() {
            match block {
                HelpBlock::Paragraph(text) => {
                    push_roff_paragraph(&mut output, text, index != 0);
                }
                HelpBlock::Entry { label, description } => {
                    push_man_entry(&mut output, label, description, None);
                }
            }
        }
    }

    finish_output(output)
}

fn push_man_entries(output: &mut String, heading: &str, entries: &[HelpEntry]) {
    if entries.is_empty() {
        return;
    }
    output.push_str(".SH ");
    output.push_str(heading);
    output.push('\n');
    for entry in entries {
        push_man_entry(
            output,
            entry.label(),
            entry.description(),
            entry.deprecation().map(|value| value.replacement()),
        );
    }
}

fn push_man_entry(output: &mut String, label: &str, description: &str, replacement: Option<&str>) {
    output.push_str(".TP\n\\fB");
    push_roff_text(output, label);
    output.push_str("\\fP\n");
    if !description.is_empty() {
        push_roff_text(output, description);
    }
    if let Some(replacement) = replacement {
        if !description.is_empty() {
            output.push(' ');
        }
        push_roff_text(output, "[deprecated: use ");
        push_roff_text(output, replacement);
        push_roff_text(output, "]");
    }
    output.push('\n');
}

fn push_roff_paragraph(output: &mut String, text: &str, separated: bool) {
    if separated {
        output.push_str(".PP\n");
    }
    push_roff_text(output, text);
    output.push('\n');
}

fn push_roff_text(output: &mut String, text: &str) {
    let mut line_start = true;
    for_normalized_chars(text, |character| {
        if character == '\n' {
            output.push('\n');
            line_start = true;
            return;
        }
        if line_start && matches!(character, '.' | '\'') {
            output.push_str("\\&");
        }
        line_start = false;
        match character {
            '\\' => output.push_str("\\e"),
            '-' => output.push_str("\\-"),
            _ => output.push(character),
        }
    });
}

fn push_roff_inline(output: &mut String, text: &str) {
    for_normalized_chars(text, |character| match character {
        '\n' => output.push('\u{fffd}'),
        '\\' => output.push_str("\\e"),
        '-' => output.push_str("\\-"),
        _ => output.push(character),
    });
}

fn push_roff_argument(output: &mut String, text: &str) {
    for_normalized_chars(text, |character| match character {
        '\n' => output.push('\u{fffd}'),
        '\\' => output.push_str("\\e"),
        '-' => output.push_str("\\-"),
        '"' => output.push_str("\\(dq"),
        _ => output.push(character),
    });
}

fn for_normalized_chars(text: &str, mut visitor: impl FnMut(char)) {
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                visitor('\n');
            }
            '\n' => visitor('\n'),
            value if value.is_control() => visitor('\u{fffd}'),
            value => visitor(value),
        }
    }
}

fn option_group_description(group: &HelpOptionGroup) -> String {
    let rule = match group.kind() {
        OptionGroupKind::AtMostOne => "at most one of ",
        OptionGroupKind::ExactlyOne => "exactly one of ",
        OptionGroupKind::AtLeastOne => "at least one of ",
        OptionGroupKind::AllOrNone => "all or none of ",
    };
    format!(
        "{rule}{}{}",
        group.option_labels().join(", "),
        presence_description(group.presence())
    )
}

fn option_relation_description(relation: &HelpOptionRelation) -> String {
    let rule = match relation.kind() {
        HelpOptionRelationKind::Requires => "requires ",
        HelpOptionRelationKind::Conflicts => "conflicts with ",
    };
    format!(
        "{rule}{}{}",
        relation.target_label(),
        presence_description(relation.presence())
    )
}

fn presence_description(presence: PresenceBasis) -> &'static str {
    match presence {
        PresenceBasis::Resolved => " [resolved]",
        PresenceBasis::CommandLine => " [command line]",
    }
}

fn finish_output(mut output: String) -> String {
    while output.ends_with('\n') {
        output.pop();
    }
    output.push('\n');
    output
}
