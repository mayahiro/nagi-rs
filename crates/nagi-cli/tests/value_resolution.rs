//! Shared Value Resolver conformance fixtures

mod support;

use std::ffi::OsString;
use std::io::{empty, sink};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use nagi_cli::{
    Argument, Command, Context, Diagnostic, DiagnosticCategory, DiagnosticCode, DiagnosticRenderer,
    DiagnosticTargetKind, JsonDiagnosticRenderer, OptionSpec, Outcome, ParseResult,
    ValueResolution, ValueResolutionRequest, ValueSource, possible_values_parser,
};

#[test]
fn value_resolution_matches_shared_fixtures() {
    for record in support::load(
        "cli/value-resolution.txt",
        "cli-value-resolution",
        &["arrangement", "argv", "env", "expected", "calls"],
    ) {
        let arrangement = record.field("arrangement");
        let command = command_for(arrangement);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let captured_calls = Arc::clone(&calls);
        let resolver = move |request: &ValueResolutionRequest<'_>| {
            captured_calls.lock().unwrap().push(format!(
                "{}:{}",
                request.command_id_path().join("/"),
                request.value_id()
            ));
            resolve(arrangement, request)
        };
        let result = command.parse_with_value_resolver(
            arguments(record.field("argv")),
            environment(record.field("env")),
            &resolver,
        );
        assert_eq!(
            snapshot_result(result),
            record.field("expected"),
            "case {}",
            record.id
        );
        assert_eq!(
            calls.lock().unwrap().join("|"),
            record.field("calls"),
            "case {} calls",
            record.id
        );
    }
}

#[test]
fn request_exposes_selected_and_declaration_paths_without_raw_fallbacks() {
    let command = command_for("selected");
    let snapshot = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&snapshot);
    command
        .parse_with_value_resolver(
            ["run"],
            std::iter::empty::<(&str, &str)>(),
            &move |request: &ValueResolutionRequest<'_>| {
                captured.lock().unwrap().push(format!(
                    "selected={}:{};declaration={}:{};value={};repeated={};sensitive={}",
                    request.selected_command_path().join("/"),
                    request.selected_command_id_path().join("/"),
                    request.command_path().join("/"),
                    request.command_id_path().join("/"),
                    request.value_id(),
                    request.is_repeated(),
                    request.is_sensitive(),
                ));
                resolve("selected", request)
            },
        )
        .unwrap();
    assert_eq!(
        *snapshot.lock().unwrap(),
        [
            "selected=root/run:root-id/run-id;declaration=root:root-id;value=root-value;repeated=false;sensitive=false",
            "selected=root/run:root-id/run-id;declaration=root/run:root-id/run-id;value=profile;repeated=false;sensitive=false",
        ]
    );
}

#[test]
fn runtime_context_resolves_values_before_handler_execution() {
    let command = Command::new("root")
        .option(OptionSpec::value("profile").long("profile"))
        .handler(|context: &mut Context, invocation: &nagi_cli::Invocation| {
            assert!(context.value_resolver().is_some());
            let value = &invocation.parsed_values("profile").unwrap()[0];
            assert_eq!(value.raw(), "workspace");
            assert_eq!(value.source(), ValueSource::External);
            assert_eq!(value.origin().identity(), Some("project-config"));
            Ok(Outcome::success())
        });
    let mut context = Context::new(
        empty(),
        sink(),
        sink(),
        std::iter::empty::<(&str, &str)>(),
        ".",
    )
    .with_value_resolver(|_: &ValueResolutionRequest<'_>| {
        Ok(ValueResolution::replace("project-config", ["workspace"]))
    });

    let outcome = command
        .run(&mut context, std::iter::empty::<&str>())
        .unwrap();
    assert_eq!(outcome.status(), nagi_cli::ExitStatus::SUCCESS);
}

