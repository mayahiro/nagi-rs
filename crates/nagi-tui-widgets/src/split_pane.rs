use std::sync::{Arc, LazyLock};

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, EventResult, Insets, KeyBinding, KeyCode,
    KeyStroke, Modifiers, MouseButton, MouseKind, Node, NodeId, PointerEventContext, RepeatPolicy,
    SplitPaneAxis, SplitPaneCollapse, SplitPaneOptions, Style,
};

use crate::{
    PANE_FOCUS_NEXT_ACTION_ID, PANE_FOCUS_PREVIOUS_ACTION_ID, PANE_RESIZE_NEXT_ACTION_ID,
    PANE_RESIZE_PREVIOUS_ACTION_ID,
};

/// Complete basis-point range used by [`SplitPaneState`]
pub const SPLIT_PANE_RATIO_SCALE: u16 = 10_000;

/// Controlled primary-pane share for a [`SplitPane`]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SplitPaneState {
    ratio: u16,
}

impl SplitPaneState {
    /// Creates state from a primary-pane share in basis points
    ///
    /// Values above 10,000 are clamped to 10,000
    #[must_use]
    pub const fn new(ratio: u16) -> Self {
        Self {
            ratio: if ratio > SPLIT_PANE_RATIO_SCALE {
                SPLIT_PANE_RATIO_SCALE
            } else {
                ratio
            },
        }
    }

    /// Returns the controlled primary-pane share in basis points
    #[must_use]
    pub const fn ratio(self) -> u16 {
        self.ratio
    }

    fn moved(self, toward_end: bool, step: u16) -> Self {
        if toward_end {
            Self::new(self.ratio.saturating_add(step))
        } else {
            Self::new(self.ratio.saturating_sub(step))
        }
    }
}

impl Default for SplitPaneState {
    fn default() -> Self {
        Self::new(5_000)
    }
}

/// Visual styles used by a [`SplitPane`]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SplitPaneStyle {
    /// Style used by the one-Cell divider
    pub divider: Style,
}

/// A controlled responsive two-pane layout
///
/// The Core split-pane node owns cell allocation and automatic collapse. This
/// widget adds semantic focus movement, controlled keyboard resizing, and
/// pointer dragging without assigning application meaning to either pane
pub struct SplitPane<Message> {
    id: NodeId,
    primary: Node<Message>,
    secondary: Node<Message>,
    state: SplitPaneState,
    axis: SplitPaneAxis,
    primary_minimum: u32,
    secondary_minimum: u32,
    collapse: SplitPaneCollapse,
    style: SplitPaneStyle,
    resize_step: u16,
    focus_targets: Option<(NodeId, NodeId)>,
    on_resize: Option<Arc<dyn Fn(SplitPaneState) -> Message>>,
}

