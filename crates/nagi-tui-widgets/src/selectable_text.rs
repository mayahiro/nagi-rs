use std::ops::Range;
use std::sync::{Arc, LazyLock};

use nagi_text::graphemes;
use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, EventResult, KeyBinding, KeyCode, KeyStroke,
    Modifiers, MouseButton, MouseKind, Node, NodeId, ParagraphOptions, PointerEventContext,
    RepeatPolicy, Style, TEXT_COPY_DOCUMENT_ACTION_ID, TEXT_COPY_SELECTION_ACTION_ID,
    TEXT_CURSOR_DOCUMENT_END_ACTION_ID, TEXT_CURSOR_DOCUMENT_START_ACTION_ID,
    TEXT_CURSOR_LEFT_ACTION_ID, TEXT_CURSOR_LINE_END_ACTION_ID, TEXT_CURSOR_LINE_START_ACTION_ID,
    TEXT_CURSOR_RIGHT_ACTION_ID, TEXT_CURSOR_WORD_LEFT_ACTION_ID, TEXT_CURSOR_WORD_RIGHT_ACTION_ID,
    TEXT_SELECT_ALL_ACTION_ID, TEXT_SELECTION_EXTEND_DOCUMENT_END_ACTION_ID,
    TEXT_SELECTION_EXTEND_DOCUMENT_START_ACTION_ID, TEXT_SELECTION_EXTEND_LEFT_ACTION_ID,
    TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID, TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID,
    TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID, TEXT_SELECTION_EXTEND_WORD_LEFT_ACTION_ID,
    TEXT_SELECTION_EXTEND_WORD_RIGHT_ACTION_ID, TextSpan,
};

/// Immutable semantic text and styled runs displayed by [`SelectableText`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectableTextContent {
    inner: Arc<SelectableTextContentInner>,
}

#[derive(Debug, Eq, PartialEq)]
struct SelectableTextContentInner {
    concatenated_text: Option<String>,
    spans: Arc<[TextSpan]>,
    copyable: bool,
}

impl SelectableTextContent {
    /// Creates content containing one default-style span
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self::styled([TextSpan::new(text, Style::default())])
    }

    /// Creates content from ordered styled spans
    ///
    /// A hidden span makes the complete content non-copyable so presentation
    /// state cannot implicitly expose hidden text through a copy callback
    #[must_use]
    pub fn styled(spans: impl IntoIterator<Item = TextSpan>) -> Self {
        let spans: Vec<TextSpan> = spans.into_iter().collect();
        let concatenated_text = (spans.len() > 1).then(|| {
            let capacity = spans
                .iter()
                .try_fold(0_usize, |total, span| total.checked_add(span.text().len()))
                .unwrap_or(0);
            let mut text = String::with_capacity(capacity);
            for span in &spans {
                text.push_str(span.text());
            }
            text
        });
        let copyable = spans.iter().all(|span| !span.style().hidden);
        Self {
            inner: Arc::new(SelectableTextContentInner {
                concatenated_text,
                spans: Arc::from(spans),
                copyable,
            }),
        }
    }

    /// Returns the semantic UTF-8 document text
    #[must_use]
    pub fn text(&self) -> &str {
        self.inner.concatenated_text.as_deref().unwrap_or_else(|| {
            self.inner
                .spans
                .first()
                .map_or("", nagi_tui::TextSpan::text)
        })
    }

    /// Returns the immutable styled runs in display order
    #[must_use]
    pub fn spans(&self) -> &[TextSpan] {
        &self.inner.spans
    }

    /// Reports whether this content may be supplied to a copy callback
    #[must_use]
    pub fn is_copyable(&self) -> bool {
        self.inner.copyable
    }

    /// Normalizes selection offsets to this content's grapheme boundaries
    #[must_use]
    pub fn normalize_state(&self, state: SelectableTextState) -> SelectableTextState {
        normalize_selectable_text_state(self.text(), state)
    }
}

impl Default for SelectableTextContent {
    fn default() -> Self {
        Self::plain("")
    }
}

/// Application-owned cursor and optional selection for [`SelectableText`]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SelectableTextState {
    cursor: usize,
    selection_anchor: usize,
    has_selection: bool,
}

