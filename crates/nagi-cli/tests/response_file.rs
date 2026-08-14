//! Shared Response File conformance fixtures

mod support;

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::Cursor;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nagi_cli::{
    Diagnostic, DiagnosticCategory, DiagnosticCode, DiagnosticRenderer, DiagnosticTargetKind,
    JsonDiagnosticRenderer, ResponseFileLimits, ResponseFileOptions, ResponseFileReadRequest,
    ResponseFileReader, ValueResolution, expand_response_files,
};

#[test]
fn response_file_expansion_matches_shared_fixtures() {
    for record in support::load(
        "cli/response-file.txt",
        "cli-response-file",
        &[
            "argv", "files", "stdin", "options", "limits", "expected", "reads",
        ],
    ) {
        let mut reader = MemoryReader::new(parse_files(record.field("files")));
        let mut standard_input = Cursor::new(hex_bytes(record.field("stdin")));
        let options = parse_options(record.field("options"), record.field("limits"));
        let result = expand_response_files(
            parse_arguments(record.field("argv")),
            Path::new("/work"),
            &options,
            &mut reader,
            &mut standard_input,
        );
        assert_eq!(
            snapshot(result),
            record.field("expected"),
            "case {}",
            record.id
        );
        assert_eq!(
            reader
                .reads
                .iter()
                .map(|path| path.to_string_lossy())
                .collect::<Vec<_>>()
                .join(","),
            record.field("reads"),
            "case {} reads",
            record.id
        );
    }
}

#[test]
fn defaults_and_zero_limits_are_explicit() {
    let defaults = ResponseFileLimits::default();
    assert_eq!(defaults.max_depth(), 16);
    assert_eq!(defaults.max_sources(), 64);
    assert_eq!(defaults.max_source_bytes(), 8 * 1024 * 1024);
    assert_eq!(defaults.max_tokens(), 65_536);
    assert_eq!(defaults.max_token_bytes(), 8 * 1024 * 1024);
    assert!(!ResponseFileOptions::default().standard_input_enabled());

    let limits = defaults
        .with_max_depth(0)
        .with_max_sources(0)
        .with_max_source_bytes(0)
        .with_max_tokens(0)
        .with_max_token_bytes(0);
    assert_eq!(limits.max_depth(), 0);
    assert_eq!(limits.max_sources(), 0);
    assert_eq!(limits.max_source_bytes(), 0);
    assert_eq!(limits.max_tokens(), 0);
    assert_eq!(limits.max_token_bytes(), 0);
}

#[test]
fn ordinary_parser_preserves_leading_at_without_opt_in() {
    let command =
        nagi_cli::Command::new("root").argument(nagi_cli::Argument::new("value").required());
    let nagi_cli::ParseResult::Invocation(invocation) = command.parse(["@literal"]).unwrap() else {
        panic!("expected Invocation");
    };
    assert_eq!(invocation.raw_value("value"), Some(OsStr::new("@literal")));
}

#[test]
fn reader_is_lazy_and_receives_a_bounded_request() {
    let mut calls = 0;
    let mut reader = |request: &ResponseFileReadRequest<'_>| {
        calls += 1;
        assert_eq!(request.path(), Path::new("/work/args"));
        assert_eq!(request.read_limit(), 4);
        Ok(b"four".to_vec())
    };
    let options = ResponseFileOptions::default()
        .with_limits(ResponseFileLimits::default().with_max_source_bytes(3));
    let diagnostic = expand_response_files(
        ["plain", "@args"],
        "/work",
        &options,
        &mut reader,
        &mut std::io::empty(),
    )
    .unwrap_err();
    assert_eq!(diagnostic.code(), DiagnosticCode::ResponseFileLimit);
    assert_eq!(calls, 1);

    let mut unused = |_: &ResponseFileReadRequest<'_>| -> Result<Vec<u8>, Diagnostic> {
        panic!("reader ran without an include")
    };
    let arguments = expand_response_files(
        ["plain"],
        "/work",
        &ResponseFileOptions::default(),
        &mut unused,
        &mut std::io::empty(),
    )
    .unwrap();
    assert_eq!(arguments, [OsString::from("plain")]);
}

#[test]
fn response_file_target_uses_the_stable_json_shape() {
    let target = nagi_cli::DiagnosticTarget::response_file("args.txt")
        .with_command_id_path(vec!["must-not-apply".to_owned()]);
    assert!(target.command_id_path().is_empty());
    let diagnostic = Diagnostic::new(
        DiagnosticCode::ResponseFileSyntax,
        "response file has invalid syntax",
    )
    .with_target(target);
    assert_eq!(diagnostic.category(), DiagnosticCategory::Usage);
    assert_eq!(
        diagnostic.targets()[0].kind(),
        DiagnosticTargetKind::ResponseFile
    );
    assert_eq!(
        JsonDiagnosticRenderer.render_diagnostic(&diagnostic),
        "{\"schema\":\"nagi.cli.diagnostic.v1\",\"code\":\"response-file-syntax\",\"category\":\"usage\",\"message\":\"response file has invalid syntax\",\"command_path\":[],\"usage\":null,\"targets\":[{\"kind\":\"response-file\",\"command_id_path\":[],\"value_id\":\"args.txt\"}],\"hints\":[]}\n"
    );
}

