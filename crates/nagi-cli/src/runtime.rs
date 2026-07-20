use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::command::Command;
use crate::diagnostic::{Diagnostic, DiagnosticCode, ExitStatus};
use crate::parser::{Invocation, ParseResult};
use crate::signal_unix::SignalGuard;

/// A cooperative cancellation source passed to handlers
#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    process_interrupt: bool,
}

impl CancellationToken {
    /// Constructs a token that is not cancelled
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            process_interrupt: false,
        }
    }

    /// Reports whether manual or process cancellation was requested
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
            || (self.process_interrupt && crate::signal_unix::interrupted())
    }

    pub(crate) fn process() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            process_interrupt: true,
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// A handle that requests cancellation of its paired token
#[derive(Clone, Debug)]
pub struct CancellationHandle {
    cancelled: Arc<AtomicBool>,
}

impl CancellationHandle {
    /// Requests cooperative cancellation
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Constructs a manually controlled cancellation token and handle
pub fn cancellation_pair() -> (CancellationToken, CancellationHandle) {
    let token = CancellationToken::new();
    let handle = CancellationHandle {
        cancelled: Arc::clone(&token.cancelled),
    };
    (token, handle)
}

/// Injected process services available to a command handler
pub struct Context {
    stdin: Box<dyn Read>,
    stdout: Box<dyn Write>,
    stderr: Box<dyn Write>,
    environment: BTreeMap<OsString, OsString>,
    current_directory: PathBuf,
    cancellation: CancellationToken,
}

impl Context {
    /// Constructs an injected context with a non-cancelled token
    pub fn new<R, O, E, I, K, V>(
        stdin: R,
        stdout: O,
        stderr: E,
        environment: I,
        current_directory: impl Into<PathBuf>,
    ) -> Self
    where
        R: Read + 'static,
        O: Write + 'static,
        E: Write + 'static,
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        Self::with_cancellation(
            stdin,
            stdout,
            stderr,
            environment,
            current_directory,
            CancellationToken::new(),
        )
    }

    /// Constructs an injected context with an explicit cancellation token
    pub fn with_cancellation<R, O, E, I, K, V>(
        stdin: R,
        stdout: O,
        stderr: E,
        environment: I,
        current_directory: impl Into<PathBuf>,
        cancellation: CancellationToken,
    ) -> Self
    where
        R: Read + 'static,
        O: Write + 'static,
        E: Write + 'static,
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        Self {
            stdin: Box::new(stdin),
            stdout: Box::new(stdout),
            stderr: Box::new(stderr),
            environment: environment
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
            current_directory: current_directory.into(),
            cancellation,
        }
    }

    /// Returns mutable standard input access
    pub fn stdin(&mut self) -> &mut dyn Read {
        &mut *self.stdin
    }

    /// Returns mutable standard output access
    pub fn stdout(&mut self) -> &mut dyn Write {
        &mut *self.stdout
    }

    /// Returns mutable standard error access
    pub fn stderr(&mut self) -> &mut dyn Write {
        &mut *self.stderr
    }

    /// Returns one injected environment value
    pub fn environment(&self, name: &OsStr) -> Option<&OsStr> {
        self.environment.get(name).map(OsString::as_os_str)
    }

    /// Iterates over the injected environment in bytewise platform order
    pub fn environment_values(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.environment
            .iter()
            .map(|(key, value)| (key.as_os_str(), value.as_os_str()))
    }

    /// Returns the injected current directory
    pub fn current_directory(&self) -> &Path {
        &self.current_directory
    }

    /// Returns the cooperative cancellation token
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }
}

/// The result of one command handler
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Outcome {
    status: ExitStatus,
}

impl Outcome {
    /// Constructs a successful outcome
    pub const fn success() -> Self {
        Self {
            status: ExitStatus::SUCCESS,
        }
    }

    /// Constructs an outcome with an explicit status
    pub const fn new(status: ExitStatus) -> Self {
        Self { status }
    }

    /// Returns the process status
    pub const fn status(self) -> ExitStatus {
        self.status
    }
}

/// A language-native command handler
pub trait Handler: Send + Sync {
    /// Executes one validated invocation
    fn handle(&self, context: &mut Context, invocation: &Invocation)
    -> Result<Outcome, Diagnostic>;
}

impl<F> Handler for F
where
    F: Fn(&mut Context, &Invocation) -> Result<Outcome, Diagnostic> + Send + Sync,
{
    fn handle(
        &self,
        context: &mut Context,
        invocation: &Invocation,
    ) -> Result<Outcome, Diagnostic> {
        self(context, invocation)
    }
}

impl Command {
    /// Parses and executes arguments through an injected Context
    pub fn run<I, S>(&self, context: &mut Context, arguments: I) -> io::Result<Outcome>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let environment: Vec<(OsString, OsString)> = context
            .environment_values()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();
        match self.parse_with_environment(arguments, environment) {
            Ok(ParseResult::Help { command_path }) => {
                let help = self.render_help(&command_path).map_err(diagnostic_io)?;
                context.stdout().write_all(help.as_bytes())?;
                Ok(Outcome::success())
            }
            Ok(ParseResult::Version { version }) => {
                writeln!(context.stdout(), "{} {version}", self.name)?;
                Ok(Outcome::success())
            }
            Ok(ParseResult::Invocation(invocation)) => {
                if context.cancellation().is_cancelled() {
                    return Ok(Outcome::new(ExitStatus::CANCELLED));
                }
                let command = self
                    .command_at_path(invocation.command_path())
                    .expect("validated invocation paths identify commands");
                let Some(handler) = &command.handler else {
                    let diagnostic = Diagnostic::new(
                        DiagnosticCode::MissingHandler,
                        format!("command '{}' has no handler", command.name),
                    )
                    .with_command_path(invocation.command_path().to_vec());
                    context.stderr().write_all(diagnostic.render().as_bytes())?;
                    return Ok(Outcome::new(diagnostic.status()));
                };
                match handler.handle(context, &invocation) {
                    Ok(outcome)
                        if context.cancellation().is_cancelled()
                            && outcome.status() == ExitStatus::SUCCESS =>
                    {
                        Ok(Outcome::new(ExitStatus::CANCELLED))
                    }
                    Ok(outcome) => Ok(outcome),
                    Err(diagnostic) => {
                        context.stderr().write_all(diagnostic.render().as_bytes())?;
                        Ok(Outcome::new(diagnostic.status()))
                    }
                }
            }
            Err(diagnostic) => {
                context.stderr().write_all(diagnostic.render().as_bytes())?;
                Ok(Outcome::new(diagnostic.status()))
            }
        }
    }

    /// Executes this command against the current Unix process
    ///
    /// The helper installs a temporary SIGINT handler and returns an Exit
    /// Status instead of terminating the process
    pub fn run_process(&self) -> io::Result<ExitStatus> {
        let _signal_guard = SignalGuard::install()?;
        let current_directory = env::current_dir()?;
        let environment: Vec<(OsString, OsString)> = env::vars_os().collect();
        let arguments: Vec<OsString> = env::args_os().skip(1).collect();
        let mut context = Context::with_cancellation(
            io::stdin(),
            io::stdout(),
            io::stderr(),
            environment,
            current_directory,
            CancellationToken::process(),
        );
        self.run(&mut context, arguments).map(Outcome::status)
    }
}

fn diagnostic_io(diagnostic: Diagnostic) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, diagnostic)
}
