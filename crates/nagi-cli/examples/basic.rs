//! Minimal Nagi CLI command application

use std::ffi::OsStr;
use std::io;
use std::process::ExitCode;

use nagi_cli::{Argument, Command, Context, Diagnostic, DiagnosticCode, Invocation, Outcome};

fn application() -> Command {
    Command::new("greet")
        .about("Print a greeting")
        .version("0.2.0")
        .argument(Argument::new("name").required().help("Name to greet"))
        .handler(|context: &mut Context, invocation: &Invocation| {
            let name = invocation
                .raw_value("name")
                .and_then(OsStr::to_str)
                .ok_or_else(|| {
                    Diagnostic::new(DiagnosticCode::HandlerError, "name is not valid UTF-8")
                })?;
            writeln!(context.stdout(), "Hello, {name}!")
                .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
            Ok(Outcome::success())
        })
}

fn main() -> io::Result<ExitCode> {
    application().run_process().map(Into::into)
}
