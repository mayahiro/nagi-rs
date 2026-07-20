use std::sync::Arc;

use nagi_tui::{EventResult, Length, Node, NodeId, ScrollAxis, ScrollViewportOptions, Style};

use crate::event::is_activation_event;
use crate::navigation::navigate_event;

/// One stable item rendered by a [`List`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListItem {
    id: NodeId,
    label: String,
}

impl ListItem {
    /// Creates an item with an application-defined stable identity
    #[must_use]
    pub fn new(id: impl Into<NodeId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }

    /// Returns the item's stable identity
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns the displayed label
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Visual styles used by a [`List`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListStyle {
    /// Style used by unselected items
    pub normal: Style,
    /// Style used by the application-selected item
    pub selected: Style,
    /// Style merged over the item that owns runtime focus
    pub focused: Style,
    /// Style used by every item while the list is disabled
    pub disabled: Style,
}

impl Default for ListStyle {
    fn default() -> Self {
        Self {
            normal: Style::default(),
            selected: Style {
                reverse: true,
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

/// A vertically arranged, keyboard and pointer selectable collection
pub struct List<Message> {
    id: NodeId,
    items: Vec<ListItem>,
    selected: usize,
    enabled: bool,
    style: ListStyle,
    filter: String,
    window: Option<(usize, usize)>,
    viewport: Option<(NodeId, Length)>,
    on_select: Arc<dyn Fn(usize) -> Message>,
}

impl<Message: 'static> List<Message> {
    /// Creates an enabled list using application-owned selection state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        items: impl IntoIterator<Item = ListItem>,
        selected: usize,
        on_select: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            items: items.into_iter().collect(),
            selected,
            enabled: true,
            style: ListStyle::default(),
            filter: String::new(),
            window: None,
            viewport: None,
            on_select: Arc::new(on_select),
        }
    }

    /// Sets whether the list can receive focus and emit selection messages
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the list styles
    #[must_use]
    pub const fn style(mut self, style: ListStyle) -> Self {
        self.style = style;
        self
    }

    /// Keeps items whose labels contain `query` using ASCII case folding
    ///
    /// Selection callbacks continue to receive original item indices
    #[must_use]
    pub fn filter(mut self, query: impl Into<String>) -> Self {
        self.filter = query.into();
        self
    }

    /// Limits rendering and navigation to a filtered item window
    ///
    /// A zero limit renders an empty window
    #[must_use]
    pub const fn window(mut self, offset: usize, limit: usize) -> Self {
        self.window = Some((offset, limit));
        self
    }

    /// Limits rendering to one zero-based page after filtering
    ///
    /// A zero page size renders an empty page
    #[must_use]
    pub const fn paginate(self, page: usize, page_size: usize) -> Self {
        self.window(page.saturating_mul(page_size), page_size)
    }

    /// Wraps the list in a Core ScrollViewport using a sizing rule
    ///
    /// `viewport_id` must differ from the list root and item IDs. Applications
    /// may control its retained offset through `Runtime::set_scroll_offset`.
    /// This does not limit item construction; use [`Self::window`] or
    /// [`Self::paginate`] before the viewport for large collections
    #[must_use]
    pub fn viewport(mut self, viewport_id: impl Into<NodeId>, height: Length) -> Self {
        self.viewport = Some((viewport_id.into(), height));
        self
    }

    /// Builds the public semantic node for this list
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let mut visible = list_visible_indices(&self.items, &self.filter);
        if let Some((offset, limit)) = self.window {
            visible = list_window(&visible, offset, limit);
        }
        let selected = normalized_list_selection(&visible, self.selected);
        let visible = Arc::new(visible);
        let mut children = Vec::with_capacity(visible.len());
        let root_id = self.id;
        for (position, original_index) in visible.iter().copied().enumerate() {
            let item = &self.items[original_index];
            let is_selected = selected == Some(position);
            let marker = if is_selected { "> " } else { "  " };
            let style = if self.enabled {
                if is_selected {
                    self.style.selected
                } else {
                    self.style.normal
                }
            } else {
                self.style.disabled
            };
            let content = format!("{marker}{}", item.label);
            if !self.enabled {
                children.push(Node::styled_text(content, style).with_id(item.id.clone()));
                continue;
            }
            let item_id = item.id.clone();
            let click_focus = root_id.clone();
            let click_select = Arc::clone(&self.on_select);
            let row = Node::styled_text(content, style)
                .with_id(item_id.clone())
                .on_event(item_id, move |event| {
                    if is_activation_event(event) {
                        EventResult::message(click_select(original_index))
                            .focus(click_focus.clone())
                    } else {
                        EventResult::ignored()
                    }
                });
            if !is_selected {
                children.push(row);
                continue;
            }
            let navigation_focus = root_id.clone();
            let navigation_select = Arc::clone(&self.on_select);
            let navigation_visible = Arc::clone(&visible);
            children.push(
                Node::column([row])
                    .focusable(root_id.clone())
                    .with_focused_style(self.style.focused)
                    .on_event(root_id.clone(), move |event| {
                        let Some(next) = navigate_event(event, navigation_visible.len(), position)
                        else {
                            return EventResult::ignored();
                        };
                        let mut result = EventResult::consumed().focus(navigation_focus.clone());
                        if next != position {
                            result = result.emit(navigation_select(navigation_visible[next]));
                        }
                        result
                    }),
            );
        }

        let root = if self.enabled && selected.is_some() {
            Node::column(children)
        } else {
            Node::column(children).with_id(root_id)
        };
        with_list_viewport(root, self.viewport)
    }
}

fn with_list_viewport<Message>(
    root: Node<Message>,
    viewport: Option<(NodeId, Length)>,
) -> Node<Message> {
    let Some((id, height)) = viewport else {
        return root;
    };
    Node::column([Node::scroll_viewport_with_options(
        id,
        root,
        ScrollViewportOptions {
            axis: ScrollAxis::Vertical,
            ensure_focused_visible: true,
            ..ScrollViewportOptions::default()
        },
    )
    .tab_stop(false)
    .with_length(height)])
}

fn list_visible_indices(items: &[ListItem], query: &str) -> Vec<usize> {
    let query = query.to_ascii_lowercase();
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            (query.is_empty() || item.label.to_ascii_lowercase().contains(&query)).then_some(index)
        })
        .collect()
}

