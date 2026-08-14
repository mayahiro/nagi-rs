use std::sync::Arc;

use nagi_tui::{Action, ActionAvailability, ActionDescriptor, EventResult, Node, NodeId, Style};

use crate::activate_action_descriptor;
use crate::event::is_pointer_activation_event;

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
///
/// Keyboard activation declares [`crate::ACTIVATE_ACTION_ID`]. Left-button
/// press remains a raw pointer event so keyboard rebinding does not remove it
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

    /// Returns the semantic activation descriptor declared by this radio
    #[must_use]
    pub fn action_descriptor(&self) -> ActionDescriptor {
        activate_action_descriptor().with_availability(if self.enabled {
            ActionAvailability::Enabled
        } else {
            ActionAvailability::DisabledPassThrough
        })
    }

    /// Builds the public semantic node for this radio
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let marker = if self.selected { 'o' } else { ' ' };
        let content = format!("({marker}) {}", self.label);
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
        let selected = self.selected;
        let on_select = self.on_select;
        let on_pointer_select = Arc::clone(&on_select);
        Node::styled_text(content, self.style.normal)
            .focusable(id.clone())
            .with_focused_style(self.style.focused)
            .on_actions(
                id.clone(),
                [Action::new(descriptor, move |_| {
                    selection_result(selected, &action_focus_id, on_select.as_ref())
                })],
            )
            .on_event(id, move |event| {
                if !is_pointer_activation_event(event) {
                    return EventResult::ignored();
                }
                selection_result(selected, &pointer_focus_id, on_pointer_select.as_ref())
            })
    }
}

fn selection_result<Message>(
    selected: bool,
    focus_id: &NodeId,
    on_select: &dyn Fn() -> Message,
) -> EventResult<Message> {
    let result = EventResult::consumed().focus(focus_id.clone());
    if selected {
        result
    } else {
        result.emit(on_select())
    }
}
