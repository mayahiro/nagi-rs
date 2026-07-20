use std::sync::Arc;

use nagi_tui::{EventResult, Node, NodeId, Style};

use crate::event::is_activation_event;

/// Visual styles used by a [`Radio`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RadioStyle {
    /// Style used while the radio is enabled and unfocused
    pub normal: Style,
    /// Style merged over the radio while it owns focus
    pub focused: Style,
    /// Style used while the radio is disabled
    pub disabled: Style,
}

impl Default for RadioStyle {
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

/// One controlled choice in an application-owned radio group
pub struct Radio<Message> {
    id: NodeId,
    label: String,
    selected: bool,
    enabled: bool,
    style: RadioStyle,
    on_select: Arc<dyn Fn() -> Message>,
}

impl<Message: 'static> Radio<Message> {
    /// Creates an enabled radio using application-owned selection state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        label: impl Into<String>,
        selected: bool,
        on_select: impl Fn() -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            selected,
            enabled: true,
            style: RadioStyle::default(),
            on_select: Arc::new(on_select),
        }
    }

    /// Sets whether the radio can receive focus and select itself
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the radio styles
    #[must_use]
    pub const fn style(mut self, style: RadioStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this radio
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let marker = if self.selected { 'o' } else { ' ' };
        let content = format!("({marker}) {}", self.label);
        if !self.enabled {
            return Node::styled_text(content, self.style.disabled).with_id(self.id);
        }

        let id = self.id;
        let focus_id = id.clone();
        let selected = self.selected;
        let on_select = self.on_select;
        Node::styled_text(content, self.style.normal)
            .focusable(id.clone())
            .with_focused_style(self.style.focused)
            .on_event(id, move |event| {
                if !is_activation_event(event) {
                    return EventResult::ignored();
                }
                let result = EventResult::consumed().focus(focus_id.clone());
                if selected {
                    result
                } else {
                    result.emit(on_select())
                }
            })
    }
}
