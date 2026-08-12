//! Shared CLI completion conformance fixtures

mod support;

use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::os::unix::ffi::OsStringExt;

use nagi_cli::{
    Argument, CancellationToken, Command, CompletionCandidate, CompletionCandidateKind,
    CompletionEngine, CompletionInput, CompletionOccurrenceKind, CompletionProviderError,
    CompletionRequest, CompletionResult, CompletionTarget, CompletionTargetKind, OptionSpec,
    possible_values_parser,
};

#[test]
fn completion_matches_shared_fixtures() {
    let engine = CompletionEngine::new(&completion_fixture_command())
        .expect("completion fixture command must be valid");
    for record in support::load(
        "cli/completion.txt",
        "cli-completion",
        &["argv", "current", "expected"],
    ) {
        let result = engine
            .complete(
                &CancellationToken::new(),
                CompletionInput::new(
                    arguments(&record.bytes("argv")),
                    OsString::from_vec(record.bytes("current")),
                ),
            )
            .unwrap_or_else(|error| panic!("case {} failed: {error}", record.id));
        assert_eq!(
            snapshot_completion(&result),
            record.field("expected"),
            "case {}",
            record.id
        );
    }
}

fn completion_fixture_command() -> Command {
    Command::new("qed")
        .id("root-id")
        .version("1.0.0")
        .option(
            OptionSpec::count("verbose")
                .long("verbose")
                .short('v')
                .inherited()
                .help("Increase verbosity"),
        )
        .option(
            OptionSpec::value("config")
                .long("config")
                .short('c')
                .inherited()
                .parser(possible_values_parser(["dev", "prod"]))
                .completion_provider(completion_fixture_provider)
                .help("Configuration profile"),
        )
        .option(
            OptionSpec::value("output")
                .long("output")
                .parser(possible_values_parser(["text", "json"])),
        )
        .subcommand(
            Command::new("run")
                .id("run-id")
                .alias("r")
                .about("Run one session")
                .option(
                    OptionSpec::flag("dry-run")
                        .long("dry-run")
                        .help("Do not execute"),
                )
                .option(
                    OptionSpec::value("agent")
                        .long("agent")
                        .short('a')
                        .completion_provider(completion_fixture_provider)
                        .help("Agent name"),
                )
                .argument(
                    Argument::new("session")
                        .repeated()
                        .parser(possible_values_parser(["local"]))
                        .completion_provider(completion_fixture_provider),
                )
                .subcommand(Command::new("exec").id("exec-id").about("Execute a task")),
        )
}

fn completion_fixture_provider(
    _cancellation: &CancellationToken,
    request: &CompletionRequest,
) -> Result<Vec<CompletionCandidate>, CompletionProviderError> {
    match request.target().value_id() {
        Some("config") => Ok(vec![
            CompletionCandidate::new("prod"),
            CompletionCandidate::new("canary").with_append_space(false),
        ]),
        Some("agent") => Ok(vec![
            CompletionCandidate::new("atlas"),
            CompletionCandidate::new("ada"),
            CompletionCandidate::new("日本"),
        ]),
        Some("session") => Ok(vec![
            CompletionCandidate::new("local"),
            CompletionCandidate::new("session-a"),
        ]),
        target => Err(CompletionProviderError::new(format!(
            "unexpected completion target {target:?}"
        ))),
    }
}

fn arguments(bytes: &[u8]) -> Vec<OsString> {
    if bytes.is_empty() {
        return Vec::new();
    }
    bytes
        .split(|byte| *byte == b'|')
        .map(|value| OsString::from_vec(value.to_vec()))
        .collect()
}

fn snapshot_completion(result: &CompletionResult) -> String {
    let request = result.request();
    let mut output = format!(
        "path={};ids={};target={};prefix={};partial=",
        request.command_path().join("/"),
        request.command_id_path().join("/"),
        snapshot_target(request.target()),
        request.prefix().to_str().expect("fixture prefix is UTF-8")
    );
    if request.partial_occurrences().is_empty() {
        output.push_str("none");
    }
    for (index, occurrence) in request.partial_occurrences().iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        write!(
            output,
            "{}={}",
            snapshot_target(occurrence.target()),
            snapshot_occurrence_kind(occurrence.kind())
        )
        .expect("writing to String cannot fail");
        if let Some(raw) = occurrence.raw() {
            write!(
                output,
                ":{}",
                raw.to_str().expect("fixture occurrence is UTF-8")
            )
            .expect("writing to String cannot fail");
        }
    }
    output.push_str(";candidates=");
    for (index, candidate) in result.candidates().iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        write!(
            output,
            "{}:{}{}",
            snapshot_candidate_kind(candidate.kind()),
            candidate.value(),
            if candidate.append_space() { '+' } else { '-' }
        )
        .expect("writing to String cannot fail");
    }
    output
}

fn snapshot_target(target: &CompletionTarget) -> String {
    let kind = match target.kind() {
        CompletionTargetKind::Command => "command",
        CompletionTargetKind::Option => "option",
        CompletionTargetKind::Argument => "argument",
    };
    let mut output = format!("{kind}@{}", target.command_id_path().join("/"));
    if let Some(value_id) = target.value_id() {
        write!(output, ":{value_id}").expect("writing to String cannot fail");
    }
    output
}

fn snapshot_occurrence_kind(kind: CompletionOccurrenceKind) -> &'static str {
    match kind {
        CompletionOccurrenceKind::Flag => "flag",
        CompletionOccurrenceKind::Count => "count",
        CompletionOccurrenceKind::Value => "value",
    }
}

fn snapshot_candidate_kind(kind: CompletionCandidateKind) -> &'static str {
    match kind {
        CompletionCandidateKind::Command => "command",
        CompletionCandidateKind::Option => "option",
        CompletionCandidateKind::Value => "value",
    }
}

#[test]
fn completion_input_owns_arguments() {
    let mut arguments = vec![OsString::from("run")];
    let input = CompletionInput::new(arguments.clone(), "");
    arguments[0] = OsString::from("changed");
    assert_eq!(input.arguments(), [OsString::from("run")]);
    assert_eq!(input.current(), OsStr::new(""));
}
