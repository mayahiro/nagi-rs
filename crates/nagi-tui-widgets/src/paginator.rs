use std::sync::{Arc, LazyLock};

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, EventResult, KeyCode, Node, NodeId, Style,
};

use crate::action::{
    SELECTION_FIRST_ACTION_ID, SELECTION_FIRST_ACTION_LABEL, SELECTION_LAST_ACTION_ID,
    SELECTION_LAST_ACTION_LABEL, SELECTION_NEXT_ACTION_ID, SELECTION_NEXT_ACTION_LABEL,
    SELECTION_PREVIOUS_ACTION_ID, SELECTION_PREVIOUS_ACTION_LABEL, repeatable_action_binding,
};
use crate::event::is_pointer_activation_event;

static PAGINATOR_ACTION_DESCRIPTORS: LazyLock<[ActionDescriptor; 4]> = LazyLock::new(|| {
    [
        ActionDescriptor::new(
            SELECTION_PREVIOUS_ACTION_ID,
            SELECTION_PREVIOUS_ACTION_LABEL,
            [
                repeatable_action_binding(KeyCode::Left),
                repeatable_action_binding(KeyCode::Up),
                repeatable_action_binding(KeyCode::PageUp),
            ],
        ),
        ActionDescriptor::new(
            SELECTION_NEXT_ACTION_ID,
            SELECTION_NEXT_ACTION_LABEL,
            [
                repeatable_action_binding(KeyCode::Right),
                repeatable_action_binding(KeyCode::Down),
                repeatable_action_binding(KeyCode::PageDown),
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

#[derive(Clone, Copy)]
enum PaginatorAction {
    Previous,
    Next,
    First,
    Last,
}

const PAGINATOR_ACTIONS: [PaginatorAction; 4] = [
    PaginatorAction::Previous,
    PaginatorAction::Next,
    PaginatorAction::First,
    PaginatorAction::Last,
];

/// Page indicator representation
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PaginatorMode {
    /// Renders one circle per visible page
    #[default]
    Dots,
    /// Renders the current and total page numbers
    Numeric,
}

/// Visual styles used by a [`Paginator`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaginatorStyle {
    /// Style used by unselected page indicators
    pub normal: Style,
    /// Style used by the application-selected page
    pub selected: Style,
    /// Style merged over the indicator that owns focus
    pub focused: Style,
    /// Style used when page changes are unavailable
    pub disabled: Style,
}

impl Default for PaginatorStyle {
    fn default() -> Self {
        Self {
            normal: Style::default(),
            selected: Style {
                bold: true,
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

/// A controlled zero-based page selector
pub struct Paginator<Message> {
    id: NodeId,
    page: usize,
    total: usize,
    limit: usize,
    mode: PaginatorMode,
    enabled: bool,
    style: PaginatorStyle,
    on_change: Arc<dyn Fn(usize) -> Message>,
}

impl<Message: 'static> Paginator<Message> {
    /// Creates an enabled controlled page selector
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        page: usize,
        total: usize,
        on_change: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            page,
            total,
            limit: 7,
            mode: PaginatorMode::Dots,
            enabled: true,
            style: PaginatorStyle::default(),
            on_change: Arc::new(on_change),
        }
    }

    /// Replaces the page indicator representation
    #[must_use]
    pub const fn mode(mut self, mode: PaginatorMode) -> Self {
        self.mode = mode;
        self
    }

    /// Limits the number of dot indicators
    ///
    /// A zero limit shows every page.
    #[must_use]
    pub const fn indicator_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Sets whether the paginator can receive focus and emit messages
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the paginator styles
    #[must_use]
    pub const fn style(mut self, style: PaginatorStyle) -> Self {
        self.style = style;
        self
    }

    /// Returns the ordered semantic navigation actions declared by the root
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; 4] {
        paginator_action_descriptors(self.enabled && self.total > 0)
    }

    /// Builds the public semantic node for this paginator
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let page = normalized_page(self.page, self.total);
        let descriptors = self.action_descriptors();
        let style = if !self.enabled || page.is_none() {
            self.style.disabled
        } else {
            self.style.normal
        };
        if self.mode == PaginatorMode::Numeric || page.is_none() {
            let current = page.map_or(0, |page| page.saturating_add(1));
            let node = Node::styled_text(format!("{current}/{}", self.total), style);
            let Some(page) = page.filter(|_| self.enabled) else {
                let id = self.id;
                return node
                    .with_id(id.clone())
                    .on_actions(id, disabled_paginator_actions(descriptors));
            };
            let id = self.id;
            return node
                .focusable(id.clone())
                .with_focused_style(self.style.focused)
                .on_actions(
                    id.clone(),
                    paginator_actions(descriptors, page, self.total, id, self.on_change),
                );
        }

        let page = page.expect("dots require a page");
        let mut descriptors = Some(descriptors);
        let (start, end) = paginator_window(self.total, page, self.limit);
        let mut children = Vec::with_capacity(end.saturating_sub(start).saturating_mul(2));
        for candidate in start..end {
            if candidate > start {
                children.push(Node::text(" "));
            }
            let candidate_id = paginator_page_id(&self.id, candidate);
            if candidate == page {
                let selected_style = if self.enabled {
                    self.style.selected
                } else {
                    self.style.disabled
                };
                let selected = Node::styled_text("●", selected_style).with_id(candidate_id);
                if !self.enabled {
                    children.push(selected);
                    continue;
                }
                let id = self.id.clone();
                children.push(
                    Node::column([selected])
                        .focusable(id.clone())
                        .with_focused_style(self.style.focused)
                        .on_actions(
                            id.clone(),
                            paginator_actions(
                                descriptors
                                    .take()
                                    .expect("selected page owns Paginator actions"),
                                page,
                                self.total,
                                id,
                                Arc::clone(&self.on_change),
                            ),
                        ),
                );
                continue;
            }
            let candidate_style = if self.enabled {
                self.style.normal
            } else {
                self.style.disabled
            };
            let mut node = Node::styled_text("○", candidate_style).with_id(candidate_id.clone());
            if self.enabled {
                let focus_id = self.id.clone();
                let on_change = Arc::clone(&self.on_change);
                node = node.on_event(candidate_id, move |event| {
                    if !is_pointer_activation_event(event) {
                        return EventResult::ignored();
                    }
                    EventResult::consumed()
                        .focus(focus_id.clone())
                        .emit(on_change(candidate))
                });
            }
            children.push(node);
        }
        let root = Node::row(children);
        if self.enabled {
            root
        } else {
            let id = self.id;
            root.with_id(id.clone()).on_actions(
                id,
                disabled_paginator_actions(
                    descriptors.expect("disabled Paginator retains action descriptors"),
                ),
            )
        }
    }
}

fn paginator_actions<Message: 'static>(
    descriptors: [ActionDescriptor; 4],
    page: usize,
    total: usize,
    focus_id: NodeId,
    on_change: Arc<dyn Fn(usize) -> Message>,
) -> [Action<Message>; 4] {
    std::array::from_fn(|index| {
        let action = PAGINATOR_ACTIONS[index];
        let focus_id = focus_id.clone();
        let on_change = Arc::clone(&on_change);
        Action::new(descriptors[index].clone(), move |_| {
            paginator_action_result(action, page, total, &focus_id, on_change.as_ref())
        })
    })
}

fn disabled_paginator_actions<Message: 'static>(
    descriptors: [ActionDescriptor; 4],
) -> [Action<Message>; 4] {
    descriptors.map(|descriptor| Action::new(descriptor, |_| EventResult::ignored()))
}

fn paginator_action_result<Message>(
    action: PaginatorAction,
    page: usize,
    total: usize,
    focus_id: &NodeId,
    on_change: &dyn Fn(usize) -> Message,
) -> EventResult<Message> {
    let next = page_for_action(page, total, action).unwrap_or(page);
    let result = EventResult::consumed().focus(focus_id.clone());
    if next == page {
        result
    } else {
        result.emit(on_change(next))
    }
}

fn normalized_page(page: usize, total: usize) -> Option<usize> {
    (total > 0).then(|| page.min(total.saturating_sub(1)))
}

fn paginator_window(total: usize, page: usize, limit: usize) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    if limit == 0 || limit >= total {
        return (0, total);
    }
    let page = page.min(total.saturating_sub(1));
    let start = page
        .saturating_sub(limit / 2)
        .min(total.saturating_sub(limit));
    (start, start.saturating_add(limit))
}

