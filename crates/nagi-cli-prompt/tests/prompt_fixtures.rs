//! Shared Prompt conformance fixtures

mod support;

use std::io::{self, Write};

use nagi_cli::CancellationToken;
use nagi_cli_prompt::{
    Confirm, Input, InputMode, Limits, PromptErrorKind, PromptIo, Prompter, ReadResult, Secret,
    Select,
};

#[test]
fn prompt_matches_shared_fixtures() {
    for record in support::load(
        "cli/prompt.txt",
        "cli-prompt",
        &[
            "kind", "terminal", "message", "default", "required", "choices", "input", "limit",
            "expected", "output", "modes",
        ],
    ) {
        let mut prompt_io = MemoryIo::new(record.bytes("input"), record.field("terminal") == "yes");
        let limits = Limits::default().with_max_input_bytes(
            record
                .field("limit")
                .parse()
                .expect("fixture limit must be numeric"),
        );
        let result = run(&record, &mut prompt_io, limits);
        assert_eq!(result, record.field("expected"), "case {}", record.id);
        assert_eq!(
            prompt_io.output,
            record.bytes("output"),
            "case {}",
            record.id
        );
        assert_eq!(
            snapshot_modes(&prompt_io.modes),
            record.field("modes"),
            "case {}",
            record.id
        );
    }
}

fn run(record: &support::Record, prompt_io: &mut MemoryIo, limits: Limits) -> String {
    let cancellation = CancellationToken::new();
    let mut prompter = Prompter::new(prompt_io).with_limits(limits);
    let result = match record.field("kind") {
        "confirm" => {
            let mut request = Confirm::new(record.text("message"));
            request = match record.field("default") {
                "yes" => request.with_default(true),
                "no" => request.with_default(false),
                "-" => request,
                value => panic!("unknown Confirm default {value}"),
            };
            prompter
                .confirm(&cancellation, &request)
                .map(|value| format!("bool:{value}"))
        }
        "select" => {
            let choices = split_choices(record.text("choices"));
            let mut request = Select::new(record.text("message"), choices);
            if record.field("default") != "-" {
                request = request.with_default(
                    record
                        .field("default")
                        .parse()
                        .expect("Select default must be numeric"),
                );
            }
            prompter
                .select(&cancellation, &request)
                .map(|value| format!("index:{value}"))
        }
        "input" => {
            let mut request = Input::new(record.text("message"));
            if record.field("default") != "-" {
                request = request.with_default(record.text("default"));
            }
            if record.field("required") == "yes" {
                request = request.required();
            }
            prompter
                .input(&cancellation, &request)
                .map(|value| format!("text:{value}"))
        }
        "secret" => {
            let mut request = Secret::new(record.text("message"));
            if record.field("required") == "yes" {
                request = request.required();
            }
            prompter
                .secret(&cancellation, &request)
                .map(|value| format!("text:{value}"))
        }
        value => panic!("unknown fixture prompt kind {value}"),
    };
    result.unwrap_or_else(|error| format!("error:{}", error_kind(error.kind())))
}

fn split_choices(value: String) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split('|').map(str::to_owned).collect()
    }
}

fn error_kind(kind: PromptErrorKind) -> &'static str {
    match kind {
        PromptErrorKind::Cancelled => "cancelled",
        PromptErrorKind::NotTerminal => "not-terminal",
        PromptErrorKind::InvalidRequest => "invalid-request",
        PromptErrorKind::InputTooLong => "input-too-long",
        PromptErrorKind::Io => "io",
    }
}

fn snapshot_modes(modes: &[InputMode]) -> String {
    modes
        .iter()
        .map(|mode| match mode {
            InputMode::Visible => "visible",
            InputMode::Secret => "secret",
        })
        .collect::<Vec<_>>()
        .join(",")
}

struct MemoryIo {
    input: Vec<u8>,
    position: usize,
    output: Vec<u8>,
    terminal: bool,
    modes: Vec<InputMode>,
}

impl MemoryIo {
    fn new(input: Vec<u8>, terminal: bool) -> Self {
        Self {
            input,
            position: 0,
            output: Vec::new(),
            terminal,
            modes: Vec::new(),
        }
    }
}

impl Write for MemoryIo {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PromptIo for MemoryIo {
    fn is_terminal(&self) -> bool {
        self.terminal
    }

    fn read_line(
        &mut self,
        _cancellation: &CancellationToken,
        mode: InputMode,
        max_bytes: usize,
    ) -> io::Result<ReadResult> {
        self.modes.push(mode);
        if self.position == self.input.len() {
            return Ok(ReadResult::EndOfFile);
        }
        let remaining = &self.input[self.position..];
        let end = remaining
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(remaining.len(), |index| index + 1);
        self.position += end;
        let mut line = remaining[..end].to_vec();
        if line.last() == Some(&b'\n') {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
        }
        if line.len() > max_bytes {
            Ok(ReadResult::InputTooLong)
        } else {
            Ok(ReadResult::Line(line))
        }
    }
}
