use std::sync::Arc;

use nagi_tui::{Event, EventResult, KeyAction, KeyCode, Node, NodeId, Style};

use crate::event::is_activation_event;
use crate::navigation::{Navigation, navigate};

/// One stable item rendered by [`Tabs`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TabItem {
    id: NodeId,
    label: String,
}

impl TabItem {
    /// Creates a tab with an application-defined stable identity
    #[must_use]
    pub fn new(id: impl Into<NodeId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }

    /// Returns the tab's stable identity
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

/// Visual styles used by [`Tabs`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabsStyle {
    /// Style used by unselected tabs
    pub normal: Style,
    /// Style used by the application-selected tab
    pub selected: Style,
    /// Style merged over the tab that owns runtime focus
    pub focused: Style,
    /// Style used by every tab while the set is disabled
    pub disabled: Style,
}

impl Default for TabsStyle {
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

/// A horizontal, keyboard and pointer selectable set of views
pub struct Tabs<Message> {
    id: NodeId,
    items: Vec<TabItem>,
    selected: usize,
    enabled: bool,
    style: TabsStyle,
    on_select: Arc<dyn Fn(usize) -> Message>,
}

impl<Message: 'static> Tabs<Message> {
    /// Creates enabled tabs using application-owned selection state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        items: impl IntoIterator<Item = TabItem>,
        selected: usize,
        on_select: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            items: items.into_iter().collect(),
            selected,
            enabled: true,
            style: TabsStyle::default(),
            on_select: Arc::new(on_select),
        }
    }

    /// Sets whether tabs can receive focus and emit selection messages
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the tab styles
    #[must_use]
    pub const fn style(mut self, style: TabsStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for these tabs
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let selected = navigate(self.items.len(), self.selected, Navigation::Normalize);
        let item_ids: Arc<Vec<NodeId>> =
            Arc::new(self.items.iter().map(|item| item.id.clone()).collect());
        let mut children = Vec::with_capacity(self.items.len());
        for (index, item) in self.items.into_iter().enumerate() {
            let is_selected = selected == Some(index);
            let content = if is_selected {
                format!("[{}]", item.label)
            } else {
                format!(" {} ", item.label)
            };
            let style = if !self.enabled {
                self.style.disabled
            } else if is_selected {
                self.style.selected
            } else {
                self.style.normal
            };
            if !self.enabled {
                children.push(Node::styled_text(content, style).with_id(item.id));
                continue;
            }
            let id = item.id;
            let focus_id = id.clone();
            let on_select = Arc::clone(&self.on_select);
            children.push(
                Node::styled_text(content, style)
                    .focusable(id.clone())
                    .with_focused_style(self.style.focused)
                    .on_event(id, move |event| {
                        if !is_activation_event(event) {
                            return EventResult::ignored();
                        }
                        let result = EventResult::consumed().focus(focus_id.clone());
                        if is_selected {
                            result
                        } else {
                            result.emit(on_select(index))
                        }
                    }),
            );
        }

        let root_id = self.id;
        let root = Node::row(children).with_id(root_id.clone());
        let Some(selected) = selected.filter(|_| self.enabled) else {
            return root;
        };
        let on_select = self.on_select;
        root.on_event(root_id, move |event| {
            let Some(next) = tabs_navigation_event(event, item_ids.len(), selected) else {
                return EventResult::ignored();
            };
            let mut result = EventResult::consumed().focus(item_ids[next].clone());
            if next != selected {
                result = result.emit(on_select(next));
            }
            result
        })
    }
}

fn tabs_navigation_event(event: &Event, count: usize, selected: usize) -> Option<usize> {
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
    let action = match key.code {
        KeyCode::Left => Navigation::Up,
        KeyCode::Right => Navigation::Down,
        KeyCode::Home => Navigation::Home,
        KeyCode::End => Navigation::End,
        _ => return None,
    };
    navigate(count, selected, action)
}
