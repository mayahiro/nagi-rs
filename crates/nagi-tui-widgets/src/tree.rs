use std::sync::{Arc, LazyLock};

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, EventResult, KeyCode, Length, Node, NodeId, Style,
};

use crate::action::{
    COLLAPSE_ACTION_ID, COLLECTION_ACTIONS, CollectionAction, EXPAND_ACTION_ID,
    repeatable_action_binding, vertical_collection_action_descriptors,
};
use crate::event::is_pointer_activation_event;
use crate::navigation::navigate;

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
///
/// The root owns standard activation, vertical selection, collapse, and expand
/// actions. Left-button press stays raw on each row so keyboard rebinding does
/// not remove pointer selection or branch toggling
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

    /// Returns the ordered semantic action descriptors declared by this tree
    ///
    /// The order is activate, previous, next, first, last, collapse, and
    /// expand. Every descriptor is disabled-pass-through when the tree is
    /// disabled or empty
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; 7] {
        tree_action_descriptors(self.enabled && !self.items.is_empty())
    }

    /// Builds the public semantic node for this tree
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let visible_indices = visible_indices(&self.items);
        let selected_position = normalized_visible_selection(&visible_indices, self.selected);
        let descriptors = tree_action_descriptors(self.enabled && selected_position.is_some());
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
                if !is_pointer_activation_event(event) {
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
                children.push(tree_action_target(
                    Node::column([row]),
                    root_id.clone(),
                    Arc::clone(&visible),
                    Arc::clone(&item_metadata),
                    selected_position.expect("selected tree item"),
                    self.style.focused,
                    Arc::clone(&self.on_select),
                    self.on_toggle.as_ref().map(Arc::clone),
                    descriptors.clone(),
                ));
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
            root.with_id(root_id.clone()).on_actions(
                root_id,
                descriptors.map(|descriptor| Action::new(descriptor, |_| EventResult::ignored())),
            )
        }
    }
}

#[derive(Clone, Copy)]
struct TreeMetadata {
    depth: u16,
    has_children: bool,
    expanded: bool,
}

static TREE_DISCLOSURE_ACTION_DESCRIPTORS: LazyLock<[ActionDescriptor; 2]> = LazyLock::new(|| {
    [
        ActionDescriptor::new(
            COLLAPSE_ACTION_ID,
            "Collapse",
            [repeatable_action_binding(KeyCode::Left)],
        ),
        ActionDescriptor::new(
            EXPAND_ACTION_ID,
            "Expand",
            [repeatable_action_binding(KeyCode::Right)],
        ),
    ]
});

#[derive(Clone, Copy)]
enum TreeSemanticAction {
    Collection(CollectionAction),
    Collapse,
    Expand,
}

const TREE_ACTIONS: [TreeSemanticAction; 7] = [
    TreeSemanticAction::Collection(COLLECTION_ACTIONS[0]),
    TreeSemanticAction::Collection(COLLECTION_ACTIONS[1]),
    TreeSemanticAction::Collection(COLLECTION_ACTIONS[2]),
    TreeSemanticAction::Collection(COLLECTION_ACTIONS[3]),
    TreeSemanticAction::Collection(COLLECTION_ACTIONS[4]),
    TreeSemanticAction::Collapse,
    TreeSemanticAction::Expand,
];

#[derive(Clone, Copy)]
enum TreeTransition {
    Select(usize),
    Toggle(bool),
}

fn tree_action_descriptors(enabled: bool) -> [ActionDescriptor; 7] {
    let [activate, previous, next, first, last] = vertical_collection_action_descriptors(enabled);
    let availability = if enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    };
    [
        activate,
        previous,
        next,
        first,
        last,
        TREE_DISCLOSURE_ACTION_DESCRIPTORS[0]
            .clone()
            .with_availability(availability),
        TREE_DISCLOSURE_ACTION_DESCRIPTORS[1]
            .clone()
            .with_availability(availability),
    ]
}

