//! Process-free test support for Nagi CLI applications

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Cursor, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use nagi_cli::{Command, Context, ExitStatus, cancellation_pair};

/// A configurable process-free command application driver
pub struct TestDriver {
    command: Command,
    arguments: Vec<OsString>,
    stdin: Vec<u8>,
    environment: BTreeMap<OsString, OsString>,
    current_directory: PathBuf,
    cancelled: bool,
}

impl TestDriver {
    /// Constructs a driver with empty input and `/` as its current directory
    pub fn new(command: Command) -> Self {
        Self {
            command,
            arguments: Vec::new(),
            stdin: Vec::new(),
            environment: BTreeMap::new(),
            current_directory: PathBuf::from("/"),
            cancelled: false,
        }
    }

    /// Sets arguments after the program name
    pub fn arguments<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.arguments = arguments.into_iter().map(Into::into).collect();
        self
    }

    /// Sets standard input bytes
    pub fn stdin(mut self, input: impl Into<Vec<u8>>) -> Self {
        self.stdin = input.into();
        self
    }

    /// Adds one environment value
    pub fn environment(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.environment.insert(name.into(), value.into());
        self
    }

    /// Sets the injected current directory
    pub fn current_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.current_directory = path.into();
        self
    }

    /// Requests cancellation before the handler runs
    pub fn cancelled(mut self, cancelled: bool) -> Self {
        self.cancelled = cancelled;
        self
    }

    /// Runs the application without a child process or signal handler
    pub fn run(self) -> io::Result<TestResult> {
        let stdout = SharedWriter::default();
        let stderr = SharedWriter::default();
        let stdout_capture = stdout.clone();
        let stderr_capture = stderr.clone();
        let (token, handle) = cancellation_pair();
        if self.cancelled {
            handle.cancel();
        }
        let mut context = Context::with_cancellation(
            Cursor::new(self.stdin),
            stdout,
            stderr,
            self.environment,
            self.current_directory,
            token,
        );
        let outcome = self.command.run(&mut context, self.arguments)?;
        Ok(TestResult {
            status: outcome.status(),
            stdout: stdout_capture.bytes(),
            stderr: stderr_capture.bytes(),
        })
    }
}

/// Captured results from one Test Driver execution
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestResult {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl TestResult {
    /// Returns the explicit Exit Status
    pub fn status(&self) -> ExitStatus {
        self.status
    }

    /// Returns captured standard output bytes
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// Returns captured standard error bytes
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }
}

#[derive(Clone, Default)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl SharedWriter {
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().expect("capture lock was poisoned").clone()
    }
}

impl Write for SharedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("capture lock was poisoned"))?
            .extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