impl SelectableTextState {
    /// Creates a collapsed selection at a UTF-8 byte offset
    ///
    /// The current content normalizes the offset when a widget is built
    #[must_use]
    pub const fn new(cursor: usize) -> Self {
        Self {
            cursor,
            selection_anchor: cursor,
            has_selection: false,
        }
    }

    /// Creates a selection between two UTF-8 byte offsets
    ///
    /// The current content normalizes both offsets when a widget is built
    #[must_use]
    pub const fn with_selection(cursor: usize, anchor: usize) -> Self {
        Self {
            cursor,
            selection_anchor: anchor,
            has_selection: cursor != anchor,
        }
    }

    /// Returns the cursor UTF-8 byte offset
    #[must_use]
    pub const fn cursor(self) -> usize {
        self.cursor
    }

    /// Returns the selection anchor when a non-empty selection exists
    #[must_use]
    pub const fn selection_anchor(self) -> Option<usize> {
        if self.has_selection {
            Some(self.selection_anchor)
        } else {
            None
        }
    }

    /// Returns the ordered non-empty UTF-8 byte selection range
    #[must_use]
    pub fn selection(self) -> Option<Range<usize>> {
        self.has_selection
            .then(|| self.cursor.min(self.selection_anchor)..self.cursor.max(self.selection_anchor))
    }

    /// Returns state selecting from `anchor` to the current cursor
    #[must_use]
    pub const fn select(mut self, anchor: usize) -> Self {
        self.selection_anchor = anchor;
        self.has_selection = anchor != self.cursor;
        self
    }

    /// Returns state with the selection collapsed at the current cursor
    #[must_use]
    pub const fn clear_selection(mut self) -> Self {
        self.selection_anchor = self.cursor;
        self.has_selection = false;
        self
    }
}

/// Semantic source represented by a [`TextCopyRequest`]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TextCopyKind {
    /// The current non-empty selection
    Selection,
    /// The complete text document
    Document,
}

/// An application-handled request to copy semantic text
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextCopyRequest {
    source: NodeId,
    kind: TextCopyKind,
    text: String,
    range: Range<usize>,
}

impl TextCopyRequest {
    /// Returns the stable source Node ID
    #[must_use]
    pub const fn source(&self) -> &NodeId {
        &self.source
    }

    /// Returns whether the request represents a selection or complete document
    #[must_use]
    pub const fn kind(&self) -> TextCopyKind {
        self.kind
    }

    /// Returns the owned semantic UTF-8 text
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the UTF-8 byte range in the original document
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
}

/// Visual styles used by [`SelectableText`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectableTextStyle {
    /// Style merged over selected graphemes
    pub selection: Style,
    /// Style merged over the widget while it owns focus
    pub focused: Style,
    /// Style merged over every span while the widget is disabled
    pub disabled: Style,
}

