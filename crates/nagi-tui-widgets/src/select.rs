use std::sync::{Arc, LazyLock};

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, EventResult, KeyCode, Node, NodeId, Style,
};

use crate::action::{
    SELECTION_FIRST_ACTION_ID, SELECTION_FIRST_ACTION_LABEL, SELECTION_LAST_ACTION_ID,
    SELECTION_LAST_ACTION_LABEL, SELECTION_NEXT_ACTION_ID, SELECTION_NEXT_ACTION_LABEL,
    SELECTION_PREVIOUS_ACTION_ID, SELECTION_PREVIOUS_ACTION_LABEL, activate_action_descriptor,
    repeatable_action_binding,
};
use crate::event::is_pointer_activation_event;
use crate::navigation::{Navigation, navigate};

static SELECT_NAVIGATION_ACTION_DESCRIPTORS: LazyLock<[ActionDescriptor; 4]> =
    LazyLock::new(|| {
        [
            ActionDescriptor::new(
                SELECTION_PREVIOUS_ACTION_ID,
                SELECTION_PREVIOUS_ACTION_LABEL,
                [
                    repeatable_action_binding(KeyCode::Left),
                    repeatable_action_binding(KeyCode::Up),
                ],
            ),
            ActionDescriptor::new(
                SELECTION_NEXT_ACTION_ID,
                SELECTION_NEXT_ACTION_LABEL,
                [
                    repeatable_action_binding(KeyCode::Right),
                    repeatable_action_binding(KeyCode::Down),
                ],
            ),
            ActionDescriptor::new(
                SELECTION_FIRST_ACTION_ID,
                SELECTION_FIRST_ACTION_LABEL,
                [repeatable_action_binding(KeyCode::Home)],
            ),
            ActionDescriptor::new(
                SELECTION_LAST_ACTION_ID,
                SELECTION_LAST_ACTION_LABEL,
                [repeatable_action_binding(KeyCode::End)],
            ),
        ]
    });

/// Visual styles used by a [`Select`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectStyle {
    /// Style used while the selector is enabled and unfocused
    pub normal: Style,
    /// Style merged over the selector while it owns focus
    pub focused: Style,
    /// Style used when the selector is disabled or empty
    pub disabled: Style,
}

impl Default for SelectStyle {
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

/// A compact selector with semantic keyboard and raw pointer selection
///
/// Keyboard handling declares standard activation and selection actions.
/// Left-button press remains raw so keyboard rebinding does not remove it
pub struct Select<Message> {
    id: NodeId,
    options: Vec<String>,
    selected: usize,
    enabled: bool,
    placeholder: String,
    style: SelectStyle,
    on_select: Arc<dyn Fn(usize) -> Message>,
}

impl<Message: 'static> Select<Message> {
    /// Creates an enabled selector using application-owned selection state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        options: impl IntoIterator<Item = impl Into<String>>,
        selected: usize,
        on_select: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            options: options.into_iter().map(Into::into).collect(),
            selected,
            enabled: true,
            placeholder: "No options".to_owned(),
            style: SelectStyle::default(),
            on_select: Arc::new(on_select),
        }
    }

    /// Sets whether the selector can receive focus and change selection
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets the text displayed when there are no options
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Replaces the selector styles
    #[must_use]
    pub const fn style(mut self, style: SelectStyle) -> Self {
        self.style = style;
        self
    }

    /// Returns the ordered semantic action descriptors declared by this selector
    ///
    /// The order is activate, previous, next, first, and last. Every descriptor
    /// is disabled-pass-through when the selector is disabled or empty
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; 5] {
        select_action_descriptors(self.enabled && !self.options.is_empty())
    }

    /// Builds the public semantic node for this selector
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let selected = navigate(self.options.len(), self.selected, Navigation::Normalize);
        let content = selected.map_or_else(
            || format!("< {} >", self.placeholder),
            |index| format!("< {} >", self.options[index]),
        );
        let descriptors = self.action_descriptors();
        let Some(selected) = selected.filter(|_| self.enabled) else {
            let id = self.id;
            return Node::styled_text(content, self.style.disabled)
                .with_id(id.clone())
                .on_actions(
                    id,
                    descriptors
                        .map(|descriptor| Action::new(descriptor, |_| EventResult::ignored())),
                );
        };

        let id = self.id;
        let count = self.options.len();
        let on_select = self.on_select;
        let actions = descriptors
            .into_iter()
            .zip(SELECT_ACTIONS)
            .map(|(descriptor, action)| {
                select_action(
                    descriptor,
                    action,
                    count,
                    selected,
                    id.clone(),
                    Arc::clone(&on_select),
                )
            });
        let pointer_focus_id = id.clone();
        let pointer_select = Arc::clone(&on_select);
        Node::styled_text(content, self.style.normal)
            .focusable(id.clone())
            .with_focused_style(self.style.focused)
            .on_actions(id.clone(), actions)
            .on_event(id, move |event| {
                if !is_pointer_activation_event(event) {
                    return EventResult::ignored();
                }
                select_action_result(
                    SelectAction::Activate,
                    count,
                    selected,
                    &pointer_focus_id,
                    pointer_select.as_ref(),
                )
            })
    }
}

