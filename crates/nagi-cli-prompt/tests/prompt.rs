//! Prompt request and injected I/O behavior

use std::io::{self, Write};

use nagi_cli::{CancellationToken, cancellation_pair};
use nagi_cli_prompt::{
    Confirm, InputMode, Limits, PromptErrorKind, PromptIo, Prompter, ReadResult, Secret, Select,
    TerminalPolicy,
};

#[test]
fn preexisting_cancellation_writes_nothing() {
    let (token, handle) = cancellation_pair();
    handle.cancel();
    let mut prompt_io = StubIo::line(b"yes", true);
    let error = Prompter::new(&mut prompt_io)
        .confirm(&token, &Confirm::new("Proceed?"))
        .expect_err("cancelled prompt must fail");
    assert_eq!(error.kind(), PromptErrorKind::Cancelled);
    assert!(prompt_io.output.is_empty());
    assert!(prompt_io.modes.is_empty());
}

#[test]
fn visible_prompts_can_explicitly_allow_non_terminal_io() {
    let mut prompt_io = StubIo::line(b"yes", false);
    let result = Prompter::new(&mut prompt_io)
        .with_terminal_policy(TerminalPolicy::AllowNonTerminal)
        .confirm(&CancellationToken::new(), &Confirm::new("Proceed?"))
        .expect("explicit non-terminal prompt must succeed");
    assert!(result);
}

#[test]
fn secret_always_rejects_non_terminal_io() {
    let mut prompt_io = StubIo::line(b"secret", false);
    let error = Prompter::new(&mut prompt_io)
        .with_terminal_policy(TerminalPolicy::AllowNonTerminal)
        .secret(&CancellationToken::new(), &Secret::new("Token"))
        .expect_err("Secret must reject non-terminal I/O");
    assert_eq!(error.kind(), PromptErrorKind::NotTerminal);
    assert!(prompt_io.output.is_empty());
}

#[test]
fn invalid_metadata_and_resource_limits_fail_before_output() {
    let requests = [Confirm::new(""), Confirm::new("unsafe\nmessage")];
    for request in requests {
        let mut prompt_io = StubIo::line(b"yes", true);
        let error = Prompter::new(&mut prompt_io)
            .confirm(&CancellationToken::new(), &request)
            .expect_err("invalid metadata must fail");
        assert_eq!(error.kind(), PromptErrorKind::InvalidRequest);
        assert!(prompt_io.output.is_empty());
    }

    let mut prompt_io = StubIo::line(b"yes", true);
    let error = Prompter::new(&mut prompt_io)
        .with_limits(Limits::default().with_max_input_bytes(0))
        .confirm(&CancellationToken::new(), &Confirm::new("Proceed?"))
        .expect_err("zero limit must fail");
    assert_eq!(error.kind(), PromptErrorKind::InvalidRequest);
}

#[test]
fn invalid_select_definition_fails_before_output() {
    for request in [
        Select::new("Profile", Vec::<String>::new()),
        Select::new("Profile", ["Local"]).with_default(1),
        Select::new("Profile", ["bad\nchoice"]),
    ] {
        let mut prompt_io = StubIo::line(b"1", true);
        let error = Prompter::new(&mut prompt_io)
            .select(&CancellationToken::new(), &request)
            .expect_err("invalid Select must fail");
        assert_eq!(error.kind(), PromptErrorKind::InvalidRequest);
        assert!(prompt_io.output.is_empty());
    }
}

#[test]
fn io_cannot_return_a_line_beyond_the_limit() {
    let mut prompt_io = StubIo::line(b"abcd", true);
    let error = Prompter::new(&mut prompt_io)
        .with_limits(Limits::default().with_max_input_bytes(3))
        .confirm(&CancellationToken::new(), &Confirm::new("Proceed?"))
        .expect_err("oversized injected line must fail");
    assert_eq!(error.kind(), PromptErrorKind::InputTooLong);
}

#[test]
fn secret_read_failure_emits_line_ending_and_preserves_source() {
    let mut prompt_io = FailingSecretIo::default();
    let error = Prompter::new(&mut prompt_io)
        .secret(&CancellationToken::new(), &Secret::new("Token"))
        .expect_err("read failure must fail the prompt");
    assert_eq!(error.kind(), PromptErrorKind::Io);
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(prompt_io.output, b"Token: \n");
}

struct StubIo {
    line: Vec<u8>,
    output: Vec<u8>,
    terminal: bool,
    modes: Vec<InputMode>,
}

impl StubIo {
    fn line(line: &[u8], terminal: bool) -> Self {
        Self {
            line: line.to_vec(),
            output: Vec::new(),
            terminal,
            modes: Vec::new(),
        }
    }
}

impl Write for StubIo {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(value);
        Ok(value.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PromptIo for StubIo {
    fn is_terminal(&self) -> bool {
        self.terminal
    }

    fn read_line(
        &mut self,
        _cancellation: &CancellationToken,
        mode: InputMode,
        _max_bytes: usize,
    ) -> io::Result<ReadResult> {
        self.modes.push(mode);
        Ok(ReadResult::Line(self.line.clone()))
    }
}

#[derive(Default)]
struct FailingSecretIo {
    output: Vec<u8>,
}

impl Write for FailingSecretIo {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(value);
        Ok(value.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PromptIo for FailingSecretIo {
    fn is_terminal(&self) -> bool {
        true
    }

    fn read_line(
        &mut self,
        _cancellation: &CancellationToken,
        mode: InputMode,
        _max_bytes: usize,
    ) -> io::Result<ReadResult> {
        assert_eq!(mode, InputMode::Secret);
        Err(io::Error::other("read failed"))
    }
}
