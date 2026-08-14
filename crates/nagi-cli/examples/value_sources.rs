//! Adapts application-owned configuration into Value Option fallbacks

use std::io;
use std::process::ExitCode;

use nagi_cli::{
    Command, Context, Diagnostic, DiagnosticCode, Invocation, OptionSpec, Outcome, ParsedValue,
    ValueResolution, ValueResolutionRequest, ValueSource,
};

fn application() -> Command {
    Command::new("value-sources")
        .about("Demonstrate application-owned value fallback resolution")
        .option(
            OptionSpec::value("profile")
                .long("profile")
                .default_value("local")
                .help("Execution profile"),
        )
        .option(
            OptionSpec::value("tag")
                .long("tag")
                .repeated()
                .default_value("baseline")
                .help("Ordered execution tag"),
        )
        .handler(run)
}

fn project_config(request: &ValueResolutionRequest<'_>) -> Result<ValueResolution, Diagnostic> {
    Ok(match request.value_id() {
        "profile" => ValueResolution::replace("project-config", ["workspace"]),
        "tag" => ValueResolution::merge("project-config", ["configured"]),
        _ => ValueResolution::unresolved(),
    })
}

fn run(context: &mut Context, invocation: &Invocation) -> Result<Outcome, Diagnostic> {
    let profile = &invocation
        .parsed_values("profile")
        .expect("profile fallback")[0];
    let tags = invocation
        .parsed_values("tag")
        .expect("tag fallback")
        .iter()
        .map(describe)
        .collect::<Vec<_>>()
        .join(",");
    writeln!(context.stdout(), "profile={}", describe(profile))
        .and_then(|_| writeln!(context.stdout(), "tags={tags}"))
        .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
    Ok(Outcome::success())
}

fn describe(value: &ParsedValue) -> String {
    let source = match value.source() {
        ValueSource::CommandLine => "command-line".to_owned(),
        ValueSource::Environment => format!(
            "environment({})",
            value.origin().identity().expect("environment identity")
        ),
        ValueSource::Default => "default".to_owned(),
        ValueSource::External => format!(
            "external({})",
            value.origin().identity().expect("external identity")
        ),
    };
    format!("{}:{source}", value.raw().to_string_lossy())
}

fn main() -> io::Result<ExitCode> {
    application()
        .run_process_with_value_resolver(project_config)
        .map(Into::into)
}
