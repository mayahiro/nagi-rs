use std::sync::LazyLock;

use nagi_tui::{
    ActionAvailability, ActionDescriptor, KeyBinding, KeyCode, KeyStroke, Modifiers, RepeatPolicy,
};

use crate::navigation::Navigation;

/// Stable Action ID shared by standard activation widgets
pub const ACTIVATE_ACTION_ID: &str = "nagi.activate";

/// Stable Action ID for selecting the previous item
pub const SELECTION_PREVIOUS_ACTION_ID: &str = "nagi.selection.previous";

/// Stable Action ID for selecting the next item
pub const SELECTION_NEXT_ACTION_ID: &str = "nagi.selection.next";

/// Stable Action ID for selecting the first item
pub const SELECTION_FIRST_ACTION_ID: &str = "nagi.selection.first";

/// Stable Action ID for selecting the last item
pub const SELECTION_LAST_ACTION_ID: &str = "nagi.selection.last";

/// Stable Action ID for collapsing the current disclosure target
pub const COLLAPSE_ACTION_ID: &str = "nagi.collapse";

/// Stable Action ID for expanding the current disclosure target
pub const EXPAND_ACTION_ID: &str = "nagi.expand";

pub(crate) const SELECTION_PREVIOUS_ACTION_LABEL: &str = "Previous";
pub(crate) const SELECTION_NEXT_ACTION_LABEL: &str = "Next";
pub(crate) const SELECTION_FIRST_ACTION_LABEL: &str = "First";
pub(crate) const SELECTION_LAST_ACTION_LABEL: &str = "Last";

static ACTIVATE_ACTION_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        ACTIVATE_ACTION_ID,
        "Activate",
        [
            KeyBinding::new(KeyStroke::new(KeyCode::Enter, Modifiers::NONE))
                .with_repeat_policy(RepeatPolicy::AllowRepeat),
            KeyBinding::new(KeyStroke::character(' ', Modifiers::NONE))
                .with_repeat_policy(RepeatPolicy::AllowRepeat),
        ],
    )
});

static VERTICAL_COLLECTION_ACTION_DESCRIPTORS: LazyLock<[ActionDescriptor; 4]> =
    LazyLock::new(|| {
        [
            ActionDescriptor::new(
                SELECTION_PREVIOUS_ACTION_ID,
                SELECTION_PREVIOUS_ACTION_LABEL,
                [repeatable_action_binding(KeyCode::Up)],
            ),
            ActionDescriptor::new(
                SELECTION_NEXT_ACTION_ID,
                SELECTION_NEXT_ACTION_LABEL,
                [repeatable_action_binding(KeyCode::Down)],
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

#[derive(Clone, Copy)]
pub(crate) enum CollectionAction {
    Activate,
    Previous,
    Next,
    First,
    Last,
}

impl CollectionAction {
    pub(crate) const fn navigation(self) -> Option<Navigation> {
        match self {
            Self::Activate => None,
            Self::Previous => Some(Navigation::Up),
            Self::Next => Some(Navigation::Down),
            Self::First => Some(Navigation::Home),
            Self::Last => Some(Navigation::End),
        }
    }
}

pub(crate) const COLLECTION_ACTIONS: [CollectionAction; 5] = [
    CollectionAction::Activate,
    CollectionAction::Previous,
    CollectionAction::Next,
    CollectionAction::First,
    CollectionAction::Last,
];

/// Returns the enabled standard activation descriptor
///
/// Enter and unmodified Space are ordered fallback bindings. Explicit repeat
/// events remain enabled to preserve standard activation-widget behavior
#[must_use]
pub fn activate_action_descriptor() -> ActionDescriptor {
    ACTIVATE_ACTION_DESCRIPTOR.clone()
}

pub(crate) fn repeatable_action_binding(code: KeyCode) -> KeyBinding {
    KeyBinding::new(KeyStroke::new(code, Modifiers::NONE))
        .with_repeat_policy(RepeatPolicy::AllowRepeat)
}

pub(crate) fn vertical_collection_action_descriptors(enabled: bool) -> [ActionDescriptor; 5] {
    let availability = if enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    };
    [
        activate_action_descriptor().with_availability(availability),
        VERTICAL_COLLECTION_ACTION_DESCRIPTORS[0]
            .clone()
            .with_availability(availability),
        VERTICAL_COLLECTION_ACTION_DESCRIPTORS[1]
            .clone()
            .with_availability(availability),
        VERTICAL_COLLECTION_ACTION_DESCRIPTORS[2]
            .clone()
            .with_availability(availability),
        VERTICAL_COLLECTION_ACTION_DESCRIPTORS[3]
            .clone()
            .with_availability(availability),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_descriptor_clones_reuse_immutable_storage() {
        let first = activate_action_descriptor();
        let second = activate_action_descriptor();

        assert!(std::ptr::eq(first.id().as_str(), second.id().as_str()));
        assert!(std::ptr::eq(first.label(), second.label()));
        assert!(std::ptr::eq(
            first.default_bindings(),
            second.default_bindings()
        ));
    }

    #[test]
    fn vertical_collection_descriptor_clones_reuse_immutable_storage() {
        let enabled = vertical_collection_action_descriptors(true);
        let disabled = vertical_collection_action_descriptors(false);

        for index in 0..enabled.len() {
            assert!(std::ptr::eq(
                enabled[index].id().as_str(),
                disabled[index].id().as_str()
            ));
            assert!(std::ptr::eq(
                enabled[index].label(),
                disabled[index].label()
            ));
            assert!(std::ptr::eq(
                enabled[index].default_bindings(),
                disabled[index].default_bindings()
            ));
        }
    }
}
