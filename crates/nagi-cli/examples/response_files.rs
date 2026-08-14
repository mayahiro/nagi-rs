//! Expands opt-in Response Files before ordinary command parsing

use std::io;
use std::process::ExitCode;

use nagi_cli::{
    Command, Context, Diagnostic, DiagnosticCode, Invocation, OptionSpec, Outcome, ProcessOptions,
    ResponseFileOptions,
};

fn application() -> Command {
    Command::new("response-files")
        .about("Demonstrate bounded Response File expansion")
        .option(
            OptionSpec::value("profile")
                .long("profile")
                .required()
                .help("Execution profile"),
        )
        .option(
            OptionSpec::value("tag")
                .long("tag")
                .repeated()
                .help("Ordered execution tag"),
        )
        .handler(run)
}

fn run(context: &mut Context, invocation: &Invocation) -> Result<Outcome, Diagnostic> {
    let profile = invocation
        .raw_value("profile")
        .expect("required profile")
        .to_string_lossy();
    let tags = invocation
        .parsed_values("tag")
        .unwrap_or_default()
        .iter()
        .map(|value| value.raw().to_string_lossy())
        .collect::<Vec<_>>()
        .join("|");
    writeln!(context.stdout(), "profile={profile}")
        .and_then(|_| writeln!(context.stdout(), "tags={tags}"))
        .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
    Ok(Outcome::success())
}

fn main() -> io::Result<ExitCode> {
    let options = ProcessOptions::default().with_response_files(ResponseFileOptions::default());
    application()
        .run_process_with_options(&options)
        .map(Into::into)
}
