//! CLI behavior outside the canonical shared fixture graph

use std::ffi::OsStr;
use std::io::{Cursor, sink};

use nagi_cli::{
    Argument, Command, Context, DiagnosticCode, ExitStatus, Invocation, OptionSpec, Outcome,
    ParseResult, cancellation_pair,
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
    ];
    for command in invalid {
        assert_eq!(
            command.validate().unwrap_err().code(),
            DiagnosticCode::InvalidSpecification
        );
    }
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
