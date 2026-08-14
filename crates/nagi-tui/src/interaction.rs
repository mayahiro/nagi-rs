use std::collections::{HashMap, HashSet};

use crate::virtual_flow::{VirtualFlowInteraction, VirtualFlowWindow};
use crate::{
    ModalFocusOptions, ModalInitialFocus, ModalReturnFocus, NodeId, VirtualFlowSource,
    VirtualFlowState,
};

#[cfg(test)]
use crate::fixture_support;

/// Interaction state retained for one TextInput node
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextInputState {
    pub(crate) cursor: usize,
    pub(crate) draft: String,
}

impl TextInputState {
    /// Returns the UTF-8 byte cursor at a grapheme boundary
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }
}

/// A two-dimensional ScrollViewport offset in cells
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ScrollOffset {
    /// Horizontal content offset
    pub x: u32,
    /// Vertical content offset
    pub y: u32,
}

/// Axes controlled by a ScrollViewport
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ScrollAxis {
    /// Scroll horizontally and vertically
    #[default]
    Both,
    /// Scroll vertically while keeping the horizontal offset at zero
    Vertical,
    /// Scroll horizontally while keeping the vertical offset at zero
    Horizontal,
}

impl ScrollAxis {
    pub(crate) const fn allows_horizontal(self) -> bool {
        matches!(self, Self::Both | Self::Horizontal)
    }

    pub(crate) const fn allows_vertical(self) -> bool {
        matches!(self, Self::Both | Self::Vertical)
    }
}

