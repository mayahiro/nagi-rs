use std::sync::Arc;

/// Replacement metadata for a deprecated Command or Option
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Deprecation {
    replacement: Arc<str>,
}

impl Deprecation {
    pub(crate) fn new(replacement: impl Into<String>) -> Self {
        Self {
            replacement: Arc::from(replacement.into()),
        }
    }

    /// Returns the application-provided replacement hint
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
}

/// Identifies whether a deprecation notice targets a Command or Option
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeprecationTargetKind {
    /// The selected Command was deprecated
    Command,
    /// A command-line Option occurrence was deprecated
    Option,
}

/// One non-fatal use of deprecated Command Graph syntax
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeprecationNotice {
    kind: DeprecationTargetKind,
    command_path: Vec<String>,
    command_id_path: Vec<String>,
    value_id: Option<String>,
    spelling: String,
    replacement: Deprecation,
}

impl DeprecationNotice {
    pub(crate) fn command(
        command_path: Vec<String>,
        command_id_path: Vec<String>,
        spelling: impl Into<String>,
        deprecation: &Deprecation,
    ) -> Self {
        Self {
            kind: DeprecationTargetKind::Command,
            command_path,
            command_id_path,
            value_id: None,
            spelling: spelling.into(),
            replacement: deprecation.clone(),
        }
    }

    pub(crate) fn option(
        command_path: Vec<String>,
        command_id_path: Vec<String>,
        value_id: impl Into<String>,
        spelling: impl Into<String>,
        deprecation: &Deprecation,
    ) -> Self {
        Self {
            kind: DeprecationTargetKind::Option,
            command_path,
            command_id_path,
            value_id: Some(value_id.into()),
            spelling: spelling.into(),
            replacement: deprecation.clone(),
        }
    }

    /// Returns whether this notice identifies a Command or Option
    pub const fn target_kind(&self) -> DeprecationTargetKind {
        self.kind
    }

    /// Returns the canonical selected command path when the syntax was used
    pub fn command_path(&self) -> &[String] {
        &self.command_path
    }

    /// Returns the stable path of the target Command or Option declaration
    pub fn command_id_path(&self) -> &[String] {
        &self.command_id_path
    }

    /// Returns the command-local option ID for an Option notice
    pub fn value_id(&self) -> Option<&str> {
        self.value_id.as_deref()
    }

    /// Returns the recognized argv spelling, or the canonical root name when
    /// the root Command itself is deprecated
    pub fn spelling(&self) -> &str {
        &self.spelling
    }

    /// Returns the application-provided replacement hint
    pub fn replacement(&self) -> &str {
        self.replacement.replacement()
    }
}

/// Renders one non-fatal deprecation notice
pub trait DeprecationNoticeRenderer: Send + Sync {
    /// Returns text with one final newline
    fn render_deprecation_notice(&self, notice: &DeprecationNotice) -> String;
}

/// Renders stable plain deprecation notice text
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlainDeprecationNoticeRenderer;

impl DeprecationNoticeRenderer for PlainDeprecationNoticeRenderer {
    fn render_deprecation_notice(&self, notice: &DeprecationNotice) -> String {
        let target = match notice.target_kind() {
            DeprecationTargetKind::Command => "command",
            DeprecationTargetKind::Option => "option",
        };
        format!(
            "warning[deprecated-{target}]: {target} '{}' is deprecated\nhint: use {}\n",
            notice.spelling(),
            notice.replacement()
        )
    }
}

pub(crate) fn valid_replacement(replacement: &str) -> bool {
    !replacement.is_empty() && !replacement.chars().any(char::is_control)
}