#[test]
fn resolver_skips_non_value_inputs_help_and_version() {
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&calls);
    let resolver = move |_: &ValueResolutionRequest<'_>| {
        captured.fetch_add(1, Ordering::Relaxed);
        Ok(ValueResolution::unresolved())
    };
    let non_values = Command::new("root")
        .option(OptionSpec::flag("force").long("force"))
        .option(OptionSpec::count("verbose").short('v'))
        .argument(Argument::new("input"));
    non_values
        .parse_with_value_resolver(
            ["--force", "-vv", "input"],
            std::iter::empty::<(&str, &str)>(),
            &resolver,
        )
        .unwrap();

    let terminal_actions = Command::new("root")
        .version("1.0.0")
        .option(OptionSpec::value("config").long("config"));
    for arguments in [["--help"], ["--version"]] {
        terminal_actions
            .parse_with_value_resolver(arguments, std::iter::empty::<(&str, &str)>(), &resolver)
            .unwrap();
    }
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn sensitive_debug_and_default_json_do_not_expose_external_identity() {
    let command = Command::new("root").option(
        OptionSpec::value("token")
            .long("token")
            .parser(possible_values_parser(["accepted"]))
            .sensitive(),
    );
    let valid = command
        .parse_with_value_resolver(
            std::iter::empty::<&str>(),
            std::iter::empty::<(&str, &str)>(),
            &|_: &ValueResolutionRequest<'_>| {
                Ok(ValueResolution::replace("private-profile", ["accepted"]))
            },
        )
        .unwrap();
    let ParseResult::Invocation(invocation) = valid else {
        panic!("expected invocation");
    };
    let debug = format!("{:?}", invocation.parsed_values("token").unwrap()[0]);
    assert!(!debug.contains("accepted"));
    assert!(!debug.contains("private-profile"));

    let resolution = ValueResolution::replace("private-profile", ["resolver-secret"]);
    let resolution_debug = format!("{resolution:?}");
    assert!(!resolution_debug.contains("resolver-secret"));
    assert!(!resolution_debug.contains("private-profile"));

    let diagnostic = command
        .parse_with_value_resolver(
            std::iter::empty::<&str>(),
            std::iter::empty::<(&str, &str)>(),
            &|_: &ValueResolutionRequest<'_>| {
                Ok(ValueResolution::replace("private-profile", ["rejected"]))
            },
        )
        .unwrap_err();
    let json = JsonDiagnosticRenderer.render_diagnostic(&diagnostic);
    assert!(!json.contains("private-profile"));
    assert!(!json.contains("origin"));
    assert!(!json.contains("rejected"));
}

fn command_for(arrangement: &str) -> Command {
    match arrangement {
        "repeated-replace" | "repeated-merge" => Command::new("root").id("root-id").option(
            OptionSpec::value("tag")
                .long("tag")
                .repeated()
                .default_value("base"),
        ),
        "selected" => Command::new("root")
            .id("root-id")
            .option(OptionSpec::value("root-value").long("root-value"))
            .subcommand(
                Command::new("unused")
                    .id("unused-id")
                    .option(OptionSpec::value("ignored").long("ignored")),
            )
            .subcommand(
                Command::new("run")
                    .id("run-id")
                    .option(OptionSpec::value("profile").long("profile")),
            ),
        "invalid-value" => Command::new("root").id("root-id").option(
            OptionSpec::value("config")
                .long("config")
                .parser(possible_values_parser(["valid"])),
        ),
        "sensitive-invalid" => Command::new("root").id("root-id").option(
            OptionSpec::value("token")
                .long("token")
                .parser(possible_values_parser(["valid"]))
                .sensitive(),
        ),
        _ => Command::new("root").id("root-id").option(
            OptionSpec::value("config")
                .long("config")
                .environment("NAGI_CONFIG")
                .default_value("default"),
        ),
    }
}

