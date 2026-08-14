//! Derived Help document conformance

use std::env;
use std::fs;
use std::path::PathBuf;

use nagi_cli::{Argument, Command, HelpSection, OptionGroup, OptionSpec};
use nagi_cli_document::{ManRenderer, MarkdownRenderer};

#[test]
fn renderers_match_shared_golden_files() {
    let document = fixture_document();
    for (name, actual) in [
        ("markdown.txt", MarkdownRenderer.render(&document)),
        ("man.txt", ManRenderer.render(&document)),
    ] {
        let expected = fs::read_to_string(fixture_root().join("cli/document").join(name))
            .unwrap_or_else(|error| panic!("failed to read {name}: {error}"));
        assert_eq!(actual, expected, "{name}");
    }
}

#[test]
fn generated_text_cannot_inject_markdown_or_roff_structure() {
    let document = fixture_document();
    let markdown = MarkdownRenderer.render(&document);
    assert!(markdown.contains(r"\.SH ATTACK"));
    assert!(!markdown.contains("\n.SH ATTACK\n"));
    assert_eq!(markdown.trim_end_matches('\n').len() + 1, markdown.len());

    let man = ManRenderer.render(&document);
    assert!(man.contains("\n\\&.SH ATTACK\n"));
    assert!(man.contains("\n\\&'quoted\n"));
    assert!(!man.contains("\n.SH ATTACK\n"));
    assert_eq!(man.trim_end_matches('\n').len() + 1, man.len());
}

fn fixture_document() -> nagi_cli::HelpDocument {
    fixture_command()
        .help_document(&["qed".to_owned(), "run".to_owned()])
        .expect("fixture command must be valid")
}

fn fixture_command() -> Command {
    Command::new("qed")
        .about("Workspace manager")
        .version("1.0.0")
        .option(
            OptionSpec::value("config")
                .long("config")
                .help("Configuration [path]")
                .default_value("top-secret")
                .sensitive()
                .inherited(),
        )
        .subcommand(
            Command::new("run")
                .about("Run *one* task\n.SH ATTACK")
                .deprecated("execute")
                .argument(
                    Argument::new("target")
                        .help("Target <name>")
                        .required()
                        .sensitive(),
                )
                .option(
                    OptionSpec::value("profile")
                        .long("profile")
                        .help("Profile `name`")
                        .default_value("prod")
                        .deprecated("context"),
                )
                .option(
                    OptionSpec::flag("force")
                        .long("force")
                        .help("Force - carefully")
                        .requires_supplied("profile"),
                )
                .option(OptionSpec::flag("json").long("json").help("JSON output"))
                .option(OptionSpec::flag("yaml").long("yaml").help("YAML output"))
                .option_group(OptionGroup::at_most_one("format", ["json", "yaml"]))
                .subcommand(Command::new("inspect").about("Inspect result"))
                .example("Run `once`", "qed run --profile dev target")
                .note("'quoted\n.danger\n    indented\nUse # carefully")
                .link(
                    "Guide [stable]",
                    "https://example.test/a path?q=(x)<unsafe>",
                )
                .help_section(
                    HelpSection::new("details", "More *details*")
                        .paragraph("Backslash \\ and - dash\n'quoted")
                        .entry(".macro", "Value | table"),
                ),
        )
        .subcommand(
            Command::new("secret")
                .hidden()
                .subcommand(Command::new("hidden-descendant")),
        )
}

fn fixture_root() -> PathBuf {
    env::var_os("NAGI_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../../fixtures"))
}
