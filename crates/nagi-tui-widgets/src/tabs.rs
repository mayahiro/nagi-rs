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

static TABS_NAVIGATION_ACTION_DESCRIPTORS: LazyLock<[ActionDescriptor; 4]> = LazyLock::new(|| {
    [
        ActionDescriptor::new(
            SELECTION_PREVIOUS_ACTION_ID,
            SELECTION_PREVIOUS_ACTION_LABEL,
            [repeatable_action_binding(KeyCode::Left)],
        ),
        ActionDescriptor::new(
            SELECTION_NEXT_ACTION_ID,
            SELECTION_NEXT_ACTION_LABEL,
            [repeatable_action_binding(KeyCode::Right)],
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

/// One stable item rendered by [`Tabs`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TabItem {
    id: NodeId,
    label: String,
}

impl TabItem {
    /// Creates a tab with an application-defined stable identity
    #[must_use]
    pub fn new(id: impl Into<NodeId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }

    /// Returns the tab's stable identity
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns the displayed label
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Visual styles used by [`Tabs`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabsStyle {
    /// Style used by unselected tabs
    pub normal: Style,
    /// Style used by the application-selected tab
    pub selected: Style,
    /// Style merged over the tab that owns runtime focus
    pub focused: Style,
    /// Style used by every tab while the set is disabled
    pub disabled: Style,
}

impl Default for TabsStyle {
    fn default() -> Self {
        Self {
            normal: Style::default(),
            selected: Style {
                reverse: true,
                ..Style::default()
            },
            focused: Style {
                underline: true,
                ..Style::default()
            },
            disabled: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// A horizontal, keyboard and pointer selectable set of views
///
/// Every tab item owns the standard activation action. The root owns the
/// previous, next, first, and last navigation actions. Left-button press stays
/// raw so keyboard rebinding does not remove pointer selection
pub struct Tabs<Message> {
    id: NodeId,
    items: Vec<TabItem>,
    selected: usize,
    enabled: bool,
    style: TabsStyle,
    on_select: Arc<dyn Fn(usize) -> Message>,
}

impl<Message: 'static> Tabs<Message> {
    /// Creates enabled tabs using application-owned selection state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        items: impl IntoIterator<Item = TabItem>,
        selected: usize,
        on_select: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            items: items.into_iter().collect(),
            selected,
            enabled: true,
            style: TabsStyle::default(),
            on_select: Arc::new(on_select),
        }
    }

    /// Sets whether tabs can receive focus and emit selection messages
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the tab styles
    #[must_use]
    pub const fn style(mut self, style: TabsStyle) -> Self {
        self.style = style;
        self
    }

    /// Returns the semantic activation descriptor declared by every tab item
    ///
    /// The descriptor is disabled-pass-through when the tabs are disabled or
    /// empty
    #[must_use]
    pub fn item_action_descriptor(&self) -> ActionDescriptor {
        tabs_item_action_descriptor(self.enabled && !self.items.is_empty())
    }

    /// Returns the ordered semantic navigation descriptors declared by the root
    ///
    /// The order is previous, next, first, and last. Every descriptor is
    /// disabled-pass-through when the tabs are disabled or empty
    #[must_use]
    pub fn navigation_action_descriptors(&self) -> [ActionDescriptor; 4] {
        tabs_navigation_action_descriptors(self.enabled && !self.items.is_empty())
    }

    /// Builds the public semantic node for these tabs
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let selected = navigate(self.items.len(), self.selected, Navigation::Normalize);
        let item_descriptor = self.item_action_descriptor();
        let navigation_descriptors = self.navigation_action_descriptors();
        let item_ids: Arc<Vec<NodeId>> =
            Arc::new(self.items.iter().map(|item| item.id.clone()).collect());
        let mut children = Vec::with_capacity(self.items.len());
        for (index, item) in self.items.into_iter().enumerate() {
            let is_selected = selected == Some(index);
            let content = if is_selected {
                format!("[{}]", item.label)
            } else {
                format!(" {} ", item.label)
            };
            let style = if !self.enabled {
                self.style.disabled
            } else if is_selected {
                self.style.selected
            } else {
                self.style.normal
            };
            if !self.enabled {
                let id = item.id;
                children.push(
                    Node::styled_text(content, style)
                        .with_id(id.clone())
                        .on_actions(
                            id,
                            [Action::new(item_descriptor.clone(), |_| {
                                EventResult::ignored()
                            })],
                        ),
                );
                continue;
            }
            let id = item.id;
            let action_focus_id = id.clone();
            let action_on_select = Arc::clone(&self.on_select);
            let pointer_focus_id = id.clone();
            let pointer_on_select = Arc::clone(&self.on_select);
            children.push(
                Node::styled_text(content, style)
                    .focusable(id.clone())
                    .with_focused_style(self.style.focused)
                    .on_actions(
                        id.clone(),
                        [Action::new(item_descriptor.clone(), move |_| {
                            tab_selection_result(
                                is_selected,
                                index,
                                &action_focus_id,
                                action_on_select.as_ref(),
                            )
                        })],
                    )
                    .on_event(id, move |event| {
                        if !is_pointer_activation_event(event) {
                            return EventResult::ignored();
                        }
                        tab_selection_result(
                            is_selected,
                            index,
                            &pointer_focus_id,
                            pointer_on_select.as_ref(),
                        )
                    }),
            );
        }

        let root_id = self.id;
        let root = Node::row(children).with_id(root_id.clone());
        let Some(selected) = selected.filter(|_| self.enabled) else {
            return root.on_actions(
                root_id,
                navigation_descriptors
                    .map(|descriptor| Action::new(descriptor, |_| EventResult::ignored())),
            );
        };
        let on_select = self.on_select;
        let actions = navigation_descriptors
            .into_iter()
            .zip(TABS_NAVIGATION_ACTIONS)
            .map(|(descriptor, action)| {
                let item_ids = Arc::clone(&item_ids);
                let on_select = Arc::clone(&on_select);
                Action::new(descriptor, move |_| {
                    tabs_navigation_result(action, selected, item_ids.as_ref(), on_select.as_ref())
                })
            });
        root.on_actions(root_id, actions)
    }
}

#[derive(Clone, Copy)]
enum TabsNavigationAction {
    Previous,
    Next,
    First,
    Last,
}

const TABS_NAVIGATION_ACTIONS: [TabsNavigationAction; 4] = [
    TabsNavigationAction::Previous,
    TabsNavigationAction::Next,
    TabsNavigationAction::First,
    TabsNavigationAction::Last,
];

fn tabs_item_action_descriptor(enabled: bool) -> ActionDescriptor {
    activate_action_descriptor().with_availability(tabs_action_availability(enabled))
}

fn tabs_navigation_action_descriptors(enabled: bool) -> [ActionDescriptor; 4] {
    let availability = tabs_action_availability(enabled);
    TABS_NAVIGATION_ACTION_DESCRIPTORS
        .clone()
        .map(|descriptor| descriptor.with_availability(availability))
}

const fn tabs_action_availability(enabled: bool) -> ActionAvailability {
    if enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    }
}

fn tab_selection_result<Message>(
    is_selected: bool,
    index: usize,
    focus_id: &NodeId,
    on_select: &dyn Fn(usize) -> Message,
) -> EventResult<Message> {
    let result = EventResult::consumed().focus(focus_id.clone());
    if is_selected {
        result
    } else {
        result.emit(on_select(index))
    }
}

fn tabs_navigation_result<Message>(
    action: TabsNavigationAction,
    selected: usize,
    item_ids: &[NodeId],
    on_select: &dyn Fn(usize) -> Message,
) -> EventResult<Message> {
    let navigation = match action {
        TabsNavigationAction::Previous => Navigation::Up,
        TabsNavigationAction::Next => Navigation::Down,
        TabsNavigationAction::First => Navigation::Home,
        TabsNavigationAction::Last => Navigation::End,
    };
    let next = navigate(item_ids.len(), selected, navigation).unwrap_or(selected);
    let result = EventResult::consumed().focus(item_ids[next].clone());
    if next == selected {
        result
    } else {
        result.emit(on_select(next))
    }
}

#[cfg(test)]
mod tests {
    use super::{tabs_item_action_descriptor, tabs_navigation_action_descriptors};

    #[test]
    fn descriptor_clones_reuse_immutable_storage() {
        let enabled_item = tabs_item_action_descriptor(true);
        let disabled_item = tabs_item_action_descriptor(false);
        assert!(std::ptr::eq(
            enabled_item.default_bindings(),
            disabled_item.default_bindings()
        ));

        let enabled_navigation = tabs_navigation_action_descriptors(true);
        let disabled_navigation = tabs_navigation_action_descriptors(false);
        for index in 0..enabled_navigation.len() {
            assert!(std::ptr::eq(
                enabled_navigation[index].id().as_str(),
                disabled_navigation[index].id().as_str()
            ));
            assert!(std::ptr::eq(
                enabled_navigation[index].label(),
                disabled_navigation[index].label()
            ));
            assert!(std::ptr::eq(
                enabled_navigation[index].default_bindings(),
                disabled_navigation[index].default_bindings()
            ));
        }
    }
}
