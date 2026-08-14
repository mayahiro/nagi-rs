//! Process-free CLI Test driver behavior

use nagi_cli::{
    Argument, Command, Context, Diagnostic, HelpDocument, HelpRenderer, Invocation, Outcome,
    RuntimePolicy, ValueResolution, ValueResolutionRequest, ValueSource,
};
use nagi_cli_test::TestDriver;
use std::ffi::OsStr;

#[test]
fn driver_injects_and_captures_process_services() {
    let command = Command::new("sample")
        .argument(Argument::new("value").required())
        .handler(|context: &mut Context, invocation: &Invocation| {
            let current_directory = context.current_directory().display().to_string();
            let value = invocation
                .raw_value("value")
                .and_then(OsStr::to_str)
                .unwrap()
                .to_owned();
            writeln!(context.stdout(), "{}:{}", current_directory, value).map_err(|error| {
                Diagnostic::new(nagi_cli::DiagnosticCode::IoError, error.to_string())
            })?;
            Ok(Outcome::success())
        });
    let result = TestDriver::new(command)
        .arguments(["value"])
        .current_directory("/work")
        .run()
        .unwrap();
    assert_eq!(result.status(), nagi_cli::ExitStatus::SUCCESS);
    assert_eq!(result.stdout(), b"/work:value\n");
    assert!(result.stderr().is_empty());
}

#[test]
fn driver_can_cancel_before_handler_execution() {
    let command = Command::new("sample")
        .handler(|_context: &mut Context, _invocation: &Invocation| Ok(Outcome::success()));
    let result = TestDriver::new(command).cancelled(true).run().unwrap();
    assert_eq!(result.status(), nagi_cli::ExitStatus::CANCELLED);
}

#[test]
fn driver_uses_runtime_policy() {
    let command = Command::new("sample").subcommand(Command::new("child"));
    let policy = RuntimePolicy::default().with_help_renderer(CommandPathRenderer);
    let result = TestDriver::new(command)
        .arguments(["help", "child"])
        .policy(policy)
        .run()
        .unwrap();
    assert_eq!(result.stdout(), b"custom help: sample/child\n");
}

#[test]
fn driver_injects_value_resolver() {
    let command = Command::new("sample")
        .option(nagi_cli::OptionSpec::value("profile").long("profile"))
        .handler(|_context: &mut Context, invocation: &Invocation| {
            let value = &invocation.parsed_values("profile").unwrap()[0];
            assert_eq!(value.raw(), "workspace");
            assert_eq!(value.source(), ValueSource::External);
            assert_eq!(value.origin().identity(), Some("test-config"));
            Ok(Outcome::success())
        });
    let result = TestDriver::new(command)
        .value_resolver(|_: &ValueResolutionRequest<'_>| {
            Ok(ValueResolution::replace("test-config", ["workspace"]))
        })
        .run()
        .unwrap();
    assert_eq!(result.status(), nagi_cli::ExitStatus::SUCCESS);
}

struct CommandPathRenderer;

impl HelpRenderer for CommandPathRenderer {
    fn render_help(&self, document: &HelpDocument) -> String {
        format!("custom help: {}\n", document.command_path().join("/"))
    }
}
