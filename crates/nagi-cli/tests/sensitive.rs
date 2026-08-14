//! Sensitive Value metadata and shared conformance fixtures

mod support;

use std::ffi::{OsStr, OsString};
use std::io::Cursor;
use std::os::unix::ffi::OsStrExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use nagi_cli::{
    Argument, Command, CompletionCandidate, CompletionEngine, CompletionInput, Diagnostic,
    DiagnosticCode, DiagnosticRenderer, DiagnosticTarget, Invocation, JsonDiagnosticRenderer,
    OptionSpec, ParseResult, RuntimePolicy, ValueSource, possible_values_parser, string_parser,
};

#[test]
fn sensitive_values_match_shared_fixtures() {
    for record in support::load(
        "cli/sensitive.txt",
        "cli-sensitive",
        &["arrangement", "expected"],
    ) {
        let actual = sensitive_arrangement(record.field("arrangement"));
        assert_eq!(actual, record.field("expected"), "case {}", record.id);
    }
}

#[test]
fn debug_projections_do_not_expose_sensitive_values() {
    let secret = "debug-secret-value";
    let command = Command::new("root").option(
        OptionSpec::value("token")
            .long("token")
            .parser(string_parser())
            .sensitive(),
    );
    let result = command
        .parse(["--token", secret])
        .expect("Sensitive value must parse");
    let ParseResult::Invocation(invocation) = &result else {
        panic!("parse must produce an Invocation");
    };
    let parsed = &invocation
        .parsed_values("token")
        .expect("token must be present")[0];
    for debug in [
        format!("{parsed:?}"),
        format!("{invocation:?}"),
        format!("{result:?}"),
    ] {
        assert!(!debug.contains(secret), "debug leaked the Sensitive value");
        assert!(debug.contains(nagi_cli::REDACTED_VALUE));
    }

    let input = CompletionInput::new(["--token", secret], "current-secret");
    let input_debug = format!("{input:?}");
    assert!(!input_debug.contains(secret));
    assert!(!input_debug.contains("current-secret"));

    let engine = CompletionEngine::new(&command).expect("command must be valid");
    let result = engine
        .complete(
            &nagi_cli::CancellationToken::new(),
            CompletionInput::new(["--token"], secret),
        )
        .expect("completion must succeed");
    let debug = format!("{result:?}");
    assert!(!debug.contains(secret));
    assert!(debug.contains(nagi_cli::REDACTED_VALUE));
}

#[test]
fn json_diagnostic_schema_stays_stable_for_sensitive_targets() {
    let secret = "diagnostic-json-secret";
    let error = Command::new("root")
        .option(sensitive_token())
        .parse(["--token", secret])
        .expect_err("parser must reject the fixture secret");
    let rendered = JsonDiagnosticRenderer.render_diagnostic(&error);
    assert!(rendered.contains(nagi_cli::REDACTED_VALUE));
    assert!(!rendered.contains(secret));
    assert!(!rendered.contains("expected one of"));
    assert!(!rendered.contains("\"sensitive\""));
}

