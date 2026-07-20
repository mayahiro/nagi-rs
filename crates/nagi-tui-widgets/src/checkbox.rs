use std::sync::Arc;

use nagi_tui::{EventResult, Node, NodeId, Style};

use crate::event::is_activation_event;

/// Visual styles used by a [`Checkbox`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckboxStyle {
    /// Style used while the checkbox is enabled and unfocused
    pub normal: Style,
    /// Style merged over the checkbox while it owns focus
    pub focused: Style,
    /// Style used while the checkbox is disabled
    pub disabled: Style,
}

impl Default for CheckboxStyle {
    fn default() -> Self {
        Self {
            normal: Style::default(),
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

/// A controlled Boolean input that emits its requested next value
pub struct Checkbox<Message> {
    id: NodeId,
    label: String,
    checked: bool,
    enabled: bool,
    style: CheckboxStyle,
    on_change: Arc<dyn Fn(bool) -> Message>,
}

impl<Message: 'static> Checkbox<Message> {
    /// Creates an enabled checkbox using application-owned checked state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        label: impl Into<String>,
        checked: bool,
        on_change: impl Fn(bool) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            checked,
            enabled: true,
            style: CheckboxStyle::default(),
            on_change: Arc::new(on_change),
        }
    }

    /// Sets whether the checkbox can receive focus and change value
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the checkbox styles
    #[must_use]
    pub const fn style(mut self, style: CheckboxStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this checkbox
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let marker = if self.checked { 'x' } else { ' ' };
        let content = format!("[{marker}] {}", self.label);
        if !self.enabled {
            return Node::styled_text(content, self.style.disabled).with_id(self.id);
        }

        let id = self.id;
        let focus_id = id.clone();
        let checked = self.checked;
        let on_change = self.on_change;
        Node::styled_text(content, self.style.normal)
            .focusable(id.clone())
            .with_focused_style(self.style.focused)
            .on_event(id, move |event| {
                if is_activation_event(event) {
                    EventResult::message(on_change(!checked)).focus(focus_id.clone())
                } else {
                    EventResult::ignored()
                }
            })
    }
}