impl Default for SelectableTextStyle {
    fn default() -> Self {
        Self {
            selection: Style {
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

/// Controlled keyboard and pointer selection over one styled text document
///
/// Copy requests are emitted as application messages. This widget does not
/// access an OS or terminal clipboard. Left-button dragging requires the
/// terminal driver to enable button-motion mouse tracking
pub struct SelectableText<Message> {
    id: NodeId,
    content: SelectableTextContent,
    state: SelectableTextState,
    options: ParagraphOptions,
    enabled: bool,
    style: SelectableTextStyle,
    on_change: Arc<dyn Fn(SelectableTextState) -> Message>,
    on_copy: Option<Arc<dyn Fn(TextCopyRequest) -> Message>>,
}

impl<Message: 'static> SelectableText<Message> {
    /// Creates an enabled selectable document using application-owned state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        content: SelectableTextContent,
        state: SelectableTextState,
        on_change: impl Fn(SelectableTextState) -> Message + 'static,
    ) -> Self {
        let state = content.normalize_state(state);
        Self {
            id: id.into(),
            content,
            state,
            options: ParagraphOptions::default(),
            enabled: true,
            style: SelectableTextStyle::default(),
            on_change: Arc::new(on_change),
            on_copy: None,
        }
    }

    /// Sets paragraph wrapping and alignment
    #[must_use]
    pub const fn paragraph_options(mut self, options: ParagraphOptions) -> Self {
        self.options = options;
        self
    }

    /// Sets whether this document can receive focus and selection actions
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces selection, focus, and disabled styles
    #[must_use]
    pub const fn style(mut self, style: SelectableTextStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the application callback for selection and document copy requests
    #[must_use]
    pub fn on_copy(mut self, handler: impl Fn(TextCopyRequest) -> Message + 'static) -> Self {
        self.on_copy = Some(Arc::new(handler));
        self
    }

    /// Returns the 19 ordered semantic action descriptors
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; SELECTABLE_TEXT_ACTION_COUNT] {
        selectable_text_action_descriptors(
            self.enabled,
            self.on_copy.is_some(),
            self.content.is_copyable(),
            &self.content,
            self.state,
        )
    }

    /// Builds the public semantic node for this selectable document
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let state = self.content.normalize_state(self.state);
        let descriptors = selectable_text_action_descriptors(
            self.enabled,
            self.on_copy.is_some(),
            self.content.is_copyable(),
            &self.content,
            state,
        );
        let spans =
            selectable_text_spans(&self.content, state.selection(), self.style, self.enabled);
        let node = Node::paragraph(spans, self.options);
        let id = self.id;
        if !self.enabled {
            return node.with_id(id.clone()).on_actions(
                id,
                descriptors
                    .into_iter()
                    .map(|descriptor| Action::new(descriptor, |_| EventResult::ignored())),
            );
        }

        let context = Arc::new(SelectableTextActionContext {
            id: id.clone(),
            content: self.content,
            state,
            on_change: self.on_change,
            on_copy: self.on_copy,
        });
        let action_context = Arc::clone(&context);
        let actions = descriptors.into_iter().zip(SELECTABLE_TEXT_ACTIONS).map(
            move |(descriptor, action)| {
                let context = Arc::clone(&action_context);
                Action::new(descriptor, move |_| {
                    selectable_text_action_result(action, context.as_ref())
                })
            },
        );
        let pointer_context = Arc::clone(&context);
        node.focusable(id.clone())
            .with_focused_style(self.style.focused)
            .on_pointer_event(id.clone(), move |pointer| {
                selectable_text_pointer_result(pointer, pointer_context.as_ref())
            })
            .on_actions(id, actions)
    }
}

const SELECTABLE_TEXT_ACTION_COUNT: usize = 19;

#[derive(Clone, Copy)]
enum SelectableTextAction {
    CursorLeft,
    CursorRight,
    CursorWordLeft,
    CursorWordRight,
    CursorLineStart,
    CursorLineEnd,
    CursorDocumentStart,
    CursorDocumentEnd,
    SelectionExtendLeft,
    SelectionExtendRight,
    SelectionExtendWordLeft,
    SelectionExtendWordRight,
    SelectionExtendLineStart,
    SelectionExtendLineEnd,
    SelectionExtendDocumentStart,
    SelectionExtendDocumentEnd,
    SelectAll,
    CopySelection,
    CopyDocument,
}

const SELECTABLE_TEXT_ACTIONS: [SelectableTextAction; SELECTABLE_TEXT_ACTION_COUNT] = [
    SelectableTextAction::CursorLeft,
    SelectableTextAction::CursorRight,
    SelectableTextAction::CursorWordLeft,
    SelectableTextAction::CursorWordRight,
    SelectableTextAction::CursorLineStart,
    SelectableTextAction::CursorLineEnd,
    SelectableTextAction::CursorDocumentStart,
    SelectableTextAction::CursorDocumentEnd,
    SelectableTextAction::SelectionExtendLeft,
    SelectableTextAction::SelectionExtendRight,
    SelectableTextAction::SelectionExtendWordLeft,
    SelectableTextAction::SelectionExtendWordRight,
    SelectableTextAction::SelectionExtendLineStart,
    SelectableTextAction::SelectionExtendLineEnd,
    SelectableTextAction::SelectionExtendDocumentStart,
    SelectableTextAction::SelectionExtendDocumentEnd,
    SelectableTextAction::SelectAll,
    SelectableTextAction::CopySelection,
    SelectableTextAction::CopyDocument,
];

static SELECTABLE_TEXT_ACTION_DESCRIPTORS: LazyLock<
    [ActionDescriptor; SELECTABLE_TEXT_ACTION_COUNT],
> = LazyLock::new(|| {
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
        selectable_text_descriptor(
            TEXT_CURSOR_LEFT_ACTION_ID,
            "Move cursor left",
            [repeatable_key(KeyCode::Left, Modifiers::NONE)],
        ),
        selectable_text_descriptor(
            TEXT_CURSOR_RIGHT_ACTION_ID,
            "Move cursor right",
            [repeatable_key(KeyCode::Right, Modifiers::NONE)],
        ),
        selectable_text_descriptor(
            TEXT_CURSOR_WORD_LEFT_ACTION_ID,
            "Move to previous word",
            [repeatable_key(KeyCode::Left, control)],
        ),
        selectable_text_descriptor(
            TEXT_CURSOR_WORD_RIGHT_ACTION_ID,
            "Move to next word",
            [repeatable_key(KeyCode::Right, control)],
        ),
        selectable_text_descriptor(
            TEXT_CURSOR_LINE_START_ACTION_ID,
            "Move to line start",
            [repeatable_key(KeyCode::Home, Modifiers::NONE)],
        ),
        selectable_text_descriptor(
            TEXT_CURSOR_LINE_END_ACTION_ID,
            "Move to line end",
            [repeatable_key(KeyCode::End, Modifiers::NONE)],
        ),
        selectable_text_descriptor(
            TEXT_CURSOR_DOCUMENT_START_ACTION_ID,
            "Move to document start",
            [repeatable_key(KeyCode::Home, control)],
        ),
        selectable_text_descriptor(
            TEXT_CURSOR_DOCUMENT_END_ACTION_ID,
            "Move to document end",
            [repeatable_key(KeyCode::End, control)],
        ),
        selectable_text_descriptor(
            TEXT_SELECTION_EXTEND_LEFT_ACTION_ID,
            "Extend selection left",
            [repeatable_key(KeyCode::Left, shift)],
        ),
        selectable_text_descriptor(
            TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID,
            "Extend selection right",
            [repeatable_key(KeyCode::Right, shift)],
        ),
        selectable_text_descriptor(
            TEXT_SELECTION_EXTEND_WORD_LEFT_ACTION_ID,
            "Extend selection to previous word",
            [repeatable_key(KeyCode::Left, control_shift)],
        ),
        selectable_text_descriptor(
            TEXT_SELECTION_EXTEND_WORD_RIGHT_ACTION_ID,
            "Extend selection to next word",
            [repeatable_key(KeyCode::Right, control_shift)],
        ),
        selectable_text_descriptor(
            TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID,
            "Extend selection to line start",
            [repeatable_key(KeyCode::Home, shift)],
        ),
        selectable_text_descriptor(
            TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID,
            "Extend selection to line end",
            [repeatable_key(KeyCode::End, shift)],
        ),
        selectable_text_descriptor(
            TEXT_SELECTION_EXTEND_DOCUMENT_START_ACTION_ID,
            "Extend selection to document start",
            [repeatable_key(KeyCode::Home, control_shift)],
        ),
        selectable_text_descriptor(
            TEXT_SELECTION_EXTEND_DOCUMENT_END_ACTION_ID,
            "Extend selection to document end",
            [repeatable_key(KeyCode::End, control_shift)],
        ),
        selectable_text_descriptor(
            TEXT_SELECT_ALL_ACTION_ID,
            "Select all",
            [repeatable_character('a', control)],
        ),
        selectable_text_descriptor(
            TEXT_COPY_SELECTION_ACTION_ID,
            "Copy selection",
            [initial_character('c', control)],
        ),
        selectable_text_descriptor(
            TEXT_COPY_DOCUMENT_ACTION_ID,
            "Copy document",
            [initial_character('c', control_shift)],
        ),
    ]
});