/// Resolved ScrollViewport position and boundaries
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ScrollState {
    /// Current cell offset after clamping
    pub offset: ScrollOffset,
    /// Greatest valid offset for the current content and viewport
    pub maximum: ScrollOffset,
    /// Whether every enabled axis is at its beginning
    pub at_start: bool,
    /// Whether every enabled axis is at its end
    pub at_end: bool,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            offset: ScrollOffset::default(),
            maximum: ScrollOffset::default(),
            at_start: true,
            at_end: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollInteraction {
    state: ScrollState,
    requested: Option<ScrollOffset>,
    axis: ScrollAxis,
    stick_to_end: bool,
    following_end: bool,
    initialized: bool,
}

#[derive(Clone, Copy, Debug)]
struct PreparedScroll {
    state: ScrollState,
    following_end: bool,
}

impl Default for ScrollInteraction {
    fn default() -> Self {
        Self {
            state: ScrollState::default(),
            requested: None,
            axis: ScrollAxis::Both,
            stick_to_end: false,
            following_end: false,
            initialized: false,
        }
    }
}

impl ScrollOffset {
    /// Creates a cell offset
    #[must_use]
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// Runtime-owned UI continuity keyed by stable [`NodeId`] values
#[derive(Clone, Debug, Default)]
pub struct InteractionState {
    pub(crate) focused: Option<NodeId>,
    pub(crate) pointer_capture: Option<NodeId>,
    pub(crate) text_inputs: HashMap<NodeId, TextInputState>,
    pub(crate) scrolls: HashMap<NodeId, ScrollInteraction>,
    pub(crate) virtual_flows: HashMap<NodeId, VirtualFlowInteraction>,
    modal_focus_stack: Vec<ModalFocusFrame>,
}

#[derive(Clone, Debug)]
struct ModalFocusFrame {
    id: NodeId,
    return_focus: Option<NodeId>,
}

enum FocusLifecycleTransition {
    Stable,
    Enter(ModalInitialFocus),
    Exit(Option<NodeId>),
}

impl InteractionState {
    /// Creates empty Interaction State
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the focused Node ID
    #[must_use]
    pub fn focused(&self) -> Option<&NodeId> {
        self.focused.as_ref()
    }

    /// Returns the node holding pointer capture
    #[must_use]
    pub fn pointer_capture(&self) -> Option<&NodeId> {
        self.pointer_capture.as_ref()
    }

    /// Returns retained TextInput state for a node
    #[must_use]
    pub fn text_input(&self, id: &NodeId) -> Option<&TextInputState> {
        self.text_inputs.get(id)
    }

    /// Returns a retained scroll offset, defaulting to zero
    #[must_use]
    pub fn scroll_offset(&self, id: &NodeId) -> ScrollOffset {
        self.scroll_state(id)
            .map_or(ScrollOffset::default(), |state| state.offset)
    }

    /// Returns resolved ScrollViewport state for a node
    #[must_use]
    pub fn scroll_state(&self, id: &NodeId) -> Option<ScrollState> {
        self.scrolls
            .get(id)
            .filter(|scroll| scroll.initialized)
            .map(|scroll| scroll.state)
    }

    /// Returns resolved variable-height flow state for a node
    #[must_use]
    pub fn virtual_flow_state(&self, id: &NodeId) -> Option<&VirtualFlowState> {
        self.virtual_flows
            .get(id)
            .and_then(VirtualFlowInteraction::state)
    }

    pub(crate) fn ensure_text_input(&mut self, id: &NodeId, value: &str) {
        let state = self
            .text_inputs
            .entry(id.clone())
            .or_insert_with(|| TextInputState {
                cursor: value.len(),
                draft: value.to_owned(),
            });
        if state.draft != value {
            state.draft = value.to_owned();
            state.cursor = crate::text_edit::normalize_cursor(value, state.cursor);
        }
    }

    pub(crate) fn request_scroll(
        &mut self,
        id: &NodeId,
        requested: ScrollOffset,
    ) -> Option<(ScrollState, bool)> {
        let scroll = self.scrolls.entry(id.clone()).or_default();
        scroll.requested = Some(requested);
        if !scroll.initialized {
            return None;
        }
        let previous = scroll.state;
        scroll.state = resolve_scroll_state(scroll.axis, scroll.state.maximum, requested);
        scroll.following_end = scroll.stick_to_end && scroll.state.at_end;
        Some((scroll.state, scroll.state != previous))
    }

    pub(crate) fn preview_scroll(
        &self,
        id: &NodeId,
        maximum: ScrollOffset,
        axis: ScrollAxis,
        stick_to_end: bool,
    ) -> ScrollState {
        let scroll = self.scrolls.get(id).copied().unwrap_or_default();
        resolve_prepared_scroll(scroll, maximum, axis, stick_to_end).state
    }

    pub(crate) fn prepare_scroll(
        &mut self,
        id: &NodeId,
        maximum: ScrollOffset,
        axis: ScrollAxis,
        stick_to_end: bool,
    ) -> ScrollState {
        let scroll = self.scrolls.entry(id.clone()).or_default();
        let prepared = resolve_prepared_scroll(*scroll, maximum, axis, stick_to_end);
        scroll.requested = None;
        scroll.axis = axis;
        scroll.stick_to_end = stick_to_end;
        scroll.state = prepared.state;
        scroll.following_end = prepared.following_end;
        scroll.initialized = true;
        scroll.state
    }

    pub(crate) fn prepare_virtual_flow<Message>(
        &mut self,
        id: &NodeId,
        source: &VirtualFlowSource<Message>,
        width: u32,
        viewport_height: u32,
        overscan: u32,
        stick_to_end: bool,
    ) -> VirtualFlowWindow {
        let scroll = self.scrolls.get(id).copied().unwrap_or_default();
        let preserved = self.virtual_flows.get(id).and_then(|flow| {
            flow.capture_anchor(scroll.state.offset.y, viewport_height, scroll.following_end)
        });
        let follows_end = scroll_follows_end_on_prepare(scroll, stick_to_end);
        let flow = self.virtual_flows.entry(id.clone()).or_default();
        flow.reconcile(source, width);
        let anchor_offset = preserved
            .as_ref()
            .map(|anchor| flow.resolve_anchor(anchor, viewport_height));
        let maximum = ScrollOffset::new(0, flow.total_height().saturating_sub(viewport_height));
        let scroll = self.scrolls.entry(id.clone()).or_default();
        if scroll.requested.is_none() && !follows_end {
            if let Some(offset) = anchor_offset {
                scroll.state.offset = ScrollOffset::new(0, offset);
            }
        }
        let prepared =
            resolve_prepared_scroll(*scroll, maximum, ScrollAxis::Vertical, stick_to_end);
        scroll.requested = None;
        scroll.axis = ScrollAxis::Vertical;
        scroll.stick_to_end = stick_to_end;
        scroll.state = prepared.state;
        scroll.following_end = prepared.following_end;
        scroll.initialized = true;
        flow.resolve_window(
            prepared.state,
            viewport_height,
            overscan,
            prepared.following_end,
        )
    }

    pub(crate) fn apply_virtual_flow_measurements(
        &mut self,
        id: &NodeId,
        measurements: &[(usize, u32)],
        viewport_height: u32,
        overscan: u32,
        stick_to_end: bool,
    ) -> Option<VirtualFlowWindow> {
        let scroll_snapshot = self.scrolls.get(id).copied().unwrap_or_default();
        let flow = self.virtual_flows.get_mut(id)?;
        let preserved = flow.capture_anchor(
            scroll_snapshot.state.offset.y,
            viewport_height,
            scroll_snapshot.following_end,
        );
        let mut changed = false;
        for &(index, height) in measurements {
            changed |= flow.set_measured(index, height);
        }
        if !changed {
            return Some(flow.resolve_window(
                scroll_snapshot.state,
                viewport_height,
                overscan,
                scroll_snapshot.following_end,
            ));
        }
        let follows_end = scroll_follows_end_on_prepare(scroll_snapshot, stick_to_end);
        let anchor_offset = preserved
            .as_ref()
            .map(|anchor| flow.resolve_anchor(anchor, viewport_height));
        let maximum = ScrollOffset::new(0, flow.total_height().saturating_sub(viewport_height));
        let scroll = self.scrolls.get_mut(id)?;
        if scroll.requested.is_none() && !follows_end {
            if let Some(offset) = anchor_offset {
                scroll.state.offset = ScrollOffset::new(0, offset);
            }
        }
        let prepared =
            resolve_prepared_scroll(*scroll, maximum, ScrollAxis::Vertical, stick_to_end);
        scroll.requested = None;
        scroll.axis = ScrollAxis::Vertical;
        scroll.stick_to_end = stick_to_end;
        scroll.state = prepared.state;
        scroll.following_end = prepared.following_end;
        scroll.initialized = true;
        Some(flow.resolve_window(
            prepared.state,
            viewport_height,
            overscan,
            prepared.following_end,
        ))
    }

    pub(crate) fn virtual_flow_item_layout(
        &self,
        id: &NodeId,
        index: usize,
    ) -> Option<(u32, u32, bool)> {
        let flow = self.virtual_flows.get(id)?;
        Some((
            flow.origin(index),
            flow.height(index),
            flow.is_measured(index),
        ))
    }

    pub(crate) fn reconcile(
        &mut self,
        active: &HashSet<NodeId>,
        previous_focus_order: &[NodeId],
        current_focus_order: &[NodeId],
        active_modal: Option<(&NodeId, &ModalFocusOptions)>,
        focus_fallback: Option<&NodeId>,
    ) {
        let transition = self.reconcile_modal_focus(active, active_modal);
        self.focused = match transition {
            FocusLifecycleTransition::Enter(ModalInitialFocus::First) => {
                current_focus_order.first().cloned()
            }
            FocusLifecycleTransition::Enter(ModalInitialFocus::Target(target)) => {
                if current_focus_order.contains(&target) {
                    Some(target)
                } else {
                    current_focus_order.first().cloned()
                }
            }
            FocusLifecycleTransition::Enter(ModalInitialFocus::None) => None,
            FocusLifecycleTransition::Exit(Some(target)) => {
                if current_focus_order.contains(&target) {
                    Some(target)
                } else {
                    reconcile_focus(
                        previous_focus_order,
                        current_focus_order,
                        self.focused.as_ref(),
                    )
                }
            }
            FocusLifecycleTransition::Exit(None) => None,
            FocusLifecycleTransition::Stable => {
                if self
                    .focused
                    .as_ref()
                    .is_some_and(|focused| !current_focus_order.contains(focused))
                    && focus_fallback.is_some_and(|target| current_focus_order.contains(target))
                {
                    focus_fallback.cloned()
                } else {
                    reconcile_focus(
                        previous_focus_order,
                        current_focus_order,
                        self.focused.as_ref(),
                    )
                }
            }
        };
        if self
            .pointer_capture
            .as_ref()
            .is_some_and(|id| !active.contains(id))
        {
            self.pointer_capture = None;
        }
        self.text_inputs.retain(|id, _| active.contains(id));
        self.scrolls.retain(|id, _| active.contains(id));
        self.virtual_flows.retain(|id, _| active.contains(id));
    }

    fn reconcile_modal_focus(
        &mut self,
        active: &HashSet<NodeId>,
        active_modal: Option<(&NodeId, &ModalFocusOptions)>,
    ) -> FocusLifecycleTransition {
        let Some((modal, options)) = active_modal else {
            if self.modal_focus_stack.is_empty() {
                return FocusLifecycleTransition::Stable;
            }
            let return_focus = self.modal_focus_stack[0].return_focus.clone();
            self.modal_focus_stack.clear();
            return FocusLifecycleTransition::Exit(return_focus);
        };

        if let Some(position) = self
            .modal_focus_stack
            .iter()
            .position(|frame| &frame.id == modal)
        {
            if position + 1 == self.modal_focus_stack.len() {
                return FocusLifecycleTransition::Stable;
            }
            let return_focus = self.modal_focus_stack[position + 1].return_focus.clone();
            self.modal_focus_stack.truncate(position + 1);
            return FocusLifecycleTransition::Exit(return_focus);
        }

        let mut previous_focus = self.focused.clone();
        while self
            .modal_focus_stack
            .last()
            .is_some_and(|frame| !active.contains(&frame.id))
        {
            previous_focus = self
                .modal_focus_stack
                .pop()
                .expect("checked non-empty modal focus stack")
                .return_focus;
        }
        let return_focus = match &options.return_focus {
            ModalReturnFocus::Previous => previous_focus,
            ModalReturnFocus::Target(target) => Some(target.clone()),
            ModalReturnFocus::None => None,
        };
        self.modal_focus_stack.push(ModalFocusFrame {
            id: modal.clone(),
            return_focus,
        });
        FocusLifecycleTransition::Enter(options.initial.clone())
    }
}

fn resolve_prepared_scroll(
    scroll: ScrollInteraction,
    maximum: ScrollOffset,
    axis: ScrollAxis,
    stick_to_end: bool,
) -> PreparedScroll {
    let was_initialized = scroll.initialized;
    let was_sticking = scroll.stick_to_end;
    let maximum = normalize_scroll_offset(axis, maximum);
    let follow_existing_end = was_initialized
        && stick_to_end
        && (scroll.following_end || (!was_sticking && scroll.state.at_end));
    let requested_offset = if let Some(requested) = scroll.requested {
        requested
    } else if !was_initialized && stick_to_end || follow_existing_end {
        maximum
    } else {
        scroll.state.offset
    };
    let state = resolve_scroll_state(axis, maximum, requested_offset);
    let following_end = stick_to_end
        && if scroll.requested.is_some() {
            state.at_end
        } else if !was_initialized {
            true
        } else {
            follow_existing_end
        };
    PreparedScroll {
        state,
        following_end,
    }
}

fn scroll_follows_end_on_prepare(scroll: ScrollInteraction, stick_to_end: bool) -> bool {
    (!scroll.initialized && stick_to_end)
        || (scroll.initialized
            && stick_to_end
            && (scroll.following_end || (!scroll.stick_to_end && scroll.state.at_end)))
}

pub(crate) fn normalize_scroll_offset(axis: ScrollAxis, offset: ScrollOffset) -> ScrollOffset {
    ScrollOffset {
        x: if axis.allows_horizontal() {
            offset.x
        } else {
            0
        },
        y: if axis.allows_vertical() { offset.y } else { 0 },
    }
}

fn resolve_scroll_state(
    axis: ScrollAxis,
    maximum: ScrollOffset,
    requested: ScrollOffset,
) -> ScrollState {
    let maximum = normalize_scroll_offset(axis, maximum);
    let requested = normalize_scroll_offset(axis, requested);
    let offset = ScrollOffset {
        x: requested.x.min(maximum.x),
        y: requested.y.min(maximum.y),
    };
    let at_start =
        (!axis.allows_horizontal() || offset.x == 0) && (!axis.allows_vertical() || offset.y == 0);
    let at_end = (!axis.allows_horizontal() || offset.x == maximum.x)
        && (!axis.allows_vertical() || offset.y == maximum.y);
    ScrollState {
        offset,
        maximum,
        at_start,
        at_end,
    }
}

pub(crate) fn reconcile_focus(
    previous: &[NodeId],
    current: &[NodeId],
    focused: Option<&NodeId>,
) -> Option<NodeId> {
    let focused = focused?;
    if current.contains(focused) {
        return Some(focused.clone());
    }
    if let Some(index) = previous.iter().position(|id| id == focused) {
        if let Some(id) = previous[index + 1..].iter().find(|id| current.contains(id)) {
            return Some(id.clone());
        }
        if let Some(id) = previous[..index]
            .iter()
            .rev()
            .find(|id| current.contains(id))
        {
            return Some(id.clone());
        }
    }
    current.first().cloned()
}

pub(crate) fn traverse_focus(
    current: &[NodeId],
    focused: Option<&NodeId>,
    forward: bool,
) -> Option<NodeId> {
    if current.is_empty() {
        return None;
    }
    let Some(index) = focused.and_then(|focused| current.iter().position(|id| id == focused))
    else {
        return if forward {
            current.first().cloned()
        } else {
            current.last().cloned()
        };
    };
    let next = if forward {
        (index + 1) % current.len()
    } else {
        (index + current.len() - 1) % current.len()
    };
    Some(current[next].clone())
}

pub(crate) fn clamp_scroll(
    content_width: u32,
    content_height: u32,
    viewport_width: u32,
    viewport_height: u32,
    requested: ScrollOffset,
) -> ScrollOffset {
    ScrollOffset {
        x: requested
            .x
            .min(content_width.saturating_sub(viewport_width)),
        y: requested
            .y
            .min(content_height.saturating_sub(viewport_height)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_transitions_match_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "interaction/focus.txt",
            "focus-transition",
            &["previous", "current", "focused", "action", "expected"],
        ) else {
            return;
        };
        for record in records {
            let previous = ids(record.field("previous"));
            let current = ids(record.field("current"));
            let focused = id(record.field("focused"));
            let actual = match record.field("action") {
                "reconcile" => reconcile_focus(&previous, &current, focused.as_ref()),
                "next" => traverse_focus(&current, focused.as_ref(), true),
                "previous" => traverse_focus(&current, focused.as_ref(), false),
                action => panic!("invalid action {action}"),
            };
            assert_eq!(actual, id(record.field("expected")), "case {}", record.id);
        }
    }

    #[test]
    fn interaction_retirement_matches_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "interaction/retirement.txt",
            "interaction-retirement",
            &[
                "previous-focus",
                "current-focus",
                "active",
                "focused",
                "capture",
                "text",
                "scroll",
                "expected-focused",
                "expected-capture",
                "expected-text",
                "expected-scroll",
            ],
        ) else {
            return;
        };
        for record in records {
            let mut state = InteractionState {
                focused: id(record.field("focused")),
                pointer_capture: id(record.field("capture")),
                ..InteractionState::new()
            };
            for id in ids(record.field("text")) {
                state.text_inputs.insert(
                    id,
                    TextInputState {
                        cursor: 0,
                        draft: String::new(),
                    },
                );
            }
            for id in ids(record.field("scroll")) {
                state.scrolls.insert(id, ScrollInteraction::default());
            }
            let active = ids(record.field("active")).into_iter().collect();

            state.reconcile(
                &active,
                &ids(record.field("previous-focus")),
                &ids(record.field("current-focus")),
                None,
                None,
            );

            assert_eq!(
                state.focused,
                id(record.field("expected-focused")),
                "case {} focus",
                record.id
            );
            assert_eq!(
                state.pointer_capture,
                id(record.field("expected-capture")),
                "case {} capture",
                record.id
            );
            let mut text: Vec<_> = state.text_inputs.keys().cloned().collect();
            text.sort();
            assert_eq!(
                text,
                ids(record.field("expected-text")),
                "case {} text",
                record.id
            );
            let mut scroll: Vec<_> = state.scrolls.keys().cloned().collect();
            scroll.sort();
            assert_eq!(
                scroll,
                ids(record.field("expected-scroll")),
                "case {} scroll",
                record.id
            );
        }
    }

    #[test]
    fn scroll_clamping_matches_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "interaction/scroll.txt",
            "scroll-clamp",
            &[
                "content-width",
                "content-height",
                "viewport-width",
                "viewport-height",
                "request-x",
                "request-y",
                "expected-x",
                "expected-y",
            ],
        ) else {
            return;
        };
        for record in records {
            let actual = clamp_scroll(
                number(record.field("content-width")),
                number(record.field("content-height")),
                number(record.field("viewport-width")),
                number(record.field("viewport-height")),
                ScrollOffset::new(
                    number(record.field("request-x")),
                    number(record.field("request-y")),
                ),
            );
            assert_eq!(
                actual,
                ScrollOffset::new(
                    number(record.field("expected-x")),
                    number(record.field("expected-y")),
                ),
                "case {}",
                record.id
            );
        }
    }

    fn ids(value: &str) -> Vec<NodeId> {
        if value == "-" {
            Vec::new()
        } else {
            value.split(',').map(NodeId::from).collect()
        }
    }

    fn id(value: &str) -> Option<NodeId> {
        (value != "none").then(|| NodeId::from(value))
    }

    fn number(value: &str) -> u32 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
    }
}
