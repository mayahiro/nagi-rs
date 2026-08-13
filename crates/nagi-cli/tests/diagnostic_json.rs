//! Shared JSON Diagnostic Renderer conformance tests

mod support;

use std::io::{Cursor, Write};
use std::sync::{Arc, Mutex};

use nagi_cli::{
    Argument, Command, Context, Diagnostic, DiagnosticCategory, DiagnosticCode, DiagnosticRenderer,
    DiagnosticTarget, ExitStatus, JSON_DIAGNOSTIC_SCHEMA, JsonDiagnosticRenderer, RuntimePolicy,
    cancellation_pair,
};

#[test]
fn json_diagnostic_renderer_matches_shared_fixtures() {
    for record in support::load(
        "cli/diagnostic-json.txt",
        "cli-diagnostic-json",
        &["arrangement", "expected"],
    ) {
        let diagnostic = arrangement(record.field("arrangement"));
        assert_eq!(
            JsonDiagnosticRenderer.render_diagnostic(&diagnostic),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

#[test]
fn runtime_policy_writes_json_without_changing_status_meaning() {
    let command = Command::new("root").argument(Argument::new("value").required());
    let stdout = SharedWriter::default();
    let stderr = SharedWriter::default();
    let (token, _handle) = cancellation_pair();
    let mut context = Context::with_cancellation(
        Cursor::new(Vec::<u8>::new()),
        stdout,
        stderr.clone(),
        Vec::<(String, String)>::new(),
        ".",
        token,
    );
    let policy = RuntimePolicy::default().with_diagnostic_renderer(JsonDiagnosticRenderer);
    let outcome = command
        .run_with_policy(&mut context, Vec::<String>::new(), &policy)
        .expect("runtime JSON rendering must succeed");
    assert_eq!(outcome.status(), ExitStatus::USAGE);
    let rendered = String::from_utf8(stderr.bytes()).expect("JSON must be UTF-8");
    assert!(rendered.starts_with(&format!(
        "{{\"schema\":\"{JSON_DIAGNOSTIC_SCHEMA}\",\"code\":\"missing-required\""
    )));
    assert!(rendered.ends_with("\n"));
    assert!(rendered.contains("\"command_path\":[\"root\"]"));
    assert!(rendered.contains("\"targets\":[{\"kind\":\"argument\""));
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

fn arrangement(name: &str) -> Diagnostic {
    match name {
        "minimal" => Diagnostic::new(DiagnosticCode::HandlerError, "failed"),
        "complete" => Diagnostic::new(
            DiagnosticCode::application("profile-blocked"),
            "profile \"prod\" blocked",
        )
        .with_category(DiagnosticCategory::Usage)
        .with_command_path(vec!["nagi".to_owned(), "deploy".to_owned()])
        .with_usage("nagi deploy --profile <PROFILE>")
        .with_target(
            DiagnosticTarget::option("profile")
                .with_command_id_path(vec!["root".to_owned(), "deploy".to_owned()]),
        )
        .with_target(
            DiagnosticTarget::argument("target")
                .with_command_id_path(vec!["root".to_owned(), "deploy".to_owned()]),
        )
        .with_hint("choose staging")
        .with_hint("see https://example.com/a?x=1&y=2"),
        "escaping" => Diagnostic::new(
            DiagnosticCode::HandlerError,
            "line\n\t\"\\\u{0008}\u{000C}\r\u{0000}<>&/\u{2028}",
        )
        .with_command_path(vec!["a\\b".to_owned(), "日本".to_owned()])
        .with_usage("")
        .with_target(DiagnosticTarget::option("x\n"))
        .with_hint("\u{001F}"),
        value => panic!("unknown arrangement {value}"),
    }
}
