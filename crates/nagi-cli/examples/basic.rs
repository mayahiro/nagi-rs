//! Minimal Nagi CLI command application

use std::io;
use std::process::ExitCode;

use nagi_cli::{
    Argument, Command, Context, Diagnostic, DiagnosticCode, DiagnosticTarget, Invocation, Outcome,
    string_parser,
};

fn application() -> Command {
    Command::new("greet")
        .about("Print a greeting")
        .version("0.2.7")
        .usage_variant("named", "<NAME> [OPTIONS]")
        .argument(
            Argument::new("name")
                .parser(string_parser())
                .required()
                .help("Name to greet"),
        )
        .example("named greeting", "greet Nagi")
        .note("Help and diagnostics are written separately from command output")
        .link(
            "guide",
            "https://github.com/mayahiro/nagi/blob/main/docs/CLI_API.md",
        )
        .handler(|context: &mut Context, invocation: &Invocation| {
            let name = invocation
                .require_value::<String>("name")
                .map_err(|error| {
                    Diagnostic::new(DiagnosticCode::HandlerError, error.to_string())
                        .with_target(DiagnosticTarget::argument("name"))
                        .with_hint("check the command schema and parser result type")
                })?;
            writeln!(context.stdout(), "Hello, {name}!")
                .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
            Ok(Outcome::success())
        })
}

fn main() -> io::Result<ExitCode> {
    application().run_process().map(Into::into)
}
