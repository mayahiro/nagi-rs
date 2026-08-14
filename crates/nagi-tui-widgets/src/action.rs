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

/// Stable Action ID for extending a selection to the previous item
pub const SELECTION_EXTEND_PREVIOUS_ACTION_ID: &str = "nagi.selection.extend-previous";

/// Stable Action ID for extending a selection to the next item
pub const SELECTION_EXTEND_NEXT_ACTION_ID: &str = "nagi.selection.extend-next";

/// Stable Action ID for extending a selection to the first item
pub const SELECTION_EXTEND_FIRST_ACTION_ID: &str = "nagi.selection.extend-first";

/// Stable Action ID for extending a selection to the last item
pub const SELECTION_EXTEND_LAST_ACTION_ID: &str = "nagi.selection.extend-last";

/// Stable Action ID for scrolling a horizontal presentation toward its start
pub const HORIZONTAL_SCROLL_PREVIOUS_ACTION_ID: &str = "nagi.scroll.horizontal-previous";

/// Stable Action ID for scrolling a horizontal presentation toward its end
pub const HORIZONTAL_SCROLL_NEXT_ACTION_ID: &str = "nagi.scroll.horizontal-next";

/// Stable Action ID for selecting one page toward the beginning
pub const SELECTION_PREVIOUS_PAGE_ACTION_ID: &str = "nagi.selection.previous-page";

/// Stable Action ID for selecting one page toward the end
pub const SELECTION_NEXT_PAGE_ACTION_ID: &str = "nagi.selection.next-page";

/// Stable Action ID for selecting the previous calendar day
pub const SELECTION_PREVIOUS_DAY_ACTION_ID: &str = "nagi.selection.previous-day";

/// Stable Action ID for selecting the next calendar day
pub const SELECTION_NEXT_DAY_ACTION_ID: &str = "nagi.selection.next-day";

/// Stable Action ID for selecting the date one week earlier
pub const SELECTION_PREVIOUS_WEEK_ACTION_ID: &str = "nagi.selection.previous-week";

/// Stable Action ID for selecting the date one week later
pub const SELECTION_NEXT_WEEK_ACTION_ID: &str = "nagi.selection.next-week";

/// Stable Action ID for selecting the date one month earlier
pub const SELECTION_PREVIOUS_MONTH_ACTION_ID: &str = "nagi.selection.previous-month";

/// Stable Action ID for selecting the date one month later
pub const SELECTION_NEXT_MONTH_ACTION_ID: &str = "nagi.selection.next-month";

/// Stable Action ID for selecting the first day of the displayed month
pub const SELECTION_FIRST_DAY_OF_MONTH_ACTION_ID: &str = "nagi.selection.first-day-of-month";

/// Stable Action ID for selecting the last day of the displayed month
pub const SELECTION_LAST_DAY_OF_MONTH_ACTION_ID: &str = "nagi.selection.last-day-of-month";

/// Stable Action ID for navigating back from the current location
pub const NAVIGATION_BACK_ACTION_ID: &str = "nagi.navigation.back";

/// Stable Action ID for collapsing the current disclosure target
pub const COLLAPSE_ACTION_ID: &str = "nagi.collapse";

/// Stable Action ID for expanding the current disclosure target
pub const EXPAND_ACTION_ID: &str = "nagi.expand";

/// Stable Action ID for dismissing the current transient surface
pub const DISMISS_ACTION_ID: &str = "nagi.dismiss";

/// Stable Action ID for invoking a dialog's explicit default action
pub const CONFIRM_ACTION_ID: &str = "nagi.confirm";

/// Stable Action ID for submitting a Composer value
pub const COMPOSER_SUBMIT_ACTION_ID: &str = "nagi.composer.submit";

/// Stable Action ID for recalling the previous history entry
pub const HISTORY_PREVIOUS_ACTION_ID: &str = "nagi.history.previous";

/// Stable Action ID for recalling the next history entry
pub const HISTORY_NEXT_ACTION_ID: &str = "nagi.history.next";

/// Stable Action ID for accepting the selected suggestion
pub const SUGGESTION_ACCEPT_ACTION_ID: &str = "nagi.suggestion.accept";

/// Stable Action ID for dismissing an open suggestion popup
pub const SUGGESTION_DISMISS_ACTION_ID: &str = "nagi.suggestion.dismiss";

/// Stable Action ID for copying the selected inspector value
pub const INSPECTOR_COPY_ACTION_ID: &str = "nagi.inspector.copy";