fn selectable_text_descriptor(
    id: &'static str,
    label: &'static str,
    bindings: impl IntoIterator<Item = KeyBinding>,
) -> ActionDescriptor {
    ActionDescriptor::new(id, label, bindings)
}

fn repeatable_key(code: KeyCode, modifiers: Modifiers) -> KeyBinding {
    KeyBinding::new(KeyStroke::new(code, modifiers)).with_repeat_policy(RepeatPolicy::AllowRepeat)
}

fn repeatable_character(character: char, modifiers: Modifiers) -> KeyBinding {
    KeyBinding::new(KeyStroke::character(character, modifiers))
        .with_repeat_policy(RepeatPolicy::AllowRepeat)
}

fn initial_character(character: char, modifiers: Modifiers) -> KeyBinding {
    KeyBinding::new(KeyStroke::character(character, modifiers))
}

fn selectable_text_action_descriptors(
    enabled: bool,
    has_copy_handler: bool,
    copyable: bool,
    content: &SelectableTextContent,
    state: SelectableTextState,
) -> [ActionDescriptor; SELECTABLE_TEXT_ACTION_COUNT] {
    let state = content.normalize_state(state);
    std::array::from_fn(|index| {
        let available = enabled
            && match SELECTABLE_TEXT_ACTIONS[index] {
                SelectableTextAction::CopySelection => {
                    has_copy_handler && copyable && state.selection().is_some()
                }
                SelectableTextAction::CopyDocument => {
                    has_copy_handler && copyable && !content.text().is_empty()
                }
                _ => true,
            };
        SELECTABLE_TEXT_ACTION_DESCRIPTORS[index]
            .clone()
            .with_availability(if available {
                ActionAvailability::Enabled
            } else {
                ActionAvailability::DisabledPassThrough
            })
    })
}