fn resolve(
    arrangement: &str,
    request: &ValueResolutionRequest<'_>,
) -> Result<ValueResolution, Diagnostic> {
    Ok(match arrangement {
        "single" => ValueResolution::replace("config-file", ["external"]),
        "unresolved" => ValueResolution::unresolved(),
        "repeated-replace" => ValueResolution::replace("config-file", ["one", "two"]),
        "repeated-merge" => ValueResolution::merge("config-file", ["one", "two"]),
        "selected" => match request.value_id() {
            "root-value" => ValueResolution::replace("project-config", ["root"]),
            "profile" => ValueResolution::replace("project-config", ["child"]),
            _ => ValueResolution::unresolved(),
        },
        "invalid-multiple" => ValueResolution::replace("config-file", ["one", "two"]),
        "invalid-merge" => ValueResolution::merge("config-file", ["one"]),
        "empty" => ValueResolution::replace("config-file", std::iter::empty::<&str>()),
        "invalid-source" => ValueResolution::replace("bad source", ["one"]),
        "raw-invalid-utf8" => {
            ValueResolution::replace("config-file", [OsString::from_vec(vec![0xff, 0xfe, b'A'])])
        }
        "invalid-value" => ValueResolution::replace("config-file", ["bad"]),
        "sensitive-invalid" => {
            assert!(request.is_sensitive());
            ValueResolution::replace("secret-store", ["bad"])
        }
        "resolver-error" => {
            return Err(Diagnostic::new(
                DiagnosticCode::application("config-load"),
                "configuration lookup failed",
            )
            .with_category(DiagnosticCategory::Execution));
        }
        other => panic!("unknown arrangement {other}"),
    })
}

fn snapshot_result(result: Result<ParseResult, Diagnostic>) -> String {
    match result {
        Ok(ParseResult::Invocation(invocation)) => {
            let scopes = invocation
                .scopes()
                .map(|scope| {
                    let values = scope
                        .value_ids()
                        .map(|id| {
                            let parsed = scope
                                .parsed_values(id)
                                .expect("value id")
                                .iter()
                                .map(snapshot_value)
                                .collect::<Vec<_>>()
                                .join("+");
                            format!("{id}={parsed}")
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    format!("{}{{{values}}}", scope.command_id_path().join("/"))
                })
                .collect::<Vec<_>>()
                .join("|");
            format!("ok;scopes={scopes}")
        }
        Ok(ParseResult::Help { .. }) => "action;kind=help".to_owned(),
        Ok(ParseResult::Version { .. }) => "action;kind=version".to_owned(),
        Err(diagnostic) => snapshot_diagnostic(&diagnostic),
    }
}

fn snapshot_value(value: &nagi_cli::ParsedValue) -> String {
    let source = match value.source() {
        ValueSource::CommandLine => "cli".to_owned(),
        ValueSource::Environment => format!(
            "env({})",
            value.origin().identity().expect("environment identity")
        ),
        ValueSource::Default => "default".to_owned(),
        ValueSource::External => format!(
            "external({})",
            value.origin().identity().expect("external identity")
        ),
    };
    format!("{source}:{}", hex(value.raw().as_bytes()))
}

fn snapshot_diagnostic(diagnostic: &Diagnostic) -> String {
    let target = diagnostic.targets().first().expect("fixture target");
    let kind = match target.kind() {
        DiagnosticTargetKind::Option => "option",
        DiagnosticTargetKind::Argument => "argument",
        DiagnosticTargetKind::ResponseFile => "response-file",
    };
    let origin = target.value_origin().map_or_else(
        || "none".to_owned(),
        |origin| match origin.source() {
            ValueSource::CommandLine => "cli".to_owned(),
            ValueSource::Environment => {
                format!("env({})", origin.identity().expect("environment identity"))
            }
            ValueSource::Default => "default".to_owned(),
            ValueSource::External => format!(
                "external({})",
                origin.identity().expect("external identity")
            ),
        },
    );
    format!(
        "error;code={};target={kind}@{}:{};origin={origin};sensitive={}",
        diagnostic.code().as_str(),
        target.command_id_path().join("/"),
        target.value_id(),
        target.is_sensitive(),
    )
}

fn arguments(value: &str) -> Vec<OsString> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split('|').map(OsString::from).collect()
    }
}

fn environment(value: &str) -> Vec<(OsString, OsString)> {
    if value.is_empty() {
        return Vec::new();
    }
    value
        .split('|')
        .map(|entry| {
            let (name, value) = entry.split_once('=').expect("fixture environment entry");
            (OsString::from(name), OsString::from(value))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}