/// Stable Action ID for moving focus to the previous pane
pub const PANE_FOCUS_PREVIOUS_ACTION_ID: &str = "nagi.pane.focus-previous";

/// Stable Action ID for moving focus to the next pane
pub const PANE_FOCUS_NEXT_ACTION_ID: &str = "nagi.pane.focus-next";

/// Stable Action ID for moving a split divider toward its main-axis start
pub const PANE_RESIZE_PREVIOUS_ACTION_ID: &str = "nagi.pane.resize-previous";

/// Stable Action ID for moving a split divider toward its main-axis end
pub const PANE_RESIZE_NEXT_ACTION_ID: &str = "nagi.pane.resize-next";

pub(crate) const ACTIVATE_ACTION_LABEL: &str = "Activate";
pub(crate) const SELECTION_PREVIOUS_ACTION_LABEL: &str = "Previous";
pub(crate) const SELECTION_NEXT_ACTION_LABEL: &str = "Next";
pub(crate) const SELECTION_FIRST_ACTION_LABEL: &str = "First";
pub(crate) const SELECTION_LAST_ACTION_LABEL: &str = "Last";
pub(crate) const SELECTION_PREVIOUS_PAGE_ACTION_LABEL: &str = "Previous page";
pub(crate) const SELECTION_NEXT_PAGE_ACTION_LABEL: &str = "Next page";
pub(crate) const SELECTION_PREVIOUS_DAY_ACTION_LABEL: &str = "Previous day";
pub(crate) const SELECTION_NEXT_DAY_ACTION_LABEL: &str = "Next day";
pub(crate) const SELECTION_PREVIOUS_WEEK_ACTION_LABEL: &str = "Previous week";
pub(crate) const SELECTION_NEXT_WEEK_ACTION_LABEL: &str = "Next week";
pub(crate) const SELECTION_PREVIOUS_MONTH_ACTION_LABEL: &str = "Previous month";
pub(crate) const SELECTION_NEXT_MONTH_ACTION_LABEL: &str = "Next month";
pub(crate) const SELECTION_FIRST_DAY_OF_MONTH_ACTION_LABEL: &str = "First day of month";
pub(crate) const SELECTION_LAST_DAY_OF_MONTH_ACTION_LABEL: &str = "Last day of month";
pub(crate) const NAVIGATION_BACK_ACTION_LABEL: &str = "Back";

static ACTIVATE_ACTION_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        ACTIVATE_ACTION_ID,
        ACTIVATE_ACTION_LABEL,
        [
            KeyBinding::new(KeyStroke::new(KeyCode::Enter, Modifiers::NONE))
                .with_repeat_policy(RepeatPolicy::AllowRepeat),
            KeyBinding::new(KeyStroke::character(' ', Modifiers::NONE))
                .with_repeat_policy(RepeatPolicy::AllowRepeat),
        ],
    )
});

static DISMISS_ACTION_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        DISMISS_ACTION_ID,
        "Dismiss",
        [repeatable_action_binding(KeyCode::Escape)],
    )
});

static CONFIRM_ACTION_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        CONFIRM_ACTION_ID,
        "Confirm",
        [KeyBinding::new(KeyStroke::new(
            KeyCode::Enter,
            Modifiers::NONE,
        ))],
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

/// Returns the enabled standard dismissal descriptor
///
/// Unmodified Escape is the default binding and explicit repeat events remain
/// enabled to preserve standard transient-surface behavior
#[must_use]
pub fn dismiss_action_descriptor() -> ActionDescriptor {
    DISMISS_ACTION_DESCRIPTOR.clone()
}

/// Returns the enabled standard dialog-confirmation descriptor
///
/// Exact unmodified Enter is the default binding. Explicit repeat events are
/// ignored by this root action
#[must_use]
pub fn confirm_action_descriptor() -> ActionDescriptor {
    CONFIRM_ACTION_DESCRIPTOR.clone()
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
        for descriptor in [
            activate_action_descriptor,
            confirm_action_descriptor,
            dismiss_action_descriptor,
        ] {
            let first = descriptor();
            let second = descriptor();

            assert!(std::ptr::eq(first.id().as_str(), second.id().as_str()));
            assert!(std::ptr::eq(first.label(), second.label()));
            assert!(std::ptr::eq(
                first.default_bindings(),
                second.default_bindings()
            ));
        }
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
