//! Hides internal syntax and reports deprecated syntax through Runtime Policy

use std::io;
use std::process::ExitCode;

use nagi_cli::{
    Command, Context, Diagnostic, DiagnosticCode, Invocation, OptionSpec, Outcome,
    PlainDeprecationNoticeRenderer, RuntimePolicy,
};

fn application() -> Command {
    Command::new("lifecycle")
        .about("Demonstrate command and option lifecycle metadata")
        .option(
            OptionSpec::flag("legacy")
                .long("legacy")
                .deprecated("--verbose")
                .help("Use legacy output"),
        )
        .option(
            OptionSpec::flag("internal")
                .long("internal")
                .hidden()
                .help("Internal switch"),
        )
        .subcommand(
            Command::new("old")
                .about("Run the old entry point")
                .deprecated("lifecycle run")
                .handler(run),
        )
        .subcommand(
            Command::new("internal")
                .about("Run internal maintenance")
                .hidden()
                .handler(run),
        )
        .subcommand(
            Command::new("run")
                .about("Run the current entry point")
                .handler(run),
        )
}

fn run(context: &mut Context, invocation: &Invocation) -> Result<Outcome, Diagnostic> {
    writeln!(
        context.stdout(),
        "running {} with legacy={}",
        invocation.command_path().join(" "),
        invocation.flag("legacy").unwrap_or(false),
    )
    .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
    Ok(Outcome::success())
}

fn main() -> io::Result<ExitCode> {
    let policy =
        RuntimePolicy::default().with_deprecation_notice_renderer(PlainDeprecationNoticeRenderer);
    application()
        .run_process_with_policy(&policy)
        .map(Into::into)
}