struct SelectableTextActionContext<Message> {
    id: NodeId,
    content: SelectableTextContent,
    state: SelectableTextState,
    on_change: Arc<dyn Fn(SelectableTextState) -> Message>,
    on_copy: Option<Arc<dyn Fn(TextCopyRequest) -> Message>>,
}

fn selectable_text_action_result<Message>(
    action: SelectableTextAction,
    context: &SelectableTextActionContext<Message>,
) -> EventResult<Message> {
    match action {
        SelectableTextAction::CopySelection => {
            return selectable_text_copy_result(TextCopyKind::Selection, context);
        }
        SelectableTextAction::CopyDocument => {
            return selectable_text_copy_result(TextCopyKind::Document, context);
        }
        _ => {}
    }
    let next = selectable_text_state_for_action(&context.content, context.state, action);
    let result = EventResult::consumed().focus(context.id.clone());
    if next == context.state {
        result
    } else {
        result.emit((context.on_change)(next))
    }
}

fn selectable_text_pointer_result<Message>(
    pointer: &PointerEventContext,
    context: &SelectableTextActionContext<Message>,
) -> EventResult<Message> {
    let event = pointer.event();
    if event.button != MouseButton::Left {
        return EventResult::ignored();
    }
    let Some(hit) = pointer.text_hit() else {
        return EventResult::ignored();
    };
    match event.kind {
        MouseKind::Press => {
            let next = if event.modifiers.shift {
                let anchor = selectable_text_anchor(context.state);
                SelectableTextState::with_selection(pointer_selection_offset(hit, anchor), anchor)
            } else {
                SelectableTextState::new(hit.start())
            };
            let result = EventResult::consumed()
                .focus(context.id.clone())
                .capture_pointer(context.id.clone());
            emit_selectable_text_change(result, next, context)
        }
        MouseKind::Move => {
            if !pointer.is_captured() {
                return EventResult::ignored();
            }
            let anchor = selectable_text_anchor(context.state);
            let next =
                SelectableTextState::with_selection(pointer_selection_offset(hit, anchor), anchor);
            let mut result = EventResult::consumed().focus(context.id.clone());
            if let Some((viewport, offset)) = pointer.edge_scroll() {
                result = result.scroll_to(viewport.clone(), offset);
            }
            emit_selectable_text_change(result, next, context)
        }
        MouseKind::Release => {
            if !pointer.is_captured() {
                return EventResult::ignored();
            }
            EventResult::consumed().release_pointer()
        }
        MouseKind::Scroll => EventResult::ignored(),
    }
}

fn emit_selectable_text_change<Message>(
    result: EventResult<Message>,
    next: SelectableTextState,
    context: &SelectableTextActionContext<Message>,
) -> EventResult<Message> {
    let next = context.content.normalize_state(next);
    if next == context.state {
        result
    } else {
        result.emit((context.on_change)(next))
    }
}

const fn selectable_text_anchor(state: SelectableTextState) -> usize {
    if state.has_selection {
        state.selection_anchor
    } else {
        state.cursor
    }
}

const fn pointer_selection_offset(hit: nagi_tui::TextHit, anchor: usize) -> usize {
    if hit.end() <= anchor {
        hit.start()
    } else {
        hit.end()
    }
}

