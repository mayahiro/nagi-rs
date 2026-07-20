use std::sync::Arc;

use nagi_tui::{Event, EventResult, KeyAction, KeyCode, Length, Node, NodeId, Style};

use crate::event::is_activation_event;

/// Application-supplied inert filesystem metadata
///
/// [`FilePicker`] never reads `path` or accesses the filesystem itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilePickerEntry {
    id: NodeId,
    name: String,
    path: String,
    directory: bool,
    hidden: bool,
}

impl FilePickerEntry {
    /// Creates one visible file entry
    #[must_use]
    pub fn file(id: impl Into<NodeId>, name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            path: path.into(),
            directory: false,
            hidden: false,
        }
    }

    /// Creates one visible directory entry
    #[must_use]
    pub fn directory(
        id: impl Into<NodeId>,
        name: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            path: path.into(),
            directory: true,
            hidden: false,
        }
    }

    /// Replaces whether the entry is hidden by default
    #[must_use]
    pub const fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    /// Returns the stable identity
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns the displayed basename or label
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns inert application metadata
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Reports whether activation should navigate into the entry
    #[must_use]
    pub const fn is_directory(&self) -> bool {
        self.directory
    }

    /// Reports whether the entry is hidden by default
    #[must_use]
    pub const fn is_hidden(&self) -> bool {
        self.hidden
    }
}

/// Visual styles used by a [`FilePicker`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilePickerStyle {
    /// Style used by ordinary files
    pub normal: Style,
    /// Style merged over directory entries
    pub directory: Style,
    /// Style merged over hidden entries when shown
    pub hidden: Style,
    /// Style merged over the application-selected entry
    pub selected: Style,
    /// Style merged over the entry that owns focus
    pub focused: Style,
    /// Style used when selection changes are unavailable
    pub disabled: Style,
    /// Style used when no entries are visible
    pub placeholder: Style,
}

impl Default for FilePickerStyle {
    fn default() -> Self {
        Self {
            normal: Style::default(),
            directory: Style {
                bold: true,
                ..Style::default()
            },
            hidden: Style {
                dim: true,
                ..Style::default()
            },
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
            placeholder: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// A controlled browser over application-supplied entries
pub struct FilePicker<Message> {
    id: NodeId,
    entries: Vec<FilePickerEntry>,
    selected: usize,
    viewport_height: usize,
    show_hidden: bool,
    placeholder: String,
    enabled: bool,
    style: FilePickerStyle,
    on_select: Arc<dyn Fn(usize) -> Message>,
    on_open: Option<Arc<dyn Fn(usize) -> Message>>,
    on_back: Option<Arc<dyn Fn() -> Message>>,
}

impl<Message: 'static> FilePicker<Message> {
    /// Creates an enabled controlled entry browser
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        entries: impl IntoIterator<Item = FilePickerEntry>,
        selected: usize,
        on_select: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            entries: entries.into_iter().collect(),
            selected,
            viewport_height: 0,
            show_hidden: false,
            placeholder: "No entries".to_owned(),
            enabled: true,
            style: FilePickerStyle::default(),
            on_select: Arc::new(on_select),
            on_open: None,
            on_back: None,
        }
    }

    /// Sets the handler receiving an original entry index on activation
    #[must_use]
    pub fn on_open(mut self, handler: impl Fn(usize) -> Message + 'static) -> Self {
        self.on_open = Some(Arc::new(handler));
        self
    }

    /// Sets the handler used by Left and Backspace
    #[must_use]
    pub fn on_back(mut self, handler: impl Fn() -> Message + 'static) -> Self {
        self.on_back = Some(Arc::new(handler));
        self
    }

    /// Sets whether hidden entries remain visible
    #[must_use]
    pub const fn show_hidden(mut self, show: bool) -> Self {
        self.show_hidden = show;
        self
    }

    /// Limits rendering to a selection-following entry window
    #[must_use]
    pub const fn viewport(mut self, height: usize) -> Self {
        self.viewport_height = height;
        self
    }

