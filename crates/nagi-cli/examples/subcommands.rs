//! Nested Nagi CLI command application

use std::io;
use std::process::ExitCode;

use nagi_cli::{
    Command, Context, Diagnostic, DiagnosticCategory, DiagnosticCode, DiagnosticTarget, Invocation,
    OptionSpec, Outcome, SubcommandUsageMode, string_parser,
};

fn application() -> Command {
    Command::new("service")
        .id("service-root")
        .about("Manage a service")
        .version("0.2.7")
        .option(
            OptionSpec::value("profile")
                .long("profile")
                .parser(string_parser())
                .default_value("default")
                .help("Root configuration profile"),
        )
        .require_subcommand()
        .subcommand_usage(SubcommandUsageMode::Expanded)
        .subcommand(
            Command::new("start")
                .id("start-command")
                .about("Start the service")
                .usage_variant("configured", "[OPTIONS]")
                .option(
                    OptionSpec::value("profile")
                        .long("profile")
                        .parser(string_parser())
                        .default_value("service")
                        .help("Service profile"),
                )
                .option(
                    OptionSpec::count("verbose")
                        .long("verbose")
                        .short('v')
                        .help("Increase verbosity"),
                )
                .validator(|invocation: &Invocation| {
                    let profile =
                        invocation
                            .require_value::<String>("profile")
                            .map_err(|error| {
                                Diagnostic::new(DiagnosticCode::Validation, error.to_string())
                                    .with_target(DiagnosticTarget::option("profile"))
                            })?;
                    if profile == "blocked" {
                        return Err(Diagnostic::new(
                            DiagnosticCode::application("reserved-profile"),
                            "profile 'blocked' cannot be started",
                        )
                        .with_category(DiagnosticCategory::Usage)
                        .with_target(DiagnosticTarget::option("profile"))
                        .with_hint("choose another service profile"));
                    }
                    Ok(())
                })
                .handler(|context: &mut Context, invocation: &Invocation| {
                    let profile =
                        invocation
                            .require_value::<String>("profile")
                            .map_err(|error| {
                                Diagnostic::new(DiagnosticCode::HandlerError, error.to_string())
                                    .with_target(DiagnosticTarget::option("profile"))
                            })?;
                    let root = invocation.scope(["service-root"]).ok_or_else(|| {
                        Diagnostic::new(
                            DiagnosticCode::HandlerError,
                            "root command scope is unavailable",
                        )
                    })?;
                    let root_profile =
                        root.require_value::<String>("profile").map_err(|error| {
                            Diagnostic::new(DiagnosticCode::HandlerError, error.to_string())
                                .with_target(
                                    DiagnosticTarget::option("profile")
                                        .with_command_id_path(vec!["service-root".to_owned()]),
                                )
                        })?;
                    writeln!(
                        context.stdout(),
                        "starting profile {profile} from root {root_profile} with verbosity {}",
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
