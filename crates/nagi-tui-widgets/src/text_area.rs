use std::ops::Range;
use std::sync::{Arc, LazyLock};

use nagi_text::{
    WidthProfile, cell_at_byte, grapheme_boundaries, grapheme_width, graphemes,
    next_grapheme_boundary, previous_grapheme_boundary, truncate,
};
use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, Event, EventResult, KeyBinding, KeyCode,
    KeyStroke, Modifiers, Node, NodeId, RepeatPolicy, Style, TEXT_CURSOR_DOWN_ACTION_ID,
    TEXT_CURSOR_LEFT_ACTION_ID, TEXT_CURSOR_LINE_END_ACTION_ID, TEXT_CURSOR_LINE_START_ACTION_ID,
    TEXT_CURSOR_RIGHT_ACTION_ID, TEXT_CURSOR_UP_ACTION_ID, TEXT_DELETE_BACKWARD_ACTION_ID,
    TEXT_DELETE_FORWARD_ACTION_ID, TEXT_INSERT_LINE_BREAK_ACTION_ID, TEXT_REDO_ACTION_ID,
    TEXT_SELECT_ALL_ACTION_ID, TEXT_SELECTION_EXTEND_DOWN_ACTION_ID,
    TEXT_SELECTION_EXTEND_LEFT_ACTION_ID, TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID,
    TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID, TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID,
    TEXT_SELECTION_EXTEND_UP_ACTION_ID, TEXT_UNDO_ACTION_ID,
};

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
///
/// Keyboard editing commands are declared as Core `nagi.text.*` semantic
/// actions. Text and Paste remain raw editing input after local action
/// resolution
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

    /// Returns the ordered semantic action descriptors declared by this text area
    ///
    /// The order is cursor movement, selection extension, select all, backward
    /// and forward deletion, line break, undo, and redo. Undo and redo are
    /// disabled-pass-through when their corresponding handler is absent. Every
    /// descriptor is disabled-pass-through when the text area is disabled
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; TEXT_AREA_ACTION_COUNT] {
        text_area_action_descriptors(self.enabled, self.on_undo.is_some(), self.on_redo.is_some())
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
        let descriptors = self.action_descriptors();
        if !self.enabled {
            let id = self.id;
            return content.with_id(id.clone()).on_actions(
                id,
                descriptors.map(|descriptor| Action::new(descriptor, |_| EventResult::ignored())),
            );
        }

        let id = self.id;
        let context = Arc::new(TextAreaActionContext {
            id: id.clone(),
            state: self.state,
            on_change: self.on_change,
            on_undo: self.on_undo,
            on_redo: self.on_redo,
        });
        let raw_context = Arc::clone(&context);
        content
            .focusable(id.clone())
            .with_focused_style(self.style.focused)
            .on_actions(id.clone(), text_area_actions(descriptors, context))
            .on_event(id, move |event| {
                let Some(next) = raw_edit_for_event(&raw_context.state, event) else {
                    return EventResult::ignored();
                };
                text_area_change_result(raw_context.as_ref(), next)
            })
    }
}

const TEXT_AREA_ACTION_COUNT: usize = 18;

#[derive(Clone, Copy)]
enum TextAreaSemanticAction {
    CursorLeft,
    CursorRight,
    CursorUp,
    CursorDown,
    CursorLineStart,
    CursorLineEnd,
    SelectionExtendLeft,
    SelectionExtendRight,
    SelectionExtendUp,
    SelectionExtendDown,
    SelectionExtendLineStart,
    SelectionExtendLineEnd,
    SelectAll,
    DeleteBackward,
    DeleteForward,
    InsertLineBreak,
    Undo,
    Redo,
}