#[allow(clippy::too_many_arguments)]
fn tree_action_target<Message: 'static>(
    node: Node<Message>,
    root_id: NodeId,
    visible: Arc<Vec<usize>>,
    items: Arc<Vec<TreeMetadata>>,
    selected: usize,
    focused_style: Style,
    on_select: Arc<dyn Fn(usize) -> Message>,
    on_toggle: Option<Arc<dyn Fn(usize, bool) -> Message>>,
    descriptors: [ActionDescriptor; 7],
) -> Node<Message> {
    node.focusable(root_id.clone())
        .with_focused_style(focused_style)
        .on_actions(
            root_id.clone(),
            tree_actions(
                descriptors,
                root_id,
                visible,
                items,
                selected,
                on_select,
                on_toggle,
            ),
        )
}

#[allow(clippy::too_many_arguments)]
fn tree_actions<Message: 'static>(
    descriptors: [ActionDescriptor; 7],
    root_id: NodeId,
    visible: Arc<Vec<usize>>,
    items: Arc<Vec<TreeMetadata>>,
    selected: usize,
    on_select: Arc<dyn Fn(usize) -> Message>,
    on_toggle: Option<Arc<dyn Fn(usize, bool) -> Message>>,
) -> impl Iterator<Item = Action<Message>> {
    descriptors
        .into_iter()
        .zip(TREE_ACTIONS)
        .map(move |(descriptor, action)| {
            let focus_id = root_id.clone();
            let visible = Arc::clone(&visible);
            let items = Arc::clone(&items);
            let on_select = Arc::clone(&on_select);
            let on_toggle = on_toggle.as_ref().map(Arc::clone);
            Action::new(descriptor, move |_| {
                tree_action_result(
                    action,
                    &focus_id,
                    &visible,
                    &items,
                    selected,
                    on_select.as_ref(),
                    on_toggle.as_deref(),
                )
            })
        })
}

#[allow(clippy::too_many_arguments)]
fn tree_action_result<Message>(
    action: TreeSemanticAction,
    root_id: &NodeId,
    visible: &[usize],
    items: &[TreeMetadata],
    selected: usize,
    on_select: &dyn Fn(usize) -> Message,
    on_toggle: Option<&dyn Fn(usize, bool) -> Message>,
) -> EventResult<Message> {
    let mut result = EventResult::consumed().focus(root_id.clone());
    match tree_transition(action, visible, items, selected) {
        Some(TreeTransition::Select(next)) if next != selected => {
            result = result.emit(on_select(visible[next]));
        }
        Some(TreeTransition::Toggle(expanded)) => {
            if let Some(on_toggle) = on_toggle {
                result = result.emit(on_toggle(visible[selected], expanded));
            }
        }
        Some(TreeTransition::Select(_)) | None => {}
    }
    result
}

fn tree_transition(
    action: TreeSemanticAction,
    visible: &[usize],
    items: &[TreeMetadata],
    selected: usize,
) -> Option<TreeTransition> {
    let current = items[visible[selected]];
    match action {
        TreeSemanticAction::Collection(CollectionAction::Activate) => current
            .has_children
            .then_some(TreeTransition::Toggle(!current.expanded)),
        TreeSemanticAction::Collection(action) => action
            .navigation()
            .and_then(|navigation| navigate(visible.len(), selected, navigation))
            .map(TreeTransition::Select),
        TreeSemanticAction::Collapse if current.has_children && current.expanded => {
            Some(TreeTransition::Toggle(false))
        }
        TreeSemanticAction::Collapse => (0..selected)
            .rev()
            .find(|position| items[visible[*position]].depth < current.depth)
            .map(TreeTransition::Select)
            .or(Some(TreeTransition::Select(selected))),
        TreeSemanticAction::Expand if current.has_children && !current.expanded => {
            Some(TreeTransition::Toggle(true))
        }
        TreeSemanticAction::Expand => {
            let child = selected.saturating_add(1);
            if current.has_children
                && child < visible.len()
                && items[visible[child]].depth > current.depth
            {
                Some(TreeTransition::Select(child))
            } else {
                Some(TreeTransition::Select(selected))
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{
        TreeItem, normalized_visible_selection, tree_action_descriptors, tree_viewport_range,
        visible_indices,
    };

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

    #[test]
    fn action_descriptor_clones_reuse_immutable_storage() {
        let enabled = tree_action_descriptors(true);
        let disabled = tree_action_descriptors(false);

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
            .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
    }
}
