use std::sync::Arc;

use nagi_text::{WidthProfile, text_width};
use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, EventResult, HorizontalAlignment,
    ModalFocusOptions, ModalInitialFocus, ModalReturnFocus, Node, NodeId, Style, VerticalAlignment,
};

use crate::{
    Button, ButtonStyle, Disclosure, confirm_action_descriptor, dismiss_action_descriptor,
};

/// Visual styles used by a [`Dialog`]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DialogStyle {
    /// Style used by the dialog panel border
    pub border: Style,
}

/// One application-defined command presented by a [`Dialog`]
///
/// Dialog action labels are single-line text. The stable ID owns focus and
/// identifies this action when a dialog selects its default or cancel target
pub struct DialogAction<Message> {
    id: NodeId,
    label: String,
    enabled: bool,
    style: ButtonStyle,
    on_activate: Arc<dyn Fn() -> Message>,
}

impl<Message: 'static> DialogAction<Message> {
    /// Creates an enabled dialog action with the standard button styles
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

    /// Sets whether this action can receive focus and activate
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces this action's button styles
    #[must_use]
    pub const fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    /// Returns the stable Node ID used by this action
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns the single-line action label
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Reports whether this action can receive focus and activate
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn button_width(&self) -> u32 {
        u32::try_from(text_width(&self.label, WidthProfile::MODERN))
            .unwrap_or(u32::MAX)
            .saturating_add(4)
    }

    fn into_node(self) -> Node<Message> {
        let on_activate = self.on_activate;
        Button::new(self.id, self.label, move || on_activate())
            .enabled(self.enabled)
            .style(self.style)
            .into_node()
    }
}

/// A centered modal panel with application-defined content and actions
///
/// The application explicitly selects default and cancel action IDs. Missing
/// selections pass through, while configured IDs that do not name an enabled
/// action consume their semantic key so it cannot escape the dialog
pub struct Dialog<Message> {
    id: NodeId,
    title: Option<Node<Message>>,
    body: Node<Message>,
    details: Option<Disclosure<Message>>,
    actions: Vec<DialogAction<Message>>,
    default_action: Option<NodeId>,
    cancel_action: Option<NodeId>,
    initial_focus: Option<ModalInitialFocus>,
    return_focus: ModalReturnFocus,
    action_wrap_width: Option<u32>,
    style: DialogStyle,
}

impl<Message: 'static> Dialog<Message> {
    /// Creates an untitled dialog without default or cancel action selection
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        body: Node<Message>,
        actions: impl IntoIterator<Item = DialogAction<Message>>,
    ) -> Self {
        Self {
            id: id.into(),
            title: None,
            body,
            details: None,
            actions: actions.into_iter().collect(),
            default_action: None,
            cancel_action: None,
            initial_focus: None,
            return_focus: ModalReturnFocus::Previous,
            action_wrap_width: None,
            style: DialogStyle::default(),
        }
    }

    /// Sets the optional title slot
    #[must_use]
    pub fn title(mut self, title: Node<Message>) -> Self {
        self.title = Some(title);
        self
    }

    /// Sets the optional controlled and lazy details disclosure
    #[must_use]
    pub fn details(mut self, details: Disclosure<Message>) -> Self {
        self.details = Some(details);
        self
    }

    /// Selects the action invoked by the semantic confirmation action
    #[must_use]
    pub fn default_action(mut self, id: impl Into<NodeId>) -> Self {
        self.default_action = Some(id.into());
        self
    }

    /// Selects the action invoked by the semantic dismissal action
    #[must_use]
    pub fn cancel_action(mut self, id: impl Into<NodeId>) -> Self {
        self.cancel_action = Some(id.into());
        self
    }

    /// Sets the focus policy used when this dialog becomes active
    ///
    /// Without this override, a configured default action is targeted and a
    /// dialog without one selects the first focusable node
    #[must_use]
    pub fn initial_focus(mut self, focus: ModalInitialFocus) -> Self {
        self.initial_focus = Some(focus);
        self
    }

    /// Sets the focus policy used when this dialog stops being active
    #[must_use]
    pub fn return_focus(mut self, focus: ModalReturnFocus) -> Self {
        self.return_focus = focus;
        self
    }

    /// Sets the maximum terminal Cell width used by each action row
    ///
    /// Actions keep input order and greedily wrap with one Cell between them.
    /// Zero is normalized to one, and an oversized action occupies its own row
    #[must_use]
    pub fn action_wrap_width(mut self, width: u32) -> Self {
        self.action_wrap_width = Some(width.max(1));
        self
    }

    /// Replaces the dialog panel styles
    #[must_use]
    pub const fn style(mut self, style: DialogStyle) -> Self {
        self.style = style;
        self
    }

    /// Returns confirmation and dismissal descriptors in semantic order
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; 2] {
        [
            confirm_action_descriptor()
                .with_availability(self.target_availability(self.default_action.as_ref())),
            dismiss_action_descriptor()
                .with_availability(self.target_availability(self.cancel_action.as_ref())),
        ]
    }

    /// Builds the public semantic node for this dialog
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let descriptors = self.action_descriptors();
        let on_default = self.target_handler(self.default_action.as_ref());
        let on_cancel = self.target_handler(self.cancel_action.as_ref());
        let initial = self.initial_focus.unwrap_or_else(|| {
            self.default_action
                .clone()
                .map_or(ModalInitialFocus::First, ModalInitialFocus::Target)
        });
        let focus = ModalFocusOptions {
            initial,
            return_focus: self.return_focus,
        };

        let mut children = Vec::with_capacity(
            2 + usize::from(self.title.is_some()) + usize::from(self.details.is_some()),
        );
        if let Some(title) = self.title {
            children.push(title);
        }
        children.push(self.body);
        if let Some(details) = self.details {
            children.push(details.into_node());
        }
        if !self.actions.is_empty() {
            children.push(dialog_action_rows(self.actions, self.action_wrap_width));
        }

        let panel = Node::border(Node::column(children), self.style.border);
        let centered = Node::align(
            panel,
            HorizontalAlignment::Center,
            VerticalAlignment::Center,
        );
        let id = self.id;
        Node::modal_with_focus(id.clone(), centered, focus).on_actions(
            id,
            [
                Action::new(descriptors[0].clone(), move |_| match &on_default {
                    Some(handler) => EventResult::message(handler()),
                    None => EventResult::ignored(),
                }),
                Action::new(descriptors[1].clone(), move |_| match &on_cancel {
                    Some(handler) => EventResult::message(handler()),
                    None => EventResult::ignored(),
                }),
            ],
        )
    }

    fn target_availability(&self, target: Option<&NodeId>) -> ActionAvailability {
        let Some(target) = target else {
            return ActionAvailability::DisabledPassThrough;
        };
        if self
            .actions
            .iter()
            .any(|action| action.id == *target && action.enabled)
        {
            ActionAvailability::Enabled
        } else {
            ActionAvailability::DisabledConsume
        }
    }

    fn target_handler(&self, target: Option<&NodeId>) -> Option<Arc<dyn Fn() -> Message>> {
        let target = target?;
        self.actions
            .iter()
            .find(|action| action.id == *target && action.enabled)
            .map(|action| Arc::clone(&action.on_activate))
    }

    fn action_style(mut self, target: &NodeId, style: ButtonStyle) -> Self {
        if let Some(action) = self.actions.iter_mut().find(|action| action.id == *target) {
            action.style = style;
        }
        self
    }
}

