use std::ops::Range;
use std::sync::{Arc, LazyLock};

use nagi_text::{
    WidthProfile, cell_at_byte, grapheme_boundaries, grapheme_width, graphemes,
    next_grapheme_boundary, previous_grapheme_boundary, truncate,
};
use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, Event, EventResult, KeyBinding, KeyCode,
    KeyStroke, Length, Modifiers, Node, NodeId, RepeatPolicy, ScrollAxis, ScrollViewportOptions,
    Style, TEXT_CURSOR_DOWN_ACTION_ID, TEXT_CURSOR_LEFT_ACTION_ID, TEXT_CURSOR_LINE_END_ACTION_ID,
    TEXT_CURSOR_LINE_START_ACTION_ID, TEXT_CURSOR_RIGHT_ACTION_ID, TEXT_CURSOR_UP_ACTION_ID,
    TEXT_DELETE_BACKWARD_ACTION_ID, TEXT_DELETE_FORWARD_ACTION_ID,
    TEXT_INSERT_LINE_BREAK_ACTION_ID, TEXT_REDO_ACTION_ID, TEXT_SELECT_ALL_ACTION_ID,
    TEXT_SELECTION_EXTEND_DOWN_ACTION_ID, TEXT_SELECTION_EXTEND_LEFT_ACTION_ID,
    TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID, TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID,
    TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID, TEXT_SELECTION_EXTEND_UP_ACTION_ID, TEXT_UNDO_ACTION_ID,
};

/// Application-owned value and grapheme-aligned cursor for a [`TextArea`]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextAreaState {
    value: String,
    cursor: usize,
    selection_anchor: usize,
    has_selection: bool,
    horizontal_offset: usize,
    preferred_column: Option<usize>,
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
            preferred_column: None,
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
            preferred_column: None,
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
            preferred_column: None,
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

    /// Returns the terminal-cell column retained across vertical movement
    #[must_use]
    pub const fn preferred_column(&self) -> Option<usize> {
        self.preferred_column
    }
}

/// Behavior of vertical movement at the first and last visual line
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TextAreaBoundaryNavigation {
    /// Consume a boundary action without emitting unchanged state
    #[default]
    Consume,
    /// Let an ancestor action handle movement beyond the visual boundary
    Bubble,
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
    wrap_width: Option<usize>,
    boundary_navigation: TextAreaBoundaryNavigation,
    viewport: Option<TextAreaViewport>,
    on_change: Arc<dyn Fn(TextAreaState) -> Message>,
    on_undo: Option<Arc<dyn Fn() -> Message>>,
    on_redo: Option<Arc<dyn Fn() -> Message>>,
}

struct TextAreaViewport {
    id: NodeId,
    caret_id: NodeId,
    height: Length,
}

pub(crate) enum TextAreaInsertionDecision {
    Accept,
    Reject,
    Replace(String),
}

