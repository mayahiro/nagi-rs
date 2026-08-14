//! Runs line-oriented Confirm, Select, Input, and Secret prompts

use std::process::ExitCode;

use nagi_cli::{Command, Context, Diagnostic, DiagnosticCode, Outcome};
use nagi_cli_prompt::{
    Confirm, Input, ProcessIo, PromptError, PromptErrorKind, Prompter, Secret, Select,
};

fn main() -> ExitCode {
    match Command::new("prompt").handler(run_prompt).run_process() {
        Ok(status) => ExitCode::from(status.code()),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run_prompt(
    context: &mut Context,
    _invocation: &nagi_cli::Invocation,
) -> Result<Outcome, Diagnostic> {
    let mut prompt_io = ProcessIo::default();
    let mut prompter = Prompter::new(&mut prompt_io);
    let profile = prompter
        .select(
            context.cancellation(),
            &Select::new("Profile", ["Local", "Production"]).with_default(0),
        )
        .map_err(prompt_diagnostic)?;
    let name = prompter
        .input(
            context.cancellation(),
            &Input::new("Display name").required(),
        )
        .map_err(prompt_diagnostic)?;
    let token = prompter
        .secret(
            context.cancellation(),
            &Secret::new("Access token").required(),
        )
        .map_err(prompt_diagnostic)?;
    let accepted = prompter
        .confirm(
            context.cancellation(),
            &Confirm::new("Save this profile?").with_default(true),
        )
        .map_err(prompt_diagnostic)?;

    writeln!(
        context.stdout(),
        "profile={} name={name} token_bytes={} accepted={accepted}",
        ["local", "production"][profile],
        token.len()
    )
    .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
    Ok(Outcome::success())
}

fn prompt_diagnostic(error: PromptError) -> Diagnostic {
    let code = match error.kind() {
        PromptErrorKind::Cancelled => DiagnosticCode::Cancelled,
        PromptErrorKind::Io => DiagnosticCode::IoError,
        PromptErrorKind::NotTerminal
        | PromptErrorKind::InvalidRequest
        | PromptErrorKind::InputTooLong => DiagnosticCode::HandlerError,
    };
    Diagnostic::new(code, error.to_string())
}
