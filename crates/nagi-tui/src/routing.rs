use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use nagi_text::WidthProfile;

use crate::{
    Event, ModalFocusOptions, MouseEvent, MouseKind, NodeId, Point, Rect, ScrollAxis, ScrollOffset,
    ScrollState, Size,
};

#[cfg(test)]
use crate::fixture_support;

/// The result of one semantic node event handler
pub struct EventResult<Message> {
    pub(crate) messages: Vec<Message>,
    pub(crate) consumed: bool,
    pub(crate) focus: FocusChange,
    pub(crate) pointer: PointerChange,
    pub(crate) scroll: Option<(NodeId, ScrollOffset)>,
    pub(crate) redraw: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FocusChange {
    Unchanged,
    Focus(NodeId),
    Release,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PointerChange {
    Unchanged,
    Capture(NodeId),
    Release,
}

/// One semantic text grapheme or collapsed line boundary under a pointer cell
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextHit {
    start: usize,
    end: usize,
}

impl TextHit {
    pub(crate) const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Returns the inclusive UTF-8 byte boundary before the hit grapheme
    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    /// Returns the exclusive UTF-8 byte boundary after the hit grapheme
    ///
    /// Empty visual regions such as a line boundary have equal start and end
    #[must_use]
    pub const fn end(self) -> usize {
        self.end
    }
}

/// The nearest ancestor ScrollViewport available to a pointer handler
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerViewport {
    id: NodeId,
    axis: ScrollAxis,
    state: ScrollState,
    visible: Rect,
}

impl PointerViewport {
    pub(crate) const fn new(
        id: NodeId,
        axis: ScrollAxis,
        state: ScrollState,
        visible: Rect,
    ) -> Self {
        Self {
            id,
            axis,
            state,
            visible,
        }
    }

    /// Returns the stable ScrollViewport identity
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns the axes controlled by the ScrollViewport
    #[must_use]
    pub const fn axis(&self) -> ScrollAxis {
        self.axis
    }

    /// Returns the resolved ScrollViewport state at dispatch time
    #[must_use]
    pub const fn state(&self) -> ScrollState {
        self.state
    }

    /// Returns the globally positioned visible ScrollViewport rectangle
    #[must_use]
    pub const fn visible_rect(&self) -> Rect {
        self.visible
    }

    fn edge_scroll_offset(&self, event: MouseEvent) -> Option<ScrollOffset> {
        if event.kind != MouseKind::Move || self.visible.is_empty() {
            return None;
        }
        let mut next = self.state.offset;
        let x = i64::from(event.x);
        let y = i64::from(event.y);
        let left = i64::from(self.visible.x);
        let top = i64::from(self.visible.y);
        let right = left.saturating_add(i64::from(self.visible.width));
        let bottom = top.saturating_add(i64::from(self.visible.height));
        if self.axis.allows_horizontal() {
            if x <= left {
                next.x = next.x.saturating_sub(1);
            } else if x >= right.saturating_sub(1) {
                next.x = next.x.saturating_add(1).min(self.state.maximum.x);
            }
        }
        if self.axis.allows_vertical() {
            if y <= top {
                next.y = next.y.saturating_sub(1);
            } else if y >= bottom.saturating_sub(1) {
                next.y = next.y.saturating_add(1).min(self.state.maximum.y);
            }
        }
        (next != self.state.offset).then_some(next)
    }
}

/// Geometry and resolved text information for one routed mouse event
#[derive(Clone, Debug)]
pub struct PointerEventContext {
    event: MouseEvent,
    local_position: Point,
    bounds: Size,
    visible_bounds: Rect,
    width_profile: WidthProfile<'static>,
    captured: bool,
    text_hit: Option<TextHit>,
    viewport: Option<PointerViewport>,
}

impl PointerEventContext {
    pub(crate) const fn new(
        event: MouseEvent,
        local_position: Point,
        bounds: Size,
        visible_bounds: Rect,
        width_profile: WidthProfile<'static>,
        captured: bool,
        viewport: Option<PointerViewport>,
    ) -> Self {
        Self {
            event,
            local_position,
            bounds,
            visible_bounds,
            width_profile,
            captured,
            text_hit: None,
            viewport,
        }
    }

    pub(crate) const fn set_text_hit(&mut self, text_hit: Option<TextHit>) {
        self.text_hit = text_hit;
    }

    /// Returns the normalized zero-based terminal mouse event
    #[must_use]
    pub const fn event(&self) -> MouseEvent {
        self.event
    }

    /// Returns the pointer cell relative to the routed Node origin
    ///
    /// Pointer capture may produce coordinates outside `bounds`
    #[must_use]
    pub const fn local_position(&self) -> Point {
        self.local_position
    }

    /// Returns the routed Node size
    #[must_use]
    pub const fn bounds(&self) -> Size {
        self.bounds
    }

    /// Returns the Node-visible rectangle in Node-local coordinates
    #[must_use]
    pub const fn visible_bounds(&self) -> Rect {
        self.visible_bounds
    }

    /// Returns the Runtime terminal cell-width policy
    #[must_use]
    pub const fn width_profile(&self) -> WidthProfile<'static> {
        self.width_profile
    }

    /// Reports whether this Node owns pointer capture for the event
    #[must_use]
    pub const fn is_captured(&self) -> bool {
        self.captured
    }

    /// Returns the rendered paragraph grapheme or line boundary under the pointer
    ///
    /// Non-paragraph Nodes return `None`
    #[must_use]
    pub const fn text_hit(&self) -> Option<TextHit> {
        self.text_hit
    }

    /// Returns the nearest ancestor ScrollViewport, when one exists
    #[must_use]
    pub const fn viewport(&self) -> Option<&PointerViewport> {
        self.viewport.as_ref()
    }

    /// Returns a one-cell scroll request for a Move at a visible edge
    ///
    /// The result is clamped to the current ScrollViewport maximum. This
    /// method does not start a timer and returns `None` for other event kinds,
    /// away from an enabled edge, or when the viewport cannot move farther
    #[must_use]
    pub fn edge_scroll(&self) -> Option<(&NodeId, ScrollOffset)> {
        let viewport = self.viewport.as_ref()?;
        viewport
            .edge_scroll_offset(self.event)
            .map(|offset| (&viewport.id, offset))
    }
}

impl<Message> EventResult<Message> {
    /// Creates an ignored result that continues ancestor routing
    #[must_use]
    pub fn ignored() -> Self {
        Self {
            messages: Vec::new(),
            consumed: false,
            focus: FocusChange::Unchanged,
            pointer: PointerChange::Unchanged,
            scroll: None,
            redraw: false,
        }
    }