fn list_window(indices: &[usize], offset: usize, limit: usize) -> Vec<usize> {
    let start = offset.min(indices.len());
    let end = start.saturating_add(limit).min(indices.len());
    indices[start..end].to_vec()
}

fn normalized_list_selection(visible: &[usize], selected: usize) -> Option<usize> {
    if visible.is_empty() {
        return None;
    }
    Some(
        visible
            .iter()
            .rposition(|index| *index <= selected)
            .unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use crate::navigation::{Navigation, navigate};

    use super::{ListItem, list_visible_indices, list_window, normalized_list_selection};

    #[test]
    fn navigation_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/list-navigation.txt",
            "widget-list-navigation",
            &["count", "selected", "action", "expected"],
        ) else {
            return;
        };
        for record in records {
            let actual = navigate(
                number(record.field("count")),
                number(record.field("selected")),
                action(record.field("action")),
            );
            let expected = match record.field("expected") {
                "none" => None,
                value => Some(number(value)),
            };
            assert_eq!(actual, expected, "case {}", record.id);
        }
    }

    #[test]
    fn filtering_window_and_selection_match_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/list-view.txt",
            "widget-list-view",
            &[
                "labels",
                "query",
                "offset",
                "limit",
                "selected",
                "visible",
                "normalized",
            ],
        ) else {
            return;
        };
        for record in records {
            let items: Vec<_> = record
                .text("labels")
                .split('|')
                .map(|label| ListItem::new(label, label))
                .collect();
            let visible = list_window(
                &list_visible_indices(&items, &record.text("query")),
                number(record.field("offset")),
                number(record.field("limit")),
            );
            assert_eq!(
                indices(&visible),
                record.field("visible"),
                "case {}",
                record.id
            );
            let normalized = normalized_list_selection(&visible, number(record.field("selected")))
                .map_or_else(|| "none".to_owned(), |value| value.to_string());
            assert_eq!(normalized, record.field("normalized"), "case {}", record.id);
        }
    }

    fn action(value: &str) -> Navigation {
        match value {
            "normalize" => Navigation::Normalize,
            "up" => Navigation::Up,
            "down" => Navigation::Down,
            "home" => Navigation::Home,
            "end" => Navigation::End,
            _ => panic!("invalid navigation {value}"),
        }
    }

    fn number(value: &str) -> usize {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
    }

    fn indices(values: &[usize]) -> String {
        if values.is_empty() {
            return "-".to_owned();
        }
        values
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }
}