const TEXT_AREA_ACTIONS: [TextAreaSemanticAction; TEXT_AREA_ACTION_COUNT] = [
    TextAreaSemanticAction::CursorLeft,
    TextAreaSemanticAction::CursorRight,
    TextAreaSemanticAction::CursorUp,
    TextAreaSemanticAction::CursorDown,
    TextAreaSemanticAction::CursorLineStart,
    TextAreaSemanticAction::CursorLineEnd,
    TextAreaSemanticAction::SelectionExtendLeft,
    TextAreaSemanticAction::SelectionExtendRight,
    TextAreaSemanticAction::SelectionExtendUp,
    TextAreaSemanticAction::SelectionExtendDown,
    TextAreaSemanticAction::SelectionExtendLineStart,
    TextAreaSemanticAction::SelectionExtendLineEnd,
    TextAreaSemanticAction::SelectAll,
    TextAreaSemanticAction::DeleteBackward,
    TextAreaSemanticAction::DeleteForward,
    TextAreaSemanticAction::InsertLineBreak,
    TextAreaSemanticAction::Undo,
    TextAreaSemanticAction::Redo,
];

static TEXT_AREA_ACTION_DESCRIPTORS: LazyLock<[ActionDescriptor; TEXT_AREA_ACTION_COUNT]> =
    LazyLock::new(|| {
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let control = Modifiers {
            control: true,
            ..Modifiers::NONE
        };
        let control_shift = Modifiers {
            shift: true,
            control: true,
            ..Modifiers::NONE
        };
        [
            text_area_descriptor(
                TEXT_CURSOR_LEFT_ACTION_ID,
                "Move cursor left",
                [text_area_binding(KeyCode::Left, Modifiers::NONE)],
            ),
            text_area_descriptor(
                TEXT_CURSOR_RIGHT_ACTION_ID,
                "Move cursor right",
                [text_area_binding(KeyCode::Right, Modifiers::NONE)],
            ),
            text_area_descriptor(
                TEXT_CURSOR_UP_ACTION_ID,
                "Move cursor up",
                [text_area_binding(KeyCode::Up, Modifiers::NONE)],
            ),
            text_area_descriptor(
                TEXT_CURSOR_DOWN_ACTION_ID,
                "Move cursor down",
                [text_area_binding(KeyCode::Down, Modifiers::NONE)],
            ),
            text_area_descriptor(
                TEXT_CURSOR_LINE_START_ACTION_ID,
                "Move to line start",
                [text_area_binding(KeyCode::Home, Modifiers::NONE)],
            ),
            text_area_descriptor(
                TEXT_CURSOR_LINE_END_ACTION_ID,
                "Move to line end",
                [text_area_binding(KeyCode::End, Modifiers::NONE)],
            ),
            text_area_descriptor(
                TEXT_SELECTION_EXTEND_LEFT_ACTION_ID,
                "Extend selection left",
                [text_area_binding(KeyCode::Left, shift)],
            ),
            text_area_descriptor(
                TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID,
                "Extend selection right",
                [text_area_binding(KeyCode::Right, shift)],
            ),
            text_area_descriptor(
                TEXT_SELECTION_EXTEND_UP_ACTION_ID,
                "Extend selection up",
                [text_area_binding(KeyCode::Up, shift)],
            ),
            text_area_descriptor(
                TEXT_SELECTION_EXTEND_DOWN_ACTION_ID,
                "Extend selection down",
                [text_area_binding(KeyCode::Down, shift)],
            ),
            text_area_descriptor(
                TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID,
                "Extend selection to line start",
                [text_area_binding(KeyCode::Home, shift)],
            ),
            text_area_descriptor(
                TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID,
                "Extend selection to line end",
                [text_area_binding(KeyCode::End, shift)],
            ),
            text_area_descriptor(
                TEXT_SELECT_ALL_ACTION_ID,
                "Select all",
                [text_area_character_binding('a', control)],
            ),
            text_area_descriptor(
                TEXT_DELETE_BACKWARD_ACTION_ID,
                "Delete backward",
                [text_area_binding(KeyCode::Backspace, Modifiers::NONE)],
            ),
            text_area_descriptor(
                TEXT_DELETE_FORWARD_ACTION_ID,
                "Delete forward",
                [text_area_binding(KeyCode::Delete, Modifiers::NONE)],
            ),
            text_area_descriptor(
                TEXT_INSERT_LINE_BREAK_ACTION_ID,
                "Insert line break",
                [text_area_binding(KeyCode::Enter, Modifiers::NONE)],
            ),
            text_area_descriptor(
                TEXT_UNDO_ACTION_ID,
                "Undo",
                [text_area_character_binding('z', control)],
            ),
            text_area_descriptor(
                TEXT_REDO_ACTION_ID,
                "Redo",
                [
                    text_area_character_binding('y', control),
                    text_area_character_binding('z', control_shift),
                ],
            ),
        ]
    });