fn page_for_action(page: usize, total: usize, action: PaginatorAction) -> Option<usize> {
    let page = normalized_page(page, total)?;
    match action {
        PaginatorAction::Previous => Some(page.saturating_sub(1)),
        PaginatorAction::Next => Some(page.saturating_add(1).min(total.saturating_sub(1))),
        PaginatorAction::First => Some(0),
        PaginatorAction::Last => Some(total.saturating_sub(1)),
    }
}

fn paginator_action_descriptors(enabled: bool) -> [ActionDescriptor; 4] {
    let availability = if enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    };
    PAGINATOR_ACTION_DESCRIPTORS
        .clone()
        .map(|descriptor| descriptor.with_availability(availability))
}

fn paginator_page_id(root: &NodeId, page: usize) -> NodeId {
    NodeId::new(format!("{}/page/{page}", root.as_str()))
}

#[cfg(test)]
mod tests {
    use super::{
        PaginatorAction, normalized_page, page_for_action, paginator_action_descriptors,
        paginator_window,
    };

    #[test]
    fn paging_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/paginator.txt",
            "widget-paginator",
            &[
                "total",
                "page",
                "limit",
                "normalized",
                "start",
                "end",
                "previous",
                "next",
            ],
        ) else {
            return;
        };
        for record in records {
            let total = number(record.field("total"));
            let page = number(record.field("page"));
            let limit = number(record.field("limit"));
            let normalized = normalized_page(page, total);
            let actual = normalized.map_or_else(|| "none".to_owned(), |page| page.to_string());
            assert_eq!(actual, record.field("normalized"), "case {}", record.id);
            assert_eq!(
                paginator_window(total, page, limit),
                (number(record.field("start")), number(record.field("end"))),
                "case {}",
                record.id
            );
            if normalized.is_none() {
                continue;
            }
            assert_eq!(
                page_for_action(page, total, PaginatorAction::Previous),
                Some(number(record.field("previous"))),
                "case {} previous",
                record.id
            );
            assert_eq!(
                page_for_action(page, total, PaginatorAction::Next),
                Some(number(record.field("next"))),
                "case {} next",
                record.id
            );
        }
    }

    #[test]
    fn descriptor_clones_reuse_immutable_storage() {
        let enabled = paginator_action_descriptors(true);
        let disabled = paginator_action_descriptors(false);

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

    fn number(value: &str) -> usize {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid usize {value}: {error}"))
    }
}
