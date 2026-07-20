use std::collections::{HashMap, HashSet};

use crate::NodeId;

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

    pub(crate) fn prepare_scroll(
        &mut self,
        id: &NodeId,
        maximum: ScrollOffset,
        axis: ScrollAxis,
        stick_to_end: bool,
    ) -> ScrollState {
        let scroll = self.scrolls.entry(id.clone()).or_default();
        let was_initialized = scroll.initialized;
        let was_sticking = scroll.stick_to_end;
        let requested = scroll.requested.take();
        let maximum = normalize_scroll_offset(axis, maximum);
        let follow_existing_end = was_initialized
            && stick_to_end
            && (scroll.following_end || (!was_sticking && scroll.state.at_end));
        let requested_offset = if let Some(requested) = requested {
            requested
        } else if !was_initialized && stick_to_end || follow_existing_end {
            maximum
        } else {
            scroll.state.offset
        };
        scroll.axis = axis;
        scroll.stick_to_end = stick_to_end;
        scroll.state = resolve_scroll_state(axis, maximum, requested_offset);
        scroll.following_end = stick_to_end
            && if requested.is_some() {
                scroll.state.at_end
            } else if !was_initialized {
                true
            } else {
                follow_existing_end
            };
        scroll.initialized = true;
        scroll.state
    }

    pub(crate) fn reconcile(
        &mut self,
        active: &HashSet<NodeId>,
        previous_focus_order: &[NodeId],
        current_focus_order: &[NodeId],
    ) {
        self.focused = reconcile_focus(
            previous_focus_order,
            current_focus_order,
            self.focused.as_ref(),
        );
        if self
            .pointer_capture
            .as_ref()
            .is_some_and(|id| !active.contains(id))
        {
            self.pointer_capture = None;
        }
        self.text_inputs.retain(|id, _| active.contains(id));
        self.scrolls.retain(|id, _| active.contains(id));
    }
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
