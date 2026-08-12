//! Generates shell scripts and handles their reserved completion protocol

use std::env;
use std::ffi::OsStr;
use std::io;
use std::process::ExitCode;

use nagi_cli::{
    CancellationToken, Command, CompletionCandidate, CompletionEngine, CompletionProviderError,
    CompletionRequest, Context, Diagnostic, DiagnosticCode, OptionSpec, Outcome, string_parser,
};
use nagi_cli_completion::{Shell, generate, handle};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let command = application_command();
    let engine = CompletionEngine::new(&command)?;
    let arguments: Vec<_> = env::args_os().skip(1).collect();

    if arguments
        .first()
        .is_some_and(|argument| argument == "generate")
    {
        let shell = arguments
            .get(1)
            .and_then(|argument| argument.to_str())
            .and_then(parse_shell)
            .ok_or("usage: completion generate <bash|zsh|fish|powershell>")?;
        print!("{}", generate(shell, &engine));
        return Ok(());
    }

    if handle(
        &CancellationToken::new(),
        &engine,
        arguments.iter().cloned(),
        &mut io::stdout(),
    )? {
        return Ok(());
    }

    let status = command.run_process()?;
    if status == nagi_cli::ExitStatus::SUCCESS {
        Ok(())
    } else {
        Err(format!("command exited with status {}", status.code()).into())
    }
}

fn application_command() -> Command {
    Command::new("completion")
        .option(
            OptionSpec::value("profile")
                .long("profile")
                .parser(string_parser())
                .completion_provider(profile_completion)
                .help("Execution profile"),
        )
        .handler(run_command)
        .subcommand(Command::new("inspect").handler(run_command))
}

fn profile_completion(
    cancellation: &CancellationToken,
    _request: &CompletionRequest,
) -> Result<Vec<CompletionCandidate>, CompletionProviderError> {
    if cancellation.is_cancelled() {
        return Err(CompletionProviderError::new("profile completion cancelled"));
    }
    Ok(vec![
        CompletionCandidate::new("dev"),
        CompletionCandidate::new("prod"),
        CompletionCandidate::new("preview").with_description("Preview profile"),
    ])
}

fn run_command(
    context: &mut Context,
    invocation: &nagi_cli::Invocation,
) -> Result<Outcome, Diagnostic> {
    let profile = invocation.raw_value("profile").unwrap_or(OsStr::new("dev"));
    writeln!(
        context.stdout(),
        "command={} profile={}",
        invocation.command_path().join("/"),
        profile.to_string_lossy()
    )
    .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
    Ok(Outcome::success())
}

fn parse_shell(value: &str) -> Option<Shell> {
    match value {
        "bash" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "fish" => Some(Shell::Fish),
        "powershell" => Some(Shell::PowerShell),
        _ => None,
    }
}
