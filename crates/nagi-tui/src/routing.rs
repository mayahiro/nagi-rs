use std::collections::{HashMap, HashSet};

use crate::{Event, NodeId, Point, Rect};

#[cfg(test)]
use crate::fixture_support;

/// The result of one semantic node event handler
pub struct EventResult<Message> {
    pub(crate) messages: Vec<Message>,
    pub(crate) consumed: bool,
    pub(crate) focus: FocusChange,
    pub(crate) pointer: PointerChange,
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

impl<Message> EventResult<Message> {
    /// Creates an ignored result that continues ancestor routing
    #[must_use]
    pub fn ignored() -> Self {
        Self {
            messages: Vec::new(),
            consumed: false,
            focus: FocusChange::Unchanged,
            pointer: PointerChange::Unchanged,
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
    /// Reports whether a handler consumed the event
    #[must_use]
    pub const fn consumed(self) -> bool {
        self.consumed
    }

    /// Returns the number of application messages enqueued by handlers
    #[must_use]
    pub const fn messages(self) -> usize {
        self.messages
    }

    /// Reports whether routing explicitly requested a frame
    #[must_use]
    pub const fn redraw_requested(self) -> bool {
        self.redraw
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InteractiveKind {
    Generic,
    TextInput,
    ScrollViewport,
    Modal,
}

#[derive(Clone, Debug)]
pub(crate) struct NodeRecord {
    pub(crate) id: NodeId,
    pub(crate) parent: Option<NodeId>,
    pub(crate) rect: Rect,
    pub(crate) clip: Rect,
    pub(crate) focusable: bool,
    pub(crate) has_handler: bool,
    pub(crate) kind: InteractiveKind,
}

#[derive(Default)]
pub(crate) struct TreeIndex {
    pub(crate) records: Vec<NodeRecord>,
    pub(crate) by_id: HashMap<NodeId, usize>,
    pub(crate) focus_order: Vec<NodeId>,
    pub(crate) active: HashSet<NodeId>,
    pub(crate) root: Option<NodeId>,
    pub(crate) active_modal: Option<NodeId>,
}

impl TreeIndex {
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
        }
        if record.focusable {
            self.focus_order.push(record.id.clone());
        }
        self.active.insert(record.id.clone());
        self.by_id.insert(record.id.clone(), index);
        self.records.push(record);
        Ok(())
    }

    pub(crate) fn record(&self, id: &NodeId) -> Option<&NodeRecord> {
        self.by_id.get(id).map(|index| &self.records[*index])
    }

    pub(crate) fn route(&self, target: Option<&NodeId>) -> Vec<NodeId> {
        let target = match (&self.active_modal, target) {
            (Some(modal), Some(target)) if self.is_within(target, modal) => Some(target),
            (Some(modal), _) => Some(modal),
            (None, target) => target,
        };
        route_path(
            &self
                .records
                .iter()
                .map(|record| (record.id.clone(), record.parent.clone()))
                .collect(),
            self.root.as_ref(),
            target,
        )
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

    pub(crate) fn focus_scope(&self) -> Vec<NodeId> {
        match &self.active_modal {
            Some(modal) => self
                .focus_order
                .iter()
                .filter(|id| self.is_within(id, modal))
                .cloned()
                .collect(),
            None => self.focus_order.clone(),
        }
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

    fn is_within(&self, id: &NodeId, ancestor: &NodeId) -> bool {
        let mut current = Some(id);
        let mut visited = HashSet::new();
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            if !visited.insert(id.clone()) {
                return false;
            }
            current = self.record(id).and_then(|record| record.parent.as_ref());
        }
        false
    }
}

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
