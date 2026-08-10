use std::marker::PhantomData;

use nagi_text::{WidthProfile, text_width};
use nagi_tui::{ActionAvailability, BindingSupport, Node, ResolvedAction, Style};

/// Arrangement of key bindings in a [`Help`] view
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HelpMode {
    /// Renders bindings on one line
    #[default]
    Compact,
    /// Renders one aligned binding per line
    Full,
}

/// One key description shown by [`Help`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpBinding {
    key: String,
    description: String,
    enabled: bool,
}

impl HelpBinding {
    /// Creates an enabled key binding
    #[must_use]
    pub fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
            enabled: true,
        }
    }

    /// Replaces whether the binding is available
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Returns the user-facing key notation
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the action description
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Reports whether the action is available
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Visual styles used by a [`Help`] view
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HelpStyle {
    /// Style used by key notation
    pub key: Style,
    /// Style used by action descriptions
    pub description: Style,
    /// Style used between compact bindings
    pub separator: Style,
    /// Style merged over unavailable bindings when they are shown
    pub disabled: Style,
}

impl Default for HelpStyle {
    fn default() -> Self {
        Self {
            key: Style {
                bold: true,
                ..Style::default()
            },
            description: Style {
                dim: true,
                ..Style::default()
            },
            separator: Style {
                dim: true,
                ..Style::default()
            },
            disabled: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// Discoverable key bindings in compact or full form
pub struct Help<Message> {
    bindings: Vec<HelpBinding>,
    mode: HelpMode,
    separator: String,
    show_disabled: bool,
    style: HelpStyle,
    message: PhantomData<fn() -> Message>,
}

impl<Message> Help<Message> {
    /// Creates compact help for the supplied bindings
    #[must_use]
    pub fn new(bindings: impl IntoIterator<Item = HelpBinding>) -> Self {
        Self {
            bindings: bindings.into_iter().collect(),
            mode: HelpMode::Compact,
            separator: " • ".to_owned(),
            show_disabled: false,
            style: HelpStyle::default(),
            message: PhantomData,
        }
    }

    /// Creates compact help from Help-visible resolved actions
    ///
    /// Each effective key becomes one binding in action and binding order
    /// without duplicating key notation. Unavailable actions and bindings with
    /// unsupported terminal metadata become disabled Help bindings
    #[must_use]
    pub fn from_resolved_actions<'a>(
        actions: impl IntoIterator<Item = &'a ResolvedAction>,
    ) -> Self {
        let mut bindings = Vec::new();
        for action in actions {
            if !action.is_help_visible() {
                continue;
            }
            let action_enabled = action.availability() == ActionAvailability::Enabled;
            bindings.extend(action.bindings().iter().map(|binding| HelpBinding {
                key: binding.stroke().notation(),
                description: action.label().to_owned(),
                enabled: action_enabled && binding.support() != BindingSupport::Unsupported,
            }));
        }
        Self::new(bindings)
    }

    /// Replaces the binding arrangement
    #[must_use]
    pub const fn mode(mut self, mode: HelpMode) -> Self {
        self.mode = mode;
        self
    }

    /// Replaces text between compact bindings
    #[must_use]
    pub fn separator(mut self, separator: impl Into<String>) -> Self {
        self.separator = separator.into();
        self
    }

    /// Sets whether unavailable bindings remain visible
    #[must_use]
    pub const fn show_disabled(mut self, show: bool) -> Self {
        self.show_disabled = show;
        self
    }

    /// Replaces the help styles
    #[must_use]
    pub const fn style(mut self, style: HelpStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this help view
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let bindings: Vec<_> = self
            .bindings
            .into_iter()
            .filter(|binding| binding.enabled || self.show_disabled)
            .collect();
        if self.mode == HelpMode::Full {
            return full_node(bindings, self.style);
        }
        let mut parts = Vec::with_capacity(bindings.len().saturating_mul(4));
        for (index, binding) in bindings.into_iter().enumerate() {
            if index > 0 {
                parts.push(Node::styled_text(&self.separator, self.style.separator));
            }
            let (mut key_style, mut description_style) = (self.style.key, self.style.description);
            if !binding.enabled {
                key_style = key_style.merged(self.style.disabled);
                description_style = description_style.merged(self.style.disabled);
            }
            parts.push(Node::styled_text(binding.key, key_style));
            parts.push(Node::text(" "));
            parts.push(Node::styled_text(binding.description, description_style));
        }
        Node::row(parts)
    }
}

fn full_node<Message>(bindings: Vec<HelpBinding>, style: HelpStyle) -> Node<Message> {
    let key_width = bindings
        .iter()
        .map(|binding| text_width(&binding.key, WidthProfile::MODERN))
        .max()
        .unwrap_or(0);
    Node::column(bindings.into_iter().map(|binding| {
        let (mut key_style, mut description_style) = (style.key, style.description);
        if !binding.enabled {
            key_style = key_style.merged(style.disabled);
            description_style = description_style.merged(style.disabled);
        }
        let padding = key_width.saturating_sub(text_width(&binding.key, WidthProfile::MODERN));
        Node::row([
            Node::styled_text(format!("{}{}", binding.key, " ".repeat(padding)), key_style),
            Node::text("  "),
            Node::styled_text(binding.description, description_style),
        ])
    }))
}

#[cfg(test)]
mod tests {
    use nagi_tui::{
        ActionDescriptor, BindingSupport, KeyBinding, KeyStroke, Modifiers, NodeId, resolve_actions,
    };

    use super::*;

    #[test]
    fn resolved_help_filters_hidden_actions_and_disables_unsupported_bindings() {
        let visible = ActionDescriptor::new(
            "app.visible",
            "Visible",
            [
                KeyBinding::new(KeyStroke::character('u', Modifiers::NONE))
                    .with_support(BindingSupport::Unsupported),
                KeyBinding::new(KeyStroke::character('v', Modifiers::NONE)),
            ],
        );
        let hidden = ActionDescriptor::new(
            "app.hidden",
            "Hidden",
            [KeyBinding::new(KeyStroke::character('h', Modifiers::NONE))],
        )
        .with_help_visible(false);
        let resolved = resolve_actions(&NodeId::from("owner"), &[visible, hidden], &[]).unwrap();

        let help = Help::<()>::from_resolved_actions(resolved.actions());

        assert_eq!(
            help.bindings
                .iter()
                .map(|binding| (binding.key(), binding.description(), binding.is_enabled()))
                .collect::<Vec<_>>(),
            [("u", "Visible", false), ("v", "Visible", true)]
        );
    }
}
