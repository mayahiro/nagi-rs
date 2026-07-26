//! Staged Nagi CLI adoption around an existing command handler

use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use nagi_cli::{
    Argument, Command, Context, Diagnostic, DiagnosticCategory, DiagnosticCode, DiagnosticTarget,
    ExitCodePolicy, ExitStatus, Invocation, Outcome, ParseResult, RuntimePolicy, string_parser,
};

fn application() -> Command {
    Command::new("tool")
        .id("tool-root")
        .about("Inspect one target")
        .version("0.2.7")
        .require_subcommand()
        .subcommand(
            Command::new("inspect")
                .id("inspect-command")
                .argument(
                    Argument::new("target")
                        .parser(string_parser())
                        .required()
                        .help("Target to inspect"),
                )
                .validator(|invocation: &Invocation| {
                    if invocation.value::<String>("target").map(String::as_str) == Some("blocked") {
                        return Err(Diagnostic::new(
                            DiagnosticCode::application("target-blocked"),
                            "target 'blocked' cannot be inspected",
                        )
                        .with_category(DiagnosticCategory::Usage)
                        .with_target(DiagnosticTarget::argument("target"))
                        .with_hint("choose another target"));
                    }
                    Ok(())
                })
                .handler(legacy_inspect),
        )
}

fn legacy_inspect(context: &mut Context, invocation: &Invocation) -> Result<Outcome, Diagnostic> {
    let target = invocation
        .require_value::<String>("target")
        .map_err(|error| {
            Diagnostic::new(DiagnosticCode::HandlerError, error.to_string())
                .with_target(DiagnosticTarget::argument("target"))
        })?;
    writeln!(context.stdout(), "legacy inspect: {target}")
        .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
    Ok(Outcome::success())
}

fn execute() -> io::Result<ExitStatus> {
    let command = application();
    let policy = RuntimePolicy::default().with_exit_code_policy(
        ExitCodePolicy::default().with_status(DiagnosticCategory::Usage, ExitStatus::FAILURE),
    );
    let result = match command.parse(env::args_os().skip(1)) {
        Ok(result) => result,
        Err(diagnostic) => {
            io::stderr().write_all(policy.render_diagnostic(&diagnostic).as_bytes())?;
            return Ok(policy.status_for_diagnostic(&diagnostic));
        }
    };
    let mut context = Context::new(
        io::stdin(),
        io::stdout(),
        io::stderr(),
        env::vars_os(),
        env::current_dir()?,
    );
    let outcome = match result {
        ParseResult::Invocation(invocation) => {
            if invocation.command_id_path() != ["tool-root", "inspect-command"] {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "no existing handler adapter for the selected command",
                ));
            }
            command.run_invocation_with_policy(&mut context, &invocation, &policy)?
        }
        action => command.run_parsed_with_policy(&mut context, action, &policy)?,
    };
    Ok(outcome.status())
}

fn main() -> io::Result<ExitCode> {
    execute().map(Into::into)
}
