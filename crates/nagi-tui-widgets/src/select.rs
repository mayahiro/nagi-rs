use std::sync::Arc;

use nagi_tui::{Event, EventResult, KeyAction, KeyCode, Node, NodeId, Style};

use crate::event::is_activation_event;
use crate::navigation::{Navigation, navigate};

/// Visual styles used by a [`Select`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectStyle {
    /// Style used while the selector is enabled and unfocused
    pub normal: Style,
    /// Style merged over the selector while it owns focus
    pub focused: Style,
    /// Style used when the selector is disabled or empty
    pub disabled: Style,
}

impl Default for SelectStyle {
    fn default() -> Self {
        Self {
            normal: Style::default(),
            focused: Style {
                reverse: true,
                ..Style::default()
            },
            disabled: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// A compact selector that exposes one application-owned option at a time
pub struct Select<Message> {
    id: NodeId,
    options: Vec<String>,
    selected: usize,
    enabled: bool,
    placeholder: String,
    style: SelectStyle,
    on_select: Arc<dyn Fn(usize) -> Message>,
}

impl<Message: 'static> Select<Message> {
    /// Creates an enabled selector using application-owned selection state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        options: impl IntoIterator<Item = impl Into<String>>,
        selected: usize,
        on_select: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            options: options.into_iter().map(Into::into).collect(),
            selected,
            enabled: true,
            placeholder: "No options".to_owned(),
            style: SelectStyle::default(),
            on_select: Arc::new(on_select),
        }
    }

    /// Sets whether the selector can receive focus and change selection
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets the text displayed when there are no options
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Replaces the selector styles
    #[must_use]
    pub const fn style(mut self, style: SelectStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this selector
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let selected = navigate(self.options.len(), self.selected, Navigation::Normalize);
        let content = selected.map_or_else(
            || format!("< {} >", self.placeholder),
            |index| format!("< {} >", self.options[index]),
        );
        let Some(selected) = selected.filter(|_| self.enabled) else {
            return Node::styled_text(content, self.style.disabled).with_id(self.id);
        };

        let id = self.id;
        let focus_id = id.clone();
        let count = self.options.len();
        let on_select = self.on_select;
        Node::styled_text(content, self.style.normal)
            .focusable(id.clone())
            .with_focused_style(self.style.focused)
            .on_event(id, move |event| {
                let Some(next) = select_event(event, count, selected) else {
                    return EventResult::ignored();
                };
                let mut result = EventResult::consumed().focus(focus_id.clone());
                if next != selected {
                    result = result.emit(on_select(next));
                }
                result
            })
    }
}

fn select_event(event: &Event, count: usize, selected: usize) -> Option<usize> {
    if is_activation_event(event) {
        return Some((selected + 1) % count);
    }
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
        KeyCode::Left | KeyCode::Up => Navigation::Up,
        KeyCode::Right | KeyCode::Down => Navigation::Down,
        KeyCode::Home => Navigation::Home,
        KeyCode::End => Navigation::End,
        _ => return None,
    };
    navigate(count, selected, action)
}
