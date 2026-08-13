//! Renders one structured Diagnostic through the stable JSON Runtime Policy

use std::io::{self, Write};

use nagi_cli::{
    Diagnostic, DiagnosticCategory, DiagnosticCode, DiagnosticTarget, JsonDiagnosticRenderer,
    RuntimePolicy,
};

fn main() -> io::Result<()> {
    let diagnostic = Diagnostic::new(
        DiagnosticCode::application("profile-blocked"),
        "profile is not available",
    )
    .with_category(DiagnosticCategory::Usage)
    .with_command_path(vec!["nagi".to_owned(), "deploy".to_owned()])
    .with_usage("nagi deploy --profile <PROFILE>")
    .with_target(
        DiagnosticTarget::option("profile")
            .with_command_id_path(vec!["root".to_owned(), "deploy".to_owned()]),
    )
    .with_hint("choose an available profile");
    let policy = RuntimePolicy::default().with_diagnostic_renderer(JsonDiagnosticRenderer);
    io::stdout().write_all(policy.render_diagnostic(&diagnostic).as_bytes())
}
