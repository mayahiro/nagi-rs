use std::ops::Range;
use std::sync::Arc;

use nagi_text::{
    WidthProfile, cell_at_byte, grapheme_boundaries, grapheme_width, graphemes,
    next_grapheme_boundary, previous_grapheme_boundary, truncate,
};
use nagi_tui::{Event, EventResult, KeyAction, KeyCode, Node, NodeId, Style};

/// Application-owned value and grapheme-aligned cursor for a [`TextArea`]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextAreaState {
    value: String,
    cursor: usize,
    selection_anchor: usize,
    has_selection: bool,
    horizontal_offset: usize,
}

impl TextAreaState {
    /// Creates state and clamps `cursor` down to a grapheme boundary
    #[must_use]
    pub fn new(value: impl Into<String>, cursor: usize) -> Self {
        let value = value.into();
        normalize_state(Self {
            value,
            cursor,
            selection_anchor: 0,
            has_selection: false,
            horizontal_offset: 0,
        })
    }

    /// Creates state with the cursor at the end of the value
    #[must_use]
    pub fn at_end(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.len();
        normalize_state(Self {
            value,
            cursor,
            selection_anchor: cursor,
            has_selection: false,
            horizontal_offset: 0,
        })
    }

    /// Creates state selecting the grapheme-aligned range between anchor and cursor
    #[must_use]
    pub fn with_selection(value: impl Into<String>, cursor: usize, anchor: usize) -> Self {
        normalize_state(Self {
            value: value.into(),
            cursor,
            selection_anchor: anchor,
            has_selection: true,
            horizontal_offset: 0,
        })
    }

    /// Returns the UTF-8 value without Unicode normalization
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns the UTF-8 byte cursor at a grapheme boundary
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Returns the ordered UTF-8 byte range selected by the state
    #[must_use]
    pub fn selection(&self) -> Option<Range<usize>> {
        let state = normalize_state(self.clone());
        state.has_selection.then(|| {
            state.cursor.min(state.selection_anchor)..state.cursor.max(state.selection_anchor)
        })
    }

    /// Returns state selecting the range between anchor and the cursor
    #[must_use]
    pub fn select(mut self, anchor: usize) -> Self {
        self.selection_anchor = anchor;
        self.has_selection = true;
        normalize_state(self)
    }

    /// Returns state with the selection collapsed at the cursor
    #[must_use]
    pub fn clear_selection(mut self) -> Self {
        self.selection_anchor = self.cursor;
        self.has_selection = false;
        normalize_state(self)
    }

    /// Returns state rendered from the requested terminal-cell offset
    #[must_use]
    pub fn with_horizontal_offset(mut self, offset: usize) -> Self {
        self.horizontal_offset = offset;
        normalize_state(self)
    }

    /// Returns the leading terminal cells omitted from each line
    #[must_use]
    pub const fn horizontal_offset(&self) -> usize {
        self.horizontal_offset
    }
}

/// Visual styles used by a [`TextArea`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextAreaStyle {
    /// Style used by editable text
    pub normal: Style,
    /// Style used by the visible cursor marker
    pub cursor: Style,
    /// Style used by placeholder text
    pub placeholder: Style,
    /// Style merged over the area while it owns focus
    pub focused: Style,
    /// Style used by text while the area is disabled
    pub disabled: Style,
}

