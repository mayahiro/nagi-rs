//! Shell generation and completion protocol behavior

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

use nagi_cli::{
    CancellationToken, Command, CompletionCandidate, CompletionEngine, CompletionErrorKind,
    CompletionProviderError, CompletionRequest, OptionSpec, possible_values_parser,
};
use nagi_cli_completion::{PROTOCOL_TOKEN, ProtocolErrorKind, Shell, generate, handle};

#[test]
fn generated_scripts_match_shared_golden_files() {
    let engine = completion_engine(false);
    for (name, shell) in [
        ("bash", Shell::Bash),
        ("zsh", Shell::Zsh),
        ("fish", Shell::Fish),
        ("powershell", Shell::PowerShell),
    ] {
        let expected = fs::read_to_string(
            fixture_root()
                .join("cli/completion")
                .join(format!("{name}.txt")),
        )
        .unwrap_or_else(|error| panic!("failed to read {name} golden file: {error}"));
        assert_eq!(generate(shell, &engine), expected, "{name}");
    }
}

#[test]
fn bash_adapter_extracts_cursor_prefix_without_evaluation() {
    let mut script = generate(Shell::Bash, &completion_engine(false));
    script.push_str(
        r#"COMP_LINE='qed --profile=preview'
COMP_POINT=17
COMP_CWORD=1
COMP_WORDS=('qed' '--profile=preview')
REPLY=''
_nagi_completion_current_qed
printf '%s\n' "$REPLY"
COMP_LINE='qed --profile=日本語'
COMP_POINT=20
COMP_CWORD=1
COMP_WORDS=('qed' '--profile=日本語')
REPLY=''
_nagi_completion_current_qed
printf '%s\n' "$REPLY"
REPLY=''
_nagi_completion_dequote_qed '"two words"'
printf '%s\n' "$REPLY"
_nagi_completion_escape_qed 'hello world' ''
printf 'unquoted=%s\n' "$REPLY"
_nagi_completion_escape_qed 'hello world' 'double'
printf 'double=%s\n' "$REPLY"
_nagi_completion_escape_qed "a'b" 'single'
printf 'single=%s\n' "$REPLY"
_nagi_completion_escape_qed 'a$b"c' 'double'
printf 'double-special=%s\n' "$REPLY"
"#,
    );

    let mut child = ProcessCommand::new("bash")
        .args(["--noprofile", "--norc", "-s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Bash must be available on supported platforms");
    child
        .stdin
        .take()
        .expect("Bash stdin must be piped")
        .write_all(script.as_bytes())
        .expect("probe script must be written");
    let output = child.wait_with_output().expect("Bash must exit");
    assert!(
        output.status.success(),
        "Bash failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("probe output must be UTF-8"),
        "--profile=pre\n--profile=日本\ntwo words\nunquoted=hello\\ world\ndouble=hello world\nsingle=a'\\''b\ndouble-special=a\\$b\\\"c\n"
    );
}

#[test]
fn protocol_handles_static_and_dynamic_candidates() {
    let engine = completion_engine(true);
    let mut bash = Vec::new();
    assert!(
        handle(
            &CancellationToken::new(),
            &engine,
            [PROTOCOL_TOKEN, "bash", "p", "--profile"],
            &mut bash,
        )
        .expect("Bash protocol must succeed")
    );
    assert_eq!(
        String::from_utf8(bash).expect("protocol is UTF-8"),
        "prod\tprod\tprod\tvalue\tspace\npreview\tpreview\tPreview profile\tvalue\tnone\n"
    );

    let mut fish = Vec::new();
    assert!(
        handle(
            &CancellationToken::new(),
            &engine,
            [PROTOCOL_TOKEN, "fish", "p", "--profile"],
            &mut fish,
        )
        .expect("Fish protocol must succeed")
    );
    assert_eq!(
        String::from_utf8(fish).expect("protocol is UTF-8"),
        "prod\tprod\npreview\tPreview profile\n"
    );
}

#[test]
fn protocol_decorates_deprecated_static_candidates() {
    let engine = CompletionEngine::new(
        &Command::new("qed")
            .subcommand(Command::new("old").about("Old command").deprecated("run"))
            .subcommand(Command::new("run")),
    )
    .expect("completion command must be valid");
    let mut output = Vec::new();
    assert!(
        handle(
            &CancellationToken::new(),
            &engine,
            [PROTOCOL_TOKEN, "bash", "o"],
            &mut output,
        )
        .expect("Bash protocol must succeed")
    );
    assert_eq!(
        String::from_utf8(output).expect("protocol is UTF-8"),
        "old\told\tOld command [deprecated: use run]\tcommand\tspace\n"
    );
}

#[test]
fn protocol_passes_through_and_reports_errors() {
    let engine = completion_engine(false);
    let mut output = Vec::new();
    assert!(
        !handle(&CancellationToken::new(), &engine, ["run"], &mut output,)
            .expect("ordinary arguments must pass through")
    );
    assert!(output.is_empty());

    for arguments in [
        vec![OsString::from(PROTOCOL_TOKEN)],
        vec![
            OsString::from(PROTOCOL_TOKEN),
            OsString::from("unknown"),
            OsString::new(),
        ],
    ] {
        let error = handle(&CancellationToken::new(), &engine, arguments, &mut output)
            .expect_err("invalid reserved request must fail");
        assert_eq!(error.kind(), ProtocolErrorKind::InvalidRequest);
    }
}

#[test]
fn protocol_preserves_completion_and_io_errors() {
    let command = Command::new("qed").option(
        OptionSpec::value("profile")
            .long("profile")
            .completion_provider(
                |_cancellation: &CancellationToken, _request: &CompletionRequest| {
                    Err(CompletionProviderError::new("provider failed"))
                },
            ),
    );
    let engine = CompletionEngine::new(&command).expect("command must be valid");
    let error = handle(
        &CancellationToken::new(),
        &engine,
        [PROTOCOL_TOKEN, "bash", "", "--profile"],
        &mut Vec::new(),
    )
    .expect_err("provider must fail");
    assert_eq!(error.kind(), ProtocolErrorKind::Completion);
    assert_eq!(
        error
            .completion_error()
            .expect("completion error must be retained")
            .kind(),
        CompletionErrorKind::Provider
    );

    let engine = completion_engine(false);
    let error = handle(
        &CancellationToken::new(),
        &engine,
        [PROTOCOL_TOKEN, "bash", "p", "--profile"],
        &mut FailingWriter,
    )
    .expect_err("writer must fail");
    assert_eq!(error.kind(), ProtocolErrorKind::Io);
}

fn completion_engine(dynamic: bool) -> CompletionEngine {
    let mut option = OptionSpec::value("profile")
        .long("profile")
        .parser(possible_values_parser(["dev", "prod"]));
    if dynamic {
        option = option.completion_provider(
            |_cancellation: &CancellationToken, _request: &CompletionRequest| {
                Ok(vec![
                    CompletionCandidate::new("prod"),
                    CompletionCandidate::new("preview")
                        .with_description("Preview profile")
                        .with_append_space(false),
                ])
            },
        );
    }
    CompletionEngine::new(
        &Command::new("qed")
            .version("1.0.0")
            .option(option)
            .subcommand(Command::new("run")),
    )
    .expect("completion command must be valid")
}

fn fixture_root() -> PathBuf {
    env::var_os("NAGI_FIXTURES")
        .map(PathBuf::from)
        .or_else(|| {
            let integrated = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures");
            integrated.is_dir().then_some(integrated)
        })
        .expect("NAGI_FIXTURES is not configured and integrated fixtures are absent")
}

struct FailingWriter;

impl io::Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("write failed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
