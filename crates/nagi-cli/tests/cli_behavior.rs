//! CLI behavior outside the canonical shared fixture graph

use std::ffi::{OsStr, OsString};
use std::io::{Cursor, sink};

use nagi_cli::{
    Argument, Command, Context, Diagnostic, DiagnosticCategory, DiagnosticCode, ExitCodePolicy,
    ExitStatus, HelpSection, Invocation, OptionGroup, OptionSpec, Outcome, ParseResult,
    RuntimePolicy, SubcommandUsageMode, ValueAccessErrorKind, cancellation_pair,
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
fn command_local_value_scopes_are_explicit_and_shadow_ancestors() {
    let command = Command::new("root")
        .id("root-id")
        .option(
            OptionSpec::value("session")
                .long("session")
                .default_value("root"),
        )
        .validator(|invocation: &Invocation| {
            assert_eq!(invocation.raw_value("session"), Some(OsStr::new("parent")));
            Ok(())
        })
        .subcommand(
            Command::new("run")
                .id("run-id")
                .option(OptionSpec::value("session").long("session"))
                .validator(|invocation: &Invocation| {
                    if invocation.contains("session") {
                        assert_eq!(invocation.raw_value("session"), Some(OsStr::new("child")));
                    }
                    Ok(())
                }),
        );
    let ParseResult::Invocation(invocation) = command
        .parse(["--session", "parent", "run", "--session", "child"])
        .unwrap()
    else {
        panic!("expected invocation");
    };
    assert_eq!(invocation.raw_value("session"), Some(OsStr::new("child")));
    let root = invocation.scope(["root-id"]).unwrap();
    assert_eq!(root.raw_value("session"), Some(OsStr::new("parent")));
    let child = invocation.scope(["root-id", "run-id"]).unwrap();
    assert_eq!(child.command_path(), ["root", "run"]);
    assert_eq!(
        child.require_value::<OsString>("session").unwrap(),
        OsStr::new("child")
    );
    assert_eq!(
        child.require_value::<i64>("session").unwrap_err().kind(),
        ValueAccessErrorKind::TypeMismatch
    );

    let ParseResult::Invocation(invocation) =
        command.parse(["--session", "parent", "run"]).unwrap()
    else {
        panic!("expected invocation");
    };
    assert_eq!(invocation.raw_value("session"), None);
    assert_eq!(invocation.scopes().count(), 2);
}

#[test]
fn graph_validation_rejects_local_and_sibling_collisions() {
    let invalid = [
        Command::new("root")
            .option(OptionSpec::flag("same").long("root-option"))
            .argument(Argument::new("same")),
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
fn subcommand_usage_presentation_is_configurable() {
    let child = || {
        Command::new("validate")
            .id("validate-id")
            .usage_variant("file", "--file <FILE>")
            .usage_variant("stdin", "--stdin")
    };
    let hidden = Command::new("root")
        .usage_variant("direct", "<OLD> <NEW>")
        .subcommand_usage(SubcommandUsageMode::Hidden)
        .subcommand(child());
    let document = hidden.help_document(&["root".to_owned()]).unwrap();
    assert_eq!(document.usage_variants().len(), 1);
    assert_eq!(document.usage_variants()[0].id(), "direct");

    let hidden_reserved_id = Command::new("root")
        .usage_variant("subcommand", "<VALUE>")
        .subcommand_usage(SubcommandUsageMode::Hidden)
        .subcommand(child());
    hidden_reserved_id.validate().unwrap();

    let expanded = Command::new("root")
        .id("root-id")
        .usage_variant("direct", "<OLD> <NEW>")
        .subcommand_usage(SubcommandUsageMode::Expanded)
        .subcommand(child());
    let document = expanded.help_document(&["root".to_owned()]).unwrap();
    assert_eq!(document.usage_variants().len(), 3);
    assert_eq!(
        document.usage_variants()[1].command_line(),
        "root validate --file <FILE>"
    );
    assert_eq!(
        document.usage_variants()[2].command_line(),
        "root validate --stdin"
    );
    assert_eq!(
        document.usage_variants()[1].command_id_path(),
        ["root-id", "validate-id"]
    );

    let required = Command::new("root")
        .require_subcommand()
        .subcommand_usage(SubcommandUsageMode::Expanded)
        .subcommand(child());
    let document = required.help_document(&["root".to_owned()]).unwrap();
    assert_eq!(document.usage_variants().len(), 2);
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
        Err(Diagnostic::new(
            DiagnosticCode::application("selection-required"),
            "rejected",
        )
        .with_category(DiagnosticCategory::Usage)
        .with_target(nagi_cli::DiagnosticTarget::option("selection"))
        .with_hint("choose one selection"))
    });
    let diagnostic = command.parse::<_, &str>([]).unwrap_err();
    assert_eq!(diagnostic.code().as_str(), "selection-required");
    assert_eq!(diagnostic.category(), DiagnosticCategory::Usage);
    assert_eq!(diagnostic.targets().len(), 1);
    assert_eq!(diagnostic.targets()[0].command_id_path(), ["root"]);
    assert_eq!(diagnostic.hints(), ["choose one selection"]);
    assert!(diagnostic.render().contains("hint: choose one selection\n"));
}

#[test]
fn parser_and_runtime_can_be_adopted_in_stages() {
    let command =
        Command::new("root").handler(|_context: &mut Context, _invocation: &Invocation| {
            Ok(Outcome::new(ExitStatus::new(7)))
        });
    let result = command.parse::<_, &str>([]).unwrap();
    assert_eq!(result.command_path(), ["root"]);
    assert_eq!(result.command_id_path(), ["root"]);
    let mut context = Context::new(
        Cursor::new(Vec::<u8>::new()),
        sink(),
        sink(),
        std::iter::empty::<(&str, &str)>(),
        "/",
    );
    assert_eq!(
        command.run_parsed(&mut context, result).unwrap().status(),
        ExitStatus::new(7)
    );

    let diagnostic = Diagnostic::new(
        DiagnosticCode::application("selection-required"),
        "rejected",
    )
    .with_category(DiagnosticCategory::Usage)
    .with_hint("choose one");
    let policy = RuntimePolicy::default().with_exit_code_policy(
        ExitCodePolicy::default().with_status(DiagnosticCategory::Usage, ExitStatus::FAILURE),
    );
    assert_eq!(
        policy.status_for_diagnostic(&diagnostic),
        ExitStatus::FAILURE
    );
    assert!(
        policy
            .render_diagnostic(&diagnostic)
            .contains("hint: choose one\n")
    );

    let foreign_result = Command::new("root")
        .id("foreign-root")
        .parse::<_, &str>([])
        .unwrap();
    assert_eq!(
        command
            .run_parsed(&mut context, foreign_result.clone())
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
    let ParseResult::Invocation(foreign) = foreign_result else {
        panic!("expected invocation");
    };
    assert_eq!(
        command
            .run_invocation(&mut context, &foreign)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
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