pub(crate) type TextAreaInsertionPolicy =
    Arc<dyn Fn(&TextAreaState, &str) -> TextAreaInsertionDecision>;

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
            wrap_width: None,
            boundary_navigation: TextAreaBoundaryNavigation::Consume,
            viewport: None,
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

    /// Uses logical lines with the application-owned horizontal offset
    #[must_use]
    pub const fn no_wrap(mut self) -> Self {
        self.wrap_width = None;
        self
    }

    /// Soft-wraps visual lines at a terminal-cell width without changing text
    ///
    /// Zero is normalized to one Cell. Applications should recompute the width
    /// from `ViewContext` after a resize
    #[must_use]
    pub fn soft_wrap(mut self, width: u32) -> Self {
        self.wrap_width = Some(width.max(1) as usize);
        self
    }

    /// Sets how Up and Down actions behave at visual-line boundaries
    #[must_use]
    pub const fn boundary_navigation(mut self, navigation: TextAreaBoundaryNavigation) -> Self {
        self.boundary_navigation = navigation;
        self
    }

    /// Wraps the editor in a vertical viewport that follows its caret
    ///
    /// `viewport_id`, `caret_id`, and the TextArea root ID must be distinct
    /// stable IDs. The viewport is not an additional Tab stop
    #[must_use]
    pub fn viewport(
        mut self,
        viewport_id: impl Into<NodeId>,
        caret_id: impl Into<NodeId>,
        height: Length,
    ) -> Self {
        self.viewport = Some(TextAreaViewport {
            id: viewport_id.into(),
            caret_id: caret_id.into(),
            height,
        });
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
    /// descriptor is disabled-pass-through when the text area is disabled.
    /// Bubble mode also disables Up or Down at its corresponding visual edge
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; TEXT_AREA_ACTION_COUNT] {
        text_area_action_descriptors(
            self.enabled,
            self.on_undo.is_some(),
            self.on_redo.is_some(),
            self.boundary_navigation,
            &self.state,
            self.wrap_width,
        )
    }

    /// Builds the public semantic node for this text area
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let descriptors = self.action_descriptors();
        self.into_node_with_actions(descriptors, std::iter::empty(), None)
    }

    pub(crate) fn into_node_with_actions(
        self,
        descriptors: [ActionDescriptor; TEXT_AREA_ACTION_COUNT],
        leading_actions: impl IntoIterator<Item = Action<Message>>,
        insertion_policy: Option<TextAreaInsertionPolicy>,
    ) -> Node<Message> {
        let caret_id = self.viewport.as_ref().map(|viewport| &viewport.caret_id);
        let content = text_area_content(
            &self.state,
            &self.placeholder,
            self.enabled,
            self.style,
            self.selection_style,
            self.wrap_width,
            caret_id,
        );
        if !self.enabled {
            let id = self.id;
            let node = content.with_id(id.clone()).on_actions(
                id,
                leading_actions.into_iter().chain(
                    descriptors
                        .map(|descriptor| Action::new(descriptor, |_| EventResult::ignored())),
                ),
            );
            return text_area_viewport(node, self.viewport);
        }

        let id = self.id;
        let context = Arc::new(TextAreaActionContext {
            id: id.clone(),
            state: self.state,
            wrap_width: self.wrap_width,
            on_change: self.on_change,
            on_undo: self.on_undo,
            on_redo: self.on_redo,
            insertion_policy,
        });
        let raw_context = Arc::clone(&context);
        let node = content
            .focusable(id.clone())
            .with_focused_style(self.style.focused)
            .on_actions(
                id.clone(),
                leading_actions
                    .into_iter()
                    .chain(text_area_actions(descriptors, context)),
            )
            .on_event(id, move |event| {
                let Some(next) = raw_edit_for_event(
                    &raw_context.state,
                    event,
                    raw_context.insertion_policy.as_ref(),
                ) else {
                    return EventResult::ignored();
                };
                let Some(next) = next else {
                    return EventResult::consumed().focus(raw_context.id.clone());
                };
                text_area_change_result(raw_context.as_ref(), next)
            });
        text_area_viewport(node, self.viewport)
    }
}

