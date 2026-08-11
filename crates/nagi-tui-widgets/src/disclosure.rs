use std::sync::{Arc, LazyLock};

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, EventResult, KeyCode, Node, NodeId, Style,
};

use crate::{
    COLLAPSE_ACTION_ID, EXPAND_ACTION_ID, action::repeatable_action_binding,
    activate_action_descriptor, event::is_pointer_activation_event,
};

/// Visual styles used by a [`Disclosure`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisclosureStyle {
    /// Style used by the disclosure marker while enabled
    pub marker: Style,
    /// Style merged over the summary header while it owns focus
    pub focused: Style,
    /// Style used by the disclosure marker while disabled
    pub disabled: Style,
}

impl Default for DisclosureStyle {
    fn default() -> Self {
        Self {
            marker: Style::default(),
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

/// A controlled summary and lazily constructed detail subtree
///
/// The summary header owns focus, semantic actions, and pointer toggling. The
/// supplied summary is therefore display-only. The body builder is called only
/// while `expanded` is true, and focused body descendants return to the header
/// when a later frame collapses the disclosure
pub struct Disclosure<Message> {
    id: NodeId,
    summary: Node<Message>,
    expanded: bool,
    enabled: bool,
    style: DisclosureStyle,
    on_toggle: Arc<dyn Fn(bool) -> Message>,
    body: Option<Box<dyn FnOnce() -> Node<Message>>>,
}

impl<Message: 'static> Disclosure<Message> {
    /// Creates an enabled controlled disclosure without a body builder
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        summary: Node<Message>,
        expanded: bool,
        on_toggle: impl Fn(bool) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            summary,
            expanded,
            enabled: true,
            style: DisclosureStyle::default(),
            on_toggle: Arc::new(on_toggle),
            body: None,
        }
    }

    /// Sets the lazy detail builder
    ///
    /// Replacing the builder does not construct either body. The final builder
    /// is invoked once by [`Disclosure::into_node`] only when expanded
    #[must_use]
    pub fn body(mut self, builder: impl FnOnce() -> Node<Message> + 'static) -> Self {
        self.body = Some(Box::new(builder));
        self
    }

    /// Sets whether the summary can receive focus and request state changes
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces marker and focus styles
    #[must_use]
    pub const fn style(mut self, style: DisclosureStyle) -> Self {
        self.style = style;
        self
    }

    /// Returns activate, collapse, and expand descriptors in semantic order
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; 3] {
        disclosure_action_descriptors(self.enabled, self.expanded)
    }

    /// Builds the public semantic node without constructing a collapsed body
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let descriptors = self.action_descriptors();
        let marker = if self.expanded { "▼ " } else { "▶ " };
        let marker_style = if self.enabled {
            self.style.marker
        } else {
            self.style.disabled
        };
        let header = Node::row([Node::styled_text(marker, marker_style), self.summary]);
        let id = self.id;
        let header = if self.enabled {
            let activate_id = id.clone();
            let collapse_id = id.clone();
            let expand_id = id.clone();
            let pointer_id = id.clone();
            let on_activate = Arc::clone(&self.on_toggle);
            let on_collapse = Arc::clone(&self.on_toggle);
            let on_expand = Arc::clone(&self.on_toggle);
            let on_pointer = Arc::clone(&self.on_toggle);
            let expanded = self.expanded;
            header
                .focusable(id.clone())
                .with_focused_style(self.style.focused)
                .on_actions(
                    id.clone(),
                    [
                        Action::new(descriptors[0].clone(), move |_| {
                            EventResult::message(on_activate(!expanded)).focus(activate_id.clone())
                        }),
                        Action::new(descriptors[1].clone(), move |_| {
                            EventResult::message(on_collapse(false)).focus(collapse_id.clone())
                        }),
                        Action::new(descriptors[2].clone(), move |_| {
                            EventResult::message(on_expand(true)).focus(expand_id.clone())
                        }),
                    ],
                )
                .on_event(id.clone(), move |event| {
                    if is_pointer_activation_event(event) {
                        EventResult::message(on_pointer(!expanded)).focus(pointer_id.clone())
                    } else {
                        EventResult::ignored()
                    }
                })
        } else {
            header.with_id(id.clone()).on_actions(
                id.clone(),
                descriptors.map(|descriptor| Action::new(descriptor, |_| EventResult::ignored())),
            )
        };

        let mut children =
            Vec::with_capacity(1 + usize::from(self.expanded && self.body.is_some()));
        children.push(header);
        if self.expanded {
            if let Some(body) = self.body {
                children.push(body().focus_fallback(id));
            }
        }
        Node::column(children)
    }
}

static DISCLOSURE_DIRECTION_ACTION_DESCRIPTORS: LazyLock<[ActionDescriptor; 2]> =
    LazyLock::new(|| {
        [
            ActionDescriptor::new(
                COLLAPSE_ACTION_ID,
                "Collapse",
                [repeatable_action_binding(KeyCode::Left)],
            ),
            ActionDescriptor::new(
                EXPAND_ACTION_ID,
                "Expand",
                [repeatable_action_binding(KeyCode::Right)],
            ),
        ]
    });

fn disclosure_action_descriptors(enabled: bool, expanded: bool) -> [ActionDescriptor; 3] {
    let toggle = if enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    };
    let collapse = if enabled && expanded {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    };
    let expand = if enabled && !expanded {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    };
    [
        activate_action_descriptor().with_availability(toggle),
        DISCLOSURE_DIRECTION_ACTION_DESCRIPTORS[0]
            .clone()
            .with_availability(collapse),
        DISCLOSURE_DIRECTION_ACTION_DESCRIPTORS[1]
            .clone()
            .with_availability(expand),
    ]
}