fn selectable_text_copy_result<Message>(
    kind: TextCopyKind,
    context: &SelectableTextActionContext<Message>,
) -> EventResult<Message> {
    let Some(handler) = context.on_copy.as_ref() else {
        return EventResult::ignored();
    };
    let range = match kind {
        TextCopyKind::Selection => {
            let Some(range) = context.state.selection() else {
                return EventResult::ignored();
            };
            range
        }
        TextCopyKind::Document => 0..context.content.text().len(),
    };
    let request = TextCopyRequest {
        source: context.id.clone(),
        kind,
        text: context.content.text()[range.clone()].to_owned(),
        range,
    };
    EventResult::consumed()
        .focus(context.id.clone())
        .emit(handler(request))
}

fn selectable_text_state_for_action(
    content: &SelectableTextContent,
    state: SelectableTextState,
    action: SelectableTextAction,
) -> SelectableTextState {
    let state = content.normalize_state(state);
    let text = content.text();
    match action {
        SelectableTextAction::CursorLeft => state.selection().map_or_else(
            || move_selectable_text(state, previous_cursor(text, state.cursor), false),
            |selection| SelectableTextState::new(selection.start),
        ),
        SelectableTextAction::CursorRight => state.selection().map_or_else(
            || move_selectable_text(state, next_cursor(text, state.cursor), false),
            |selection| SelectableTextState::new(selection.end),
        ),
        SelectableTextAction::CursorWordLeft => {
            move_selectable_text(state, previous_word(text, state.cursor), false)
        }
        SelectableTextAction::CursorWordRight => {
            move_selectable_text(state, next_word(text, state.cursor), false)
        }
        SelectableTextAction::CursorLineStart => {
            move_selectable_text(state, current_line(text, state.cursor).start, false)
        }
        SelectableTextAction::CursorLineEnd => {
            move_selectable_text(state, current_line(text, state.cursor).end, false)
        }
        SelectableTextAction::CursorDocumentStart => move_selectable_text(state, 0, false),
        SelectableTextAction::CursorDocumentEnd => move_selectable_text(state, text.len(), false),
        SelectableTextAction::SelectionExtendLeft => {
            move_selectable_text(state, previous_cursor(text, state.cursor), true)
        }
        SelectableTextAction::SelectionExtendRight => {
            move_selectable_text(state, next_cursor(text, state.cursor), true)
        }
        SelectableTextAction::SelectionExtendWordLeft => {
            move_selectable_text(state, previous_word(text, state.cursor), true)
        }
        SelectableTextAction::SelectionExtendWordRight => {
            move_selectable_text(state, next_word(text, state.cursor), true)
        }
        SelectableTextAction::SelectionExtendLineStart => {
            move_selectable_text(state, current_line(text, state.cursor).start, true)
        }
        SelectableTextAction::SelectionExtendLineEnd => {
            move_selectable_text(state, current_line(text, state.cursor).end, true)
        }
        SelectableTextAction::SelectionExtendDocumentStart => move_selectable_text(state, 0, true),
        SelectableTextAction::SelectionExtendDocumentEnd => {
            move_selectable_text(state, text.len(), true)
        }
        SelectableTextAction::SelectAll => SelectableTextState::with_selection(text.len(), 0),
        SelectableTextAction::CopySelection | SelectableTextAction::CopyDocument => state,
    }
}

fn move_selectable_text(
    state: SelectableTextState,
    target: usize,
    extend: bool,
) -> SelectableTextState {
    let anchor = if state.has_selection {
        state.selection_anchor
    } else {
        state.cursor
    };
    if extend {
        SelectableTextState::with_selection(target, anchor)
    } else {
        SelectableTextState::new(target)
    }
}

fn previous_cursor(text: &str, cursor: usize) -> usize {
    nagi_text::previous_grapheme_boundary(text, cursor).unwrap_or(0)
}

fn next_cursor(text: &str, cursor: usize) -> usize {
    nagi_text::next_grapheme_boundary(text, cursor).unwrap_or(text.len())
}

fn previous_word(text: &str, cursor: usize) -> usize {
    let mut word_start = 0;
    let mut in_word = false;
    for grapheme in graphemes(&text[..cursor]) {
        if word_separator(grapheme.text()) {
            in_word = false;
        } else if !in_word {
            word_start = grapheme.start();
            in_word = true;
        }
    }
    word_start
}