impl<Message: 'static> SplitPane<Message> {
    /// Creates a horizontal controlled split that collapses its secondary pane
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        primary: Node<Message>,
        secondary: Node<Message>,
        state: SplitPaneState,
    ) -> Self {
        Self {
            id: id.into(),
            primary,
            secondary,
            state,
            axis: SplitPaneAxis::Horizontal,
            primary_minimum: 1,
            secondary_minimum: 1,
            collapse: SplitPaneCollapse::Secondary,
            style: SplitPaneStyle::default(),
            resize_step: 500,
            focus_targets: None,
            on_resize: None,
        }
    }

    /// Sets the main-axis direction
    #[must_use]
    pub const fn axis(mut self, axis: SplitPaneAxis) -> Self {
        self.axis = axis;
        self
    }

    /// Sets expanded minima, each normalized to at least one Cell
    #[must_use]
    pub const fn minimums(mut self, primary: u32, secondary: u32) -> Self {
        self.primary_minimum = primary;
        self.secondary_minimum = secondary;
        self
    }

    /// Sets which pane is omitted when both minima and the divider do not fit
    #[must_use]
    pub const fn collapse(mut self, collapse: SplitPaneCollapse) -> Self {
        self.collapse = collapse;
        self
    }

    /// Replaces the divider style
    #[must_use]
    pub const fn style(mut self, style: SplitPaneStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the keyboard resize increment in basis points
    ///
    /// Zero disables state movement at either ratio boundary but the bound
    /// actions remain consumed while a resize handler exists
    #[must_use]
    pub const fn resize_step(mut self, step: u16) -> Self {
        self.resize_step = step;
        self
    }

    /// Sets stable focus targets used for F6 pane traversal and collapse fallback
    #[must_use]
    pub fn focus_targets(
        mut self,
        primary: impl Into<NodeId>,
        secondary: impl Into<NodeId>,
    ) -> Self {
        self.focus_targets = Some((primary.into(), secondary.into()));
        self
    }

    /// Sets the application message mapper for keyboard and pointer resizing
    #[must_use]
    pub fn on_resize(mut self, handler: impl Fn(SplitPaneState) -> Message + 'static) -> Self {
        self.on_resize = Some(Arc::new(handler));
        self
    }

    /// Returns focus-previous, focus-next, resize-previous, and resize-next descriptors
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; 4] {
        split_pane_action_descriptors(
            self.axis,
            self.focus_targets.is_some(),
            self.on_resize.is_some(),
        )
    }

    /// Builds the public semantic node for this split pane
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let descriptors = self.action_descriptors();
        let primary_owner = split_pane_owner_id(&self.id, "primary");
        let secondary_owner = split_pane_owner_id(&self.id, "secondary");
        let (primary_target, secondary_target) = self
            .focus_targets
            .clone()
            .map_or((None, None), |(primary, secondary)| {
                (Some(primary), Some(secondary))
            });
        let primary = pane_focus_node(
            self.primary,
            primary_owner,
            secondary_target.clone(),
            &descriptors,
        );
        let secondary = pane_focus_node(
            self.secondary,
            secondary_owner,
            primary_target.clone(),
            &descriptors,
        );
        let options = SplitPaneOptions {
            axis: self.axis,
            ratio: self.state.ratio(),
            primary_minimum: self.primary_minimum,
            secondary_minimum: self.secondary_minimum,
            collapse: self.collapse,
            divider_style: self.style.divider,
        };
        let id = self.id;
        let mut node = Node::split_pane(primary, secondary, options).with_id(id.clone());
        let resize_actions = resize_actions(
            descriptors[2].clone(),
            descriptors[3].clone(),
            self.state,
            self.resize_step,
            self.on_resize.clone(),
        );
        node = node.on_actions(id.clone(), resize_actions);
        if let Some(on_resize) = self.on_resize {
            let pointer_id = id.clone();
            let state = self.state;
            let axis = self.axis;
            let primary_minimum = self.primary_minimum;
            let secondary_minimum = self.secondary_minimum;
            node = node.on_pointer_event(id, move |context| {
                resize_pointer_event(
                    &pointer_id,
                    context,
                    state,
                    axis,
                    primary_minimum,
                    secondary_minimum,
                    on_resize.as_ref(),
                )
            });
        }
        node
    }
}

fn pane_focus_node<Message: 'static>(
    node: Node<Message>,
    owner: NodeId,
    target: Option<NodeId>,
    descriptors: &[ActionDescriptor; 4],
) -> Node<Message> {
    let previous_target = target.clone();
    let next_target = target.clone();
    let mut node = Node::padding(node, Insets::all(0))
        .with_id(owner.clone())
        .on_actions(
            owner,
            [
                Action::new(descriptors[0].clone(), move |_| {
                    previous_target
                        .clone()
                        .map_or_else(EventResult::ignored, |target| {
                            EventResult::consumed().focus(target)
                        })
                }),
                Action::new(descriptors[1].clone(), move |_| {
                    next_target
                        .clone()
                        .map_or_else(EventResult::ignored, |target| {
                            EventResult::consumed().focus(target)
                        })
                }),
            ],
        );
    if let Some(target) = target {
        node = node.focus_fallback(target);
    }
    node
}