#[derive(Clone, Copy)]
enum SelectAction {
    Activate,
    Previous,
    Next,
    First,
    Last,
}

const SELECT_ACTIONS: [SelectAction; 5] = [
    SelectAction::Activate,
    SelectAction::Previous,
    SelectAction::Next,
    SelectAction::First,
    SelectAction::Last,
];

fn select_action_descriptors(enabled: bool) -> [ActionDescriptor; 5] {
    let availability = if enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    };
    [
        activate_action_descriptor().with_availability(availability),
        SELECT_NAVIGATION_ACTION_DESCRIPTORS[0]
            .clone()
            .with_availability(availability),
        SELECT_NAVIGATION_ACTION_DESCRIPTORS[1]
            .clone()
            .with_availability(availability),
        SELECT_NAVIGATION_ACTION_DESCRIPTORS[2]
            .clone()
            .with_availability(availability),
        SELECT_NAVIGATION_ACTION_DESCRIPTORS[3]
            .clone()
            .with_availability(availability),
    ]
}

fn select_action<Message: 'static>(
    descriptor: ActionDescriptor,
    action: SelectAction,
    count: usize,
    selected: usize,
    focus_id: NodeId,
    on_select: Arc<dyn Fn(usize) -> Message>,
) -> Action<Message> {
    Action::new(descriptor, move |_| {
        select_action_result(action, count, selected, &focus_id, on_select.as_ref())
    })
}

fn select_action_result<Message>(
    action: SelectAction,
    count: usize,
    selected: usize,
    focus_id: &NodeId,
    on_select: &dyn Fn(usize) -> Message,
) -> EventResult<Message> {
    let next = match action {
        SelectAction::Activate => (selected + 1) % count,
        SelectAction::Previous => navigate(count, selected, Navigation::Up).unwrap_or(selected),
        SelectAction::Next => navigate(count, selected, Navigation::Down).unwrap_or(selected),
        SelectAction::First => navigate(count, selected, Navigation::Home).unwrap_or(selected),
        SelectAction::Last => navigate(count, selected, Navigation::End).unwrap_or(selected),
    };
    let result = EventResult::consumed().focus(focus_id.clone());
    if next == selected {
        result
    } else {
        result.emit(on_select(next))
    }
}

#[cfg(test)]
mod tests {
    use super::select_action_descriptors;

    #[test]
    fn navigation_descriptor_clones_reuse_immutable_storage() {
        let first = select_action_descriptors(true);
        let second = select_action_descriptors(false);

        for index in 1..first.len() {
            assert!(std::ptr::eq(
                first[index].id().as_str(),
                second[index].id().as_str()
            ));
            assert!(std::ptr::eq(first[index].label(), second[index].label()));
            assert!(std::ptr::eq(
                first[index].default_bindings(),
                second[index].default_bindings()
            ));
        }
    }
}