fn text_area_descriptor(
    id: &'static str,
    label: &'static str,
    bindings: impl IntoIterator<Item = KeyBinding>,
) -> ActionDescriptor {
    ActionDescriptor::new(id, label, bindings)
}

fn text_area_binding(code: KeyCode, modifiers: Modifiers) -> KeyBinding {
    KeyBinding::new(KeyStroke::new(code, modifiers)).with_repeat_policy(RepeatPolicy::AllowRepeat)
}

fn text_area_character_binding(character: char, modifiers: Modifiers) -> KeyBinding {
    KeyBinding::new(KeyStroke::character(character, modifiers))
        .with_repeat_policy(RepeatPolicy::AllowRepeat)
}

fn text_area_action_descriptors(
    enabled: bool,
    has_undo: bool,
    has_redo: bool,
) -> [ActionDescriptor; TEXT_AREA_ACTION_COUNT] {
    std::array::from_fn(|index| {
        let available = enabled
            && match TEXT_AREA_ACTIONS[index] {
                TextAreaSemanticAction::Undo => has_undo,
                TextAreaSemanticAction::Redo => has_redo,
                _ => true,
            };
        let availability = if available {
            ActionAvailability::Enabled
        } else {
            ActionAvailability::DisabledPassThrough
        };
        TEXT_AREA_ACTION_DESCRIPTORS[index]
            .clone()
            .with_availability(availability)
    })
}

struct TextAreaActionContext<Message> {
    id: NodeId,
    state: TextAreaState,
    on_change: Arc<dyn Fn(TextAreaState) -> Message>,
    on_undo: Option<Arc<dyn Fn() -> Message>>,
    on_redo: Option<Arc<dyn Fn() -> Message>>,
}

fn text_area_actions<Message: 'static>(
    descriptors: [ActionDescriptor; TEXT_AREA_ACTION_COUNT],
    context: Arc<TextAreaActionContext<Message>>,
) -> impl Iterator<Item = Action<Message>> {
    descriptors
        .into_iter()
        .zip(TEXT_AREA_ACTIONS)
        .map(move |(descriptor, action)| {
            let context = Arc::clone(&context);
            Action::new(descriptor, move |_| {
                text_area_action_result(action, context.as_ref())
            })
        })
}

fn text_area_action_result<Message>(
    action: TextAreaSemanticAction,
    context: &TextAreaActionContext<Message>,
) -> EventResult<Message> {
    match action {
        TextAreaSemanticAction::Undo => {
            return context
                .on_undo
                .as_ref()
                .map_or_else(EventResult::ignored, |handler| {
                    EventResult::consumed()
                        .focus(context.id.clone())
                        .emit(handler())
                });
        }
        TextAreaSemanticAction::Redo => {
            return context
                .on_redo
                .as_ref()
                .map_or_else(EventResult::ignored, |handler| {
                    EventResult::consumed()
                        .focus(context.id.clone())
                        .emit(handler())
                });
        }
        _ => {}
    }
    let next = text_area_state_for_action(&context.state, action);
    text_area_change_result(context, next)
}

fn text_area_change_result<Message>(
    context: &TextAreaActionContext<Message>,
    next: TextAreaState,
) -> EventResult<Message> {
    let result = EventResult::consumed().focus(context.id.clone());
    if next == context.state {
        result
    } else {
        result.emit((context.on_change)(next))
    }
}

