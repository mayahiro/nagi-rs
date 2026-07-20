use std::sync::Arc;

use nagi_tui::{Event, EventResult, KeyAction, KeyCode, Node, NodeId, Style};

use crate::event::is_activation_event;

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

    /// Builds the public semantic node for this paginator
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let page = normalized_page(self.page, self.total);
        let style = if !self.enabled || page.is_none() {
            self.style.disabled
        } else {
            self.style.normal
        };
        if self.mode == PaginatorMode::Numeric || page.is_none() {
            let current = page.map_or(0, |page| page.saturating_add(1));
            let node = Node::styled_text(format!("{current}/{}", self.total), style);
            let Some(page) = page.filter(|_| self.enabled) else {
                return node.with_id(self.id);
            };
            let id = self.id;
            let focus_id = id.clone();
            let on_change = self.on_change;
            return node
                .focusable(id.clone())
                .with_focused_style(self.style.focused)
                .on_event(id, move |event| {
                    paginator_event_result(event, page, self.total, &focus_id, &on_change)
                });
        }

        let page = page.expect("dots require a page");
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
                let focus_id = self.id.clone();
                let event_id = self.id.clone();
                let on_change = Arc::clone(&self.on_change);
                children.push(
                    Node::column([selected])
                        .focusable(self.id.clone())
                        .with_focused_style(self.style.focused)
                        .on_event(event_id, move |event| {
                            paginator_event_result(event, page, self.total, &focus_id, &on_change)
                        }),
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
                    if !is_activation_event(event) {
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
            root.with_id(self.id)
        }
    }
}

fn paginator_event_result<Message>(
    event: &Event,
    page: usize,
    total: usize,
    focus_id: &NodeId,
    on_change: &Arc<dyn Fn(usize) -> Message>,
) -> EventResult<Message> {
    let Some(next) = page_for_event(page, total, event) else {
        return EventResult::ignored();
    };
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

fn page_for_event(page: usize, total: usize, event: &Event) -> Option<usize> {
    let page = normalized_page(page, total)?;
    let Event::Key(key) = event else {
        return None;
    };
    if key.action == KeyAction::Release
        || key.modifiers.alt
        || key.modifiers.control
        || key.modifiers.meta
    {
        return None;
    }
    match key.code {
        KeyCode::Left | KeyCode::Up | KeyCode::PageUp => Some(page.saturating_sub(1)),
        KeyCode::Right | KeyCode::Down | KeyCode::PageDown => {
            Some(page.saturating_add(1).min(total.saturating_sub(1)))
        }
        KeyCode::Home => Some(0),
        KeyCode::End => Some(total.saturating_sub(1)),
        _ => None,
    }
}

fn paginator_page_id(root: &NodeId, page: usize) -> NodeId {
    NodeId::new(format!("{}/page/{page}", root.as_str()))
}

#[cfg(test)]
mod tests {
    use nagi_tui::{Event, KeyAction, KeyCode, KeyEvent, KeyProtocol, Modifiers};

    use super::{normalized_page, page_for_event, paginator_window};

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
                page_for_event(page, total, &key(KeyCode::Left)),
                Some(number(record.field("previous"))),
                "case {} previous",
                record.id
            );
            assert_eq!(
                page_for_event(page, total, &key(KeyCode::Right)),
                Some(number(record.field("next"))),
                "case {} next",
                record.id
            );
        }
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers::NONE,
            action: KeyAction::Press,
            text: None,
            protocol: KeyProtocol::Legacy,
        })
    }

    fn number(value: &str) -> usize {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid usize {value}: {error}"))
    }
}