    /// Creates a consumed result without a message
    #[must_use]
    pub fn consumed() -> Self {
        Self::ignored().consume()
    }

    /// Creates a consumed result that emits one application message
    #[must_use]
    pub fn message(message: Message) -> Self {
        Self::ignored().emit(message).consume()
    }

    /// Adds an application message to this result
    #[must_use]
    pub fn emit(mut self, message: Message) -> Self {
        self.messages.push(message);
        self
    }

    /// Stops ancestor routing after applying this result
    #[must_use]
    pub const fn consume(mut self) -> Self {
        self.consumed = true;
        self
    }

    /// Requests focus for a stable Node ID
    #[must_use]
    pub fn focus(mut self, id: impl Into<NodeId>) -> Self {
        self.focus = FocusChange::Focus(id.into());
        self
    }

    /// Releases node focus
    #[must_use]
    pub fn release_focus(mut self) -> Self {
        self.focus = FocusChange::Release;
        self
    }

    /// Captures pointer routing for a stable Node ID
    #[must_use]
    pub fn capture_pointer(mut self, id: impl Into<NodeId>) -> Self {
        self.pointer = PointerChange::Capture(id.into());
        self
    }

    /// Releases pointer capture
    #[must_use]
    pub fn release_pointer(mut self) -> Self {
        self.pointer = PointerChange::Release;
        self
    }

    /// Requests a ScrollViewport offset during this event dispatch
    ///
    /// The latest request in one result wins. The Runtime clamps the request
    /// and queues a resulting ScrollViewport callback after explicit messages
    #[must_use]
    pub fn scroll_to(mut self, id: impl Into<NodeId>, offset: ScrollOffset) -> Self {
        self.scroll = Some((id.into(), offset));
        self
    }

    /// Requests a frame even when no message changes application state
    #[must_use]
    pub const fn redraw(mut self) -> Self {
        self.redraw = true;
        self
    }
}

impl<Message> Default for EventResult<Message> {
    fn default() -> Self {
        Self::ignored()
    }
}

/// Observable outcome of routing one normalized event
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventDispatch {
    pub(crate) consumed: bool,
    pub(crate) messages: usize,
    pub(crate) redraw: bool,
}

impl EventDispatch {
    /// Reports whether routing consumed the event
    #[must_use]
    pub const fn consumed(self) -> bool {
        self.consumed
    }