fn resize_actions<Message: 'static>(
    previous: ActionDescriptor,
    next: ActionDescriptor,
    state: SplitPaneState,
    step: u16,
    on_resize: Option<Arc<dyn Fn(SplitPaneState) -> Message>>,
) -> [Action<Message>; 2] {
    let previous_handler = on_resize.clone();
    let next_handler = on_resize;
    [
        Action::new(previous, move |_| {
            previous_handler
                .as_ref()
                .map_or_else(EventResult::ignored, |handler| {
                    resize_action_result(state, false, step, handler.as_ref())
                })
        }),
        Action::new(next, move |_| {
            next_handler
                .as_ref()
                .map_or_else(EventResult::ignored, |handler| {
                    resize_action_result(state, true, step, handler.as_ref())
                })
        }),
    ]
}

fn resize_action_result<Message>(
    state: SplitPaneState,
    toward_end: bool,
    step: u16,
    on_resize: &dyn Fn(SplitPaneState) -> Message,
) -> EventResult<Message> {
    let next = state.moved(toward_end, step);
    if next == state {
        EventResult::consumed()
    } else {
        EventResult::message(on_resize(next))
    }
}

fn resize_pointer_event<Message>(
    id: &NodeId,
    context: &PointerEventContext,
    state: SplitPaneState,
    axis: SplitPaneAxis,
    primary_minimum: u32,
    secondary_minimum: u32,
    on_resize: &dyn Fn(SplitPaneState) -> Message,
) -> EventResult<Message> {
    let event = context.event();
    let main = match axis {
        SplitPaneAxis::Horizontal => context.bounds().width,
        SplitPaneAxis::Vertical => context.bounds().height,
    };
    let position = match axis {
        SplitPaneAxis::Horizontal => context.local_position().x,
        SplitPaneAxis::Vertical => context.local_position().y,
    };
    match event.kind {
        MouseKind::Press if event.button == MouseButton::Left => {
            if split_divider_position(main, state, primary_minimum, secondary_minimum)
                == u32::try_from(position).ok()
            {
                EventResult::consumed().capture_pointer(id.clone())
            } else {
                EventResult::ignored()
            }
        }
        MouseKind::Move if context.is_captured() => {
            pointer_resize_result(position, main, state, on_resize, false)
        }
        MouseKind::Release if context.is_captured() => {
            pointer_resize_result(position, main, state, on_resize, true)
        }
        MouseKind::Press | MouseKind::Release | MouseKind::Move | MouseKind::Scroll => {
            EventResult::ignored()
        }
    }
}

fn pointer_resize_result<Message>(
    position: i32,
    main: u32,
    state: SplitPaneState,
    on_resize: &dyn Fn(SplitPaneState) -> Message,
    release: bool,
) -> EventResult<Message> {
    let Some(next) = pointer_ratio(position, main) else {
        return if release {
            EventResult::consumed().release_pointer()
        } else {
            EventResult::consumed()
        };
    };
    let mut result = if next == state {
        EventResult::consumed()
    } else {
        EventResult::message(on_resize(next))
    };
    if release {
        result = result.release_pointer();
    }
    result
}

fn split_divider_position(
    main: u32,
    state: SplitPaneState,
    primary_minimum: u32,
    secondary_minimum: u32,
) -> Option<u32> {
    let primary_minimum = primary_minimum.max(1);
    let secondary_minimum = secondary_minimum.max(1);
    let required = u64::from(primary_minimum) + 1 + u64::from(secondary_minimum);
    if u64::from(main) < required {
        return None;
    }
    let usable = main - 1;
    let ideal = u64::from(usable) * u64::from(state.ratio()) / u64::from(SPLIT_PANE_RATIO_SCALE);
    Some(
        u32::try_from(ideal)
            .unwrap_or(u32::MAX)
            .clamp(primary_minimum, usable - secondary_minimum),
    )
}

fn pointer_ratio(position: i32, main: u32) -> Option<SplitPaneState> {
    let usable = main.checked_sub(1)?;
    if usable == 0 {
        return None;
    }
    let position = u32::try_from(position).unwrap_or(0).min(usable);
    let ratio = (u64::from(position) * u64::from(SPLIT_PANE_RATIO_SCALE)
        + u64::from(usable).saturating_sub(1))
        / u64::from(usable);
    Some(SplitPaneState::new(
        u16::try_from(ratio).unwrap_or(SPLIT_PANE_RATIO_SCALE),
    ))
}