impl Default for TextAreaStyle {
    fn default() -> Self {
        Self {
            normal: Style::default(),
            cursor: Style {
                reverse: true,
                ..Style::default()
            },
            placeholder: Style {
                dim: true,
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

/// A controlled multiline editor with grapheme-safe cursor movement
pub struct TextArea<Message> {
    id: NodeId,
    state: TextAreaState,
    placeholder: String,
    enabled: bool,
    style: TextAreaStyle,
    selection_style: Style,
    on_change: Arc<dyn Fn(TextAreaState) -> Message>,
    on_undo: Option<Arc<dyn Fn() -> Message>>,
    on_redo: Option<Arc<dyn Fn() -> Message>>,
}

impl<Message: 'static> TextArea<Message> {
    /// Creates an enabled text area using application-owned editing state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        state: TextAreaState,
        on_change: impl Fn(TextAreaState) -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            state: normalize_state(state),
            placeholder: String::new(),
            enabled: true,
            style: TextAreaStyle::default(),
            selection_style: Style {
                reverse: true,
                ..Style::default()
            },
            on_change: Arc::new(on_change),
            on_undo: None,
            on_redo: None,
        }
    }

    /// Sets placeholder text shown when the value is empty
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Sets whether the area can receive focus and edit its value
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the text area styles
    #[must_use]
    pub const fn style(mut self, style: TextAreaStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the style merged over selected text
    #[must_use]
    pub const fn selection_style(mut self, style: Style) -> Self {
        self.selection_style = style;
        self
    }

    /// Sets the message emitted for Control-Z
    #[must_use]
    pub fn on_undo(mut self, handler: impl Fn() -> Message + 'static) -> Self {
        self.on_undo = Some(Arc::new(handler));
        self
    }

    /// Sets the message emitted for Control-Y and Control-Shift-Z
    #[must_use]
    pub fn on_redo(mut self, handler: impl Fn() -> Message + 'static) -> Self {
        self.on_redo = Some(Arc::new(handler));
        self
    }

    /// Builds the public semantic node for this text area
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let content = text_area_content(
            &self.state,
            &self.placeholder,
            self.enabled,
            self.style,
            self.selection_style,
        );
        if !self.enabled {
            return content.with_id(self.id);
        }

        let id = self.id;
        let focus_id = id.clone();
        let state = self.state;
        let on_change = self.on_change;
        let on_undo = self.on_undo;
        let on_redo = self.on_redo;
        content
            .focusable(id.clone())
            .with_focused_style(self.style.focused)
            .on_event(id, move |event| {
                if let Some(action) = history_action_for_event(event) {
                    let handler = match action {
                        TextAreaHistoryAction::Undo => on_undo.as_ref(),
                        TextAreaHistoryAction::Redo => on_redo.as_ref(),
                    };
                    return handler.map_or_else(EventResult::ignored, |handler| {
                        EventResult::consumed()
                            .focus(focus_id.clone())
                            .emit(handler())
                    });
                }
                let Some(next) = edit_for_event(&state, event) else {
                    return EventResult::ignored();
                };
                let result = EventResult::consumed().focus(focus_id.clone());
                if next == state {
                    result
                } else {
                    result.emit(on_change(next))
                }
            })
    }
}

fn text_area_content<Message>(
    state: &TextAreaState,
    placeholder: &str,
    enabled: bool,
    style: TextAreaStyle,
    selection_style: Style,
) -> Node<Message> {
    let state = normalize_state(state.clone());
    if state.value.is_empty() {
        if enabled {
            return Node::row([
                Node::styled_text("▏", style.cursor),
                Node::styled_text(placeholder, style.placeholder),
            ]);
        }
        return Node::styled_text(placeholder, style.disabled);
    }

    let lines = line_ranges(&state.value);
    let cursor_line = lines
        .iter()
        .position(|line| state.cursor >= line.start && state.cursor <= line.end)
        .unwrap_or(lines.len().saturating_sub(1));
    let mut nodes = Vec::with_capacity(lines.len());
    for (index, line) in lines.into_iter().enumerate() {
        let line_style = if enabled {
            style.normal
        } else {
            style.disabled
        };
        nodes.push(text_area_line_content(
            &state,
            line,
            enabled && index == cursor_line,
            line_style,
            style.cursor,
            selection_style,
        ));
    }
    Node::column(nodes)
}

fn text_area_line_content<Message>(
    state: &TextAreaState,
    line: Range<usize>,
    cursor_line: bool,
    normal_style: Style,
    cursor_style: Style,
    selection_style: Style,
) -> Node<Message> {
    let line_text = &state.value[line.clone()];
    let visible_start = line
        .start
        .saturating_add(text_area_visible_start(line_text, state.horizontal_offset));
    let selection = state.selection();
    let mut boundaries = vec![visible_start, line.end];
    if let Some(selection) = &selection {
        append_boundary(&mut boundaries, selection.start, visible_start, line.end);
        append_boundary(&mut boundaries, selection.end, visible_start, line.end);
    }
    if cursor_line {
        append_boundary(&mut boundaries, state.cursor, visible_start, line.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut cursor_visible = cursor_line
        && state.cursor >= line.start
        && state.cursor <= line.end
        && cell_at_byte(
            line_text,
            state.cursor.saturating_sub(line.start),
            WidthProfile::MODERN,
        )
        .is_some_and(|cell| cell >= state.horizontal_offset);
    let mut parts = Vec::with_capacity(boundaries.len().saturating_mul(2));
    for pair in boundaries.windows(2) {
        let start = pair[0];
        let end = pair[1];
        if cursor_visible && state.cursor == start {
            parts.push(Node::styled_text("▏", cursor_style));
            cursor_visible = false;
        }
        if start == end {
            continue;
        }
        let mut part_style = normal_style;
        if selection
            .as_ref()
            .is_some_and(|selection| start >= selection.start && start < selection.end)
        {
            part_style = part_style.merged(selection_style);
        }
        parts.push(Node::styled_text(&state.value[start..end], part_style));
    }
    if cursor_visible && state.cursor == line.end {
        parts.push(Node::styled_text("▏", cursor_style));
    }
    if parts.is_empty() {
        Node::styled_text("", normal_style)
    } else {
        Node::row(parts)
    }
}

fn append_boundary(boundaries: &mut Vec<usize>, boundary: usize, start: usize, end: usize) {
    if boundary >= start && boundary <= end {
        boundaries.push(boundary);
    }
}

fn text_area_visible_start(line: &str, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let mut cells = 0_usize;
    for grapheme in graphemes(line) {
        if cells >= offset {
            return grapheme.start();
        }
        cells = cells.saturating_add(grapheme_width(grapheme.text(), WidthProfile::MODERN));
    }
    line.len()
}

#[derive(Clone, Copy)]
enum TextAreaEdit<'a> {
    Insert(&'a str),
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Backspace,
    Delete,
}

fn edit_for_event(state: &TextAreaState, event: &Event) -> Option<TextAreaState> {
    match event {
        Event::Text(text) | Event::Paste(text) => {
            Some(apply_edit(state, TextAreaEdit::Insert(text)))
        }
        Event::Key(key)
            if key.action != KeyAction::Release && !key.modifiers.alt && !key.modifiers.meta =>
        {
            if key.modifiers.control {
                if matches!(key.code, KeyCode::Character(character) if character.eq_ignore_ascii_case(&'a'))
                {
                    let mut next = normalize_state(state.clone());
                    next.cursor = next.value.len();
                    next.selection_anchor = 0;
                    next.has_selection = next.cursor != 0;
                    return Some(next);
                }
                return None;
            }
            let edit = match key.code {
                KeyCode::Enter => TextAreaEdit::Insert("\n"),
                KeyCode::Left => TextAreaEdit::Left,
                KeyCode::Right => TextAreaEdit::Right,
                KeyCode::Up => TextAreaEdit::Up,
                KeyCode::Down => TextAreaEdit::Down,
                KeyCode::Home => TextAreaEdit::Home,
                KeyCode::End => TextAreaEdit::End,
                KeyCode::Backspace => TextAreaEdit::Backspace,
                KeyCode::Delete => TextAreaEdit::Delete,
                _ => return None,
            };
            if matches!(
                edit,
                TextAreaEdit::Left
                    | TextAreaEdit::Right
                    | TextAreaEdit::Up
                    | TextAreaEdit::Down
                    | TextAreaEdit::Home
                    | TextAreaEdit::End
            ) {
                Some(apply_movement(state, edit, key.modifiers.shift))
            } else {
                Some(apply_edit(state, edit))
            }
        }
        _ => None,
    }
}

fn apply_edit(state: &TextAreaState, edit: TextAreaEdit<'_>) -> TextAreaState {
    let state = normalize_state(state.clone());
    let value = &state.value;
    let cursor = state.cursor;
    match edit {
        TextAreaEdit::Insert(inserted) => {
            let selection = state.selection();
            let start = selection.as_ref().map_or(cursor, |range| range.start);
            let end = selection.as_ref().map_or(cursor, |range| range.end);
            let mut output = String::with_capacity(
                value
                    .len()
                    .saturating_sub(end.saturating_sub(start))
                    .saturating_add(inserted.len()),
            );
            output.push_str(&value[..start]);
            output.push_str(inserted);
            output.push_str(&value[end..]);
            let intended = start.saturating_add(inserted.len());
            let cursor = grapheme_boundaries(&output)
                .into_iter()
                .find(|boundary| *boundary >= intended)
                .unwrap_or(output.len());
            edited_state(&state, output, cursor)
        }
        TextAreaEdit::Left
        | TextAreaEdit::Right
        | TextAreaEdit::Up
        | TextAreaEdit::Down
        | TextAreaEdit::Home
        | TextAreaEdit::End => apply_movement(&state, edit, false),
        TextAreaEdit::Backspace => {
            if let Some(selection) = state.selection() {
                let mut output = value.clone();
                output.replace_range(selection.clone(), "");
                return edited_state(&state, output, selection.start);
            }
            let start = previous_grapheme_boundary(value, cursor).unwrap_or(cursor);
            let mut output = value.clone();
            output.replace_range(start..cursor, "");
            edited_state(&state, output, start)
        }
        TextAreaEdit::Delete => {
            if let Some(selection) = state.selection() {
                let mut output = value.clone();
                output.replace_range(selection.clone(), "");
                return edited_state(&state, output, selection.start);
            }
            let end = next_grapheme_boundary(value, cursor).unwrap_or(cursor);
            let mut output = value.clone();
            output.replace_range(cursor..end, "");
            edited_state(&state, output, cursor)
        }
    }
}

fn apply_movement(state: &TextAreaState, edit: TextAreaEdit<'_>, extend: bool) -> TextAreaState {
    let state = normalize_state(state.clone());
    if !extend && state.has_selection {
        if let Some(selection) = state.selection() {
            if matches!(edit, TextAreaEdit::Left) {
                return moved_state(state, selection.start, false);
            }
            if matches!(edit, TextAreaEdit::Right) {
                return moved_state(state, selection.end, false);
            }
        }
    }
    let target = match edit {
        TextAreaEdit::Left => previous_grapheme_boundary(&state.value, state.cursor).unwrap_or(0),
        TextAreaEdit::Right => {
            next_grapheme_boundary(&state.value, state.cursor).unwrap_or(state.value.len())
        }
        TextAreaEdit::Up => vertical_cursor(&state.value, state.cursor, false),
        TextAreaEdit::Down => vertical_cursor(&state.value, state.cursor, true),
        TextAreaEdit::Home => current_line(&state.value, state.cursor).start,
        TextAreaEdit::End => current_line(&state.value, state.cursor).end,
        TextAreaEdit::Insert(_) | TextAreaEdit::Backspace | TextAreaEdit::Delete => {
            panic!("widget: invalid text area movement")
        }
    };
    moved_state(state, target, extend)
}

fn moved_state(mut state: TextAreaState, cursor: usize, extend: bool) -> TextAreaState {
    let anchor = if state.has_selection {
        state.selection_anchor
    } else {
        state.cursor
    };
    state.cursor = cursor;
    if extend {
        state.selection_anchor = anchor;
        state.has_selection = anchor != cursor;
    } else {
        state.selection_anchor = cursor;
        state.has_selection = false;
    }
    normalize_state(state)
}

fn edited_state(state: &TextAreaState, value: String, cursor: usize) -> TextAreaState {
    normalize_state(TextAreaState {
        value,
        cursor,
        selection_anchor: cursor,
        has_selection: false,
        horizontal_offset: state.horizontal_offset,
    })
}

fn vertical_cursor(value: &str, cursor: usize, down: bool) -> usize {
    let lines = line_ranges(value);
    let current = lines
        .iter()
        .position(|line| cursor >= line.start && cursor <= line.end)
        .unwrap_or(lines.len().saturating_sub(1));
    let target = if down {
        current.saturating_add(1).min(lines.len().saturating_sub(1))
    } else {
        current.saturating_sub(1)
    };
    let current_line = &value[lines[current].clone()];
    let target_line = &value[lines[target].clone()];
    let column = cell_at_byte(
        current_line,
        cursor.saturating_sub(lines[current].start),
        WidthProfile::MODERN,
    )
    .unwrap_or(0);
    let relative = truncate(target_line, column, WidthProfile::MODERN).len();
    lines[target].start.saturating_add(relative)
}

#[derive(Clone, Copy)]
enum TextAreaHistoryAction {
    Undo,
    Redo,
}

fn history_action_for_event(event: &Event) -> Option<TextAreaHistoryAction> {
    let Event::Key(key) = event else {
        return None;
    };
    if key.action == KeyAction::Release
        || !key.modifiers.control
        || key.modifiers.alt
        || key.modifiers.meta
    {
        return None;
    }
    match key.code {
        KeyCode::Character(character) if character.eq_ignore_ascii_case(&'z') => {
            if key.modifiers.shift {
                Some(TextAreaHistoryAction::Redo)
            } else {
                Some(TextAreaHistoryAction::Undo)
            }
        }
        KeyCode::Character(character) if character.eq_ignore_ascii_case(&'y') => {
            Some(TextAreaHistoryAction::Redo)
        }
        _ => None,
    }
}

fn current_line(value: &str, cursor: usize) -> Range<usize> {
    line_ranges(value)
        .into_iter()
        .find(|line| cursor >= line.start && cursor <= line.end)
        .unwrap_or(value.len()..value.len())
}

fn line_ranges(value: &str) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for grapheme in graphemes(value) {
        if matches!(grapheme.text(), "\r" | "\n" | "\r\n") {
            lines.push(start..grapheme.start());
            start = grapheme.end();
        }
    }
    lines.push(start..value.len());
    lines
}

fn normalize_cursor(value: &str, cursor: usize) -> usize {
    grapheme_boundaries(value)
        .into_iter()
        .take_while(|boundary| *boundary <= cursor.min(value.len()))
        .last()
        .unwrap_or(0)
}

fn normalize_state(mut state: TextAreaState) -> TextAreaState {
    state.cursor = normalize_cursor(&state.value, state.cursor);
    state.selection_anchor = normalize_cursor(&state.value, state.selection_anchor);
    if !state.has_selection || state.selection_anchor == state.cursor {
        state.selection_anchor = state.cursor;
        state.has_selection = false;
    }
    state
}

#[cfg(test)]
mod tests {
    use nagi_tui::{Event, KeyAction, KeyCode, KeyEvent, KeyProtocol, Modifiers};

    use super::{TextAreaEdit, TextAreaState, apply_edit, edit_for_event};

    #[test]
    fn editing_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/text-area-edit.txt",
            "widget-text-area-edit",
            &[
                "initial",
                "cursor",
                "operation",
                "text",
                "expected",
                "expected-cursor",
            ],
        ) else {
            return;
        };
        for record in records {
            let initial = record.text("initial");
            let inserted = record.text("text");
            let edit = match record.field("operation") {
                "insert" => TextAreaEdit::Insert(&inserted),
                "left" => TextAreaEdit::Left,
                "right" => TextAreaEdit::Right,
                "up" => TextAreaEdit::Up,
                "down" => TextAreaEdit::Down,
                "home" => TextAreaEdit::Home,
                "end" => TextAreaEdit::End,
                "backspace" => TextAreaEdit::Backspace,
                "delete" => TextAreaEdit::Delete,
                operation => panic!("invalid operation {operation}"),
            };
            let actual = apply_edit(
                &TextAreaState::new(initial, number(record.field("cursor"))),
                edit,
            );
            assert_eq!(
                actual,
                TextAreaState::new(
                    record.text("expected"),
                    number(record.field("expected-cursor"))
                ),
                "case {}",
                record.id
            );
        }
    }

