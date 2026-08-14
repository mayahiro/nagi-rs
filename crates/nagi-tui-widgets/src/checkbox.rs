use std::sync::Arc;

use nagi_tui::{Action, ActionAvailability, ActionDescriptor, EventResult, Node, NodeId, Style};

use crate::activate_action_descriptor;
use crate::event::is_pointer_activation_event;

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

/// A controlled Boolean input that emits its requested opposite value
///
/// Keyboard activation declares [`crate::ACTIVATE_ACTION_ID`]. Left-button
/// press remains a raw pointer event so keyboard rebinding does not remove it
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

    /// Returns the semantic activation descriptor declared by this checkbox
    #[must_use]
    pub fn action_descriptor(&self) -> ActionDescriptor {
        activate_action_descriptor().with_availability(if self.enabled {
            ActionAvailability::Enabled
        } else {
            ActionAvailability::DisabledPassThrough
        })
    }

    /// Builds the public semantic node for this checkbox
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let marker = if self.checked { 'x' } else { ' ' };
        let content = format!("[{marker}] {}", self.label);
        let descriptor = self.action_descriptor();
        if !self.enabled {
            let id = self.id;
            return Node::styled_text(content, self.style.disabled)
                .with_id(id.clone())
                .on_actions(id, [Action::new(descriptor, |_| EventResult::ignored())]);
        }

        let id = self.id;
        let action_focus_id = id.clone();
        let pointer_focus_id = id.clone();
        let checked = self.checked;
        let on_change = self.on_change;
        let on_pointer_change = Arc::clone(&on_change);
        Node::styled_text(content, self.style.normal)
            .focusable(id.clone())
            .with_focused_style(self.style.focused)
            .on_actions(
                id.clone(),
                [Action::new(descriptor, move |_| {
                    EventResult::message(on_change(!checked)).focus(action_focus_id.clone())
                })],
            )
            .on_event(id, move |event| {
                if is_pointer_activation_event(event) {
                    EventResult::message(on_pointer_change(!checked))
                        .focus(pointer_focus_id.clone())
                } else {
                    EventResult::ignored()
                }
            })
    }
}