#[test]
fn runtime_validates_the_graph_before_response_file_io() {
    let command = nagi_cli::Command::new("root")
        .option(nagi_cli::OptionSpec::flag("one").long("same"))
        .option(nagi_cli::OptionSpec::flag("two").long("same"));
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&calls);
    let mut context = nagi_cli::Context::new(
        std::io::empty(),
        std::io::sink(),
        std::io::sink(),
        std::iter::empty::<(&str, &str)>(),
        "/work",
    )
    .with_response_files(
        ResponseFileOptions::default(),
        move |_: &ResponseFileReadRequest<'_>| {
            captured.fetch_add(1, Ordering::Relaxed);
            Ok(Vec::new())
        },
    );
    let outcome = command.run(&mut context, ["@args"]).unwrap();
    assert_eq!(outcome.status(), nagi_cli::ExitStatus::USAGE);
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn process_options_compose_response_files_and_value_resolution() {
    let response_options = ResponseFileOptions::default().with_standard_input(true);
    let options = nagi_cli::ProcessOptions::default()
        .with_response_files(response_options)
        .with_value_resolver(|_: &nagi_cli::ValueResolutionRequest<'_>| {
            Ok(ValueResolution::unresolved())
        });
    assert_eq!(options.response_file_options(), Some(response_options));
    assert!(options.value_resolver().is_some());
    assert!(
        !options
            .policy()
            .render_diagnostic(&Diagnostic::new(DiagnosticCode::HandlerError, "failed"))
            .is_empty()
    );

    let options = options.without_response_files().without_value_resolver();
    assert!(options.response_file_options().is_none());
    assert!(options.value_resolver().is_none());
}

struct MemoryReader {
    files: BTreeMap<PathBuf, Vec<u8>>,
    reads: Vec<PathBuf>,
}

impl MemoryReader {
    fn new(files: BTreeMap<PathBuf, Vec<u8>>) -> Self {
        Self {
            files,
            reads: Vec::new(),
        }
    }
}

impl ResponseFileReader for MemoryReader {
    fn read(&mut self, request: &ResponseFileReadRequest<'_>) -> Result<Vec<u8>, Diagnostic> {
        self.reads.push(request.path().to_path_buf());
        if request.path() == Path::new("/work/custom.args") {
            return Err(Diagnostic::new(
                DiagnosticCode::application("source-read"),
                "custom reader failed",
            ));
        }
        let Some(bytes) = self.files.get(request.path()) else {
            return Err(Diagnostic::new(
                DiagnosticCode::ResponseFileIo,
                "could not read response file",
            ));
        };
        Ok(bytes[..bytes.len().min(request.read_limit())].to_vec())
    }
}

fn parse_options(options: &str, limits: &str) -> ResponseFileOptions {
    let mut result = ResponseFileOptions::default();
    match options {
        "default" => {}
        "stdin" => result = result.with_standard_input(true),
        other => panic!("unknown Response File options {other}"),
    }
    let mut parsed_limits = ResponseFileLimits::default();
    if limits != "default" {
        let (name, value) = limits.split_once('=').expect("fixture limit");
        let value = value.parse::<usize>().expect("numeric fixture limit");
        parsed_limits = match name {
            "depth" => parsed_limits.with_max_depth(value),
            "sources" => parsed_limits.with_max_sources(value),
            "source-bytes" => parsed_limits.with_max_source_bytes(value),
            "tokens" => parsed_limits.with_max_tokens(value),
            "token-bytes" => parsed_limits.with_max_token_bytes(value),
            other => panic!("unknown Response File limit {other}"),
        };
    }
    result.with_limits(parsed_limits)
}

fn parse_files(value: &str) -> BTreeMap<PathBuf, Vec<u8>> {
    if value.is_empty() {
        return BTreeMap::new();
    }
    value
        .split(',')
        .map(|entry| {
            let (path, contents) = entry.split_once(':').expect("fixture file entry");
            (
                PathBuf::from(OsString::from_vec(hex_bytes(path))),
                hex_bytes(contents),
            )
        })
        .collect()
}

fn parse_arguments(value: &str) -> Vec<OsString> {
    if value.is_empty() {
        return Vec::new();
    }
    value
        .split(',')
        .map(|value| OsString::from_vec(hex_bytes(value)))
        .collect()
}

fn snapshot(result: Result<Vec<OsString>, Diagnostic>) -> String {
    match result {
        Ok(arguments) => format!(
            "ok;argv={}",
            arguments
                .iter()
                .map(|argument| {
                    if argument.is_empty() {
                        "-".to_owned()
                    } else {
                        hex(argument.as_bytes())
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        ),
        Err(diagnostic) => {
            let target = diagnostic.targets().first().map_or("none", |target| {
                assert_eq!(target.kind(), DiagnosticTargetKind::ResponseFile);
                assert!(target.command_id_path().is_empty());
                assert!(!target.is_sensitive());
                assert!(target.value_origin().is_none());
                target.value_id()
            });
            format!(
                "error;code={};target={target};message={}",
                diagnostic.code().as_str(),
                diagnostic.message()
            )
        }
    }
}

fn hex_bytes(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0, "odd fixture hex");
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|digits| (hex_digit(digits[0]) << 4) | hex_digit(digits[1]))
        .collect()
}

fn hex_digit(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'A'..=b'F' => digit - b'A' + 10,
        _ => panic!("invalid fixture hex"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}
