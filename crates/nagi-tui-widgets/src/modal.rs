use std::sync::Arc;

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, EventResult, HorizontalAlignment, Node, NodeId,
    Style, VerticalAlignment,
};

use crate::dismiss_action_descriptor;

/// Visual styles used by a [`Modal`]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModalStyle {
    /// Style used by the panel border
    pub border: Style,
    /// Style used by a non-empty title
    pub title: Style,
}

/// A centered panel that restricts routing and focus to its subtree
pub struct Modal<Message> {
    id: NodeId,
    child: Node<Message>,
    title: String,
    style: ModalStyle,
    on_escape: Option<Arc<dyn Fn() -> Message>>,
}

impl<Message: 'static> Modal<Message> {
    /// Creates an untitled modal panel
    #[must_use]
    pub fn new(id: impl Into<NodeId>, child: Node<Message>) -> Self {
        Self {
            id: id.into(),
            child,
            title: String::new(),
            style: ModalStyle::default(),
            on_escape: None,
        }
    }

    /// Sets the title rendered above the modal content
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Replaces the modal styles
    #[must_use]
    pub const fn style(mut self, style: ModalStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the message handler used by the semantic dismissal action
    #[must_use]
    pub fn on_escape(mut self, handler: impl Fn() -> Message + 'static) -> Self {
        self.on_escape = Some(Arc::new(handler));
        self
    }

    /// Returns the semantic dismissal descriptor declared by the modal root
    #[must_use]
    pub fn action_descriptor(&self) -> ActionDescriptor {
        dismiss_action_descriptor().with_availability(if self.on_escape.is_some() {
            ActionAvailability::Enabled
        } else {
            ActionAvailability::DisabledPassThrough
        })
    }

    /// Builds the public semantic node for this modal
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let descriptor = self.action_descriptor();
        let content = if self.title.is_empty() {
            self.child
        } else {
            Node::column([Node::styled_text(self.title, self.style.title), self.child])
        };
        let panel = Node::border(content, self.style.border);
        let centered = Node::align(
            panel,
            HorizontalAlignment::Center,
            VerticalAlignment::Center,
        );
        let id = self.id;
        let modal = Node::modal(id.clone(), centered);
        let action = match self.on_escape {
            Some(on_escape) => Action::new(descriptor, move |_| EventResult::message(on_escape())),
            None => Action::new(descriptor, |_| EventResult::ignored()),
        };
        modal.on_actions(id, [action])
    }
}