fn text_area_viewport<Message>(
    node: Node<Message>,
    viewport: Option<TextAreaViewport>,
) -> Node<Message> {
    let Some(viewport) = viewport else {
        return node;
    };
    Node::scroll_viewport_with_options(
        viewport.id,
        node,
        ScrollViewportOptions {
            axis: ScrollAxis::Vertical,
            ..ScrollViewportOptions::default()
        },
    )
    .reveal_descendant(viewport.caret_id)
    .tab_stop(false)
    .with_length(viewport.height)
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

pub(crate) fn text_area_action_descriptors(
    enabled: bool,
    has_undo: bool,
    has_redo: bool,
    boundary_navigation: TextAreaBoundaryNavigation,
    state: &TextAreaState,
    wrap_width: Option<usize>,
) -> [ActionDescriptor; TEXT_AREA_ACTION_COUNT] {
    let (has_up, has_down) = if boundary_navigation == TextAreaBoundaryNavigation::Bubble {
        visual_line_directions(state, wrap_width)
    } else {
        (true, true)
    };
    std::array::from_fn(|index| {
        let available = enabled
            && match TEXT_AREA_ACTIONS[index] {
                TextAreaSemanticAction::Undo => has_undo,
                TextAreaSemanticAction::Redo => has_redo,
                TextAreaSemanticAction::CursorUp | TextAreaSemanticAction::SelectionExtendUp => {
                    has_up
                }
                TextAreaSemanticAction::CursorDown
                | TextAreaSemanticAction::SelectionExtendDown => has_down,
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
    wrap_width: Option<usize>,
    on_change: Arc<dyn Fn(TextAreaState) -> Message>,
    on_undo: Option<Arc<dyn Fn() -> Message>>,
    on_redo: Option<Arc<dyn Fn() -> Message>>,
    insertion_policy: Option<TextAreaInsertionPolicy>,
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
        TextAreaSemanticAction::InsertLineBreak => {
            let Some(next) =
                apply_insertion(&context.state, "\n", context.insertion_policy.as_ref())
            else {
                return EventResult::consumed().focus(context.id.clone());
            };
            return text_area_change_result(context, next);
        }
        _ => {}
    }
    let next = text_area_state_for_action(&context.state, action, context.wrap_width);
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
    wrap_width: Option<usize>,
) -> TextAreaState {
    match action {
        TextAreaSemanticAction::CursorLeft => apply_movement(state, TextAreaEdit::Left, false),
        TextAreaSemanticAction::CursorRight => apply_movement(state, TextAreaEdit::Right, false),
        TextAreaSemanticAction::CursorUp => {
            apply_vertical_movement(state, false, false, wrap_width)
        }
        TextAreaSemanticAction::CursorDown => {
            apply_vertical_movement(state, true, false, wrap_width)
        }
        TextAreaSemanticAction::CursorLineStart => apply_movement(state, TextAreaEdit::Home, false),
        TextAreaSemanticAction::CursorLineEnd => apply_movement(state, TextAreaEdit::End, false),
        TextAreaSemanticAction::SelectionExtendLeft => {
            apply_movement(state, TextAreaEdit::Left, true)
        }
        TextAreaSemanticAction::SelectionExtendRight => {
            apply_movement(state, TextAreaEdit::Right, true)
        }
        TextAreaSemanticAction::SelectionExtendUp => {
            apply_vertical_movement(state, false, true, wrap_width)
        }
        TextAreaSemanticAction::SelectionExtendDown => {
            apply_vertical_movement(state, true, true, wrap_width)
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
    wrap_width: Option<usize>,
    caret_id: Option<&NodeId>,
) -> Node<Message> {
    let state = normalize_state(state.clone());
    if state.value.is_empty() {
        if enabled {
            return Node::row([
                text_area_caret(style.cursor, caret_id),
                Node::styled_text(placeholder, style.placeholder),
            ]);
        }
        return Node::styled_text(placeholder, style.disabled);
    }

    let lines = visual_line_ranges(&state.value, wrap_width);
    let cursor_line = visual_line_index(&lines, state.cursor);
    let horizontal_offset = if wrap_width.is_some() {
        0
    } else {
        state.horizontal_offset
    };
    let mut nodes = Vec::with_capacity(lines.len());
    for (index, line) in lines.into_iter().enumerate() {
        let line_style = if enabled {
            style.normal
        } else {
            style.disabled
        };
        let line_caret_id = if enabled && index == cursor_line {
            caret_id
        } else {
            None
        };
        nodes.push(text_area_line_content(
            &state,
            line,
            TextAreaLineRender {
                cursor_line: enabled && index == cursor_line,
                normal_style: line_style,
                cursor_style: style.cursor,
                selection_style,
                horizontal_offset,
                caret_id: line_caret_id,
            },
        ));
    }
    Node::column(nodes)
}

struct TextAreaLineRender<'a> {
    cursor_line: bool,
    normal_style: Style,
    cursor_style: Style,
    selection_style: Style,
    horizontal_offset: usize,
    caret_id: Option<&'a NodeId>,
}

fn text_area_line_content<Message>(
    state: &TextAreaState,
    line: Range<usize>,
    render: TextAreaLineRender<'_>,
) -> Node<Message> {
    let line_text = &state.value[line.clone()];
    let visible_start = line
        .start
        .saturating_add(text_area_visible_start(line_text, render.horizontal_offset));
    let selection = state.selection();
    let mut boundaries = vec![visible_start, line.end];
    if let Some(selection) = &selection {
        append_boundary(&mut boundaries, selection.start, visible_start, line.end);
        append_boundary(&mut boundaries, selection.end, visible_start, line.end);
    }
    if render.cursor_line {
        append_boundary(&mut boundaries, state.cursor, visible_start, line.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut cursor_visible = render.cursor_line
        && state.cursor >= line.start
        && state.cursor <= line.end
        && cell_at_byte(
            line_text,
            state.cursor.saturating_sub(line.start),
            WidthProfile::MODERN,
        )
        .is_some_and(|cell| cell >= render.horizontal_offset);
    let mut parts = Vec::with_capacity(boundaries.len().saturating_mul(2));
    if render.cursor_line && !cursor_visible && render.caret_id.is_some() {
        parts.push(text_area_hidden_caret(render.normal_style, render.caret_id));
    }
    for pair in boundaries.windows(2) {
        let start = pair[0];
        let end = pair[1];
        if cursor_visible && state.cursor == start {
            parts.push(text_area_caret(render.cursor_style, render.caret_id));
            cursor_visible = false;
        }
        if start == end {
            continue;
        }
        let mut part_style = render.normal_style;
        if selection
            .as_ref()
            .is_some_and(|selection| start >= selection.start && start < selection.end)
        {
            part_style = part_style.merged(render.selection_style);
        }
        parts.push(Node::styled_text(&state.value[start..end], part_style));
    }
    if cursor_visible && state.cursor == line.end {
        parts.push(text_area_caret(render.cursor_style, render.caret_id));
    }
    if parts.is_empty() {
        Node::styled_text("", render.normal_style)
    } else {
        Node::row(parts)
    }
}

fn text_area_caret<Message>(style: Style, id: Option<&NodeId>) -> Node<Message> {
    let caret = Node::styled_text("▏", style);
    match id {
        Some(id) => caret.with_id(id.clone()),
        None => caret,
    }
}

fn text_area_hidden_caret<Message>(style: Style, id: Option<&NodeId>) -> Node<Message> {
    let caret = Node::styled_text("", style);
    match id {
        Some(id) => caret.with_id(id.clone()),
        None => caret,
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
    Home,
    End,
    Backspace,
    Delete,
}

fn raw_edit_for_event(
    state: &TextAreaState,
    event: &Event,
    insertion_policy: Option<&TextAreaInsertionPolicy>,
) -> Option<Option<TextAreaState>> {
    match event {
        Event::Text(text) | Event::Paste(text) => {
            Some(apply_insertion(state, text, insertion_policy))
        }
        _ => None,
    }
}

fn apply_insertion(
    state: &TextAreaState,
    inserted: &str,
    insertion_policy: Option<&TextAreaInsertionPolicy>,
) -> Option<TextAreaState> {
    match insertion_policy.map_or(TextAreaInsertionDecision::Accept, |policy| {
        policy(state, inserted)
    }) {
        TextAreaInsertionDecision::Accept => {
            Some(apply_edit(state, TextAreaEdit::Insert(inserted)))
        }
        TextAreaInsertionDecision::Reject => None,
        TextAreaInsertionDecision::Replace(replacement) => {
            Some(apply_edit(state, TextAreaEdit::Insert(&replacement)))
        }
    }
}

fn select_all(state: &TextAreaState) -> TextAreaState {
    let mut next = normalize_state(state.clone());
    next.cursor = next.value.len();
    next.selection_anchor = 0;
    next.has_selection = next.cursor != 0;
    next.preferred_column = None;
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
        TextAreaEdit::Left | TextAreaEdit::Right | TextAreaEdit::Home | TextAreaEdit::End => {
            apply_movement(&state, edit, false)
        }
        TextAreaEdit::Backspace => {
            if let Some(selection) = state.selection() {
                let mut output = value.clone();
                output.replace_range(selection.clone(), "");
                return edited_state(&state, output, selection.start);
            }
            let start = previous_grapheme_boundary(value, cursor).unwrap_or(cursor);
            if start == cursor {
                return state;
            }
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
            if end == cursor {
                return state;
            }
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
                return moved_state(state, selection.start, false, None);
            }
            if matches!(edit, TextAreaEdit::Right) {
                return moved_state(state, selection.end, false, None);
            }
        }
    }
    let target = match edit {
        TextAreaEdit::Left => previous_grapheme_boundary(&state.value, state.cursor).unwrap_or(0),
        TextAreaEdit::Right => {
            next_grapheme_boundary(&state.value, state.cursor).unwrap_or(state.value.len())
        }
        TextAreaEdit::Home => current_line(&state.value, state.cursor).start,
        TextAreaEdit::End => current_line(&state.value, state.cursor).end,
        TextAreaEdit::Insert(_) | TextAreaEdit::Backspace | TextAreaEdit::Delete => {
            panic!("widget: invalid text area movement")
        }
    };
    if target == state.cursor && !state.has_selection {
        state
    } else {
        moved_state(state, target, extend, None)
    }
}

fn apply_vertical_movement(
    state: &TextAreaState,
    down: bool,
    extend: bool,
    wrap_width: Option<usize>,
) -> TextAreaState {
    let state = normalize_state(state.clone());
    let lines = visual_line_ranges(&state.value, wrap_width);
    let current = visual_line_index(&lines, state.cursor);
    let target = if down {
        current.saturating_add(1).min(lines.len().saturating_sub(1))
    } else {
        current.saturating_sub(1)
    };
    if target == current {
        return state;
    }
    let current_line = &state.value[lines[current].clone()];
    let preferred = state.preferred_column.unwrap_or_else(|| {
        cell_at_byte(
            current_line,
            state.cursor.saturating_sub(lines[current].start),
            WidthProfile::MODERN,
        )
        .unwrap_or(0)
    });
    let target_line = &state.value[lines[target].clone()];
    let relative = truncate(target_line, preferred, WidthProfile::MODERN).len();
    moved_state(
        state,
        lines[target].start.saturating_add(relative),
        extend,
        Some(preferred),
    )
}

fn moved_state(
    mut state: TextAreaState,
    cursor: usize,
    extend: bool,
    preferred_column: Option<usize>,
) -> TextAreaState {
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
    state.preferred_column = preferred_column;
    normalize_state(state)
}

fn edited_state(state: &TextAreaState, value: String, cursor: usize) -> TextAreaState {
    if value == state.value && cursor == state.cursor && !state.has_selection {
        return state.clone();
    }
    normalize_state(TextAreaState {
        value,
        cursor,
        selection_anchor: cursor,
        has_selection: false,
        horizontal_offset: state.horizontal_offset,
        preferred_column: None,
    })
}

fn current_line(value: &str, cursor: usize) -> Range<usize> {
    line_ranges(value)
        .into_iter()
        .find(|line| cursor >= line.start && cursor <= line.end)
        .unwrap_or(value.len()..value.len())
}

fn visual_line_ranges(value: &str, wrap_width: Option<usize>) -> Vec<Range<usize>> {
    let Some(wrap_width) = wrap_width else {
        return line_ranges(value);
    };
    let wrap_width = wrap_width.max(1);
    let logical_lines = line_ranges(value);
    let mut visual_lines = Vec::with_capacity(logical_lines.len());
    for logical in logical_lines {
        if logical.is_empty() {
            visual_lines.push(logical);
            continue;
        }
        let mut start = logical.start;
        let mut cells = 0_usize;
        for grapheme in graphemes(&value[logical.clone()]) {
            let grapheme_start = logical.start.saturating_add(grapheme.start());
            let width = grapheme_width(grapheme.text(), WidthProfile::MODERN);
            let next = cells.saturating_add(width);
            if width != 0 && grapheme_start != start && next > wrap_width {
                visual_lines.push(start..grapheme_start);
                start = grapheme_start;
                cells = width;
            } else {
                cells = next;
            }
        }
        visual_lines.push(start..logical.end);
    }
    visual_lines
}

pub(crate) fn text_area_visual_line_count(
    state: &TextAreaState,
    wrap_width: Option<usize>,
) -> usize {
    let state = normalize_state(state.clone());
    visual_line_ranges(&state.value, wrap_width).len()
}

fn visual_line_index(lines: &[Range<usize>], cursor: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .position(|(index, line)| {
            if cursor < line.start || cursor > line.end {
                return false;
            }
            cursor < line.end
                || lines
                    .get(index.saturating_add(1))
                    .is_none_or(|next| next.start != line.end)
        })
        .unwrap_or(lines.len().saturating_sub(1))
}

fn visual_line_directions(state: &TextAreaState, wrap_width: Option<usize>) -> (bool, bool) {
    let state = normalize_state(state.clone());
    let lines = visual_line_ranges(&state.value, wrap_width);
    let current = visual_line_index(&lines, state.cursor);
    (current > 0, current.saturating_add(1) < lines.len())
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
        ActionAvailability, RepeatPolicy, TextAreaBoundaryNavigation, TextAreaEdit,
        TextAreaSemanticAction, TextAreaState, apply_edit, apply_movement, apply_vertical_movement,
        select_all, text_area_action_descriptors, text_area_state_for_action,
        visual_line_directions, visual_line_index, visual_line_ranges,
    };
    use std::ops::Range;

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
            let state = TextAreaState::new(initial, number(record.field("cursor")));
            let mut actual = match record.field("operation") {
                "insert" => apply_edit(&state, TextAreaEdit::Insert(&inserted)),
                "left" => apply_edit(&state, TextAreaEdit::Left),
                "right" => apply_edit(&state, TextAreaEdit::Right),
                "up" => apply_vertical_movement(&state, false, false, None),
                "down" => apply_vertical_movement(&state, true, false, None),
                "home" => apply_edit(&state, TextAreaEdit::Home),
                "end" => apply_edit(&state, TextAreaEdit::End),
                "backspace" => apply_edit(&state, TextAreaEdit::Backspace),
                "delete" => apply_edit(&state, TextAreaEdit::Delete),
                operation => panic!("invalid operation {operation}"),
            };
            actual.preferred_column = None;
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
    fn visual_editing_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/text-area-visual.txt",
            "widget-text-area-visual",
            &[
                "initial",
                "cursor",
                "wrap",
                "boundary",
                "events",
                "height",
                "expected-cursor",
                "expected-preferred",
                "expected-selection",
                "expected-ranges",
                "expected-cursor-line",
                "expected-offset",
                "expected-up",
                "expected-down",
                "expected-consumed",
            ],
        ) else {
            return;
        };

        for record in records {
            let wrap_width = optional_number(record.field("wrap"));
            let mut state =
                TextAreaState::new(record.text("initial"), number(record.field("cursor")));
            if record.field("events") != "-" {
                for event in record.field("events").split(',') {
                    state = text_area_state_for_action(
                        &state,
                        visual_fixture_action(event),
                        wrap_width.map(|width| width.max(1)),
                    );
                }
            }

            assert_eq!(
                state.cursor(),
                number(record.field("expected-cursor")),
                "case {}",
                record.id
            );
            assert_eq!(
                state.preferred_column(),
                optional_number(record.field("expected-preferred")),
                "case {}",
                record.id
            );
            assert_eq!(
                state.selection(),
                optional_range(record.field("expected-selection")),
                "case {}",
                record.id
            );
            let lines = visual_line_ranges(&state.value, wrap_width.map(|width| width.max(1)));
            assert_eq!(
                lines,
                ranges(record.field("expected-ranges")),
                "case {}",
                record.id
            );
            assert_eq!(
                visual_line_index(&lines, state.cursor),
                number(record.field("expected-cursor-line")),
                "case {}",
                record.id
            );
            let directions = visual_line_directions(&state, wrap_width.map(|width| width.max(1)));
            let boundary = match record.field("boundary") {
                "consume" => TextAreaBoundaryNavigation::Consume,
                "bubble" => TextAreaBoundaryNavigation::Bubble,
                value => panic!("invalid boundary {value}"),
            };
            let descriptors = text_area_action_descriptors(
                true,
                false,
                false,
                boundary,
                &state,
                wrap_width.map(|width| width.max(1)),
            );
            assert_eq!(
                descriptors[2].availability(),
                fixture_availability(record.field("expected-up")),
                "case {} up direction {directions:?}",
                record.id
            );
            assert_eq!(
                descriptors[3].availability(),
                fixture_availability(record.field("expected-down")),
                "case {} down direction {directions:?}",
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
        let state = TextAreaState::default();
        let enabled = text_area_action_descriptors(
            true,
            true,
            true,
            TextAreaBoundaryNavigation::Consume,
            &state,
            None,
        );
        let unavailable = text_area_action_descriptors(
            false,
            false,
            false,
            TextAreaBoundaryNavigation::Consume,
            &state,
            None,
        );

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

    fn optional_number(value: &str) -> Option<usize> {
        (value != "-").then(|| number(value))
    }

    fn visual_fixture_action(value: &str) -> TextAreaSemanticAction {
        match value {
            "up" => TextAreaSemanticAction::CursorUp,
            "down" => TextAreaSemanticAction::CursorDown,
            "left" => TextAreaSemanticAction::CursorLeft,
            "right" => TextAreaSemanticAction::CursorRight,
            "shift-up" => TextAreaSemanticAction::SelectionExtendUp,
            "shift-down" => TextAreaSemanticAction::SelectionExtendDown,
            value => panic!("invalid visual event {value}"),
        }
    }

    fn ranges(value: &str) -> Vec<Range<usize>> {
        value.split(',').map(required_range).collect()
    }

    fn optional_range(value: &str) -> Option<Range<usize>> {
        (value != "-").then(|| required_range(value))
    }

    fn required_range(value: &str) -> Range<usize> {
        let (start, end) = value
            .split_once(':')
            .unwrap_or_else(|| panic!("invalid range {value}"));
        number(start)..number(end)
    }

    fn fixture_availability(value: &str) -> ActionAvailability {
        match value {
            "enabled" => ActionAvailability::Enabled,
            "pass" => ActionAvailability::DisabledPassThrough,
            value => panic!("invalid availability {value}"),
        }
    }
}