fn sensitive_arrangement(arrangement: &str) -> String {
    match arrangement {
        "metadata" => {
            let option = OptionSpec::value("token").sensitive();
            let argument = Argument::new("credential").sensitive();
            let plain = OptionSpec::value("mode");
            format!(
                "option={};argument={};plain={}",
                option.is_sensitive(),
                argument.is_sensitive(),
                plain.is_sensitive()
            )
        }
        "help" => sensitive_help_snapshot(false),
        "inherited-help" => sensitive_help_snapshot(true),
        "invalid-option" => invalid_snapshot(
            &Command::new("root").option(sensitive_token()),
            ["--token", "bad"],
            std::iter::empty::<(OsString, OsString)>(),
        ),
        "invalid-attached" => invalid_snapshot(
            &Command::new("root").option(sensitive_token()),
            ["--token=bad"],
            std::iter::empty::<(OsString, OsString)>(),
        ),
        "invalid-environment" => invalid_snapshot(
            &Command::new("root").option(sensitive_token().environment("NAGI_TOKEN")),
            std::iter::empty::<&str>(),
            [("NAGI_TOKEN", "bad")],
        ),
        "invalid-default" => invalid_snapshot(
            &Command::new("root").option(sensitive_token().default_value("bad")),
            std::iter::empty::<&str>(),
            std::iter::empty::<(OsString, OsString)>(),
        ),
        "invalid-argument" => invalid_snapshot(
            &Command::new("root").argument(
                Argument::new("credential")
                    .parser(possible_values_parser(["accepted"]))
                    .sensitive(),
            ),
            ["bad"],
            std::iter::empty::<(OsString, OsString)>(),
        ),
        "nonsensitive-invalid" => invalid_snapshot(
            &Command::new("root").option(
                OptionSpec::value("mode")
                    .long("mode")
                    .parser(possible_values_parser(["safe", "fast"])),
            ),
            ["--mode", "bad"],
            std::iter::empty::<(OsString, OsString)>(),
        ),
        "raw-access" => {
            let command = Command::new("root").option(
                OptionSpec::value("token")
                    .long("token")
                    .parser(string_parser())
                    .sensitive(),
            );
            let ParseResult::Invocation(invocation) = command
                .parse(["--token", "s3cr3t"])
                .expect("Sensitive value must parse")
            else {
                panic!("parse must produce an Invocation");
            };
            let parsed = &invocation
                .parsed_values("token")
                .expect("token must be present")[0];
            format!(
                "raw={};typed={};sensitive={};visible={};scope={};source={}",
                hex(parsed.raw().as_bytes()),
                parsed
                    .downcast_ref::<String>()
                    .expect("String parser result must remain accessible"),
                parsed.is_sensitive(),
                invocation.value_is_sensitive("token"),
                invocation.current_scope().value_is_sensitive("token"),
                source_name(parsed.source())
            )
        }
        "completion-option" => sensitive_option_completion_snapshot(),
        "completion-argument" => sensitive_argument_completion_snapshot(),
        "completion-partial" => sensitive_partial_completion_snapshot(),
        "validator-target" => {
            let command = Command::new("root")
                .option(
                    OptionSpec::value("token")
                        .long("token")
                        .parser(string_parser())
                        .sensitive(),
                )
                .validator(|_invocation: &Invocation| {
                    Err(Diagnostic::new(DiagnosticCode::Validation, "rejected")
                        .with_target(DiagnosticTarget::option("token")))
                });
            let error = command
                .parse(["--token", "s3cr3t"])
                .expect_err("validator must reject");
            error.targets()[0].is_sensitive().to_string()
        }
        "inherited-validator-target" => inherited_validator_target_snapshot().to_string(),
        "handler-target" => handler_target_snapshot(),
        "unmatched-target" => format!(
            "unknown={};mismatched={}",
            validator_target_snapshot(
                DiagnosticTarget::option("token").with_command_id_path(vec!["unknown".to_owned()])
            ),
            validator_target_snapshot(DiagnosticTarget::argument("token"))
        ),
        "invalid-flag" => Command::new("root")
            .option(OptionSpec::flag("verbose").long("verbose").sensitive())
            .validate()
            .expect_err("Sensitive Flag must be invalid")
            .code()
            .as_str()
            .to_owned(),
        "invalid-count" => Command::new("root")
            .option(OptionSpec::count("verbose").long("verbose").sensitive())
            .validate()
            .expect_err("Sensitive Count must be invalid")
            .code()
            .as_str()
            .to_owned(),
        unknown => panic!("unknown Sensitive arrangement {unknown}"),
    }
}

fn inherited_validator_target_snapshot() -> bool {
    let command = Command::new("root")
        .option(
            OptionSpec::value("token")
                .long("token")
                .parser(string_parser())
                .sensitive()
                .inherited(),
        )
        .subcommand(Command::new("run").validator(|_invocation: &Invocation| {
            Err(
                Diagnostic::new(DiagnosticCode::Validation, "rejected").with_target(
                    DiagnosticTarget::option("token").with_command_id_path(vec!["root".to_owned()]),
                ),
            )
        }));
    command
        .parse(["run", "--token", "s3cr3t"])
        .expect_err("validator must reject")
        .targets()[0]
        .is_sensitive()
}

