//! Shared CLI conformance fixtures

mod support;

use std::ffi::{OsStr, OsString};
use std::io::{Cursor, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::sync::{Arc, Mutex};

use nagi_cli::{
    Argument, Command, Context, Diagnostic, DiagnosticCategory, DiagnosticCode, ExitCodePolicy,
    ExitStatus, HelpSection, Invocation, OptionGroup, OptionSpec, Outcome, ParseResult,
    PlainDiagnosticRenderer, RuntimePolicy, SubcommandUsageMode, ValueSource, integer_parser,
    possible_values_parser, value_parser,
};

#[test]
fn parsing_matches_shared_fixtures() {
    let command = fixture_command();
    for record in support::load(
        "cli/parsing.txt",
        "cli-parsing",
        &["argv", "env", "expected"],
    ) {
        let result = command
            .parse_with_environment(
                arguments(&record.bytes("argv")),
                environment(&record.bytes("env")),
            )
            .unwrap_or_else(|error| panic!("case {} failed: {error}", record.id));
        assert_eq!(
            snapshot_parse(result),
            record.field("expected"),
            "case {}",
            record.id
        );
    }
}

#[test]
fn command_local_scopes_match_shared_fixtures() {
    let command = Command::new("root")
        .id("root-id")
        .option(
            OptionSpec::value("session")
                .long("session")
                .default_value("root"),
        )
        .subcommand(
            Command::new("run")
                .id("run-id")
                .option(OptionSpec::value("session").long("session")),
        );
    for record in support::load("cli/scopes.txt", "cli-scopes", &["argv", "expected"]) {
        let result = command
            .parse(arguments(&record.bytes("argv")))
            .unwrap_or_else(|error| panic!("case {} failed: {error}", record.id));
        let ParseResult::Invocation(invocation) = result else {
            panic!("case {} did not produce an Invocation", record.id);
        };
        let root = invocation
            .scope(["root-id"])
            .expect("root scope must be present");
        let current = invocation
            .raw_value("session")
            .and_then(OsStr::to_str)
            .unwrap_or("none");
        let root_value = root
            .raw_value("session")
            .and_then(OsStr::to_str)
            .unwrap_or("none");
        let snapshot = format!(
            "command={};ids={};current={current};root={root_value};current-supplied={};root-supplied={}",
            invocation.command_path().join("/"),
            invocation.command_id_path().join("/"),
            invocation.supplied("session"),
            root.supplied("session"),
        );
        assert_eq!(snapshot, record.field("expected"), "case {}", record.id);
    }
}

#[test]
fn inherited_options_match_shared_fixtures() {
    let command = inherited_fixture_command();
    for record in support::load(
        "cli/inherited-options.txt",
        "cli-inherited-options",
        &["argv", "env", "expected"],
    ) {
        let snapshot = match command.parse_with_environment(
            arguments(&record.bytes("argv")),
            environment(&record.bytes("env")),
        ) {
            Ok(ParseResult::Invocation(invocation)) => snapshot_scoped_invocation(&invocation),
            Ok(_) => panic!("case {} did not produce an Invocation", record.id),
            Err(error) => snapshot_inherited_error(&error),
        };
        assert_eq!(snapshot, record.field("expected"), "case {}", record.id);
    }
}

#[test]
fn inherited_option_validation_matches_shared_fixtures() {
    for record in support::load(
        "cli/inherited-option-validation.txt",
        "cli-inherited-option-validation",
        &["shape", "expected"],
    ) {
        let snapshot = match inherited_validation_command(record.field("shape")).validate() {
            Ok(()) => "ok",
            Err(error) => error.code().as_str(),
        };
        assert_eq!(snapshot, record.field("expected"), "case {}", record.id);
    }
}

#[test]
fn errors_match_shared_fixtures() {
    let command = fixture_command();
    for record in support::load(
        "cli/errors.txt",
        "cli-errors",
        &["argv", "env", "expected", "category"],
    ) {
        let error = command
            .parse_with_environment(
                arguments(&record.bytes("argv")),
                environment(&record.bytes("env")),
            )
            .expect_err("fixture case must fail");
        assert_eq!(
            error.code().as_str(),
            record.field("expected"),
            "case {}",
            record.id
        );
        assert_eq!(
            error.category().as_str(),
            record.field("category"),
            "case {} category",
            record.id
        );
    }
}

#[test]
fn diagnostic_metadata_matches_shared_fixtures() {
    let command = fixture_command();
    for record in support::load(
        "cli/diagnostics.txt",
        "cli-diagnostics",
        &["argv", "expected"],
    ) {
        let error = command
            .parse(arguments(&record.bytes("argv")))
            .expect_err("fixture case must fail");
        let targets = error
            .targets()
            .iter()
            .map(|target| {
                let kind = match target.kind() {
                    nagi_cli::DiagnosticTargetKind::Option => "option",
                    nagi_cli::DiagnosticTargetKind::Argument => "argument",
                };
                format!(
                    "{kind}@{}:{}",
                    target.command_id_path().join("/"),
                    target.value_id()
                )
            })
            .collect::<Vec<_>>()
            .join("+");
        let snapshot = format!(
            "code={};category={};targets={targets};hints={}",
            error.code().as_str(),
            error.category().as_str(),
            error.hints().join("+"),
        );
        assert_eq!(snapshot, record.field("expected"), "case {}", record.id);
    }
}

#[test]
fn help_matches_shared_fixtures() {
    let command = fixture_command();
    for record in support::load("cli/help.txt", "cli-help", &["path", "expected"]) {
        let mut path = vec!["nagi".to_owned()];
        if !record.field("path").is_empty() {
            path.extend(record.field("path").split('/').map(str::to_owned));
        }
        assert_eq!(
            command.render_help(&path).unwrap(),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

#[test]
fn help_presentation_matches_shared_fixtures() {
    for record in support::load(
        "cli/help-presentation.txt",
        "cli-help-presentation",
        &["mode", "required", "expected"],
    ) {
        let mode = match record.field("mode") {
            "auto" => SubcommandUsageMode::Auto,
            "hidden" => SubcommandUsageMode::Hidden,
            "expanded" => SubcommandUsageMode::Expanded,
            value => panic!("case {} has invalid mode {value}", record.id),
        };
        let required = record.field("required") == "true";
        let mut command = Command::new("root")
            .id("root-id")
            .subcommand_usage(mode)
            .subcommand(
                Command::new("compare")
                    .id("compare-id")
                    .usage_variant("file", "--file <FILE>")
                    .usage_variant("stdin", "--stdin"),
            )
            .subcommand(Command::new("status").id("status-id"));
        if required {
            command = command.require_subcommand();
        } else {
            command = command.usage_variant("direct", "<ROOT>");
        }
        let document = command
            .help_document(&["root".to_owned()])
            .unwrap_or_else(|error| panic!("case {} failed: {error}", record.id));
        let snapshot = document
            .usage_variants()
            .iter()
            .map(|variant| {
                format!(
                    "{}:{}={}",
                    variant.command_id_path().join("/"),
                    variant.id(),
                    variant.syntax()
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(snapshot, record.field("expected"), "case {}", record.id);
    }
}

#[test]
fn inherited_option_help_matches_shared_fixtures() {
    let command = inherited_help_command();
    for record in support::load(
        "cli/inherited-option-help.txt",
        "cli-inherited-option-help",
        &["path", "structured", "expected"],
    ) {
        let path = record
            .field("path")
            .split('/')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut command_path = vec!["root".to_owned()];
        command_path.extend(path);
        let document = command
            .help_document(&command_path)
            .unwrap_or_else(|error| panic!("case {} failed: {error}", record.id));
        let structured = document
            .inherited_options()
            .iter()
            .map(|option| {
                format!(
                    "{}@{}:{}={}",
                    option.command_id_path().join("/"),
                    option.command_path().join("/"),
                    option.id(),
                    option.label()
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(
            structured,
            record.field("structured"),
            "case {} structured",
            record.id
        );
        assert_eq!(
            command.render_help(&command_path).unwrap(),
            record.text("expected"),
            "case {} rendered",
            record.id
        );
    }
}

#[test]
fn runtime_matches_shared_fixtures() {
    for record in support::load(
        "cli/runtime.txt",
        "cli-runtime",
        &[
            "argv",
            "env",
            "stdin",
            "cwd",
            "cancelled",
            "usage-status",
            "error-prefix",
            "show-usage",
            "status",
            "stdout",
            "stderr",
        ],
    ) {
        let stdout = SharedWriter::default();
        let stderr = SharedWriter::default();
        let (token, handle) = nagi_cli::cancellation_pair();
        if record.field("cancelled") == "true" {
            handle.cancel();
        }
        let mut context = Context::with_cancellation(
            Cursor::new(record.bytes("stdin")),
            stdout.clone(),
            stderr.clone(),
            environment(&record.bytes("env")),
            record.field("cwd"),
            token,
        );
        let usage_status = record
            .field("usage-status")
            .parse::<u8>()
            .expect("fixture usage status is one byte");
        let policy = RuntimePolicy::default()
            .with_exit_code_policy(
                ExitCodePolicy::default()
                    .with_status(DiagnosticCategory::Usage, ExitStatus::new(usage_status)),
            )
            .with_diagnostic_renderer(
                PlainDiagnosticRenderer::default()
                    .with_prefix(record.field("error-prefix"))
                    .with_usage(record.field("show-usage") == "true"),
            );
        let outcome = runtime_command()
            .run_with_policy(&mut context, arguments(&record.bytes("argv")), &policy)
            .unwrap_or_else(|error| panic!("case {} failed: {error}", record.id));
        assert_eq!(
            outcome.status().code().to_string(),
            record.field("status"),
            "case {} status",
            record.id
        );
        assert_eq!(
            stdout.bytes(),
            record.bytes("stdout"),
            "case {} stdout",
            record.id
        );
        assert_eq!(
            stderr.bytes(),
            record.bytes("stderr"),
            "case {} stderr",
            record.id
        );
    }
}

#[test]
fn definition_validation_and_typed_values_are_public() {
    let invalid = Command::new("root")
        .option(OptionSpec::flag("first").long("same"))
        .option(OptionSpec::flag("second").long("same"));
    let error = invalid.validate().unwrap_err();
    assert_eq!(error.code(), DiagnosticCode::InvalidSpecification);
    assert_eq!(error.category(), DiagnosticCategory::Specification);

    let invocation = fixture_command()
        .parse(["serve", "--mode", "http", "--port", "42", "host"])
        .unwrap();
    let ParseResult::Invocation(invocation) = invocation else {
        panic!("expected invocation");
    };
    assert_eq!(invocation.value::<i64>("port"), Some(&42));
}

fn fixture_command() -> Command {
    Command::new("nagi")
        .about("Nagi fixture command")
        .version("1.2.3")
        .option(
            OptionSpec::count("verbose")
                .long("verbose")
                .short('v')
                .help("Increase verbosity"),
        )
        .option(
            OptionSpec::value("output")
                .long("output")
                .short('o')
                .parser(value_parser("PATH", |value: &OsStr| Ok(value.to_owned())))
                .help("Output path"),
        )
        .option(
            OptionSpec::value("color")
                .long("color")
                .parser(possible_values_parser(["auto", "always", "never"]))
                .default_value("auto")
                .help("Color mode"),
        )
        .option(
            OptionSpec::value("config")
                .long("config")
                .short('c')
                .environment("NAGI_CONFIG")
                .help("Config path"),
        )
        .option(
            OptionSpec::value("tag")
                .long("tag")
                .short('t')
                .repeated()
                .help("Tag value"),
        )
        .option(
            OptionSpec::flag("force")
                .long("force")
                .short('f')
                .conflicts("dry-run")
                .help("Force operation"),
        )
        .option(
            OptionSpec::flag("dry-run")
                .long("dry-run")
                .short('n')
                .help("Dry run"),
        )
        .option(
            OptionSpec::value("token")
                .long("token")
                .requires("config")
                .help("Token value"),
        )
        .option_group(OptionGroup::at_most_one("output-mode", ["color", "output"]))
        .argument(Argument::new("input").help("Input value"))
        .argument(Argument::new("extra").repeated().help("Extra values"))
        .example("basic", "nagi file")
        .note("Values use command line, environment, then default precedence")
        .link(
            "guide",
            "https://github.com/mayahiro/nagi/blob/main/docs/CLI_API.md",
        )
        .help_section(
            HelpSection::new("output-formats", "Output formats")
                .entry("plain", "Default deterministic text")
                .paragraph("Custom renderers consume the same Help Document"),
        )
        .validator(|invocation: &Invocation| {
            if invocation.raw_value("input") == Some(OsStr::new("blocked")) {
                return Err(Diagnostic::new(
                    DiagnosticCode::Validation,
                    "input 'blocked' is not allowed",
                )
                .with_target(nagi_cli::DiagnosticTarget::argument("input"))
                .with_hint("choose another input"));
            }
            Ok(())
        })
        .subcommand(
            Command::new("serve")
                .alias("s")
                .about("Serve files")
                .usage_variant("host", "<HOST> [OPTIONS]")
                .usage_variant("mode", "--mode <http|https> <HOST> [OPTIONS]")
                .option(
                    OptionSpec::value("port")
                        .long("port")
                        .short('p')
                        .parser(integer_parser())
                        .default_value("8080")
                        .help("Port"),
                )
                .option(
                    OptionSpec::value("mode")
                        .long("mode")
                        .short('m')
                        .parser(possible_values_parser(["http", "https"]))
                        .required()
                        .help("Mode"),
                )
                .option(
                    OptionSpec::value("header")
                        .long("header")
                        .short('H')
                        .repeated()
                        .help("Header value"),
                )
                .argument(Argument::new("host").required().help("Host name")),
        )
}

fn inherited_fixture_command() -> Command {
    Command::new("root")
        .id("root-id")
        .option(
            OptionSpec::count("verbose")
                .long("verbose")
                .short('v')
                .inherited(),
        )
        .option(
            OptionSpec::value("config")
                .long("config")
                .short('c')
                .environment("NAGI_CONFIG")
                .default_value("default")
                .inherited(),
        )
        .option(
            OptionSpec::value("tag")
                .long("tag")
                .short('t')
                .repeated()
                .inherited(),
        )
        .option(
            OptionSpec::value("jobs")
                .long("jobs")
                .parser(integer_parser())
                .default_value("1")
                .inherited(),
        )
        .option(OptionSpec::flag("root-only").long("root-only"))
        .subcommand(
            Command::new("run")
                .id("run-id")
                .option(OptionSpec::flag("dry-run").long("dry-run").short('n'))
                .option(
                    OptionSpec::value("profile")
                        .long("profile")
                        .short('p')
                        .inherited(),
                )
                .argument(Argument::new("args").repeated())
                .subcommand(Command::new("exec").id("exec-id")),
        )
}

fn inherited_validation_command(shape: &str) -> Command {
    match shape {
        "local-reuse" => Command::new("root")
            .option(OptionSpec::flag("root-value").long("same"))
            .subcommand(Command::new("child").option(OptionSpec::flag("child-value").long("same"))),
        "ancestor-local-child-inherited" => Command::new("root")
            .option(OptionSpec::flag("root-value").long("same"))
            .subcommand(
                Command::new("child")
                    .option(OptionSpec::flag("child-value").long("same").inherited()),
            ),
        "ancestor-inherited-child-local-long" => Command::new("root")
            .option(OptionSpec::flag("root-value").long("same").inherited())
            .subcommand(Command::new("child").option(OptionSpec::flag("child-value").long("same"))),
        "ancestor-inherited-child-local-short" => Command::new("root")
            .option(OptionSpec::flag("root-value").short('s').inherited())
            .subcommand(Command::new("child").option(OptionSpec::flag("child-value").short('s'))),
        "ancestor-inherited-child-inherited" => Command::new("root")
            .option(OptionSpec::flag("root-value").long("same").inherited())
            .subcommand(
                Command::new("child")
                    .option(OptionSpec::flag("child-value").long("same").inherited()),
            ),
        "transitive-collision" => Command::new("root")
            .option(OptionSpec::flag("root-value").long("same").inherited())
            .subcommand(
                Command::new("child").subcommand(
                    Command::new("grandchild")
                        .option(OptionSpec::flag("grandchild-value").long("same")),
                ),
            ),
        "unrelated-siblings" => Command::new("root")
            .subcommand(
                Command::new("first")
                    .option(OptionSpec::flag("first-value").long("same").inherited()),
            )
            .subcommand(
                Command::new("second")
                    .option(OptionSpec::flag("second-value").long("same").inherited()),
            ),
        "same-id-different-spelling" => Command::new("root")
            .option(OptionSpec::flag("value").long("root-value").inherited())
            .subcommand(
                Command::new("child").option(OptionSpec::flag("value").long("child-value")),
            ),
        value => panic!("unknown inherited option validation shape {value}"),
    }
}

fn inherited_help_command() -> Command {
    Command::new("root")
        .id("root-id")
        .option(
            OptionSpec::count("verbose")
                .long("verbose")
                .short('v')
                .help("Increase verbosity")
                .inherited(),
        )
        .option(
            OptionSpec::value("config")
                .long("config")
                .short('c')
                .help("Configuration path")
                .inherited(),
        )
        .subcommand(
            Command::new("run")
                .id("run-id")
                .about("Run command")
                .option(
                    OptionSpec::flag("dry-run")
                        .long("dry-run")
                        .short('n')
                        .help("Dry run"),
                )
                .option(
                    OptionSpec::value("profile")
                        .long("profile")
                        .short('p')
                        .help("Execution profile")
                        .inherited(),
                )
                .subcommand(
                    Command::new("exec")
                        .id("exec-id")
                        .about("Execute command")
                        .option(
                            OptionSpec::flag("trace")
                                .long("trace")
                                .short('x')
                                .help("Trace execution"),
                        ),
                ),
        )
}

fn runtime_command() -> Command {
    Command::new("nagi")
        .about("Nagi fixture command")
        .version("1.2.3")
        .option(
            OptionSpec::count("verbose")
                .long("verbose")
                .short('v')
                .help("Increase verbosity"),
        )
        .option(
            OptionSpec::value("output")
                .long("output")
                .short('o')
                .parser(value_parser("PATH", |value: &OsStr| Ok(value.to_owned())))
                .help("Output path"),
        )
        .argument(Argument::new("input").help("Input value"))
        .argument(Argument::new("extra").repeated().help("Extra values"))
        .handler(|context: &mut Context, invocation: &Invocation| {
            if invocation.raw_value("input") == Some(OsStr::new("fail")) {
                return Err(Diagnostic::new(
                    DiagnosticCode::HandlerError,
                    "requested failure",
                ));
            }
            let command = invocation.command_path().join("/");
            let cwd = context.current_directory().display().to_string();
            let environment = context
                .environment(OsStr::new("NAGI_TEST"))
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_owned();
            let mut input = Vec::new();
            context
                .stdin()
                .read_to_end(&mut input)
                .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
            write!(
                context.stdout(),
                "command={command}\ncwd={cwd}\nenv={environment}\nstdin={}\n",
                String::from_utf8_lossy(&input)
            )
            .map_err(|error| Diagnostic::new(DiagnosticCode::IoError, error.to_string()))?;
            Ok(Outcome::success())
        })
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

fn environment(bytes: &[u8]) -> Vec<(OsString, OsString)> {
    if bytes.is_empty() {
        return Vec::new();
    }
    bytes
        .split(|byte| *byte == b'|')
        .map(|entry| {
            let separator = entry
                .iter()
                .position(|byte| *byte == b'=')
                .expect("fixture environment entry has equals sign");
            (
                OsString::from_vec(entry[..separator].to_vec()),
                OsString::from_vec(entry[separator + 1..].to_vec()),
            )
        })
        .collect()
}

fn snapshot_parse(result: ParseResult) -> String {
    match result {
        ParseResult::Help { command_path, .. } => {
            format!("action;kind=help;command={}", command_path.join("/"))
        }
        ParseResult::Version { version, .. } => format!("action;kind=version;value={version}"),
        ParseResult::Invocation(invocation) => snapshot_invocation(&invocation),
    }
}

fn snapshot_invocation(invocation: &Invocation) -> String {
    let mut values = Vec::new();
    for id in invocation.value_ids() {
        if invocation.flag(id).is_some() {
            values.push(format!("{id}=flag:true"));
        } else if let Some(count) = invocation.count(id) {
            values.push(format!("{id}=count:{count}"));
        } else if let Some(parsed) = invocation.parsed_values(id) {
            if !invocation.is_repeated(id) {
                values.push(format!(
                    "{id}=value:{}:{}",
                    source(parsed[0].source()),
                    hex(parsed[0].raw().as_bytes())
                ));
            } else {
                let joined = parsed
                    .iter()
                    .map(|value| {
                        format!("{}:{}", source(value.source()), hex(value.raw().as_bytes()))
                    })
                    .collect::<Vec<_>>()
                    .join("+");
                values.push(format!("{id}=values:{joined}"));
            }
        }
    }
    format!(
        "ok;command={};values={}",
        invocation.command_path().join("/"),
        values.join(",")
    )
}

fn snapshot_scoped_invocation(invocation: &Invocation) -> String {
    let scopes = invocation
        .scopes()
        .map(|scope| {
            let mut values = Vec::new();
            for id in scope.value_ids() {
                if scope.flag(id).is_some() {
                    values.push(format!("{id}=flag"));
                } else if let Some(count) = scope.count(id) {
                    values.push(format!("{id}=count:{count}"));
                } else if let Some(parsed) = scope.parsed_values(id) {
                    if !scope.is_repeated(id) {
                        values.push(format!(
                            "{id}=value:{}:{}",
                            source(parsed[0].source()),
                            hex(parsed[0].raw().as_bytes())
                        ));
                    } else {
                        let joined = parsed
                            .iter()
                            .map(|value| {
                                format!(
                                    "{}:{}",
                                    source(value.source()),
                                    hex(value.raw().as_bytes())
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("+");
                        values.push(format!("{id}=values:{joined}"));
                    }
                }
            }
            format!(
                "{}{{{}}}",
                scope.command_id_path().join("/"),
                values.join(",")
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    format!(
        "ok;command={};scopes={scopes}",
        invocation.command_path().join("/")
    )
}

fn snapshot_inherited_error(error: &Diagnostic) -> String {
    let targets = error
        .targets()
        .iter()
        .map(|target| {
            let kind = match target.kind() {
                nagi_cli::DiagnosticTargetKind::Option => "option",
                nagi_cli::DiagnosticTargetKind::Argument => "argument",
            };
            format!(
                "{kind}@{}:{}",
                target.command_id_path().join("/"),
                target.value_id()
            )
        })
        .collect::<Vec<_>>()
        .join("+");
    format!("error;code={};targets={targets}", error.code().as_str())
}

fn source(source: ValueSource) -> &'static str {
    match source {
        ValueSource::CommandLine => "cli",
        ValueSource::Environment => "env",
        ValueSource::Default => "default",
        ValueSource::External => "external",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

#[derive(Clone, Default)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl SharedWriter {
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }
}

impl Write for SharedWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
