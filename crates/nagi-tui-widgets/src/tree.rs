use std::sync::Arc;

use nagi_tui::{Event, EventResult, KeyAction, KeyCode, Length, Node, NodeId, Style};

use crate::event::is_activation_event;
use crate::navigation::{Navigation, navigate};

/// One preorder item rendered by a [`Tree`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeItem {
    id: NodeId,
    label: String,
    depth: u16,
    has_children: bool,
    expanded: bool,
}

impl TreeItem {
    /// Creates a leaf at the supplied zero-based depth
    #[must_use]
    pub fn leaf(id: impl Into<NodeId>, label: impl Into<String>, depth: u16) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            depth,
            has_children: false,
            expanded: false,
        }
    }

    /// Creates a branch with application-owned expansion state
    #[must_use]
    pub fn branch(
        id: impl Into<NodeId>,
        label: impl Into<String>,
        depth: u16,
        expanded: bool,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            depth,
            has_children: true,
            expanded,
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

    /// Returns the zero-based preorder depth
    #[must_use]
    pub const fn depth(&self) -> u16 {
        self.depth
    }

    /// Reports whether the item represents a branch
    #[must_use]
    pub const fn has_children(&self) -> bool {
        self.has_children
    }

    /// Reports application-owned expansion state
    #[must_use]
    pub const fn expanded(&self) -> bool {
        self.expanded
    }

    /// Replaces application-owned expansion state for this item
    #[must_use]
    pub const fn with_expanded(mut self, expanded: bool) -> Self {
        if self.has_children {
            self.expanded = expanded;
        }
        self
    }
}

/// Visual styles used by a [`Tree`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TreeStyle {
    /// Style used by unselected visible items
    pub normal: Style,
    /// Style used by the application-selected item
    pub selected: Style,
    /// Style merged over the selected item while the tree owns focus
    pub focused: Style,
    /// Style used by every item while the tree is disabled
    pub disabled: Style,
}