fn validator_target_snapshot(target: DiagnosticTarget) -> bool {
    let command = Command::new("root")
        .option(
            OptionSpec::value("token")
                .long("token")
                .parser(string_parser())
                .sensitive(),
        )
        .validator(move |_invocation: &Invocation| {
            Err(Diagnostic::new(DiagnosticCode::Validation, "rejected").with_target(target.clone()))
        });
    command
        .parse(["--token", "s3cr3t"])
        .expect_err("validator must reject")
        .targets()[0]
        .is_sensitive()
}

#[derive(Clone)]
struct SensitiveTargetRenderer(Arc<Mutex<Option<bool>>>);

impl DiagnosticRenderer for SensitiveTargetRenderer {
    fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String {
        *self
            .0
            .lock()
            .expect("renderer state lock must be available") =
            Some(diagnostic.targets()[0].is_sensitive());
        String::new()
    }
}

fn handler_target_snapshot() -> String {
    let command = Command::new("root")
        .option(
            OptionSpec::value("token")
                .long("token")
                .parser(string_parser())
                .sensitive(),
        )
        .handler(
            |_context: &mut nagi_cli::Context, _invocation: &Invocation| {
                Err(Diagnostic::new(DiagnosticCode::HandlerError, "rejected")
                    .with_target(DiagnosticTarget::option("token")))
            },
        );
    let ParseResult::Invocation(invocation) = command
        .parse(["--token", "s3cr3t"])
        .expect("Sensitive value must parse")
    else {
        panic!("parse must produce an Invocation");
    };
    let captured = Arc::new(Mutex::new(None));
    let policy = RuntimePolicy::default()
        .with_diagnostic_renderer(SensitiveTargetRenderer(Arc::clone(&captured)));
    let mut context = nagi_cli::Context::new(
        Cursor::new(Vec::<u8>::new()),
        Vec::<u8>::new(),
        Vec::<u8>::new(),
        std::iter::empty::<(OsString, OsString)>(),
        ".",
    );
    let outcome = command
        .run_invocation_with_policy(&mut context, &invocation, &policy)
        .expect("handler Diagnostic must render");
    assert_ne!(outcome.status(), nagi_cli::ExitStatus::SUCCESS);
    captured
        .lock()
        .expect("renderer state lock must be available")
        .expect("renderer must observe a Diagnostic")
        .to_string()
}

fn sensitive_token() -> OptionSpec {
    OptionSpec::value("token")
        .long("token")
        .parser(possible_values_parser(["accepted"]))
        .sensitive()
}

fn sensitive_help_snapshot(inherited: bool) -> String {
    let token = OptionSpec::value("token")
        .long("token")
        .help("Token")
        .parser(possible_values_parser(["alpha", "beta"]))
        .environment("NAGI_TOKEN")
        .default_value("s3cr3t")
        .sensitive();
    let command = if inherited {
        Command::new("root")
            .option(token.inherited())
            .subcommand(Command::new("run"))
    } else {
        Command::new("root")
            .option(token)
            .option(
                OptionSpec::value("mode")
                    .long("mode")
                    .help("Mode")
                    .parser(possible_values_parser(["safe", "fast"]))
                    .default_value("safe"),
            )
            .argument(Argument::new("credential").help("Credential").sensitive())
    };
    let path = if inherited {
        vec!["root".to_owned(), "run".to_owned()]
    } else {
        vec!["root".to_owned()]
    };
    let document = command
        .help_document(&path)
        .expect("Help Document must build");
    if inherited {
        let entry = &document.inherited_options()[0];
        return format!("{}:{}", entry.is_sensitive(), entry.description());
    }
    let token = document
        .options()
        .iter()
        .find(|entry| entry.id() == "token")
        .expect("token Help entry must exist");
    let argument = document
        .arguments()
        .iter()
        .find(|entry| entry.id() == "credential")
        .expect("credential Help entry must exist");
    let plain = document
        .options()
        .iter()
        .find(|entry| entry.id() == "mode")
        .expect("mode Help entry must exist");
    format!(
        "option={}:{};argument={}:{};plain={}:{}",
        token.is_sensitive(),
        token.description(),
        argument.is_sensitive(),
        argument.description(),
        plain.is_sensitive(),
        plain.description()
    )
}

