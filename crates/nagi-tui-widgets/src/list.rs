use std::sync::Arc;

use nagi_tui::{
    EventResult, Insets, Length, Node, NodeId, ScrollAxis, ScrollOffset, ScrollViewportOptions,
    Size, Style, VirtualFragment,
};

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

    /// Wraps the list in a virtual Core ScrollViewport using a sizing rule
    ///
    /// `viewport_id` must differ from the list root and item IDs. Applications
    /// may control its retained offset through `Runtime::set_scroll_offset`.
    /// Semantic row construction is bounded by the visible height. Item
    /// metadata collection and filtering remain eager. Viewport rows are one
    /// Cell high and clip content that would otherwise wrap
    #[must_use]
    pub fn viewport(mut self, viewport_id: impl Into<NodeId>, height: Length) -> Self {
        self.viewport = Some((viewport_id.into(), height));
        self
    }

    /// Builds the public semantic node for this list
    #[must_use]
    pub fn into_node(mut self) -> Node<Message> {
        let mut visible = list_visible_indices(&self.items, &self.filter);
        if let Some((offset, limit)) = self.window {
            visible = list_window(&visible, offset, limit);
        }
        let selected = normalized_list_selection(&visible, self.selected);
        let visible = Arc::new(visible);
        if let Some((viewport_id, height)) = self.viewport.take() {
            return virtual_list_node(
                self.id,
                self.items,
                visible,
                selected,
                self.enabled,
                self.style,
                self.on_select,
                viewport_id,
                height,
            );
        }
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

        if self.enabled && selected.is_some() {
            Node::column(children)
        } else {
            Node::column(children).with_id(root_id)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn virtual_list_node<Message: 'static>(
    root_id: NodeId,
    items: Vec<ListItem>,
    visible: Arc<Vec<usize>>,
    selected: Option<usize>,
    enabled: bool,
    style: ListStyle,
    on_select: Arc<dyn Fn(usize) -> Message>,
    viewport_id: NodeId,
    height: Length,
) -> Node<Message> {
    let content_height = u32::try_from(visible.len()).unwrap_or(u32::MAX);
    let items = Arc::new(items);
    let outer_root_id = root_id.clone();
    let viewport = Node::virtual_scroll_viewport_with_options(
        viewport_id,
        Size::new(0, content_height),
        ScrollViewportOptions {
            axis: ScrollAxis::Vertical,
            ensure_focused_visible: true,
            ..ScrollViewportOptions::default()
        },
        move |viewport| {
            let start = usize::try_from(viewport.offset.y)
                .unwrap_or(usize::MAX)
                .min(visible.len());
            let count = usize::try_from(viewport.size.height).unwrap_or(usize::MAX);
            let end = start.saturating_add(count).min(visible.len());
            let rows = (start..end).map(|position| {
                virtual_list_row(
                    &items, &visible, selected, position, enabled, style, &root_id, &on_select,
                )
            });
            let visible_rows = Node::padding(
                Node::column(rows),
                Insets::new(u32::try_from(start).unwrap_or(u32::MAX), 0, 0, 0),
            );
            let mut layers = vec![visible_rows];
            if enabled {
                if let Some(position) =
                    selected.filter(|position| *position < start || *position >= end)
                {
                    let proxy = list_navigation_target(
                        Node::spacer(0, 1),
                        root_id.clone(),
                        Arc::clone(&visible),
                        position,
                        None,
                        Arc::clone(&on_select),
                    );
                    layers.push(Node::padding(
                        proxy,
                        Insets::new(u32::try_from(position).unwrap_or(u32::MAX), 0, 0, 0),
                    ));
                }
            }
            VirtualFragment::new(ScrollOffset::default(), Node::stack(layers))
        },
    )
    .tab_stop(false)
    .with_length(height);
    let root = Node::column([viewport]);
    if enabled && selected.is_some() {
        root
    } else {
        root.with_id(outer_root_id)
    }
}

#[allow(clippy::too_many_arguments)]
fn virtual_list_row<Message: 'static>(
    items: &[ListItem],
    visible: &Arc<Vec<usize>>,
    selected: Option<usize>,
    position: usize,
    enabled: bool,
    style: ListStyle,
    root_id: &NodeId,
    on_select: &Arc<dyn Fn(usize) -> Message>,
) -> Node<Message> {
    let original_index = visible[position];
    let item = &items[original_index];
    let is_selected = selected == Some(position);
    let marker = if is_selected { "> " } else { "  " };
    let row_style = if !enabled {
        style.disabled
    } else if is_selected {
        style.selected
    } else {
        style.normal
    };
    let content = format!("{marker}{}", item.label);
    if !enabled {
        return Node::styled_text(content, row_style)
            .with_id(item.id.clone())
            .with_length(Length::Fixed(1));
    }
    let item_id = item.id.clone();
    let click_focus = root_id.clone();
    let click_select = Arc::clone(on_select);
    let row = Node::styled_text(content, row_style)
        .with_id(item_id.clone())
        .on_event(item_id, move |event| {
            if is_activation_event(event) {
                EventResult::message(click_select(original_index)).focus(click_focus.clone())
            } else {
                EventResult::ignored()
            }
        });
    if !is_selected {
        return row.with_length(Length::Fixed(1));
    }
    list_navigation_target(
        Node::column([row]),
        root_id.clone(),
        Arc::clone(visible),
        position,
        Some(style.focused),
        Arc::clone(on_select),
    )
    .with_length(Length::Fixed(1))
}

fn list_navigation_target<Message: 'static>(
    node: Node<Message>,
    root_id: NodeId,
    visible: Arc<Vec<usize>>,
    position: usize,
    focused_style: Option<Style>,
    on_select: Arc<dyn Fn(usize) -> Message>,
) -> Node<Message> {
    let navigation_focus = root_id.clone();
    let mut node = node
        .focusable(root_id.clone())
        .on_event(root_id, move |event| {
            let Some(next) = navigate_event(event, visible.len(), position) else {
                return EventResult::ignored();
            };
            let mut result = EventResult::consumed().focus(navigation_focus.clone());
            if next != position {
                result = result.emit(on_select(visible[next]));
            }
            result
        });
    if let Some(style) = focused_style {
        node = node.with_focused_style(style);
    }
    node
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
