//! Nested Nagi CLI command application

use std::io;
use std::process::ExitCode;

use nagi_cli::{Command, Context, Diagnostic, DiagnosticCode, Invocation, OptionSpec, Outcome};

fn application() -> Command {
    Command::new("service")
        .about("Manage a service")
        .version("0.2.0")
        .require_subcommand()
        .subcommand(
            Command::new("start")
                .about("Start the service")
                .option(
                    OptionSpec::count("verbose")
                        .long("verbose")
                        .short('v')
                        .help("Increase verbosity"),
                )
                .handler(|context: &mut Context, invocation: &Invocation| {
                    writeln!(
                        context.stdout(),
                        "starting with verbosity {}",
                        invocation.count("verbose").unwrap_or(0)
                    )
                    .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
                    Ok(Outcome::success())
                }),
        )
}

fn main() -> io::Result<ExitCode> {
    application().run_process().map(Into::into)
}
