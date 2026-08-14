//! Process-free CLI Test driver behavior

use nagi_cli::{
    Argument, Command, Context, Diagnostic, HelpDocument, HelpRenderer, Invocation, Outcome,
    ResponseFileOptions, ResponseFileReadRequest, RuntimePolicy, ValueResolution,
    ValueResolutionRequest, ValueSource,
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

#[test]
fn driver_injects_response_files() {
    let command = Command::new("sample")
        .option(nagi_cli::OptionSpec::value("profile").long("profile"))
        .option(nagi_cli::OptionSpec::value("theme").long("theme"))
        .handler(|context: &mut Context, invocation: &Invocation| {
            assert!(context.response_file_options().is_some());
            assert!(invocation.supplied("profile"));
            let value = &invocation.parsed_values("profile").unwrap()[0];
            assert_eq!(value.raw(), "workspace");
            assert_eq!(value.source(), ValueSource::CommandLine);
            let theme = &invocation.parsed_values("theme").unwrap()[0];
            assert_eq!(theme.raw(), "dark");
            assert_eq!(theme.source(), ValueSource::External);
            Ok(Outcome::success())
        });
    let result = TestDriver::new(command)
        .arguments(["@args.txt"])
        .current_directory("/work")
        .value_resolver(|request: &ValueResolutionRequest<'_>| {
            assert_eq!(request.value_id(), "theme");
            Ok(ValueResolution::replace("test-config", ["dark"]))
        })
        .response_files(
            ResponseFileOptions::default(),
            |request: &ResponseFileReadRequest<'_>| {
                assert_eq!(request.path(), std::path::Path::new("/work/args.txt"));
                Ok(b"--profile workspace".to_vec())
            },
        )
        .run()
        .unwrap();
    assert_eq!(result.status(), nagi_cli::ExitStatus::SUCCESS);
}

#[test]
fn response_file_values_keep_sensitive_redaction() {
    let command = Command::new("sample").option(
        nagi_cli::OptionSpec::value("token")
            .long("token")
            .parser(nagi_cli::possible_values_parser(["valid"]))
            .sensitive(),
    );
    let result = TestDriver::new(command)
        .arguments(["@args.txt"])
        .response_files(
            ResponseFileOptions::default(),
            |_: &ResponseFileReadRequest<'_>| Ok(b"--token response-secret".to_vec()),
        )
        .run()
        .unwrap();
    let stderr = String::from_utf8(result.stderr().to_vec()).unwrap();
    assert_eq!(result.status(), nagi_cli::ExitStatus::USAGE);
    assert!(stderr.contains(nagi_cli::REDACTED_VALUE));
    assert!(!stderr.contains("response-secret"));
}

#[test]
fn standard_input_response_file_consumes_the_context_stream_once() {
    let command = Command::new("sample")
        .option(nagi_cli::OptionSpec::value("profile").long("profile"))
        .handler(|context: &mut Context, invocation: &Invocation| {
            assert_eq!(
                invocation.raw_value("profile"),
                Some(OsStr::new("workspace"))
            );
            let mut remaining = Vec::new();
            context.stdin().read_to_end(&mut remaining).unwrap();
            assert!(remaining.is_empty());
            Ok(Outcome::success())
        });
    let result = TestDriver::new(command)
        .arguments(["@-"])
        .stdin(b"--profile workspace".to_vec())
        .response_files(
            ResponseFileOptions::default().with_standard_input(true),
            |_: &ResponseFileReadRequest<'_>| -> Result<Vec<u8>, Diagnostic> {
                panic!("filesystem reader ran for standard input")
            },
        )
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