    #[test]
    fn selection_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/text-area-selection.txt",
            "widget-text-area-selection",
            &[
                "initial",
                "cursor",
                "anchor",
                "operation",
                "text",
                "offset",
                "expected",
                "expected-cursor",
                "expected-anchor",
            ],
        ) else {
            return;
        };
        for record in records {
            let mut state =
                TextAreaState::new(record.text("initial"), number(record.field("cursor")))
                    .with_horizontal_offset(number(record.field("offset")));
            if record.field("anchor") != "-" {
                state = state.select(number(record.field("anchor")));
            }
            let actual = match record.field("operation") {
                "insert" => Some(apply_edit(
                    &state,
                    TextAreaEdit::Insert(&record.text("text")),
                )),
                "backspace" => Some(apply_edit(&state, TextAreaEdit::Backspace)),
                "delete" => Some(apply_edit(&state, TextAreaEdit::Delete)),
                "left" => edit_for_event(&state, &key(KeyCode::Left, false, false)),
                "right" => edit_for_event(&state, &key(KeyCode::Right, false, false)),
                "shift-left" => edit_for_event(&state, &key(KeyCode::Left, true, false)),
                "shift-right" => edit_for_event(&state, &key(KeyCode::Right, true, false)),
                "select-all" => edit_for_event(&state, &key(KeyCode::Character('a'), false, true)),
                operation => panic!("invalid operation {operation}"),
            }
            .unwrap_or_else(|| panic!("operation was not handled for case {}", record.id));
            let mut expected = TextAreaState::new(
                record.text("expected"),
                number(record.field("expected-cursor")),
            )
            .with_horizontal_offset(number(record.field("offset")));
            if record.field("expected-anchor") != "-" {
                expected = expected.select(number(record.field("expected-anchor")));
            }
            assert_eq!(actual, expected, "case {}", record.id);
        }
    }

    fn key(code: KeyCode, shift: bool, control: bool) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers {
                shift,
                control,
                ..Modifiers::NONE
            },
            action: KeyAction::Press,
            text: None,
            protocol: KeyProtocol::Legacy,
        })
    }

    fn number(value: &str) -> usize {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
    }
}