fn text_area_state_for_action(
    state: &TextAreaState,
    action: TextAreaSemanticAction,
) -> TextAreaState {
    match action {
        TextAreaSemanticAction::CursorLeft => apply_movement(state, TextAreaEdit::Left, false),
        TextAreaSemanticAction::CursorRight => apply_movement(state, TextAreaEdit::Right, false),
        TextAreaSemanticAction::CursorUp => apply_movement(state, TextAreaEdit::Up, false),
        TextAreaSemanticAction::CursorDown => apply_movement(state, TextAreaEdit::Down, false),
        TextAreaSemanticAction::CursorLineStart => apply_movement(state, TextAreaEdit::Home, false),
        TextAreaSemanticAction::CursorLineEnd => apply_movement(state, TextAreaEdit::End, false),
        TextAreaSemanticAction::SelectionExtendLeft => {
            apply_movement(state, TextAreaEdit::Left, true)
        }
        TextAreaSemanticAction::SelectionExtendRight => {
            apply_movement(state, TextAreaEdit::Right, true)
        }
        TextAreaSemanticAction::SelectionExtendUp => apply_movement(state, TextAreaEdit::Up, true),
        TextAreaSemanticAction::SelectionExtendDown => {
            apply_movement(state, TextAreaEdit::Down, true)
        }
        TextAreaSemanticAction::SelectionExtendLineStart => {
            apply_movement(state, TextAreaEdit::Home, true)
        }
        TextAreaSemanticAction::SelectionExtendLineEnd => {
            apply_movement(state, TextAreaEdit::End, true)
        }
        TextAreaSemanticAction::SelectAll => select_all(state),
        TextAreaSemanticAction::DeleteBackward => apply_edit(state, TextAreaEdit::Backspace),
        TextAreaSemanticAction::DeleteForward => apply_edit(state, TextAreaEdit::Delete),
        TextAreaSemanticAction::InsertLineBreak => apply_edit(state, TextAreaEdit::Insert("\n")),
        TextAreaSemanticAction::Undo | TextAreaSemanticAction::Redo => {
            unreachable!("history actions do not produce text area state")
        }
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

fn raw_edit_for_event(state: &TextAreaState, event: &Event) -> Option<TextAreaState> {
    match event {
        Event::Text(text) | Event::Paste(text) => {
            Some(apply_edit(state, TextAreaEdit::Insert(text)))
        }
        _ => None,
    }
}

fn select_all(state: &TextAreaState) -> TextAreaState {
    let mut next = normalize_state(state.clone());
    next.cursor = next.value.len();
    next.selection_anchor = 0;
    next.has_selection = next.cursor != 0;
    next
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
    use super::{
        RepeatPolicy, TextAreaEdit, TextAreaState, apply_edit, apply_movement, select_all,
        text_area_action_descriptors,
    };

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
                "insert" => apply_edit(&state, TextAreaEdit::Insert(&record.text("text"))),
                "backspace" => apply_edit(&state, TextAreaEdit::Backspace),
                "delete" => apply_edit(&state, TextAreaEdit::Delete),
                "left" => apply_movement(&state, TextAreaEdit::Left, false),
                "right" => apply_movement(&state, TextAreaEdit::Right, false),
                "shift-left" => apply_movement(&state, TextAreaEdit::Left, true),
                "shift-right" => apply_movement(&state, TextAreaEdit::Right, true),
                "select-all" => select_all(&state),
                operation => panic!("invalid operation {operation}"),
            };
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

    #[test]
    fn action_descriptor_clones_reuse_immutable_storage() {
        let enabled = text_area_action_descriptors(true, true, true);
        let unavailable = text_area_action_descriptors(false, false, false);

        for index in 0..enabled.len() {
            assert!(std::ptr::eq(
                enabled[index].id().as_str(),
                unavailable[index].id().as_str()
            ));
            assert!(std::ptr::eq(
                enabled[index].label(),
                unavailable[index].label()
            ));
            assert!(std::ptr::eq(
                enabled[index].default_bindings(),
                unavailable[index].default_bindings()
            ));
            assert!(
                enabled[index]
                    .default_bindings()
                    .iter()
                    .all(|binding| binding.repeat_policy() == RepeatPolicy::AllowRepeat)
            );
        }
    }

    fn number(value: &str) -> usize {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
    }
}