fn next_word(text: &str, cursor: usize) -> usize {
    let mut saw_word = false;
    let mut saw_separator_after_word = false;
    for grapheme in graphemes(&text[cursor..]) {
        let separator = word_separator(grapheme.text());
        if !separator && !saw_word {
            if grapheme.start() != 0 {
                return cursor.saturating_add(grapheme.start());
            }
            saw_word = true;
        } else if separator && saw_word {
            saw_separator_after_word = true;
        } else if !separator && saw_separator_after_word {
            return cursor.saturating_add(grapheme.start());
        }
    }
    text.len()
}

fn word_separator(text: &str) -> bool {
    text.chars()
        .all(|character| matches!(character, '\u{0009}'..='\u{000d}' | ' '))
}

fn current_line(text: &str, cursor: usize) -> Range<usize> {
    let mut start = 0;
    for grapheme in graphemes(text) {
        if matches!(grapheme.text(), "\r" | "\n" | "\r\n") {
            if cursor >= start && cursor <= grapheme.start() {
                return start..grapheme.start();
            }
            start = grapheme.end();
        }
    }
    start..text.len()
}

fn normalize_selectable_text_state(
    text: &str,
    mut state: SelectableTextState,
) -> SelectableTextState {
    let requested_cursor = state.cursor.min(text.len());
    let requested_anchor = state.selection_anchor.min(text.len());
    state.cursor = 0;
    state.selection_anchor = 0;
    for grapheme in graphemes(text) {
        if grapheme.end() <= requested_cursor {
            state.cursor = grapheme.end();
        }
        if grapheme.end() <= requested_anchor {
            state.selection_anchor = grapheme.end();
        }
        if grapheme.end() > requested_cursor && grapheme.end() > requested_anchor {
            break;
        }
    }
    if !state.has_selection || state.cursor == state.selection_anchor {
        state.selection_anchor = state.cursor;
        state.has_selection = false;
    }
    state
}

fn selectable_text_spans(
    content: &SelectableTextContent,
    selection: Option<Range<usize>>,
    style: SelectableTextStyle,
    enabled: bool,
) -> Vec<TextSpan> {
    let mut offset = 0_usize;
    let mut output = Vec::with_capacity(content.spans().len().saturating_add(2));
    for span in content.spans() {
        let start = offset;
        let end = start.saturating_add(span.text().len());
        offset = end;
        let base = if enabled {
            span.style()
        } else {
            span.style().merged(style.disabled)
        };
        let Some(selection) = selection
            .as_ref()
            .filter(|selection| selection.start < end && selection.end > start)
        else {
            output.push(span.clone().with_style(base));
            continue;
        };
        let selected_start = selection.start.max(start).saturating_sub(start);
        let selected_end = selection.end.min(end).saturating_sub(start);
        push_text_span(&mut output, &span.text()[..selected_start], base);
        push_text_span(
            &mut output,
            &span.text()[selected_start..selected_end],
            base.merged(style.selection),
        );
        push_text_span(&mut output, &span.text()[selected_end..], base);
    }
    output
}

fn push_text_span(output: &mut Vec<TextSpan>, text: &str, style: Style) {
    if !text.is_empty() {
        output.push(TextSpan::new(text, style));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_clones_share_immutable_storage() {
        let content = SelectableTextContent::styled([
            TextSpan::new("left", Style::default()),
            TextSpan::new("right", Style::default()),
        ]);
        let clone = content.clone();
        assert!(Arc::ptr_eq(&content.inner, &clone.inner));
        assert_eq!(content.text(), "leftright");

        let plain = SelectableTextContent::plain("one allocation");
        assert!(plain.inner.concatenated_text.is_none());
        assert!(std::ptr::eq(
            plain.text().as_ptr(),
            plain.spans()[0].text().as_ptr()
        ));
    }

    #[test]
    fn descriptor_clones_reuse_immutable_storage() {
        let content = SelectableTextContent::plain("text");
        let first = selectable_text_action_descriptors(
            true,
            true,
            true,
            &content,
            SelectableTextState::with_selection(4, 0),
        );
        let second = selectable_text_action_descriptors(
            false,
            false,
            true,
            &content,
            SelectableTextState::default(),
        );
        for index in 0..first.len() {
            assert!(std::ptr::eq(
                first[index].id().as_str(),
                second[index].id().as_str()
            ));
            assert!(std::ptr::eq(first[index].label(), second[index].label()));
            assert!(std::ptr::eq(
                first[index].default_bindings(),
                second[index].default_bindings()
            ));
        }
    }
}