    /// Returns the number of application messages enqueued during routing
    ///
    /// This includes an optional ScrollViewport callback produced by a changed
    /// event-local scroll request
    #[must_use]
    pub const fn messages(self) -> usize {
        self.messages
    }

    /// Reports whether routing made an urgent frame necessary
    #[must_use]
    pub const fn redraw_requested(self) -> bool {
        self.redraw
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InteractiveKind {
    Generic,
    TextInput,
    ScrollViewportVertical,
    ScrollViewportHorizontal,
    Modal,
}

impl InteractiveKind {
    pub(crate) const fn scroll_axis(self) -> Option<ScrollAxis> {
        match self {
            Self::ScrollViewportVertical => Some(ScrollAxis::Vertical),
            Self::ScrollViewportHorizontal => Some(ScrollAxis::Horizontal),
            Self::Generic | Self::TextInput | Self::Modal => None,
        }
    }

    pub(crate) const fn is_scroll_viewport(self) -> bool {
        matches!(
            self,
            Self::ScrollViewportVertical | Self::ScrollViewportHorizontal
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NodeRecord {
    pub(crate) id: NodeId,
    pub(crate) parent: Option<NodeId>,
    pub(crate) rect: Rect,
    pub(crate) clip: Rect,
    pub(crate) focusable: bool,
    pub(crate) has_handler: bool,
    pub(crate) blocks_unhandled_events: bool,
    pub(crate) kind: InteractiveKind,
}

#[derive(Default)]
pub(crate) struct TreeIndex {
    pub(crate) records: Vec<NodeRecord>,
    pub(crate) by_id: HashMap<NodeId, usize>,
    pub(crate) focus_order: Vec<NodeId>,
    pub(crate) reveal_targets: Vec<(NodeId, NodeId)>,
    focus_fallbacks: Vec<(usize, NodeId)>,
    pub(crate) active: HashSet<NodeId>,
    pub(crate) root: Option<NodeId>,
    pub(crate) active_modal: Option<NodeId>,
    pub(crate) active_modal_focus: ModalFocusOptions,
}

impl TreeIndex {
    pub(crate) fn clear(&mut self) {
        self.records.clear();
        self.by_id.clear();
        self.focus_order.clear();
        self.reveal_targets.clear();
        self.focus_fallbacks.clear();
        self.active.clear();
        self.root = None;
        self.active_modal = None;
        self.active_modal_focus = ModalFocusOptions::default();
    }

    pub(crate) fn register(&mut self, record: NodeRecord, is_root: bool) -> Result<(), NodeId> {
        if self.by_id.contains_key(&record.id) {
            return Err(record.id);
        }
        let index = self.records.len();
        if is_root {
            self.root = Some(record.id.clone());
        }
        if record.kind == InteractiveKind::Modal {
            self.active_modal = Some(record.id.clone());
            self.active_modal_focus = ModalFocusOptions::default();
        }
        if record.focusable {
            self.focus_order.push(record.id.clone());
        }
        self.active.insert(record.id.clone());
        self.by_id.insert(record.id.clone(), index);
        self.records.push(record);
        Ok(())
    }

    pub(crate) fn register_reveal(&mut self, viewport: NodeId, target: NodeId) {
        self.reveal_targets.push((viewport, target));
    }

    pub(crate) fn register_focus_fallback(&mut self, id: &NodeId, target: NodeId) {
        if let Some(index) = self.by_id.get(id) {
            self.focus_fallbacks.push((*index, target));
        }
    }

    pub(crate) fn set_active_modal_focus(&mut self, id: &NodeId, focus: ModalFocusOptions) {
        if self.active_modal.as_ref() == Some(id) {
            self.active_modal_focus = focus;
        }
    }

    pub(crate) fn record(&self, id: &NodeId) -> Option<&NodeRecord> {
        self.by_id.get(id).map(|index| &self.records[*index])
    }

    pub(crate) fn focus_fallback(&self, id: &NodeId) -> Option<&NodeId> {
        let record = *self.by_id.get(id)?;
        let position = self
            .focus_fallbacks
            .binary_search_by_key(&record, |(index, _)| *index)
            .ok()?;
        Some(&self.focus_fallbacks[position].1)
    }

    pub(crate) fn route(&self, target: Option<&NodeId>) -> Vec<NodeId> {
        let mut route = Vec::new();
        self.route_into(target, &mut route);
        route
    }

    pub(crate) fn route_into(&self, target: Option<&NodeId>, route: &mut Vec<NodeId>) {
        let target = match (&self.active_modal, target) {
            (Some(modal), Some(target)) if self.is_within(target, modal) => Some(target),
            (Some(modal), _) => Some(modal),
            (None, target) => target,
        };
        self.raw_route_into(target, route);
    }

    pub(crate) fn raw_route(&self, target: Option<&NodeId>) -> Vec<NodeId> {
        let mut route = Vec::new();
        self.raw_route_into(target, &mut route);
        route
    }

    fn raw_route_into(&self, target: Option<&NodeId>, route: &mut Vec<NodeId>) {
        route.clear();
        let mut current = target.cloned();
        let mut remaining = self.records.len().saturating_add(1);
        while let Some(id) = current {
            if remaining == 0 {
                break;
            }
            remaining -= 1;
            route.push(id.clone());
            current = self.record(&id).and_then(|record| record.parent.clone());
        }
        if let Some(root) = &self.root {
            if !route.contains(root) {
                route.push(root.clone());
            }
        }
    }

    pub(crate) fn hit_test(&self, point: Point) -> Option<NodeId> {
        self.records
            .iter()
            .rev()
            .find(|record| {
                self.active_modal
                    .as_ref()
                    .is_none_or(|modal| self.is_within(&record.id, modal))
                    && record.rect.intersection(record.clip).contains(point)
                    && (record.has_handler
                        || record.focusable
                        || record.kind != InteractiveKind::Generic)
            })
            .map(|record| record.id.clone())
            .or_else(|| self.active_modal.clone())
    }

    pub(crate) fn focus_scope(&self) -> Cow<'_, [NodeId]> {
        match &self.active_modal {
            Some(modal) => Cow::Owned(
                self.focus_order
                    .iter()
                    .filter(|id| self.is_within(id, modal))
                    .cloned()
                    .collect(),
            ),
            None => Cow::Borrowed(&self.focus_order),
        }
    }

    pub(crate) fn focus_action_owner(&self, focused: Option<&NodeId>) -> Option<NodeId> {
        focused
            .cloned()
            .or_else(|| self.active_modal.clone())
            .or_else(|| self.root.clone())
    }

    pub(crate) fn allows_focus(&self, id: &NodeId) -> bool {
        self.record(id).is_some_and(|record| record.focusable) && self.allows_interaction(id)
    }

    pub(crate) fn allows_interaction(&self, id: &NodeId) -> bool {
        self.active.contains(id)
            && self
                .active_modal
                .as_ref()
                .is_none_or(|modal| self.is_within(id, modal))
    }

    pub(crate) fn is_within(&self, id: &NodeId, ancestor: &NodeId) -> bool {
        let mut current = Some(id);
        let mut remaining = self.records.len().saturating_add(1);
        while let Some(id) = current {
            if remaining == 0 {
                return false;
            }
            remaining -= 1;
            if id == ancestor {
                return true;
            }
            current = self.record(id).and_then(|record| record.parent.as_ref());
        }
        false
    }
}

#[cfg(test)]
fn route_path(
    parents: &HashMap<NodeId, Option<NodeId>>,
    root: Option<&NodeId>,
    target: Option<&NodeId>,
) -> Vec<NodeId> {
    let mut route = Vec::new();
    let mut current = target.cloned();
    let mut visited = HashSet::new();
    while let Some(id) = current {
        if !visited.insert(id.clone()) {
            break;
        }
        route.push(id.clone());
        current = parents.get(&id).cloned().flatten();
    }
    if let Some(root) = root {
        if !route.contains(root) {
            route.push(root.clone());
        }
    }
    route
}

pub(crate) type EventHandler<Message> = dyn Fn(&Event) -> EventResult<Message>;
pub(crate) type PointerEventHandler<Message> = dyn Fn(&PointerEventContext) -> EventResult<Message>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_routing_matches_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "interaction/routing.txt",
            "event-routing",
            &["paths", "root", "focused", "consume", "expected"],
        ) else {
            return;
        };
        for record in records {
            let parents: HashMap<_, _> = record
                .field("paths")
                .split(',')
                .map(|path| {
                    let (child, parent) = path
                        .split_once(':')
                        .unwrap_or_else(|| panic!("invalid path {path}"));
                    (
                        NodeId::from(child),
                        (parent != "-").then(|| NodeId::from(parent)),
                    )
                })
                .collect();
            let root = NodeId::from(record.field("root"));
            let focused =
                (record.field("focused") != "none").then(|| NodeId::from(record.field("focused")));
            let consume = record.field("consume");
            let mut actual = Vec::new();
            for id in route_path(&parents, Some(&root), focused.as_ref()) {
                actual.push(id.clone());
                if id.as_str() == consume {
                    break;
                }
            }
            assert_eq!(actual, ids(record.field("expected")), "case {}", record.id);
        }
    }

    #[test]
    fn modal_routing_matches_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "interaction/modal.txt",
            "modal-routing",
            &["paths", "root", "modal", "target", "consume", "expected"],
        ) else {
            return;
        };
        for record in records {
            let root = NodeId::from(record.field("root"));
            let modal = NodeId::from(record.field("modal"));
            let mut index = TreeIndex::default();
            for path in record.field("paths").split(',') {
                let (child, parent) = path
                    .split_once(':')
                    .unwrap_or_else(|| panic!("invalid path {path}"));
                let child = NodeId::from(child);
                index
                    .register(
                        NodeRecord {
                            id: child.clone(),
                            parent: (parent != "-").then(|| NodeId::from(parent)),
                            rect: Rect::new(0, 0, 10, 10),
                            clip: Rect::new(0, 0, 10, 10),
                            focusable: false,
                            has_handler: true,
                            blocks_unhandled_events: false,
                            kind: if child == modal {
                                InteractiveKind::Modal
                            } else {
                                InteractiveKind::Generic
                            },
                        },
                        child == root,
                    )
                    .unwrap();
            }
            let target = id(record.field("target"));
            let consume = record.field("consume");
            let mut actual = Vec::new();
            for id in index.route(target.as_ref()) {
                actual.push(id.clone());
                if id.as_str() == consume {
                    break;
                }
            }
            assert_eq!(actual, ids(record.field("expected")), "case {}", record.id);
        }
    }