fn split_pane_owner_id(root: &NodeId, pane: &str) -> NodeId {
    NodeId::new(format!("{}:split-pane:{pane}", root.as_str()))
}

static PANE_FOCUS_DESCRIPTORS: LazyLock<[ActionDescriptor; 2]> = LazyLock::new(|| {
    [
        ActionDescriptor::new(
            PANE_FOCUS_PREVIOUS_ACTION_ID,
            "Focus previous pane",
            [KeyBinding::new(KeyStroke::function(
                6,
                Modifiers {
                    shift: true,
                    ..Modifiers::NONE
                },
            ))],
        ),
        ActionDescriptor::new(
            PANE_FOCUS_NEXT_ACTION_ID,
            "Focus next pane",
            [KeyBinding::new(KeyStroke::function(6, Modifiers::NONE))],
        ),
    ]
});

static HORIZONTAL_RESIZE_DESCRIPTORS: LazyLock<[ActionDescriptor; 2]> =
    LazyLock::new(|| pane_resize_descriptors(KeyCode::Left, KeyCode::Right));

static VERTICAL_RESIZE_DESCRIPTORS: LazyLock<[ActionDescriptor; 2]> =
    LazyLock::new(|| pane_resize_descriptors(KeyCode::Up, KeyCode::Down));

fn pane_resize_descriptors(previous: KeyCode, next: KeyCode) -> [ActionDescriptor; 2] {
    let modifiers = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };
    [
        ActionDescriptor::new(
            PANE_RESIZE_PREVIOUS_ACTION_ID,
            "Resize pane toward start",
            [KeyBinding::new(KeyStroke::new(previous, modifiers))
                .with_repeat_policy(RepeatPolicy::AllowRepeat)],
        ),
        ActionDescriptor::new(
            PANE_RESIZE_NEXT_ACTION_ID,
            "Resize pane toward end",
            [KeyBinding::new(KeyStroke::new(next, modifiers))
                .with_repeat_policy(RepeatPolicy::AllowRepeat)],
        ),
    ]
}

fn split_pane_action_descriptors(
    axis: SplitPaneAxis,
    focus_enabled: bool,
    resize_enabled: bool,
) -> [ActionDescriptor; 4] {
    let focus_availability = if focus_enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    };
    let resize_availability = if resize_enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    };
    let resize = match axis {
        SplitPaneAxis::Horizontal => &*HORIZONTAL_RESIZE_DESCRIPTORS,
        SplitPaneAxis::Vertical => &*VERTICAL_RESIZE_DESCRIPTORS,
    };
    [
        PANE_FOCUS_DESCRIPTORS[0]
            .clone()
            .with_availability(focus_availability),
        PANE_FOCUS_DESCRIPTORS[1]
            .clone()
            .with_availability(focus_availability),
        resize[0].clone().with_availability(resize_availability),
        resize[1].clone().with_availability(resize_availability),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_clamps_and_moves_without_overflow() {
        assert_eq!(SplitPaneState::new(u16::MAX).ratio(), 10_000);
        assert_eq!(SplitPaneState::new(100).moved(false, 500).ratio(), 0);
        assert_eq!(SplitPaneState::new(9_900).moved(true, 500).ratio(), 10_000);
    }

    #[test]
    fn pointer_ratio_round_trips_divider_cells() {
        for position in 0..=8 {
            let state = pointer_ratio(position, 10).expect("non-empty split");
            assert_eq!(
                split_divider_position(10, state, 1, 1),
                Some(u32::try_from(position.clamp(1, 8)).expect("non-negative position"))
            );
        }
    }

    #[test]
    fn divider_detection_collapses_when_extreme_minima_do_not_fit() {
        assert_eq!(
            split_divider_position(u32::MAX, SplitPaneState::default(), u32::MAX, 1,),
            None,
        );
    }
}
