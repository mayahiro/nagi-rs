//! Shared Hidden and Deprecated Command Graph conformance tests

mod support;

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{self, Cursor, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use nagi_cli::{
    Command, CompletionCandidate, CompletionEngine, CompletionInput, DeprecationNotice,
    DeprecationTargetKind, Diagnostic, Invocation, OptionGroup, OptionSpec, Outcome,
    PlainDeprecationNoticeRenderer, RuntimePolicy, cancellation_pair,
};

#[test]
fn lifecycle_metadata_matches_shared_fixtures() {
    for record in support::load(
        "cli/lifecycle.txt",
        "cli-lifecycle",
        &["arrangement", "expected"],
    ) {
        let actual = lifecycle_arrangement(record.field("arrangement"));
        assert_eq!(actual, record.text("expected"), "case {}", record.id);
    }
}

#[test]
fn help_plain_renderer_marks_deprecated_entries_without_exposing_hidden_entries() {
    let command = lifecycle_command();
    let root = command
        .render_help(&["nagi".to_owned()])
        .expect("root Help must render");
    assert!(root.contains("Old command [deprecated: use nagi run]"));
    assert!(root.contains("Legacy mode [deprecated: use --verbose]"));
    assert!(!root.contains("internal"));

    let old = command
        .render_help(&["nagi".to_owned(), "old".to_owned()])
        .expect("deprecated direct Help must render");
    assert!(old.contains("Deprecated: use nagi run"));

    let only_hidden = Command::new("nagi").subcommand(Command::new("internal").hidden());
    let only_hidden = only_hidden
        .render_help(&["nagi".to_owned()])
        .expect("Help with only hidden children must render");
    assert!(!only_hidden.contains("<COMMAND>"));
    assert!(!only_hidden.contains("internal"));
}

#[test]
fn invocation_clone_owns_deprecation_notice_storage() {
    let invocation = invocation(lifecycle_command(), ["--legacy", "run", "-m"]);
    let cloned = invocation.clone();
    drop(invocation);
    assert_eq!(cloned.deprecation_notices().len(), 2);
    assert_eq!(cloned.deprecation_notices()[1].replacement(), "--mode");
}

#[test]
fn hidden_option_value_completion_does_not_run_its_provider() {
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let command = Command::new("nagi").option(
        OptionSpec::value("secret")
            .long("secret")
            .hidden()
            .completion_provider({
                let provider_calls = Arc::clone(&provider_calls);
                move |_cancellation: &nagi_cli::CancellationToken,
                      _request: &nagi_cli::CompletionRequest| {
                    provider_calls.fetch_add(1, Ordering::Relaxed);
                    Ok(vec![CompletionCandidate::new("private")])
                }
            }),
    );
    let engine = CompletionEngine::new(&command).expect("Completion graph must build");
    let result = engine
        .complete(
            &nagi_cli::CancellationToken::new(),
            CompletionInput::new([OsString::from("--secret")], ""),
        )
        .expect("hidden option completion must resolve");
    assert!(result.candidates().is_empty());
    assert_eq!(provider_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn notice_output_failure_prevents_handler_execution() {
    let handler_calls = Arc::new(AtomicUsize::new(0));
    let command = Command::new("nagi")
        .option(
            OptionSpec::count("legacy")
                .long("legacy")
                .deprecated("--verbose"),
        )
        .handler({
            let handler_calls = Arc::clone(&handler_calls);
            move |_context: &mut nagi_cli::Context, _invocation: &Invocation| {
                handler_calls.fetch_add(1, Ordering::Relaxed);
                Ok(Outcome::success())
            }
        });
    let mut context = nagi_cli::Context::new(
        Cursor::new(Vec::<u8>::new()),
        Cursor::new(Vec::<u8>::new()),
        FailingWriter,
        Vec::<(String, String)>::new(),
        ".",
    );
    let error = command
        .run_with_policy(
            &mut context,
            ["--legacy"],
            &RuntimePolicy::default()
                .with_deprecation_notice_renderer(PlainDeprecationNoticeRenderer),
        )
        .expect_err("notice output must fail");
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert_eq!(handler_calls.load(Ordering::Relaxed), 0);
}

fn lifecycle_arrangement(name: &str) -> String {
    match name {
        "root-help" => snapshot_help(&lifecycle_command(), &["nagi"]),
        "old-help" => snapshot_help(&lifecycle_command(), &["nagi", "old"]),
        "hidden-help" => snapshot_help(&lifecycle_command(), &["nagi", "internal"]),
        "root-completion" => snapshot_completion(&lifecycle_command(), &[], ""),
        "hidden-completion" => snapshot_completion(&lifecycle_command(), &["internal"], ""),
        "hidden-value-completion" => snapshot_completion(&lifecycle_command(), &["--secret"], ""),
        "only-hidden-help" => snapshot_help(
            &Command::new("nagi").subcommand(Command::new("internal").hidden()),
            &["nagi"],
        ),
        "only-hidden-completion" => snapshot_completion(
            &Command::new("nagi").subcommand(Command::new("internal").hidden()),
            &[],
            "",
        ),
        "hidden-parse" => {
            let invocation = invocation(lifecycle_command(), ["internal", "--trace", "--internal"]);
            format!(
                "path={};internal={};trace={};notices={}",
                invocation.command_path().join("/"),
                invocation.flag("internal").unwrap_or(false),
                invocation.flag("trace").unwrap_or(false),
                snapshot_notices(invocation.deprecation_notices()),
            )
        }
        "deprecated-alias" => {
            let invocation = invocation(lifecycle_command(), ["o"]);
            snapshot_notices(invocation.deprecation_notices())
        }
        "deprecated-root" => snapshot_notices(
            invocation(
                Command::new("nagi")
                    .id("root-id")
                    .deprecated("qed")
                    .handler(success),
                [],
            )
            .deprecation_notices(),
        ),
        "deprecated-command-order" => snapshot_notices(
            invocation(lifecycle_command(), ["--legacy", "old"]).deprecation_notices(),
        ),
        "deprecated-order" => {
            let invocation = invocation(lifecycle_command(), ["--legacy", "--legacy", "run", "-m"]);
            snapshot_notices(invocation.deprecation_notices())
        }
        "deprecated-default" => snapshot_notices(
            invocation(
                Command::new("nagi")
                    .option(
                        OptionSpec::value("old")
                            .long("old")
                            .default_value("fallback")
                            .deprecated("--new"),
                    )
                    .handler(success),
                [],
            )
            .deprecation_notices(),
        ),
        "deprecated-environment" => {
            let command = Command::new("nagi")
                .option(
                    OptionSpec::value("old")
                        .long("old")
                        .environment("NAGI_OLD")
                        .deprecated("--new"),
                )
                .handler(success);
            let result = command
                .parse_with_environment(std::iter::empty::<&str>(), [("NAGI_OLD", "fallback")])
                .expect("environment fallback must parse");
            match result {
                nagi_cli::ParseResult::Invocation(invocation) => {
                    snapshot_notices(invocation.deprecation_notices())
                }
                _ => panic!("arrangement must produce an Invocation"),
            }
        }
        "runtime-enabled" => runtime_output(true),
        "runtime-default" => runtime_output(false),
        "invalid-command" => validation_code(Command::new("nagi").deprecated("")),
        "invalid-option" => validation_code(
            Command::new("nagi").option(
                OptionSpec::flag("old")
                    .long("old")
                    .deprecated("bad\nreplacement"),
            ),
        ),
        unknown => panic!("unknown lifecycle arrangement {unknown}"),
    }
}

fn lifecycle_command() -> Command {
    Command::new("nagi")
        .id("root-id")
        .option(
            OptionSpec::flag("quiet")
                .long("quiet")
                .inherited()
                .help("Quiet output"),
        )
        .option(
            OptionSpec::flag("internal")
                .long("internal")
                .inherited()
                .hidden()
                .help("Internal switch"),
        )
        .option(
            OptionSpec::value("secret")
                .long("secret")
                .hidden()
                .parser(nagi_cli::possible_values_parser(["one", "two"])),
        )
        .option(
            OptionSpec::count("legacy")
                .long("legacy")
                .short('l')
                .inherited()
                .deprecated("--verbose")
                .help("Legacy mode"),
        )
        .option(
            OptionSpec::count("verbose")
                .long("verbose")
                .short('v')
                .inherited()
                .help("Verbosity"),
        )
        .option_group(OptionGroup::at_most_one("verbosity", ["legacy", "verbose"]))
        .option_group(OptionGroup::at_most_one(
            "internal-pair",
            ["internal", "quiet"],
        ))
        .subcommand(
            Command::new("old")
                .id("old-id")
                .alias("o")
                .about("Old command")
                .deprecated("nagi run")
                .handler(success),
        )
        .subcommand(
            Command::new("internal")
                .id("internal-id")
                .hidden()
                .about("Internal command")
                .option(OptionSpec::flag("trace").long("trace").hidden())
                .handler(success),
        )
        .subcommand(
            Command::new("run")
                .id("run-id")
                .about("Run")
                .option(
                    OptionSpec::flag("old-mode")
                        .long("old-mode")
                        .short('m')
                        .deprecated("--mode"),
                )
                .option(OptionSpec::flag("mode").long("mode"))
                .handler(success),
        )
}

fn success(
    _context: &mut nagi_cli::Context,
    _invocation: &Invocation,
) -> Result<Outcome, Diagnostic> {
    Ok(Outcome::success())
}

fn invocation<const N: usize>(command: Command, arguments: [&str; N]) -> Invocation {
    let result = command.parse(arguments).expect("arguments must parse");
    match result {
        nagi_cli::ParseResult::Invocation(invocation) => invocation,
        _ => panic!("arrangement must produce an Invocation"),
    }
}

fn snapshot_help(command: &Command, path: &[&str]) -> String {
    let path: Vec<_> = path.iter().map(ToString::to_string).collect();
    let document = command.help_document(&path).expect("Help must build");
    let mut output = String::new();
    write!(
        output,
        "command={};commands={};options={};inherited={};relations={};groups={}",
        replacement(document.deprecation()),
        entries(document.commands()),
        entries(document.options()),
        inherited_entries(document.inherited_options()),
        if document.option_relations().is_empty() {
            "none".to_owned()
        } else {
            document
                .option_relations()
                .iter()
                .map(|relation| relation.source_id().to_owned())
                .collect::<Vec<_>>()
                .join(",")
        },
        if document.option_groups().is_empty() {
            "none".to_owned()
        } else {
            document
                .option_groups()
                .iter()
                .map(|group| group.id().to_owned())
                .collect::<Vec<_>>()
                .join(",")
        },
    )
    .expect("String writes cannot fail");
    output
}

fn entries(entries: &[nagi_cli::HelpEntry]) -> String {
    if entries.is_empty() {
        return "none".to_owned();
    }
    entries
        .iter()
        .map(|entry| format!("{}:{}", entry.id(), replacement(entry.deprecation())))
        .collect::<Vec<_>>()
        .join(",")
}

fn inherited_entries(entries: &[nagi_cli::HelpInheritedOption]) -> String {
    if entries.is_empty() {
        return "none".to_owned();
    }
    entries
        .iter()
        .map(|entry| format!("{}:{}", entry.id(), replacement(entry.deprecation())))
        .collect::<Vec<_>>()
        .join(",")
}

fn replacement(deprecation: Option<&nagi_cli::Deprecation>) -> &str {
    deprecation.map_or("none", nagi_cli::Deprecation::replacement)
}

fn snapshot_completion(command: &Command, arguments: &[&str], current: &str) -> String {
    let engine = CompletionEngine::new(command).expect("Completion graph must build");
    let result = engine
        .complete(
            &nagi_cli::CancellationToken::new(),
            CompletionInput::new(arguments.iter().map(OsString::from), current),
        )
        .expect("completion must resolve");
    result
        .candidates()
        .iter()
        .map(candidate_snapshot)
        .collect::<Vec<_>>()
        .join(",")
}

fn candidate_snapshot(candidate: &CompletionCandidate) -> String {
    format!(
        "{}:{}",
        candidate.value(),
        replacement(candidate.deprecation())
    )
}

fn snapshot_notices(notices: &[DeprecationNotice]) -> String {
    if notices.is_empty() {
        return "none".to_owned();
    }
    notices
        .iter()
        .map(|notice| {
            format!(
                "{}@{}@{}@{}@{}@{}",
                match notice.target_kind() {
                    DeprecationTargetKind::Command => "command",
                    DeprecationTargetKind::Option => "option",
                },
                notice.command_path().join("/"),
                notice.command_id_path().join("/"),
                notice.value_id().unwrap_or("-"),
                notice.spelling(),
                notice.replacement(),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn runtime_output(enabled: bool) -> String {
    let command = lifecycle_command();
    let stderr = SharedWriter::default();
    let (token, _handle) = cancellation_pair();
    let mut context = nagi_cli::Context::with_cancellation(
        Cursor::new(Vec::<u8>::new()),
        Cursor::new(Vec::<u8>::new()),
        stderr.clone(),
        Vec::<(String, String)>::new(),
        ".",
        token,
    );
    let mut policy = RuntimePolicy::default();
    if enabled {
        policy = policy.with_deprecation_notice_renderer(PlainDeprecationNoticeRenderer);
    }
    let outcome = command
        .run_with_policy(&mut context, ["--legacy", "run", "-m"], &policy)
        .expect("runtime must succeed");
    assert_eq!(outcome, Outcome::success());
    String::from_utf8(stderr.bytes()).expect("notice output must be UTF-8")
}

fn validation_code(command: Command) -> String {
    command
        .validate()
        .expect_err("invalid replacement must fail")
        .code()
        .as_str()
        .to_owned()
}

#[derive(Clone, Default)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl SharedWriter {
    fn bytes(&self) -> Vec<u8> {
        self.0
            .lock()
            .expect("writer lock must be available")
            .clone()
    }
}

impl Write for SharedWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("writer lock must be available")
            .extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("notice output failed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