    #[test]
    fn pointer_routing_matches_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "interaction/pointer.txt",
            "pointer-routing",
            &["records", "point", "capture", "expected"],
        ) else {
            return;
        };
        for record in records {
            let mut index = TreeIndex::default();
            for item in record.field("records").split(';') {
                let parts: Vec<_> = item.split('@').collect();
                assert_eq!(parts.len(), 3, "case {}", record.id);
                index
                    .register(
                        NodeRecord {
                            id: NodeId::from(parts[0]),
                            parent: None,
                            rect: rect(parts[1]),
                            clip: rect(parts[2]),
                            focusable: false,
                            has_handler: true,
                            blocks_unhandled_events: false,
                            kind: InteractiveKind::Generic,
                        },
                        false,
                    )
                    .unwrap();
            }
            let capture = id(record.field("capture"));
            let point = point(record.field("point"));
            let actual = capture.or_else(|| index.hit_test(point));
            assert_eq!(actual, id(record.field("expected")), "case {}", record.id);
        }
    }

    #[test]
    fn edge_scroll_is_derived_only_for_move_events() {
        let viewport = PointerViewport::new(
            NodeId::from("scroll"),
            ScrollAxis::Vertical,
            ScrollState {
                offset: ScrollOffset::new(0, 1),
                maximum: ScrollOffset::new(0, 3),
                at_start: false,
                at_end: false,
            },
            Rect::new(0, 0, 4, 2),
        );
        let event = |kind| MouseEvent {
            kind,
            button: crate::MouseButton::Left,
            x: 1,
            y: 1,
            modifiers: crate::Modifiers::NONE,
        };

        assert_eq!(viewport.edge_scroll_offset(event(MouseKind::Press)), None);
        assert_eq!(viewport.edge_scroll_offset(event(MouseKind::Release)), None);
        assert_eq!(
            viewport.edge_scroll_offset(event(MouseKind::Move)),
            Some(ScrollOffset::new(0, 2))
        );
    }

    fn ids(value: &str) -> Vec<NodeId> {
        value.split(',').map(NodeId::from).collect()
    }

    fn id(value: &str) -> Option<NodeId> {
        (value != "none").then(|| NodeId::from(value))
    }

    fn point(value: &str) -> Point {
        let values = numbers(value, 2);
        Point::new(values[0] as i32, values[1] as i32)
    }

    fn rect(value: &str) -> Rect {
        let values = numbers(value, 4);
        Rect::new(values[0] as i32, values[1] as i32, values[2], values[3])
    }

    fn numbers(value: &str, count: usize) -> Vec<u32> {
        let values: Vec<_> = value.split(',').map(|part| part.parse().unwrap()).collect();
        assert_eq!(values.len(), count);
        values
    }
}
