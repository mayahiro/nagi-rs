//! CLI behavior outside the canonical shared fixture graph

use std::ffi::OsStr;
use std::io::{Cursor, sink};

use nagi_cli::{
    Argument, Command, Context, Diagnostic, DiagnosticCategory, DiagnosticCode, ExitCodePolicy,
    ExitStatus, HelpSection, Invocation, OptionGroup, OptionSpec, Outcome, ParseResult,
    RuntimePolicy, cancellation_pair,
};

#[test]
fn command_selection_errors_are_distinct() {
    let command = Command::new("root")
        .require_subcommand()
        .subcommand(Command::new("child"));
    assert_eq!(
        command.parse::<_, &str>([]).unwrap_err().code(),
        DiagnosticCode::MissingSubcommand
    );
    assert_eq!(
        command.parse(["other"]).unwrap_err().code(),
        DiagnosticCode::UnknownCommand
    );
}

#[test]
fn positionals_disable_later_subcommand_selection() {
    let command = Command::new("root")
        .argument(Argument::new("values").repeated())
        .subcommand(Command::new("child"));
    let ParseResult::Invocation(invocation) = command.parse(["value", "child"]).unwrap() else {
        panic!("expected invocation");
    };
    assert_eq!(invocation.command_path(), ["root"]);
    let values = invocation.parsed_values("values").unwrap();
    assert_eq!(values.len(), 2);
    assert_eq!(values[1].raw(), OsStr::new("child"));
}

#[test]
fn parent_options_are_not_recognized_after_child_selection() {
    let command = Command::new("root")
        .option(OptionSpec::flag("root-option").long("root-option"))
        .subcommand(Command::new("child"));
    assert_eq!(
        command
            .parse(["child", "--root-option"])
            .unwrap_err()
            .code(),
        DiagnosticCode::UnknownOption
    );
}

#[test]
fn graph_validation_rejects_path_and_sibling_collisions() {
    let invalid = [
        Command::new("root")
            .option(OptionSpec::flag("same").long("root-option"))
            .subcommand(Command::new("child").argument(Argument::new("same"))),
        Command::new("root")
            .subcommand(Command::new("first").alias("shared"))
            .subcommand(Command::new("shared")),
        Command::new("root")
            .subcommand(Command::new("first").id("shared"))
            .subcommand(Command::new("second").id("shared")),
        Command::new("root")
            .argument(Argument::new("many").repeated())
            .argument(Argument::new("last")),
        Command::new("root")
            .option(OptionSpec::flag("parent").long("parent"))
            .subcommand(
                Command::new("child").option(
                    OptionSpec::flag("child-option")
                        .long("child-option")
                        .requires("parent"),
                ),
            ),
        Command::new("root")
            .option(OptionSpec::flag("known").long("known"))
            .option_group(OptionGroup::exactly_one("source", ["known", "missing"])),
        Command::new("root")
            .option(OptionSpec::flag("known").long("known"))
            .option_group(OptionGroup::at_most_one("source", ["known", "known"])),
        Command::new("root")
            .usage_variant("node", "<NODE>")
            .usage_variant("node", "<X> <Y>"),
        Command::new("root").usage_variant("node", "<NODE>\n"),
        Command::new("root")
            .require_subcommand()
            .usage_variant("node", "<NODE>")
            .subcommand(Command::new("child")),
        Command::new("root")
            .usage_variant("subcommand", "<NODE>")
            .subcommand(Command::new("child")),
    ];
    for command in invalid {
        assert_eq!(
            command.validate().unwrap_err().code(),
            DiagnosticCode::InvalidSpecification
        );
    }
}

#[test]
fn supplied_distinguishes_command_line_from_fallbacks() {
    let command = Command::new("root")
        .option(OptionSpec::value("mode").long("mode").default_value("auto"))
        .option(
            OptionSpec::flag("strict")
                .long("strict")
                .requires_supplied("mode"),
        )
        .option(
            OptionSpec::flag("all")
                .long("all")
                .conflicts_supplied("mode"),
        );

    let ParseResult::Invocation(invocation) = command.parse::<_, &str>([]).unwrap() else {
        panic!("expected invocation");
    };
    assert!(invocation.contains("mode"));
    assert!(!invocation.supplied("mode"));

    assert_eq!(
        command.parse(["--strict"]).unwrap_err().code(),
        DiagnosticCode::Requires
    );

    command.parse(["--all"]).unwrap();
    assert_eq!(
        command
            .parse(["--all", "--mode", "manual"])
            .unwrap_err()
            .code(),
        DiagnosticCode::Conflicts
    );

    let ParseResult::Invocation(invocation) =
        command.parse(["--strict", "--mode", "manual"]).unwrap()
    else {
        panic!("expected invocation");
    };
    assert!(invocation.supplied("strict"));
    assert!(invocation.supplied("mode"));
}