impl Default for TreeStyle {
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

/// A preorder tree with application-owned selection and expansion state
pub struct Tree<Message> {
    id: NodeId,
    items: Vec<TreeItem>,
    selected: usize,
    viewport_height: usize,
    enabled: bool,
    style: TreeStyle,
    on_select: Arc<dyn Fn(usize) -> Message>,
    on_toggle: Option<Arc<dyn Fn(usize, bool) -> Message>>,
}

impl<Message: 'static> Tree<Message> {
    /// Creates an enabled tree using an original preorder selection index
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        items: impl IntoIterator<Item = TreeItem>,
        selected: usize,
        on_select: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            items: items.into_iter().collect(),
            selected,
            viewport_height: 0,
            enabled: true,
            style: TreeStyle::default(),
            on_select: Arc::new(on_select),
            on_toggle: None,
        }
    }

    /// Sets the handler that receives original preorder index and next expansion state
    #[must_use]
    pub fn on_toggle(mut self, handler: impl Fn(usize, bool) -> Message + 'static) -> Self {
        self.on_toggle = Some(Arc::new(handler));
        self
    }

    /// Sets whether the tree can receive focus and emit messages
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the tree styles
    #[must_use]
    pub const fn style(mut self, style: TreeStyle) -> Self {
        self.style = style;
        self
    }

    /// Limits rendering to a deterministic window that follows selection
    ///
    /// A zero height disables the viewport. In viewport mode the Tree root ID
    /// remains the single stable keyboard focus target as the window moves.
    #[must_use]
    pub const fn viewport(mut self, height: usize) -> Self {
        self.viewport_height = height;
        self
    }

    /// Builds the public semantic node for this tree
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let visible_indices = visible_indices(&self.items);
        let selected_position = normalized_visible_selection(&visible_indices, self.selected);
        let item_metadata: Arc<Vec<TreeMetadata>> = Arc::new(
            self.items
                .iter()
                .map(|item| TreeMetadata {
                    depth: item.depth,
                    has_children: item.has_children,
                    expanded: item.expanded,
                })
                .collect(),
        );
        let viewport = self.viewport_height > 0;
        let (start, end) = if viewport {
            tree_viewport_range(
                visible_indices.len(),
                selected_position.unwrap_or(0),
                self.viewport_height,
            )
        } else {
            (0, visible_indices.len())
        };
        let visible = Arc::new(visible_indices);
        let root_id = self.id.clone();
        let mut children = Vec::with_capacity(end.saturating_sub(start));
        for position in start..end {
            let original_index = visible[position];
            let item = &self.items[original_index];
            let is_selected = selected_position == Some(position);
            let style = if !self.enabled {
                self.style.disabled
            } else if is_selected {
                self.style.selected
            } else {
                self.style.normal
            };
            let disclosure = match (item.has_children, item.expanded) {
                (true, true) => "▼ ",
                (true, false) => "▶ ",
                (false, _) => "  ",
            };
            let content = format!(
                "{}{}{}",
                "  ".repeat(usize::from(item.depth)),
                disclosure,
                item.label
            );
            let node = Node::styled_text(content, style);
            if !self.enabled {
                children.push(node.with_id(item.id.clone()));
                continue;
            }
            let id = item.id.clone();
            let click_focus = root_id.clone();
            let click_select = Arc::clone(&self.on_select);
            let click_toggle = self.on_toggle.as_ref().map(Arc::clone);
            let has_children = item.has_children;
            let expanded = item.expanded;
            let row = node.with_id(id.clone()).on_event(id, move |event| {
                if !is_activation_event(event) {
                    return EventResult::ignored();
                }
                let mut result = EventResult::consumed().focus(click_focus.clone());
                if !is_selected {
                    result = result.emit(click_select(original_index));
                }
                if has_children {
                    if let Some(on_toggle) = &click_toggle {
                        result = result.emit(on_toggle(original_index, !expanded));
                    }
                }
                result
            });
            if is_selected {
                let visible = Arc::clone(&visible);
                let item_metadata = Arc::clone(&item_metadata);
                let on_select = Arc::clone(&self.on_select);
                let on_toggle = self.on_toggle.as_ref().map(Arc::clone);
                let focus_id = root_id.clone();
                children.push(
                    Node::column([row])
                        .focusable(root_id.clone())
                        .with_focused_style(self.style.focused)
                        .on_event(root_id.clone(), move |event| {
                            tree_event_result(
                                event,
                                &visible,
                                &item_metadata,
                                selected_position.expect("selected tree item"),
                                &focus_id,
                                &on_select,
                                on_toggle.as_ref(),
                            )
                        }),
                );
                continue;
            }
            children.push(row);
        }

        let mut root = Node::column(children);
        if viewport {
            root = root.with_length(Length::Fixed(
                u32::try_from(self.viewport_height).unwrap_or(u32::MAX),
            ));
        }
        if self.enabled && selected_position.is_some() {
            root
        } else {
            root.with_id(root_id)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn tree_event_result<Message>(
    event: &Event,
    visible: &[usize],
    items: &[TreeMetadata],
    selected_position: usize,
    root_id: &NodeId,
    on_select: &Arc<dyn Fn(usize) -> Message>,
    on_toggle: Option<&Arc<dyn Fn(usize, bool) -> Message>>,
) -> EventResult<Message> {
    if is_activation_event(event) {
        let original_index = visible[selected_position];
        let current = items[original_index];
        let result = EventResult::consumed().focus(root_id.clone());
        if current.has_children {
            if let Some(on_toggle) = on_toggle {
                return result.emit(on_toggle(original_index, !current.expanded));
            }
        }
        return result;
    }
    let Some(action) = tree_navigation_event(event, visible, items, selected_position) else {
        return EventResult::ignored();
    };
    match action {
        TreeAction::Select(next) => {
            let mut result = EventResult::consumed().focus(root_id.clone());
            if next != selected_position {
                result = result.emit(on_select(visible[next]));
            }
            result
        }
        TreeAction::Toggle(expanded) => {
            let original_index = visible[selected_position];
            let result = EventResult::consumed().focus(root_id.clone());
            if let Some(on_toggle) = on_toggle {
                result.emit(on_toggle(original_index, expanded))
            } else {
                result
            }
        }
    }
}

#[derive(Clone, Copy)]
struct TreeMetadata {
    depth: u16,
    has_children: bool,
    expanded: bool,
}

enum TreeAction {
    Select(usize),
    Toggle(bool),
}

fn visible_indices(items: &[TreeItem]) -> Vec<usize> {
    let mut visible = Vec::with_capacity(items.len());
    let mut collapsed_depth = None;
    for (index, item) in items.iter().enumerate() {
        if let Some(depth) = collapsed_depth {
            if item.depth > depth {
                continue;
            }
            collapsed_depth = None;
        }
        visible.push(index);
        if item.has_children && !item.expanded {
            collapsed_depth = Some(item.depth);
        }
    }
    visible
}

fn normalized_visible_selection(visible: &[usize], selected: usize) -> Option<usize> {
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

fn tree_viewport_range(count: usize, selected: usize, height: usize) -> (usize, usize) {
    if count == 0 || height == 0 {
        return (0, count);
    }
    let height = height.min(count);
    let selected = selected.min(count.saturating_sub(1));
    let start = selected
        .saturating_sub(height / 2)
        .min(count.saturating_sub(height));
    (start, start.saturating_add(height))
}

fn tree_navigation_event(
    event: &Event,
    visible: &[usize],
    items: &[TreeMetadata],
    selected: usize,
) -> Option<TreeAction> {
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
        KeyCode::Up => navigate(visible.len(), selected, Navigation::Up).map(TreeAction::Select),
        KeyCode::Down => {
            navigate(visible.len(), selected, Navigation::Down).map(TreeAction::Select)
        }
        KeyCode::Home => {
            navigate(visible.len(), selected, Navigation::Home).map(TreeAction::Select)
        }
        KeyCode::End => navigate(visible.len(), selected, Navigation::End).map(TreeAction::Select),
        KeyCode::Left => {
            let current = items[visible[selected]];
            if current.has_children && current.expanded {
                return Some(TreeAction::Toggle(false));
            }
            (0..selected)
                .rev()
                .find(|position| items[visible[*position]].depth < current.depth)
                .map(TreeAction::Select)
                .or(Some(TreeAction::Select(selected)))
        }
        KeyCode::Right => {
            let current = items[visible[selected]];
            if current.has_children && !current.expanded {
                return Some(TreeAction::Toggle(true));
            }
            let child = selected.saturating_add(1);
            if current.has_children
                && child < visible.len()
                && items[visible[child]].depth > current.depth
            {
                Some(TreeAction::Select(child))
            } else {
                Some(TreeAction::Select(selected))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{TreeItem, normalized_visible_selection, tree_viewport_range, visible_indices};

    #[test]
    fn collapsed_branches_hide_only_their_descendants() {
        let items = [
            TreeItem::branch("a", "A", 0, false),
            TreeItem::leaf("a-child", "child", 1),
            TreeItem::branch("b", "B", 0, true),
            TreeItem::leaf("b-child", "child", 1),
        ];
        let visible = visible_indices(&items);
        assert_eq!(visible, [0, 2, 3]);
        assert_eq!(normalized_visible_selection(&visible, 1), Some(0));
        assert_eq!(normalized_visible_selection(&visible, 99), Some(2));
    }

    #[test]
    fn viewport_ranges_match_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/tree-viewport.txt",
            "widget-tree-viewport",
            &["count", "selected", "height", "start", "end"],
        ) else {
            return;
        };
        for record in records {
            let actual = tree_viewport_range(
                number(record.field("count")),
                number(record.field("selected")),
                number(record.field("height")),
            );
            assert_eq!(
                actual,
                (number(record.field("start")), number(record.field("end"))),
                "case {}",
                record.id
            );
        }
    }

    fn number(value: &str) -> usize {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
    }
}