/// The explicit default action selected for a [`ConfirmDialog`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfirmDialogDefault {
    /// Make the confirm action the default
    Confirm,
    /// Make the cancel action the default
    Cancel,
}

/// A two-action convenience over [`Dialog`]
///
/// The constructor requires an explicit default selection. Three or more
/// choices belong in a generic [`Dialog`] action list
pub struct ConfirmDialog<Message> {
    dialog: Dialog<Message>,
    confirm_id: NodeId,
}

impl<Message: 'static> ConfirmDialog<Message> {
    /// Creates a confirm-and-cancel dialog with an explicit default action
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        body: Node<Message>,
        confirm: DialogAction<Message>,
        cancel: DialogAction<Message>,
        default: ConfirmDialogDefault,
    ) -> Self {
        let confirm_id = confirm.id.clone();
        let cancel_id = cancel.id.clone();
        let default_id = match default {
            ConfirmDialogDefault::Confirm => confirm_id.clone(),
            ConfirmDialogDefault::Cancel => cancel_id.clone(),
        };
        Self {
            dialog: Dialog::new(id, body, [confirm, cancel])
                .default_action(default_id)
                .cancel_action(cancel_id),
            confirm_id,
        }
    }

    /// Sets the optional title slot
    #[must_use]
    pub fn title(mut self, title: Node<Message>) -> Self {
        self.dialog = self.dialog.title(title);
        self
    }

    /// Sets the optional controlled and lazy details disclosure
    #[must_use]
    pub fn details(mut self, details: Disclosure<Message>) -> Self {
        self.dialog = self.dialog.details(details);
        self
    }

    /// Applies an application-supplied destructive style to the confirm action
    #[must_use]
    pub fn destructive_style(mut self, style: ButtonStyle) -> Self {
        self.dialog = self.dialog.action_style(&self.confirm_id, style);
        self
    }

    /// Sets the focus policy used when this dialog becomes active
    #[must_use]
    pub fn initial_focus(mut self, focus: ModalInitialFocus) -> Self {
        self.dialog = self.dialog.initial_focus(focus);
        self
    }

    /// Sets the focus policy used when this dialog stops being active
    #[must_use]
    pub fn return_focus(mut self, focus: ModalReturnFocus) -> Self {
        self.dialog = self.dialog.return_focus(focus);
        self
    }

    /// Sets the maximum terminal Cell width used by each action row
    #[must_use]
    pub fn action_wrap_width(mut self, width: u32) -> Self {
        self.dialog = self.dialog.action_wrap_width(width);
        self
    }

    /// Replaces the dialog panel styles
    #[must_use]
    pub fn style(mut self, style: DialogStyle) -> Self {
        self.dialog = self.dialog.style(style);
        self
    }

    /// Returns confirmation and dismissal descriptors in semantic order
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; 2] {
        self.dialog.action_descriptors()
    }

    /// Builds the public semantic node for this confirm dialog
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        self.dialog.into_node()
    }
}

fn dialog_action_rows<Message: 'static>(
    actions: Vec<DialogAction<Message>>,
    wrap_width: Option<u32>,
) -> Node<Message> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut used = 0_u32;

    for action in actions {
        let width = action.button_width();
        let required = if row.is_empty() {
            width
        } else {
            used.saturating_add(1).saturating_add(width)
        };
        if !row.is_empty() && wrap_width.is_some_and(|limit| required > limit) {
            rows.push(Node::row(row));
            row = Vec::new();
            used = 0;
        }
        if !row.is_empty() {
            row.push(Node::gap(1));
            used = used.saturating_add(1);
        }
        row.push(action.into_node());
        used = used.saturating_add(width);
    }
    if !row.is_empty() {
        rows.push(Node::row(row));
    }
    Node::column(rows)
}
