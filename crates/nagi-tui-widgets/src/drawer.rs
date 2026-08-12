use std::sync::Arc;

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, EventResult, Length, ModalFocusOptions,
    ModalInitialFocus, ModalReturnFocus, Node, NodeId, Style,
};

use crate::dismiss_action_descriptor;

/// Viewport edge from which a [`Drawer`] is presented
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum DrawerSide {
    /// Present the drawer against the left edge
    #[default]
    Left,
    /// Present the drawer against the right edge
    Right,
    /// Present the drawer against the top edge
    Top,
    /// Present the drawer against the bottom edge
    Bottom,
}

/// Visual styles used by a [`Drawer`]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DrawerStyle {
    /// Style used by the drawer border
    pub border: Style,
}

/// A controlled edge overlay with a lazily constructed body
///
/// The base remains present while the drawer is open. Modal drawers restrict
/// routing and focus to the body through the public Core modal node. Closed
/// drawers do not invoke their body builder or add a semantic subtree
pub struct Drawer<Message> {
    id: NodeId,
    base: Node<Message>,
    open: bool,
    side: DrawerSide,
    size: Length,
    style: DrawerStyle,
    modal: bool,
    focus: ModalFocusOptions,
    on_dismiss: Option<Arc<dyn Fn() -> Message>>,
    body: Option<Box<dyn FnOnce() -> Node<Message>>>,
}

impl<Message: 'static> Drawer<Message> {
    /// Creates a left-side modal drawer whose body occupies 40 percent
    #[must_use]
    pub fn new(id: impl Into<NodeId>, base: Node<Message>, open: bool) -> Self {
        Self {
            id: id.into(),
            base,
            open,
            side: DrawerSide::Left,
            size: Length::Percent(40),
            style: DrawerStyle::default(),
            modal: true,
            focus: ModalFocusOptions::default(),
            on_dismiss: None,
            body: None,
        }
    }

    /// Sets the lazy drawer body builder
    ///
    /// The final builder is invoked once by [`Drawer::into_node`] only while
    /// the controlled drawer is open
    #[must_use]
    pub fn body(mut self, builder: impl FnOnce() -> Node<Message> + 'static) -> Self {
        self.body = Some(Box::new(builder));
        self
    }

    /// Sets the viewport edge used by the drawer
    #[must_use]
    pub const fn side(mut self, side: DrawerSide) -> Self {
        self.side = side;
        self
    }

    /// Sets the drawer main-axis size
    #[must_use]
    pub const fn size(mut self, size: Length) -> Self {
        self.size = size;
        self
    }

    /// Replaces the drawer border style
    #[must_use]
    pub const fn style(mut self, style: DrawerStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets whether an open drawer restricts routing and focus to its subtree
    #[must_use]
    pub const fn modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }

    /// Sets the focus policy used when a modal drawer opens
    #[must_use]
    pub fn initial_focus(mut self, focus: ModalInitialFocus) -> Self {
        self.focus.initial = focus;
        self
    }

    /// Sets the focus policy used when a modal drawer closes
    #[must_use]
    pub fn return_focus(mut self, focus: ModalReturnFocus) -> Self {
        self.focus.return_focus = focus;
        self
    }

    /// Sets the message handler used by the semantic dismissal action
    #[must_use]
    pub fn on_dismiss(mut self, handler: impl Fn() -> Message + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(handler));
        self
    }

    /// Returns the semantic dismissal descriptor declared by an open drawer
    #[must_use]
    pub fn action_descriptor(&self) -> ActionDescriptor {
        dismiss_action_descriptor().with_availability(if self.open && self.on_dismiss.is_some() {
            ActionAvailability::Enabled
        } else {
            ActionAvailability::DisabledPassThrough
        })
    }

    /// Builds the public semantic node without constructing a closed body
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        if !self.open {
            return self.base;
        }

        let descriptor = self.action_descriptor();
        let content = self.body.map_or_else(|| Node::column([]), |body| body());
        let bordered = Node::border(content, self.style.border);
        let id = self.id;
        let mut drawer = if self.modal {
            Node::modal_with_focus(id.clone(), bordered, self.focus)
        } else {
            bordered.with_id(id.clone())
        };
        let action = match self.on_dismiss {
            Some(on_dismiss) => {
                Action::new(descriptor, move |_| EventResult::message(on_dismiss()))
            }
            None => Action::new(descriptor, |_| EventResult::ignored()),
        };
        drawer = drawer.on_actions(id, [action]).with_length(self.size);

        let filler = Node::spacer(0, 0).with_length(Length::Flex(1));
        let layer = match self.side {
            DrawerSide::Left => Node::row([drawer, filler]),
            DrawerSide::Right => Node::row([filler, drawer]),
            DrawerSide::Top => Node::column([drawer, filler]),
            DrawerSide::Bottom => Node::column([filler, drawer]),
        };
        Node::stack([self.base, layer])
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn closed_drawer_does_not_build_body() {
        let builds = Rc::new(Cell::new(0));
        let observed = Rc::clone(&builds);

        let _node = Drawer::<()>::new("drawer", Node::text("base"), false)
            .body(move || {
                observed.set(observed.get() + 1);
                Node::text("body")
            })
            .into_node();

        assert_eq!(builds.get(), 0);
    }

    #[test]
    fn open_drawer_builds_body_once() {
        let builds = Rc::new(Cell::new(0));
        let observed = Rc::clone(&builds);

        let _node = Drawer::<()>::new("drawer", Node::text("base"), true)
            .body(move || {
                observed.set(observed.get() + 1);
                Node::text("body")
            })
            .into_node();

        assert_eq!(builds.get(), 1);
    }
}