    /// Replaces text shown when no entries are visible
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Sets whether the picker can receive focus and emit messages
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the file picker styles
    #[must_use]
    pub const fn style(mut self, style: FilePickerStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this file picker
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let visible_indices = visible_indices(&self.entries, self.show_hidden);
        let Some(selected_position) = normalized_selection(&visible_indices, self.selected) else {
            return Node::styled_text(self.placeholder, self.style.placeholder).with_id(self.id);
        };
        let (start, end) = if self.viewport_height > 0 {
            viewport_range(
                visible_indices.len(),
                selected_position,
                self.viewport_height,
            )
        } else {
            (0, visible_indices.len())
        };
        let visible = Arc::new(visible_indices);
        let mut children = Vec::with_capacity(end.saturating_sub(start));
        for position in start..end {
            let original_index = visible[position];
            let entry = &self.entries[original_index];
            let is_selected = position == selected_position;
            let mut style = self.style.normal;
            if entry.directory {
                style = style.merged(self.style.directory);
            }
            if entry.hidden {
                style = style.merged(self.style.hidden);
            }
            if is_selected {
                style = style.merged(self.style.selected);
            }
            if !self.enabled {
                style = self.style.disabled;
            }
            let prefix = if entry.directory { "▸ " } else { "  " };
            let row = Node::styled_text(format!("{prefix}{}", entry.name), style);
            if !self.enabled {
                children.push(row.with_id(entry.id.clone()));
                continue;
            }
            if is_selected {
                let visible = Arc::clone(&visible);
                let on_select = Arc::clone(&self.on_select);
                let on_open = self.on_open.as_ref().map(Arc::clone);
                let on_back = self.on_back.as_ref().map(Arc::clone);
                let focus_id = self.id.clone();
                children.push(
                    Node::column([row.with_id(entry.id.clone())])
                        .focusable(self.id.clone())
                        .with_focused_style(self.style.focused)
                        .on_event(self.id.clone(), move |event| {
                            selected_event_result(
                                event,
                                &visible,
                                selected_position,
                                self.viewport_height,
                                &focus_id,
                                &on_select,
                                on_open.as_ref(),
                                on_back.as_ref(),
                            )
                        }),
                );
                continue;
            }
            let id = entry.id.clone();
            let focus_id = self.id.clone();
            let on_select = Arc::clone(&self.on_select);
            let on_open = self.on_open.as_ref().map(Arc::clone);
            children.push(row.with_id(id.clone()).on_event(id, move |event| {
                if !is_activation_event(event) {
                    return EventResult::ignored();
                }
                let mut result = EventResult::consumed()
                    .focus(focus_id.clone())
                    .emit(on_select(original_index));
                if let Some(on_open) = &on_open {
                    result = result.emit(on_open(original_index));
                }
                result
            }));
        }
        let mut root = Node::column(children);
        if self.viewport_height > 0 {
            root = root.with_length(Length::Fixed(
                u32::try_from(self.viewport_height).unwrap_or(u32::MAX),
            ));
        }
        if self.enabled {
            root
        } else {
            root.with_id(self.id)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn selected_event_result<Message>(
    event: &Event,
    visible: &[usize],
    selected: usize,
    viewport_height: usize,
    focus_id: &NodeId,
    on_select: &Arc<dyn Fn(usize) -> Message>,
    on_open: Option<&Arc<dyn Fn(usize) -> Message>>,
    on_back: Option<&Arc<dyn Fn() -> Message>>,
) -> EventResult<Message> {
    if is_activation_event(event) {
        let result = EventResult::consumed().focus(focus_id.clone());
        return match on_open {
            Some(on_open) => result.emit(on_open(visible[selected])),
            None => result,
        };
    }
    let Some(action) = action_for_event(selected, visible.len(), viewport_height, event) else {
        return EventResult::ignored();
    };
    let result = EventResult::consumed().focus(focus_id.clone());
    match action {
        FilePickerAction::Select(position) if position != selected => {
            result.emit(on_select(visible[position]))
        }
        FilePickerAction::Select(_) => result,
        FilePickerAction::Open => on_open.map_or_else(EventResult::ignored, |on_open| {
            result.emit(on_open(visible[selected]))
        }),
        FilePickerAction::Back => {
            on_back.map_or_else(EventResult::ignored, |on_back| result.emit(on_back()))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FilePickerAction {
    Select(usize),
    Open,
    Back,
}

fn action_for_event(
    selected: usize,
    count: usize,
    viewport_height: usize,
    event: &Event,
) -> Option<FilePickerAction> {
    if count == 0 {
        return None;
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
    let selected = selected.min(count.saturating_sub(1));
    match key.code {
        KeyCode::Up => Some(FilePickerAction::Select(selected.saturating_sub(1))),
        KeyCode::Down => Some(FilePickerAction::Select(
            selected.saturating_add(1).min(count.saturating_sub(1)),
        )),
        KeyCode::Home => Some(FilePickerAction::Select(0)),
        KeyCode::End => Some(FilePickerAction::Select(count.saturating_sub(1))),
        KeyCode::PageUp | KeyCode::PageDown => {
            let step = if viewport_height == 0 {
                count.min(10)
            } else {
                viewport_height
            };
            if key.code == KeyCode::PageUp {
                Some(FilePickerAction::Select(selected.saturating_sub(step)))
            } else {
                Some(FilePickerAction::Select(
                    selected.saturating_add(step).min(count.saturating_sub(1)),
                ))
            }
        }
        KeyCode::Right => Some(FilePickerAction::Open),
        KeyCode::Left | KeyCode::Backspace => Some(FilePickerAction::Back),
        _ => None,
    }
}

fn visible_indices(entries: &[FilePickerEntry], show_hidden: bool) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| (show_hidden || !entry.hidden).then_some(index))
        .collect()
}

fn normalized_selection(visible: &[usize], selected: usize) -> Option<usize> {
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

fn viewport_range(count: usize, selected: usize, height: usize) -> (usize, usize) {
    if count == 0 || height == 0 {
        return (0, count);
    }
    let height = height.min(count);
    let start = selected
        .min(count.saturating_sub(1))
        .saturating_sub(height / 2)
        .min(count.saturating_sub(height));
    (start, start.saturating_add(height))
}

#[cfg(test)]
mod tests {
    use nagi_tui::{Event, KeyAction, KeyCode, KeyEvent, KeyProtocol, Modifiers};

    use super::{
        FilePickerAction, FilePickerEntry, action_for_event, normalized_selection, viewport_range,
        visible_indices,
    };

    #[test]
    fn filtering_and_navigation_match_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/file-picker.txt",
            "widget-file-picker",
            &[
                "hidden",
                "show-hidden",
                "selected",
                "viewport",
                "visible",
                "normalized",
                "start",
                "end",
                "page-up",
                "page-down",
            ],
        ) else {
            return;
        };
        for record in records {
            let hidden: Vec<_> = if record.field("hidden") == "-" {
                Vec::new()
            } else {
                record.field("hidden").split(',').map(boolean).collect()
            };
            let entries: Vec<_> = hidden
                .into_iter()
                .enumerate()
                .map(|(index, hidden)| {
                    FilePickerEntry::file(index.to_string(), index.to_string(), index.to_string())
                        .hidden(hidden)
                })
                .collect();
            let visible = visible_indices(&entries, boolean(record.field("show-hidden")));
            assert_eq!(
                indices(&visible),
                record.field("visible"),
                "case {}",
                record.id
            );
            let selected = normalized_selection(&visible, number(record.field("selected")));
            let actual = selected.map_or_else(|| "none".to_owned(), |value| value.to_string());
            assert_eq!(actual, record.field("normalized"), "case {}", record.id);
            let viewport = number(record.field("viewport"));
            let range = if viewport > 0 {
                selected.map_or((0, visible.len()), |selected| {
                    viewport_range(visible.len(), selected, viewport)
                })
            } else {
                (0, visible.len())
            };
            assert_eq!(
                range,
                (number(record.field("start")), number(record.field("end"))),
                "case {}",
                record.id
            );
            let Some(selected) = selected else {
                continue;
            };
            assert_eq!(
                action_for_event(selected, visible.len(), viewport, &key(KeyCode::PageUp)),
                Some(FilePickerAction::Select(number(record.field("page-up")))),
                "case {} page-up",
                record.id
            );
            assert_eq!(
                action_for_event(selected, visible.len(), viewport, &key(KeyCode::PageDown)),
                Some(FilePickerAction::Select(number(record.field("page-down")))),
                "case {} page-down",
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

    fn boolean(value: &str) -> bool {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid bool {value}: {error}"))
    }

    fn indices(values: &[usize]) -> String {
        if values.is_empty() {
            "-".to_owned()
        } else {
            values
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        }
    }
}