fn invalid_snapshot<I, S, E, K, V>(command: &Command, arguments: I, environment: E) -> String
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    E: IntoIterator<Item = (K, V)>,
    K: Into<OsString>,
    V: Into<OsString>,
{
    let error = command
        .parse_with_environment(arguments, environment)
        .expect_err("arrangement must reject the value");
    format!(
        "message={};target={}",
        error.message(),
        error.targets()[0].is_sensitive()
    )
}

fn sensitive_option_completion_snapshot() -> String {
    let calls = Arc::new(AtomicUsize::new(0));
    let command = Command::new("root").option(sensitive_token().completion_provider({
        let calls = Arc::clone(&calls);
        move |_cancellation: &nagi_cli::CancellationToken,
              _request: &nagi_cli::CompletionRequest| {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(vec![CompletionCandidate::new("accepted")])
        }
    }));
    let engine = CompletionEngine::new(&command).expect("command must be valid");
    let result = engine
        .complete(
            &nagi_cli::CancellationToken::new(),
            CompletionInput::new(["--token"], ""),
        )
        .expect("completion must succeed");
    format!(
        "target={};candidates={};provider={}",
        result.request().target().is_sensitive(),
        result.candidates().len(),
        calls.load(Ordering::Relaxed)
    )
}

fn sensitive_argument_completion_snapshot() -> String {
    let calls = Arc::new(AtomicUsize::new(0));
    let command = Command::new("root").argument(
        Argument::new("credential")
            .parser(possible_values_parser(["accepted"]))
            .sensitive()
            .completion_provider({
                let calls = Arc::clone(&calls);
                move |_cancellation: &nagi_cli::CancellationToken,
                      _request: &nagi_cli::CompletionRequest| {
                    calls.fetch_add(1, Ordering::Relaxed);
                    Ok(vec![CompletionCandidate::new("accepted")])
                }
            }),
    );
    let engine = CompletionEngine::new(&command).expect("command must be valid");
    let result = engine
        .complete(
            &nagi_cli::CancellationToken::new(),
            CompletionInput::new(Vec::<OsString>::new(), "a"),
        )
        .expect("completion must succeed");
    format!(
        "target={};candidates={};provider={}",
        result.request().target().is_sensitive(),
        result.candidates().len(),
        calls.load(Ordering::Relaxed)
    )
}

fn sensitive_partial_completion_snapshot() -> String {
    let calls = Arc::new(AtomicUsize::new(0));
    let command = Command::new("root")
        .option(OptionSpec::value("token").long("token").sensitive())
        .option(
            OptionSpec::value("resource")
                .long("resource")
                .completion_provider({
                    let calls = Arc::clone(&calls);
                    move |_cancellation: &nagi_cli::CancellationToken,
                          request: &nagi_cli::CompletionRequest| {
                        let occurrence = &request.partial_occurrences()[0];
                        assert_eq!(occurrence.raw(), Some(OsStr::new("s3cr3t")));
                        assert!(occurrence.target().is_sensitive());
                        calls.fetch_add(1, Ordering::Relaxed);
                        Ok(Vec::new())
                    }
                }),
        );
    let engine = CompletionEngine::new(&command).expect("command must be valid");
    let result = engine
        .complete(
            &nagi_cli::CancellationToken::new(),
            CompletionInput::new(["--token", "s3cr3t", "--resource"], ""),
        )
        .expect("completion must succeed");
    let occurrence = &result.request().partial_occurrences()[0];
    format!(
        "target={};raw={};partial-sensitive={};provider={}",
        result.request().target().is_sensitive(),
        occurrence
            .raw()
            .and_then(OsStr::to_str)
            .expect("fixture value must be UTF-8"),
        occurrence.target().is_sensitive(),
        calls.load(Ordering::Relaxed)
    )
}

fn source_name(source: ValueSource) -> &'static str {
    match source {
        ValueSource::CommandLine => "command-line",
        ValueSource::Environment => "environment",
        ValueSource::Default => "default",
        ValueSource::External => "external",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
