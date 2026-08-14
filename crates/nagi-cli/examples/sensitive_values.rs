//! Redacts a generic Sensitive Value while preserving explicit handler access

use std::io;
use std::process::ExitCode;

use nagi_cli::{
    Command, Context, Diagnostic, DiagnosticCode, Invocation, OptionSpec, Outcome, ValueSource,
    string_parser,
};

fn application() -> Command {
    Command::new("sensitive-values")
        .about("Demonstrate generic Sensitive Value metadata")
        .option(
            OptionSpec::value("token")
                .long("token")
                .parser(string_parser())
                .environment("NAGI_TOKEN")
                .default_value("demo-token")
                .sensitive()
                .help("Authentication token"),
        )
        .handler(run)
}

fn run(context: &mut Context, invocation: &Invocation) -> Result<Outcome, Diagnostic> {
    let parsed = &invocation
        .parsed_values("token")
        .and_then(|values| values.first())
        .expect("the example declares a default token");
    let source = match parsed.source() {
        ValueSource::CommandLine => "command line",
        ValueSource::Environment => "environment",
        ValueSource::Default => "default",
        ValueSource::External => "external",
    };
    writeln!(
        context.stdout(),
        "received a {}-byte token from {source}; sensitive={}",
        parsed.raw().as_encoded_bytes().len(),
        parsed.is_sensitive(),
    )
    .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
    Ok(Outcome::success())
}

fn main() -> io::Result<ExitCode> {
    application().run_process().map(Into::into)
}
