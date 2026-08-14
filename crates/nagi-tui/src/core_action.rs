use std::sync::{Arc, LazyLock};

use nagi_vt::{Event, KeyCode, Modifiers};

use crate::{
    ActionAvailability, ActionDescriptor, BindingConflict, FOCUS_NEXT_ACTION_ID,
    FOCUS_PREVIOUS_ACTION_ID, KeyBinding, KeyScope, KeyStroke, NodeId, RepeatPolicy,
    ResolvedAction, ResolvedActions, SCROLL_END_ACTION_ID, SCROLL_PAGE_DOWN_ACTION_ID,
    SCROLL_PAGE_UP_ACTION_ID, SCROLL_START_ACTION_ID, ScrollAxis, resolve_actions,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CoreAction {
    FocusNext,
    FocusPrevious,
    ScrollPageUp,
    ScrollPageDown,
    ScrollStart,
    ScrollEnd,
}

pub(crate) struct CoreActionGroup {
    pub(crate) actions: &'static [CoreAction],
    pub(crate) descriptors: &'static [ActionDescriptor],
    default_actions: &'static Arc<[ResolvedAction]>,
}

const FOCUS_ACTIONS: [CoreAction; 2] = [CoreAction::FocusNext, CoreAction::FocusPrevious];
const SCROLL_ACTIONS: [CoreAction; 4] = [
    CoreAction::ScrollPageUp,
    CoreAction::ScrollPageDown,
    CoreAction::ScrollStart,
    CoreAction::ScrollEnd,
];
const FOCUS_SCROLL_ACTIONS: [CoreAction; 6] = [
    CoreAction::FocusNext,
    CoreAction::FocusPrevious,
    CoreAction::ScrollPageUp,
    CoreAction::ScrollPageDown,
    CoreAction::ScrollStart,
    CoreAction::ScrollEnd,
];

static FOCUS_DESCRIPTORS: LazyLock<Vec<ActionDescriptor>> =
    LazyLock::new(|| descriptors(true, None));
static SCROLL_VERTICAL_DESCRIPTORS: LazyLock<Vec<ActionDescriptor>> =
    LazyLock::new(|| descriptors(false, Some(ScrollAxis::Vertical)));
static SCROLL_HORIZONTAL_DESCRIPTORS: LazyLock<Vec<ActionDescriptor>> =
    LazyLock::new(|| descriptors(false, Some(ScrollAxis::Horizontal)));
static FOCUS_SCROLL_VERTICAL_DESCRIPTORS: LazyLock<Vec<ActionDescriptor>> =
    LazyLock::new(|| descriptors(true, Some(ScrollAxis::Vertical)));
static FOCUS_SCROLL_HORIZONTAL_DESCRIPTORS: LazyLock<Vec<ActionDescriptor>> =
    LazyLock::new(|| descriptors(true, Some(ScrollAxis::Horizontal)));
static FOCUS_DEFAULT_ACTIONS: LazyLock<Arc<[ResolvedAction]>> =
    LazyLock::new(|| default_actions(FOCUS_DESCRIPTORS.as_slice()));
static SCROLL_VERTICAL_DEFAULT_ACTIONS: LazyLock<Arc<[ResolvedAction]>> =
    LazyLock::new(|| default_actions(SCROLL_VERTICAL_DESCRIPTORS.as_slice()));
static SCROLL_HORIZONTAL_DEFAULT_ACTIONS: LazyLock<Arc<[ResolvedAction]>> =
    LazyLock::new(|| default_actions(SCROLL_HORIZONTAL_DESCRIPTORS.as_slice()));
static FOCUS_SCROLL_VERTICAL_DEFAULT_ACTIONS: LazyLock<Arc<[ResolvedAction]>> =
    LazyLock::new(|| default_actions(FOCUS_SCROLL_VERTICAL_DESCRIPTORS.as_slice()));
static FOCUS_SCROLL_HORIZONTAL_DEFAULT_ACTIONS: LazyLock<Arc<[ResolvedAction]>> =
    LazyLock::new(|| default_actions(FOCUS_SCROLL_HORIZONTAL_DESCRIPTORS.as_slice()));

pub(crate) fn action_group(
    includes_focus: bool,
    scroll_axis: Option<ScrollAxis>,
) -> Option<CoreActionGroup> {
    match (includes_focus, scroll_axis) {
        (false, None) => None,
        (true, None) => Some(CoreActionGroup {
            actions: &FOCUS_ACTIONS,
            descriptors: FOCUS_DESCRIPTORS.as_slice(),
            default_actions: &FOCUS_DEFAULT_ACTIONS,
        }),
        (false, Some(ScrollAxis::Both | ScrollAxis::Vertical)) => Some(CoreActionGroup {
            actions: &SCROLL_ACTIONS,
            descriptors: SCROLL_VERTICAL_DESCRIPTORS.as_slice(),
            default_actions: &SCROLL_VERTICAL_DEFAULT_ACTIONS,
        }),
        (false, Some(ScrollAxis::Horizontal)) => Some(CoreActionGroup {
            actions: &SCROLL_ACTIONS,
            descriptors: SCROLL_HORIZONTAL_DESCRIPTORS.as_slice(),
            default_actions: &SCROLL_HORIZONTAL_DEFAULT_ACTIONS,
        }),
        (true, Some(ScrollAxis::Both | ScrollAxis::Vertical)) => Some(CoreActionGroup {
            actions: &FOCUS_SCROLL_ACTIONS,
            descriptors: FOCUS_SCROLL_VERTICAL_DESCRIPTORS.as_slice(),
            default_actions: &FOCUS_SCROLL_VERTICAL_DEFAULT_ACTIONS,
        }),
        (true, Some(ScrollAxis::Horizontal)) => Some(CoreActionGroup {
            actions: &FOCUS_SCROLL_ACTIONS,
            descriptors: FOCUS_SCROLL_HORIZONTAL_DESCRIPTORS.as_slice(),
            default_actions: &FOCUS_SCROLL_HORIZONTAL_DEFAULT_ACTIONS,
        }),
    }
}

pub(crate) fn resolve_action_group(
    owner: &NodeId,
    group: &CoreActionGroup,
    scopes: &[KeyScope],
) -> Result<ResolvedActions, BindingConflict> {
    let has_override = scopes.iter().any(|scope| {
        group.descriptors.iter().any(|descriptor| {
            scope.key_map().bindings(descriptor.id()).is_some()
                || scope.key_map().label(descriptor.id()).is_some()
        })
    });
    if has_override {
        resolve_actions(owner, group.descriptors, scopes)
    } else {
        Ok(ResolvedActions::from_shared_defaults(
            owner,
            scopes,
            Arc::clone(group.default_actions),
        ))
    }
}

pub(crate) fn default_focus_action(event: &Event) -> Option<CoreAction> {
    FOCUS_ACTIONS
        .iter()
        .copied()
        .zip(FOCUS_DESCRIPTORS.iter())
        .find_map(|(action, descriptor)| {
            descriptor
                .default_bindings()
                .iter()
                .any(|binding| binding.matches(event))
                .then_some(action)
        })
}

fn descriptors(includes_focus: bool, scroll_axis: Option<ScrollAxis>) -> Vec<ActionDescriptor> {
    let mut descriptors = Vec::with_capacity(usize::from(includes_focus) * 2 + 4);
    if includes_focus {
        descriptors.extend([
            descriptor(
                FOCUS_NEXT_ACTION_ID,
                "Focus next",
                KeyCode::Tab,
                Modifiers::NONE,
            ),
            descriptor(
                FOCUS_PREVIOUS_ACTION_ID,
                "Focus previous",
                KeyCode::Tab,
                Modifiers {
                    shift: true,
                    ..Modifiers::NONE
                },
            ),
        ]);
    }
    if let Some(axis) = scroll_axis {
        let page_availability = if axis.allows_vertical() {
            ActionAvailability::Enabled
        } else {
            ActionAvailability::DisabledPassThrough
        };
        descriptors.extend([
            descriptor(
                SCROLL_PAGE_UP_ACTION_ID,
                "Scroll page up",
                KeyCode::PageUp,
                Modifiers::NONE,
            )
            .with_availability(page_availability),
            descriptor(
                SCROLL_PAGE_DOWN_ACTION_ID,
                "Scroll page down",
                KeyCode::PageDown,
                Modifiers::NONE,
            )
            .with_availability(page_availability),
            descriptor(
                SCROLL_START_ACTION_ID,
                "Scroll to start",
                KeyCode::Home,
                Modifiers::NONE,
            ),
            descriptor(
                SCROLL_END_ACTION_ID,
                "Scroll to end",
                KeyCode::End,
                Modifiers::NONE,
            ),
        ]);
    }
    descriptors
}

fn default_actions(descriptors: &[ActionDescriptor]) -> Arc<[ResolvedAction]> {
    descriptors
        .iter()
        .map(ResolvedAction::from_descriptor_defaults)
        .collect()
}

fn descriptor(
    id: &'static str,
    label: &'static str,
    code: KeyCode,
    modifiers: Modifiers,
) -> ActionDescriptor {
    ActionDescriptor::new(
        id,
        label,
        [KeyBinding::new(KeyStroke::new(code, modifiers))
            .with_repeat_policy(RepeatPolicy::AllowRepeat)],
    )
}

#[cfg(test)]
mod tests {
    use crate::KeyMap;

    use super::*;

    #[test]
    fn core_descriptor_groups_are_static_ordered_and_axis_aware() {
        let group = action_group(true, Some(ScrollAxis::Horizontal)).unwrap();
        assert_eq!(
            group
                .descriptors
                .iter()
                .map(|descriptor| descriptor.id().as_str())
                .collect::<Vec<_>>(),
            [
                FOCUS_NEXT_ACTION_ID,
                FOCUS_PREVIOUS_ACTION_ID,
                SCROLL_PAGE_UP_ACTION_ID,
                SCROLL_PAGE_DOWN_ACTION_ID,
                SCROLL_START_ACTION_ID,
                SCROLL_END_ACTION_ID,
            ]
        );
        assert_eq!(
            group.descriptors[0].default_bindings()[0].repeat_policy(),
            RepeatPolicy::AllowRepeat
        );
        assert_eq!(
            group.descriptors[2].availability(),
            ActionAvailability::DisabledPassThrough
        );
        assert_eq!(
            group.descriptors[3].availability(),
            ActionAvailability::DisabledPassThrough
        );
        assert_eq!(
            group.descriptors[4].availability(),
            ActionAvailability::Enabled
        );

        let again = action_group(true, Some(ScrollAxis::Horizontal)).unwrap();
        assert!(std::ptr::eq(group.descriptors, again.descriptors));
    }

    #[test]
    fn core_resolution_honors_a_label_only_scope() {
        let group = action_group(true, None).unwrap();
        let key_map = KeyMap::new()
            .relabel(FOCUS_NEXT_ACTION_ID, "次へ移動")
            .unwrap();
        let resolved = resolve_action_group(
            &NodeId::from("owner"),
            &group,
            &[KeyScope::new("localized", key_map)],
        )
        .unwrap();

        assert_eq!(resolved.actions()[0].label(), "次へ移動");
        assert_eq!(
            resolved.actions()[0].bindings(),
            group.descriptors[0].default_bindings()
        );
    }
}
