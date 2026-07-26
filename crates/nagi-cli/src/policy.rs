use std::sync::Arc;

use crate::diagnostic::{Diagnostic, DiagnosticCategory, ExitStatus};
use crate::help::{HelpDocument, HelpRenderer, PlainHelpRenderer};

/// Maps semantic Diagnostic categories to process statuses
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExitCodePolicy {
    specification: ExitStatus,
    usage: ExitStatus,
    execution: ExitStatus,
    cancellation: ExitStatus,
    io: ExitStatus,
}

impl ExitCodePolicy {
    /// Returns a copy with one category mapping replaced
    pub const fn with_status(mut self, category: DiagnosticCategory, status: ExitStatus) -> Self {
        match category {
            DiagnosticCategory::Specification => self.specification = status,
            DiagnosticCategory::Usage => self.usage = status,
            DiagnosticCategory::Execution => self.execution = status,
            DiagnosticCategory::Cancellation => self.cancellation = status,
            DiagnosticCategory::Io => self.io = status,
        }
        self
    }

    /// Returns the process status for a semantic category
    pub const fn status_for(self, category: DiagnosticCategory) -> ExitStatus {
        match category {
            DiagnosticCategory::Specification => self.specification,
            DiagnosticCategory::Usage => self.usage,
            DiagnosticCategory::Execution => self.execution,
            DiagnosticCategory::Cancellation => self.cancellation,
            DiagnosticCategory::Io => self.io,
        }
    }
}

impl Default for ExitCodePolicy {
    fn default() -> Self {
        Self {
            specification: ExitStatus::USAGE,
            usage: ExitStatus::USAGE,
            execution: ExitStatus::FAILURE,
            cancellation: ExitStatus::CANCELLED,
            io: ExitStatus::FAILURE,
        }
    }
}

/// Renders one structured Diagnostic
pub trait DiagnosticRenderer: Send + Sync {
    /// Returns text with one final newline
    fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String;
}

/// Renders stable plain Diagnostic text
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlainDiagnosticRenderer {
    prefix: String,
    show_usage: bool,
}

impl PlainDiagnosticRenderer {
    /// Returns a copy using a prefix before the diagnostic code
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Returns a copy that includes or omits available usage text
    pub const fn with_usage(mut self, show: bool) -> Self {
        self.show_usage = show;
        self
    }
}

impl Default for PlainDiagnosticRenderer {
    fn default() -> Self {
        Self {
            prefix: "error".to_owned(),
            show_usage: true,
        }
    }
}

impl DiagnosticRenderer for PlainDiagnosticRenderer {
    fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String {
        let mut output = format!(
            "{}[{}]: {}\n",
            self.prefix,
            diagnostic.code().as_str(),
            diagnostic.message()
        );
        for hint in diagnostic.hints() {
            output.push_str("hint: ");
            output.push_str(hint);
            output.push('\n');
        }
        if self.show_usage {
            if let Some(usage) = diagnostic.usage() {
                output.push_str("usage: ");
                output.push_str(usage);
                output.push('\n');
            }
        }
        output
    }
}

/// Selects rendering and category-to-status mapping
#[derive(Clone)]
pub struct RuntimePolicy {
    exit_codes: ExitCodePolicy,
    help_renderer: Arc<dyn HelpRenderer>,
    diagnostic_renderer: Arc<dyn DiagnosticRenderer>,
}

impl RuntimePolicy {
    /// Returns a copy using the provided status mapping
    pub const fn with_exit_code_policy(mut self, policy: ExitCodePolicy) -> Self {
        self.exit_codes = policy;
        self
    }

    /// Returns a copy using the provided Help renderer
    pub fn with_help_renderer<R>(mut self, renderer: R) -> Self
    where
        R: HelpRenderer + 'static,
    {
        self.help_renderer = Arc::new(renderer);
        self
    }

    /// Returns a copy using the provided Diagnostic renderer
    pub fn with_diagnostic_renderer<R>(mut self, renderer: R) -> Self
    where
        R: DiagnosticRenderer + 'static,
    {
        self.diagnostic_renderer = Arc::new(renderer);
        self
    }

    /// Returns the configured exit-code mapping
    pub const fn exit_code_policy(&self) -> ExitCodePolicy {
        self.exit_codes
    }

    /// Renders one Help Document without writing to process output
    pub fn render_help(&self, document: &HelpDocument) -> String {
        self.help_renderer.render_help(document)
    }

    /// Renders one Diagnostic without writing to process output
    pub fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String {
        self.diagnostic_renderer.render_diagnostic(diagnostic)
    }

    /// Returns the configured process status for one Diagnostic
    pub fn status_for_diagnostic(&self, diagnostic: &Diagnostic) -> ExitStatus {
        self.exit_codes.status_for(diagnostic.category())
    }
}

impl Default for RuntimePolicy {
    fn default() -> Self {
        Self {
            exit_codes: ExitCodePolicy::default(),
            help_renderer: Arc::new(PlainHelpRenderer),
            diagnostic_renderer: Arc::new(PlainDiagnosticRenderer::default()),
        }
    }
}
