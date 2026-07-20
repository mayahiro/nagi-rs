use std::sync::Arc;

use nagi_tui::{EventResult, Node, NodeId, Style};

use crate::event::is_activation_event;

/// Visual styles used by a [`Button`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ButtonStyle {
    /// Style used while the button is enabled and unfocused
    pub normal: Style,
    /// Style merged over the button while it owns focus
    pub focused: Style,
    /// Style used while the button is disabled
    pub disabled: Style,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            normal: Style {
                bold: true,
                ..Style::default()
            },
            focused: Style {
                reverse: true,
                ..Style::default()
            },
            disabled: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// A focusable command that emits one message when activated
pub struct Button<Message> {
    id: NodeId,
    label: String,
    enabled: bool,
    style: ButtonStyle,
    on_activate: Arc<dyn Fn() -> Message>,
}

impl<Message: 'static> Button<Message> {
    /// Creates an enabled button with default styles
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        label: impl Into<String>,
        on_activate: impl Fn() -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
            style: ButtonStyle::default(),
            on_activate: Arc::new(on_activate),
        }
    }

    /// Sets whether the button can receive focus and activate
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the button styles
    #[must_use]
    pub const fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this button
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let content = format!("[ {} ]", self.label);
        if !self.enabled {
            return Node::styled_text(content, self.style.disabled).with_id(self.id);
        }

        let id = self.id;
        let handler_id = id.clone();
        let on_activate = self.on_activate;
        Node::styled_text(content, self.style.normal)
            .focusable(id.clone())
            .with_focused_style(self.style.focused)
            .on_event(id, move |event| {
                if is_activation_event(event) {
                    EventResult::message(on_activate()).focus(handler_id.clone())
                } else {
                    EventResult::ignored()
                }
            })
    }
}