#[test]
fn option_groups_use_command_line_presence_by_default() {
    let command = |group| {
        Command::new("root")
            .option(
                OptionSpec::value("session")
                    .long("session")
                    .default_value("default"),
            )
            .option(OptionSpec::flag("all").long("all"))
            .option_group(group)
    };
    command(OptionGroup::at_most_one("target", ["session", "all"]))
        .parse(["--all"])
        .unwrap();
    assert_eq!(
        command(
            OptionGroup::at_most_one("target", ["session", "all"])
                .presence(nagi_cli::PresenceBasis::Resolved),
        )
        .parse(["--all"])
        .unwrap_err()
        .code(),
        DiagnosticCode::OptionGroup
    );
}

#[test]
fn option_group_kinds_are_enforced() {
    let cases = [
        (
            OptionGroup::at_most_one("group", ["a", "b"]),
            vec!["--a"],
            false,
        ),
        (
            OptionGroup::at_most_one("group", ["a", "b"]),
            vec!["--a", "--b"],
            true,
        ),
        (
            OptionGroup::exactly_one("group", ["a", "b"]),
            vec!["--b"],
            false,
        ),
        (OptionGroup::exactly_one("group", ["a", "b"]), vec![], true),
        (
            OptionGroup::at_least_one("group", ["a", "b"]),
            vec!["--a", "--b"],
            false,
        ),
        (OptionGroup::at_least_one("group", ["a", "b"]), vec![], true),
        (OptionGroup::all_or_none("group", ["a", "b"]), vec![], false),
        (
            OptionGroup::all_or_none("group", ["a", "b"]),
            vec!["--a", "--b"],
            false,
        ),
        (
            OptionGroup::all_or_none("group", ["a", "b"]),
            vec!["--a"],
            true,
        ),
    ];
    for (group, arguments, should_fail) in cases {
        let command = Command::new("root")
            .option(OptionSpec::flag("a").long("a"))
            .option(OptionSpec::flag("b").long("b"))
            .option_group(group);
        let result = command.parse(arguments);
        if should_fail {
            assert_eq!(result.unwrap_err().code(), DiagnosticCode::OptionGroup);
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn help_document_exposes_structured_additions() {
    let command = Command::new("root")
        .usage_variant("node", "<NODE> [OPTIONS]")
        .usage_variant("coordinates", "<X> <Y> [OPTIONS]")
        .option(OptionSpec::flag("a").long("a").conflicts("b"))
        .option(OptionSpec::flag("b").long("b"))
        .option_group(OptionGroup::at_most_one("selection", ["a", "b"]))
        .example("basic", "root --a")
        .note("Choose one source")
        .link("guide", "https://example.com/guide")
        .help_section(HelpSection::new("details", "Details").paragraph("Additional text"))
        .subcommand(Command::new("child"));
    let document = command.help_document(&["root".to_owned()]).unwrap();
    assert_eq!(document.examples().len(), 1);
    assert_eq!(document.notes().len(), 1);
    assert_eq!(document.links().len(), 1);
    assert_eq!(document.sections().len(), 1);
    assert_eq!(document.option_relations().len(), 1);
    assert_eq!(document.option_groups().len(), 1);
    assert_eq!(document.usage_variants().len(), 3);
    assert_eq!(document.usage_variants()[0].id(), "node");
    assert_eq!(document.usage_variants()[0].syntax(), "<NODE> [OPTIONS]");
    assert_eq!(
        document.usage_variants()[0].command_line(),
        "root <NODE> [OPTIONS]"
    );
    assert_eq!(document.usage()[1], "root <X> <Y> [OPTIONS]");
    assert_eq!(document.usage_variants()[2].id(), "subcommand");
    assert_eq!(
        document.usage_variants()[2].command_line(),
        "root [OPTIONS] <COMMAND>"
    );
    assert_eq!(document.options()[0].id(), "a");
    assert_eq!(document.option_groups()[0].option_ids()[0], "a");
    assert_eq!(document.option_groups()[0].option_labels()[0], "--a");
    assert_eq!(document.option_relations()[0].source_id(), "a");
    assert_eq!(document.option_relations()[0].target_id(), "b");
    assert_eq!(
        document.option_relations()[0].kind(),
        nagi_cli::HelpOptionRelationKind::Conflicts
    );
}

#[test]
fn usage_variants_remain_help_only() {
    let generated = Command::new("root")
        .help_document(&["root".to_owned()])
        .unwrap();
    assert_eq!(generated.usage_variants().len(), 1);
    assert_eq!(generated.usage_variants()[0].id(), "default");
    assert_eq!(generated.usage_variants()[0].syntax(), "[OPTIONS]");

    let command = Command::new("root")
        .usage_variant("node", "<NODE>")
        .argument(Argument::new("value").required());
    let document = command.help_document(&["root".to_owned()]).unwrap();
    assert_eq!(document.usage()[0], "root <NODE>");
    assert_eq!(
        command.parse::<_, &str>([]).unwrap_err().usage(),
        Some("root [OPTIONS] <VALUE>")
    );
}

#[test]
fn validator_returns_a_usage_diagnostic() {
    let command = Command::new("root").validator(|_invocation: &Invocation| {
        Err(Diagnostic::new(DiagnosticCode::Validation, "rejected"))
    });
    let diagnostic = command.parse::<_, &str>([]).unwrap_err();
    assert_eq!(diagnostic.code(), DiagnosticCode::Validation);
    assert_eq!(diagnostic.category(), DiagnosticCategory::Usage);
}

#[test]
fn runtime_reports_a_missing_handler() {
    let mut context = Context::new(
        Cursor::new(Vec::<u8>::new()),
        sink(),
        sink(),
        std::iter::empty::<(&str, &str)>(),
        "/",
    );
    let outcome = Command::new("root")
        .run(&mut context, std::iter::empty::<&str>())
        .unwrap();
    assert_eq!(outcome.status(), ExitStatus::FAILURE);
}

#[test]
fn cancellation_after_handler_overrides_only_success() {
    assert_eq!(
        run_with_cancellation(ExitStatus::SUCCESS),
        ExitStatus::CANCELLED
    );
    assert_eq!(
        run_with_cancellation(ExitStatus::new(7)),
        ExitStatus::new(7)
    );
}

#[test]
fn runtime_policy_maps_cancellation() {
    let (token, handle) = cancellation_pair();
    handle.cancel();
    let command = Command::new("root").handler(
        |_context: &mut Context, _invocation: &Invocation| -> Result<Outcome, Diagnostic> {
            panic!("handler ran after cancellation")
        },
    );
    let policy = RuntimePolicy::default().with_exit_code_policy(
        ExitCodePolicy::default()
            .with_status(DiagnosticCategory::Cancellation, ExitStatus::new(75)),
    );
    let mut context = Context::with_cancellation(
        Cursor::new(Vec::<u8>::new()),
        sink(),
        sink(),
        std::iter::empty::<(&str, &str)>(),
        "/",
        token,
    );
    let outcome = command
        .run_with_policy(&mut context, std::iter::empty::<&str>(), &policy)
        .unwrap();
    assert_eq!(outcome.status(), ExitStatus::new(75));
}

fn run_with_cancellation(handler_status: ExitStatus) -> ExitStatus {
    let (token, handle) = cancellation_pair();
    let command =
        Command::new("root").handler(move |_context: &mut Context, _invocation: &Invocation| {
            handle.cancel();
            Ok(Outcome::new(handler_status))
        });
    let mut context = Context::with_cancellation(
        Cursor::new(Vec::<u8>::new()),
        sink(),
        sink(),
        std::iter::empty::<(&str, &str)>(),
        "/",
        token,
    );
    command
        .run(&mut context, std::iter::empty::<&str>())
        .unwrap()
        .status()
}
