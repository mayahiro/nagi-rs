use std::ops::Range;
use std::sync::{Arc, LazyLock};

use nagi_text::{grapheme_width, graphemes};
use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, Event, EventResult, KeyAction, KeyBinding,
    KeyCode, KeyStroke, Length, Modifiers, Node, NodeId, ParagraphOptions, RepeatPolicy, Style,
    TEXT_COPY_DOCUMENT_ACTION_ID, TEXT_COPY_SELECTION_ACTION_ID, TEXT_SELECT_ALL_ACTION_ID,
    TextSpan, WrapMode,
};

use crate::action::{
    HORIZONTAL_SCROLL_NEXT_ACTION_ID, HORIZONTAL_SCROLL_PREVIOUS_ACTION_ID,
    SELECTION_EXTEND_FIRST_ACTION_ID, SELECTION_EXTEND_LAST_ACTION_ID,
    SELECTION_EXTEND_NEXT_ACTION_ID, SELECTION_EXTEND_PREVIOUS_ACTION_ID,
    SELECTION_FIRST_ACTION_ID, SELECTION_LAST_ACTION_ID, SELECTION_NEXT_ACTION_ID,
    SELECTION_PREVIOUS_ACTION_ID,
};
use crate::code::{CodeLayout, CodeRowCheckpoint, CodeVisualRow};
use crate::event::is_pointer_activation_event;

/// Application-owned logical-line selection and horizontal code offset
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CodeViewState {
    cursor: usize,
    anchor: usize,
    has_selection: bool,
    horizontal_offset: u32,
}

impl CodeViewState {
    /// Creates a collapsed logical-line selection
    #[must_use]
    pub const fn new(cursor: usize) -> Self {
        Self {
            cursor,
            anchor: cursor,
            has_selection: false,
            horizontal_offset: 0,
        }
    }

    /// Creates an inclusive logical-line selection between cursor and anchor
    #[must_use]
    pub const fn with_selection(cursor: usize, anchor: usize) -> Self {
        Self {
            cursor,
            anchor,
            has_selection: cursor != anchor,
            horizontal_offset: 0,
        }
    }

    /// Returns the selected logical line
    #[must_use]
    pub const fn cursor(self) -> usize {
        self.cursor
    }

    /// Returns the selection anchor when multiple logical lines are selected
    #[must_use]
    pub const fn selection_anchor(self) -> Option<usize> {
        if self.has_selection {
            Some(self.anchor)
        } else {
            None
        }
    }

    /// Returns the inclusive selection as an ordered end-exclusive line range
    #[must_use]
    pub fn selected_lines(self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor).saturating_add(1)
    }

    /// Returns the horizontal cell offset applied only in no-wrap layouts
    #[must_use]
    pub const fn horizontal_offset(self) -> u32 {
        self.horizontal_offset
    }

    /// Returns this state with a replacement horizontal cell offset
    #[must_use]
    pub const fn with_horizontal_offset(mut self, value: u32) -> Self {
        self.horizontal_offset = value;
        self
    }
}

/// Semantic source represented by a [`CodeCopyRequest`]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CodeCopyKind {
    /// The selected complete logical lines
    Selection,
    /// The complete code document
    Document,
}

/// Application-handled request that independently owns semantic source text
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeCopyRequest {
    source: NodeId,
    kind: CodeCopyKind,
    text: String,
    lines: Range<usize>,
    bytes: Range<usize>,
}

impl CodeCopyRequest {
    /// Returns the stable CodeView root Node ID
    #[must_use]
    pub const fn source(&self) -> &NodeId {
        &self.source
    }

    /// Returns whether selected lines or the complete document were requested
    #[must_use]
    pub const fn kind(&self) -> CodeCopyKind {
        self.kind
    }

    /// Returns independently owned semantic UTF-8 source text
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the ordered logical-line range in the original document
    #[must_use]
    pub fn lines(&self) -> Range<usize> {
        self.lines.clone()
    }

    /// Returns the UTF-8 byte range in the original document
    #[must_use]
    pub fn bytes(&self) -> Range<usize> {
        self.bytes.clone()
    }
}

/// Independent semantic style slots used by a [`CodeView`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodeViewStyle {
    /// Style for the line-number gutter
    pub line_number: Style,
    /// Style for the continuation marker in a wrapped row
    pub continuation: Style,
    /// Style merged over the current logical line
    pub current: Style,
    /// Style merged over every selected logical line
    pub selection: Style,
    /// Style merged over the complete view while it owns focus
    pub focused: Style,
    /// Style merged over every span while the view is disabled
    pub disabled: Style,
}

impl Default for CodeViewStyle {
    fn default() -> Self {
        Self {
            line_number: Style {
                dim: true,
                ..Style::default()
            },
            continuation: Style {
                dim: true,
                ..Style::default()
            },
            current: Style::default(),
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

/// Controlled, bounded terminal view over one immutable [`CodeLayout`]
///
/// Navigation and copy requests are returned to the application. The view
/// does not parse syntax, read files, or access a clipboard backend
pub struct CodeView<Message> {
    id: NodeId,
    layout: CodeLayout,
    state: CodeViewState,
    viewport_height: usize,
    horizontal_step: u32,
    enabled: bool,
    style: CodeViewStyle,
    on_change: Arc<dyn Fn(CodeViewState) -> Message>,
    on_copy: Option<Arc<dyn Fn(CodeCopyRequest) -> Message>>,
}

impl<Message: 'static> CodeView<Message> {
    /// Creates an enabled view using application-owned controlled state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        layout: CodeLayout,
        state: CodeViewState,
        on_change: impl Fn(CodeViewState) -> Message + 'static,
    ) -> Self {
        let state = normalize_state(&layout, state);
        Self {
            id: id.into(),
            layout,
            state,
            viewport_height: 0,
            horizontal_step: 4,
            enabled: true,
            style: CodeViewStyle::default(),
            on_change: Arc::new(on_change),
            on_copy: None,
        }
    }

    /// Returns the visually normalized controlled state
    #[must_use]
    pub const fn state(&self) -> CodeViewState {
        self.state
    }

    /// Sets whether the view can receive focus and emit messages
    #[must_use]
    pub const fn enabled(mut self, value: bool) -> Self {
        self.enabled = value;
        self
    }

    /// Limits constructed rows to a deterministic window following selection
    ///
    /// Zero constructs every visual row
    #[must_use]
    pub const fn viewport(mut self, height: usize) -> Self {
        self.viewport_height = height;
        self
    }

    /// Sets the positive no-wrap horizontal navigation step in cells
    ///
    /// Zero uses one cell
    #[must_use]
    pub const fn horizontal_step(mut self, value: u32) -> Self {
        self.horizontal_step = if value == 0 { 1 } else { value };
        self
    }

    /// Replaces every semantic style slot
    #[must_use]
    pub const fn style(mut self, value: CodeViewStyle) -> Self {
        self.style = value;
        self
    }

    /// Sets the application callback for semantic source copy requests
    #[must_use]
    pub fn on_copy(mut self, handler: impl Fn(CodeCopyRequest) -> Message + 'static) -> Self {
        self.on_copy = Some(Arc::new(handler));
        self
    }

    /// Returns the thirteen ordered semantic action descriptors
    ///
    /// The order is previous, next, first, last, four selection-extension
    /// actions, horizontal previous and next, select all, copy selection, and
    /// copy document
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; CODE_VIEW_ACTION_COUNT] {
        code_view_action_descriptors(
            self.enabled,
            self.on_copy.is_some(),
            &self.layout,
            self.state,
        )
    }

    /// Builds the public semantic node for this view
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        build_code_view_node(self)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum CodeViewAction {
    Previous,
    Next,
    First,
    Last,
    ExtendPrevious,
    ExtendNext,
    ExtendFirst,
    ExtendLast,
    HorizontalPrevious,
    HorizontalNext,
    SelectAll,
    CopySelection,
    CopyDocument,
}

pub(crate) const CODE_VIEW_ACTIONS: [CodeViewAction; 13] = [
    CodeViewAction::Previous,
    CodeViewAction::Next,
    CodeViewAction::First,
    CodeViewAction::Last,
    CodeViewAction::ExtendPrevious,
    CodeViewAction::ExtendNext,
    CodeViewAction::ExtendFirst,
    CodeViewAction::ExtendLast,
    CodeViewAction::HorizontalPrevious,
    CodeViewAction::HorizontalNext,
    CodeViewAction::SelectAll,
    CodeViewAction::CopySelection,
    CodeViewAction::CopyDocument,
];

pub(crate) const CODE_VIEW_ACTION_COUNT: usize = CODE_VIEW_ACTIONS.len();

pub(crate) static CODE_VIEW_ACTION_DESCRIPTORS: LazyLock<
    [ActionDescriptor; CODE_VIEW_ACTION_COUNT],
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
        control: true,
        shift: true,
        ..Modifiers::NONE
    };
    [
        repeatable_descriptor(
            SELECTION_PREVIOUS_ACTION_ID,
            "Previous line",
            KeyCode::Up,
            Modifiers::NONE,
        ),
        repeatable_descriptor(
            SELECTION_NEXT_ACTION_ID,
            "Next line",
            KeyCode::Down,
            Modifiers::NONE,
        ),
        repeatable_descriptor(
            SELECTION_FIRST_ACTION_ID,
            "First line",
            KeyCode::Home,
            control,
        ),
        repeatable_descriptor(SELECTION_LAST_ACTION_ID, "Last line", KeyCode::End, control),
        repeatable_descriptor(
            SELECTION_EXTEND_PREVIOUS_ACTION_ID,
            "Extend to previous line",
            KeyCode::Up,
            shift,
        ),
        repeatable_descriptor(
            SELECTION_EXTEND_NEXT_ACTION_ID,
            "Extend to next line",
            KeyCode::Down,
            shift,
        ),
        repeatable_descriptor(
            SELECTION_EXTEND_FIRST_ACTION_ID,
            "Extend to first line",
            KeyCode::Home,
            control_shift,
        ),
        repeatable_descriptor(
            SELECTION_EXTEND_LAST_ACTION_ID,
            "Extend to last line",
            KeyCode::End,
            control_shift,
        ),
        repeatable_descriptor(
            HORIZONTAL_SCROLL_PREVIOUS_ACTION_ID,
            "Scroll left",
            KeyCode::Left,
            Modifiers::NONE,
        ),
        repeatable_descriptor(
            HORIZONTAL_SCROLL_NEXT_ACTION_ID,
            "Scroll right",
            KeyCode::Right,
            Modifiers::NONE,
        ),
        ActionDescriptor::new(
            TEXT_SELECT_ALL_ACTION_ID,
            "Select all lines",
            [KeyBinding::new(KeyStroke::character('a', control))
                .with_repeat_policy(RepeatPolicy::AllowRepeat)],
        ),
        ActionDescriptor::new(
            TEXT_COPY_SELECTION_ACTION_ID,
            "Copy selected lines",
            [KeyBinding::new(KeyStroke::character('c', control))],
        ),
        ActionDescriptor::new(
            TEXT_COPY_DOCUMENT_ACTION_ID,
            "Copy document",
            [KeyBinding::new(KeyStroke::character('c', control_shift))],
        ),
    ]
});

fn repeatable_descriptor(
    id: &'static str,
    label: &'static str,
    code: KeyCode,
    modifiers: Modifiers,
) -> ActionDescriptor {
    ActionDescriptor::new(
        id,
        label,
        [KeyBinding::new(KeyStroke::new(code, modifiers))
            .with_repeat_policy(RepeatPolicy::AllowRepeat)],
    )
}

fn code_view_action_descriptors(
    enabled: bool,
    has_copy_handler: bool,
    layout: &CodeLayout,
    state: CodeViewState,
) -> [ActionDescriptor; CODE_VIEW_ACTION_COUNT] {
    let lines = layout.document().line_count();
    let state = normalize_state(layout, state);
    std::array::from_fn(|index| {
        let action = CODE_VIEW_ACTIONS[index];
        let available = enabled
            && lines > 0
            && match action {
                CodeViewAction::HorizontalPrevious | CodeViewAction::HorizontalNext => {
                    !layout.options().wraps() && layout.maximum_row_width() > layout.code_width()
                }
                CodeViewAction::CopySelection => {
                    has_copy_handler && layout.document().is_range_copyable(state.selected_lines())
                }
                CodeViewAction::CopyDocument => {
                    has_copy_handler && layout.document().is_range_copyable(0..lines)
                }
                _ => true,
            };
        CODE_VIEW_ACTION_DESCRIPTORS[index]
            .clone()
            .with_availability(if available {
                ActionAvailability::Enabled
            } else {
                ActionAvailability::DisabledPassThrough
            })
    })
}

struct CodeViewActionContext<Message> {
    id: NodeId,
    layout: CodeLayout,
    state: CodeViewState,
    horizontal_step: u32,
    on_change: Arc<dyn Fn(CodeViewState) -> Message>,
    on_copy: Option<Arc<dyn Fn(CodeCopyRequest) -> Message>>,
}

fn build_code_view_node<Message: 'static>(view: CodeView<Message>) -> Node<Message> {
    let state = normalize_state(&view.layout, view.state);
    let descriptors =
        code_view_action_descriptors(view.enabled, view.on_copy.is_some(), &view.layout, state);
    let context = Arc::new(CodeViewActionContext {
        id: view.id.clone(),
        layout: view.layout.clone(),
        state,
        horizontal_step: view.horizontal_step,
        on_change: Arc::clone(&view.on_change),
        on_copy: view.on_copy.as_ref().map(Arc::clone),
    });
    let rows = visible_row_window(&view.layout, state.cursor, view.viewport_height);
    let selected = state.selected_lines();
    let mut nodes = Vec::with_capacity(rows.len());
    for row_index in rows {
        let record = view.layout.row(row_index).expect("known visual row");
        let line = record.line;
        let mut spans = code_view_row_spans(
            &view.layout,
            record,
            state.horizontal_offset,
            selected.contains(&record.line),
            record.line == state.cursor,
            view.enabled,
            view.style,
        );
        if spans.is_empty() {
            spans.push(TextSpan::new("", Style::default()));
        }
        let row_id = code_view_row_id(&view.id, row_index);
        let mut row = Node::paragraph(
            spans,
            ParagraphOptions {
                wrap: WrapMode::None,
                ..ParagraphOptions::default()
            },
        )
        .with_id(row_id.clone());
        if view.enabled {
            let pointer_context = Arc::clone(&context);
            row = row.on_event(row_id, move |event| {
                code_view_pointer_result(event, line, pointer_context.as_ref())
            });
        }
        nodes.push(row);
    }

    let mut root = Node::column(nodes);
    if view.viewport_height > 0 {
        root = root.with_length(Length::Fixed(
            u32::try_from(view.viewport_height).unwrap_or(u32::MAX),
        ));
    }
    let id = view.id;
    let actions_context = Arc::clone(&context);
    let actions =
        descriptors
            .into_iter()
            .zip(CODE_VIEW_ACTIONS)
            .map(move |(descriptor, action)| {
                let context = Arc::clone(&actions_context);
                Action::new(descriptor, move |_| {
                    code_view_action_result(action, context.as_ref())
                })
            });
    if !view.enabled {
        return root.with_id(id.clone()).on_actions(id, actions);
    }
    root.focusable(id.clone())
        .with_focused_style(view.style.focused)
        .on_actions(id.clone(), actions)
        .on_event(id, move |event| {
            let (selection_copy, document_copy) =
                copy_actions_enabled(&context.layout, context.state, context.on_copy.is_some());
            if is_blocked_copy_repeat(event, selection_copy, document_copy) {
                EventResult::consumed()
            } else {
                EventResult::ignored()
            }
        })
}

fn code_view_action_result<Message>(
    action: CodeViewAction,
    context: &CodeViewActionContext<Message>,
) -> EventResult<Message> {
    if matches!(action, CodeViewAction::CopySelection) {
        return code_view_copy_result(CodeCopyKind::Selection, context);
    }
    if matches!(action, CodeViewAction::CopyDocument) {
        return code_view_copy_result(CodeCopyKind::Document, context);
    }
    let next = state_for_action(
        &context.layout,
        context.state,
        action,
        context.horizontal_step,
    );
    emit_code_view_change(
        EventResult::consumed().focus(context.id.clone()),
        next,
        context,
    )
}

fn code_view_copy_result<Message>(
    kind: CodeCopyKind,
    context: &CodeViewActionContext<Message>,
) -> EventResult<Message> {
    let Some(handler) = context.on_copy.as_ref() else {
        return EventResult::ignored();
    };
    let Some(request) = code_view_copy_request(kind, context) else {
        return EventResult::ignored();
    };
    EventResult::consumed()
        .focus(context.id.clone())
        .emit(handler(request))
}

fn code_view_copy_request<Message>(
    kind: CodeCopyKind,
    context: &CodeViewActionContext<Message>,
) -> Option<CodeCopyRequest> {
    let document = context.layout.document();
    let lines = match kind {
        CodeCopyKind::Selection => context.state.selected_lines(),
        CodeCopyKind::Document => 0..document.line_count(),
    };
    if !document.is_range_copyable(lines.clone()) {
        return None;
    }
    let bytes = document.byte_range_for_lines(lines.clone())?;
    Some(CodeCopyRequest {
        source: context.id.clone(),
        kind,
        text: document.text()[bytes.clone()].to_owned(),
        lines,
        bytes,
    })
}

fn code_view_pointer_result<Message>(
    event: &Event,
    line: usize,
    context: &CodeViewActionContext<Message>,
) -> EventResult<Message> {
    if !is_pointer_activation_event(event) {
        return EventResult::ignored();
    }
    let Event::Mouse(mouse) = event else {
        return EventResult::ignored();
    };
    let next = if mouse.modifiers.shift {
        let anchor = context
            .state
            .selection_anchor()
            .unwrap_or(context.state.cursor);
        CodeViewState::with_selection(line, anchor)
            .with_horizontal_offset(context.state.horizontal_offset)
    } else {
        CodeViewState::new(line).with_horizontal_offset(context.state.horizontal_offset)
    };
    emit_code_view_change(
        EventResult::consumed().focus(context.id.clone()),
        next,
        context,
    )
}

fn emit_code_view_change<Message>(
    result: EventResult<Message>,
    next: CodeViewState,
    context: &CodeViewActionContext<Message>,
) -> EventResult<Message> {
    let next = normalize_state(&context.layout, next);
    if next == context.state {
        result
    } else {
        result.emit((context.on_change)(next))
    }
}

pub(crate) fn state_for_action(
    layout: &CodeLayout,
    state: CodeViewState,
    action: CodeViewAction,
    horizontal_step: u32,
) -> CodeViewState {
    let state = normalize_state(layout, state);
    let lines = layout.document().line_count();
    if lines == 0 {
        return state;
    }
    let last = lines - 1;
    let move_to =
        |target: usize| CodeViewState::new(target).with_horizontal_offset(state.horizontal_offset);
    let extend_to = |target: usize| {
        let anchor = state.selection_anchor().unwrap_or(state.cursor);
        CodeViewState::with_selection(target, anchor)
            .with_horizontal_offset(state.horizontal_offset)
    };
    match action {
        CodeViewAction::Previous => move_to(state.cursor.saturating_sub(1)),
        CodeViewAction::Next => move_to(state.cursor.saturating_add(1).min(last)),
        CodeViewAction::First => move_to(0),
        CodeViewAction::Last => move_to(last),
        CodeViewAction::ExtendPrevious => extend_to(state.cursor.saturating_sub(1)),
        CodeViewAction::ExtendNext => extend_to(state.cursor.saturating_add(1).min(last)),
        CodeViewAction::ExtendFirst => extend_to(0),
        CodeViewAction::ExtendLast => extend_to(last),
        CodeViewAction::HorizontalPrevious => {
            state.with_horizontal_offset(state.horizontal_offset.saturating_sub(horizontal_step))
        }
        CodeViewAction::HorizontalNext => {
            state.with_horizontal_offset(state.horizontal_offset.saturating_add(horizontal_step))
        }
        CodeViewAction::SelectAll => {
            CodeViewState::with_selection(last, 0).with_horizontal_offset(state.horizontal_offset)
        }
        CodeViewAction::CopySelection | CodeViewAction::CopyDocument => state,
    }
}

pub(crate) fn normalize_state(layout: &CodeLayout, state: CodeViewState) -> CodeViewState {
    let lines = layout.document().line_count();
    let last = lines.saturating_sub(1);
    let mut next = if state.has_selection {
        CodeViewState::with_selection(state.cursor.min(last), state.anchor.min(last))
    } else {
        CodeViewState::new(state.cursor.min(last))
    };
    let maximum_offset = if layout.options().wraps() {
        0
    } else {
        layout
            .maximum_row_width()
            .saturating_sub(layout.code_width())
    };
    next.horizontal_offset = state.horizontal_offset.min(maximum_offset);
    next
}

pub(crate) fn visible_row_window(
    layout: &CodeLayout,
    selected_line: usize,
    height: usize,
) -> Range<usize> {
    if height == 0 || height >= layout.visual_row_count() {
        return 0..layout.visual_row_count();
    }
    let selected = layout.rows_for_line(selected_line);
    let selected_row = selected.start;
    let mut start = selected_row.saturating_sub(height / 2);
    let maximum_start = layout.visual_row_count().saturating_sub(height);
    start = start.min(maximum_start);
    start..start.saturating_add(height)
}

fn code_view_row_spans(
    layout: &CodeLayout,
    row: &CodeVisualRow,
    horizontal_offset: u32,
    selected: bool,
    current: bool,
    enabled: bool,
    style: CodeViewStyle,
) -> Vec<TextSpan> {
    let mut output = Vec::new();
    if layout.gutter_width() > 0 {
        let content = if row.continuation {
            format!(
                "{:>width$} ",
                ">",
                width = layout.gutter_width() as usize - 1
            )
        } else {
            format!(
                "{:>width$} | ",
                row.line + 1,
                width = layout.gutter_width() as usize - 3
            )
        };
        let gutter_style = if row.continuation {
            style.line_number.merged(style.continuation)
        } else {
            style.line_number
        };
        output.push(TextSpan::new(
            content,
            row_overlay(gutter_style, selected, current, enabled, style),
        ));
    }
    let visible = slice_spans_by_cells(
        &row.spans,
        &row.checkpoints,
        horizontal_offset,
        layout.code_width(),
        layout.options().width_profile(),
    );
    output.extend(visible.into_iter().map(|span| {
        TextSpan::new(
            span.text().to_owned(),
            row_overlay(span.style(), selected, current, enabled, style),
        )
    }));
    output
}

fn row_overlay(
    base: Style,
    selected: bool,
    current: bool,
    enabled: bool,
    style: CodeViewStyle,
) -> Style {
    let mut result = base;
    if current {
        result = result.merged(style.current);
    }
    if selected {
        result = result.merged(style.selection);
    }
    if !enabled {
        result = result.merged(style.disabled);
    }
    result
}

pub(crate) fn slice_spans_by_cells(
    spans: &[TextSpan],
    checkpoints: &[CodeRowCheckpoint],
    offset: u32,
    width: u32,
    profile: nagi_text::WidthProfile<'_>,
) -> Vec<TextSpan> {
    let end = offset.saturating_add(width);
    let checkpoint = checkpoints
        .partition_point(|checkpoint| checkpoint.cell <= offset)
        .checked_sub(1)
        .and_then(|index| checkpoints.get(index));
    let mut cell = checkpoint.map_or(0, |checkpoint| checkpoint.cell);
    let first_span = checkpoint.map_or(0, |checkpoint| checkpoint.span);
    let first_byte = checkpoint.map_or(0, |checkpoint| checkpoint.byte);
    let mut builder: Vec<(String, Style)> = Vec::new();
    for (span_index, span) in spans.iter().enumerate().skip(first_span) {
        let text = if span_index == first_span {
            &span.text()[first_byte..]
        } else {
            span.text()
        };
        for grapheme in graphemes(text) {
            let grapheme_width =
                u32::try_from(grapheme_width(grapheme.text(), profile)).unwrap_or(u32::MAX);
            let next = cell.saturating_add(grapheme_width);
            let visible = cell >= offset && next <= end && (grapheme_width > 0 || cell < end);
            if visible {
                if let Some((text, existing_style)) = builder.last_mut()
                    && *existing_style == span.style()
                {
                    text.push_str(grapheme.text());
                } else {
                    builder.push((grapheme.text().to_owned(), span.style()));
                }
            }
            cell = next;
            if cell >= end {
                break;
            }
        }
        if cell >= end {
            break;
        }
    }
    builder
        .into_iter()
        .map(|(text, style)| TextSpan::new(text, style))
        .collect()
}

fn code_view_row_id(root: &NodeId, row: usize) -> NodeId {
    NodeId::new(format!("{}:visual-row:{row}", root.as_str()))
}

fn copy_actions_enabled(
    layout: &CodeLayout,
    state: CodeViewState,
    has_copy_handler: bool,
) -> (bool, bool) {
    let document = layout.document();
    (
        has_copy_handler && document.is_range_copyable(state.selected_lines()),
        has_copy_handler && document.is_range_copyable(0..document.line_count()),
    )
}

pub(crate) fn is_blocked_copy_repeat(
    event: &Event,
    selection_copy_enabled: bool,
    document_copy_enabled: bool,
) -> bool {
    let Event::Key(key) = event else {
        return false;
    };
    if key.action != KeyAction::Repeat {
        return false;
    }
    let control = Modifiers {
        control: true,
        ..Modifiers::NONE
    };
    let control_shift = Modifiers {
        control: true,
        shift: true,
        ..Modifiers::NONE
    };
    matches!(key.code, KeyCode::Character('c'))
        && (selection_copy_enabled && key.modifiers == control
            || document_copy_enabled && key.modifiers == control_shift)
}

#[cfg(test)]
mod tests {
    use nagi_tui::{MouseButton, MouseKind};

    use super::*;
    use crate::{CodeDocument, CodeLayoutOptions, CodeLine};

    fn layout(lines: &[&str], wrap: bool, width: u32) -> CodeLayout {
        let document = CodeDocument::new(
            lines.iter().map(|line| CodeLine::plain(*line).unwrap()),
            true,
        )
        .unwrap();
        CodeLayout::new(
            document,
            crate::CodeLayoutOptions::default()
                .with_viewport_width(width)
                .with_line_numbers(false)
                .with_wrap(wrap),
        )
        .unwrap()
    }

    #[test]
    fn state_normalizes_selection_and_horizontal_offset() {
        let no_wrap = layout(&["a", "abcdefgh"], false, 4);
        let state = normalize_state(
            &no_wrap,
            CodeViewState::with_selection(9, 0).with_horizontal_offset(99),
        );
        assert_eq!(state.cursor(), 1);
        assert_eq!(state.selected_lines(), 0..2);
        assert_eq!(state.horizontal_offset(), 4);
        let wrapped = layout(&["abcdefgh"], true, 4);
        assert_eq!(normalize_state(&wrapped, state).horizontal_offset(), 0);
    }

    #[test]
    fn action_navigation_and_extension_are_line_oriented() {
        let layout = layout(&["a", "b", "c"], false, 4);
        let state = state_for_action(
            &layout,
            CodeViewState::new(1),
            CodeViewAction::ExtendNext,
            4,
        );
        assert_eq!(state.cursor(), 2);
        assert_eq!(state.selected_lines(), 1..3);
        let state = state_for_action(&layout, state, CodeViewAction::Previous, 4);
        assert_eq!(state, CodeViewState::new(1));
    }

    #[test]
    fn viewport_is_bounded_and_follows_selected_line() {
        let values = (0..100).map(|value| value.to_string()).collect::<Vec<_>>();
        let references = values.iter().map(String::as_str).collect::<Vec<_>>();
        let layout = layout(&references, false, 4);
        assert_eq!(visible_row_window(&layout, 50, 7), 47..54);
    }

    #[test]
    fn cell_crop_preserves_complete_graphemes_and_styles() {
        let spans = [TextSpan::new("A日B", Style::default())];
        let cropped = slice_spans_by_cells(&spans, &[], 1, 2, nagi_text::WidthProfile::MODERN);
        assert_eq!(cropped.len(), 1);
        assert_eq!(cropped[0].text(), "日");
    }

    #[test]
    fn no_wrap_crop_uses_sparse_checkpoints_near_a_long_line_end() {
        let text = format!("{}END", "a".repeat(1_024));
        let layout = layout(&[&text], false, 4);
        let row = layout.row(0).unwrap();
        assert!(row.checkpoints.len() >= 4);
        let cropped = slice_spans_by_cells(
            &row.spans,
            &row.checkpoints,
            1_024,
            3,
            nagi_text::WidthProfile::MODERN,
        );
        assert_eq!(
            cropped.iter().map(TextSpan::text).collect::<String>(),
            "END"
        );
    }

    #[test]
    fn copy_request_owns_selected_complete_lines() {
        let layout = layout(&["a", "b", "c"], false, 4);
        let context = CodeViewActionContext {
            id: NodeId::new("code"),
            layout,
            state: CodeViewState::with_selection(1, 0),
            horizontal_step: 4,
            on_change: Arc::new(|_| ()),
            on_copy: Some(Arc::new(|_| ())),
        };
        let request =
            code_view_copy_request(CodeCopyKind::Selection, &context).expect("copy request");
        assert_eq!(request.text(), "a\nb");
        assert_eq!(request.lines(), 0..2);
        assert_eq!(request.bytes(), 0..3);
    }

    #[test]
    fn pointer_shift_extends_from_existing_anchor() {
        let layout = layout(&["a", "b", "c"], false, 4);
        let context = CodeViewActionContext {
            id: NodeId::new("code"),
            layout,
            state: CodeViewState::new(1),
            horizontal_step: 4,
            on_change: Arc::new(|state| state),
            on_copy: None,
        };
        let event = Event::Mouse(nagi_tui::MouseEvent {
            kind: MouseKind::Press,
            button: MouseButton::Left,
            x: 0,
            y: 0,
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
        });
        let anchor = context
            .state
            .selection_anchor()
            .unwrap_or(context.state.cursor);
        let next = CodeViewState::with_selection(2, anchor);
        assert_eq!(next.selected_lines(), 1..3);
        assert!(is_pointer_activation_event(&event));
    }

    #[test]
    fn action_descriptors_disable_copy_without_handler() {
        let non_empty_layout = layout(&["a"], false, 4);
        let descriptors =
            code_view_action_descriptors(true, false, &non_empty_layout, CodeViewState::default());
        assert_eq!(
            descriptors[11].availability(),
            ActionAvailability::DisabledPassThrough
        );
        assert_eq!(
            descriptors[12].availability(),
            ActionAvailability::DisabledPassThrough
        );

        let empty_line = layout(&[""], false, 4);
        let descriptors =
            code_view_action_descriptors(true, true, &empty_line, CodeViewState::default());
        assert_eq!(descriptors[12].availability(), ActionAvailability::Enabled);
    }

    #[test]
    fn repeat_copy_is_blocked_only_for_an_enabled_exact_binding() {
        let layout = layout(&["a"], false, 4);
        let (selection, document) = copy_actions_enabled(&layout, CodeViewState::default(), true);
        let event = |modifiers| {
            Event::Key(nagi_tui::KeyEvent {
                code: KeyCode::Character('c'),
                modifiers,
                action: KeyAction::Repeat,
                text: None,
                protocol: nagi_tui::KeyProtocol::Legacy,
            })
        };
        let control = Modifiers {
            control: true,
            ..Modifiers::NONE
        };
        assert!(is_blocked_copy_repeat(&event(control), selection, document));
        assert!(!is_blocked_copy_repeat(
            &event(Modifiers {
                control: true,
                alt: true,
                ..Modifiers::NONE
            }),
            selection,
            document
        ));

        let hidden = CodeLine::styled([TextSpan::new(
            "secret",
            Style {
                hidden: true,
                ..Style::default()
            },
        )])
        .unwrap();
        let source = CodeDocument::new([hidden], false).unwrap();
        let hidden_layout = CodeLayout::new(
            source,
            CodeLayoutOptions::default().with_line_numbers(false),
        )
        .unwrap();
        let (selection, document) =
            copy_actions_enabled(&hidden_layout, CodeViewState::default(), true);
        assert!(!is_blocked_copy_repeat(
            &event(control),
            selection,
            document
        ));
    }

    #[test]
    fn wrap_layout_uses_zero_horizontal_offset() {
        let layout = layout(&["abcdefgh"], true, 4);
        assert!(layout.options().wraps());
        assert_eq!(layout.visual_row_count(), 2);
        assert_eq!(layout.rows_for_line(0), 0..2);
    }

    #[test]
    fn style_slots_are_attribute_only_by_default() {
        let style = CodeViewStyle::default();
        assert!(style.line_number.dim);
        assert!(style.continuation.dim);
        assert!(style.selection.reverse);
        assert!(style.focused.underline);
        assert!(style.disabled.dim);
    }

    #[test]
    fn layout_options_are_available_to_view_tests() {
        let options = CodeLayoutOptions::default();
        assert!(options.shows_line_numbers());
    }

    #[test]
    fn code_view_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/code-view.txt",
            "widget-code-view",
            &[
                "cursor",
                "anchor",
                "offset",
                "wrap",
                "action",
                "step",
                "expected-cursor",
                "expected-anchor",
                "expected-offset",
                "expected-copy",
                "expected-lines",
                "expected-copy-lines",
                "expected-bytes",
            ],
        ) else {
            return;
        };
        for record in records {
            let layout = layout(
                &["a", "abcdefgh", "日", "last"],
                fixture_bool(record.field("wrap")),
                4,
            );
            let cursor = fixture_usize(record.field("cursor"));
            let mut state = if record.field("anchor") == "-" {
                CodeViewState::new(cursor)
            } else {
                CodeViewState::with_selection(cursor, fixture_usize(record.field("anchor")))
            }
            .with_horizontal_offset(fixture_u32(record.field("offset")));
            let copy_kind = match record.field("action") {
                "normalize" => None,
                "previous" => {
                    state = state_for_action(
                        &layout,
                        state,
                        CodeViewAction::Previous,
                        fixture_u32(record.field("step")),
                    );
                    None
                }
                "next" => {
                    state = state_for_action(
                        &layout,
                        state,
                        CodeViewAction::Next,
                        fixture_u32(record.field("step")),
                    );
                    None
                }
                "first" => {
                    state = state_for_action(
                        &layout,
                        state,
                        CodeViewAction::First,
                        fixture_u32(record.field("step")),
                    );
                    None
                }
                "last" => {
                    state = state_for_action(
                        &layout,
                        state,
                        CodeViewAction::Last,
                        fixture_u32(record.field("step")),
                    );
                    None
                }
                "extend-previous" => {
                    state = state_for_action(
                        &layout,
                        state,
                        CodeViewAction::ExtendPrevious,
                        fixture_u32(record.field("step")),
                    );
                    None
                }
                "extend-next" => {
                    state = state_for_action(
                        &layout,
                        state,
                        CodeViewAction::ExtendNext,
                        fixture_u32(record.field("step")),
                    );
                    None
                }
                "extend-first" => {
                    state = state_for_action(
                        &layout,
                        state,
                        CodeViewAction::ExtendFirst,
                        fixture_u32(record.field("step")),
                    );
                    None
                }
                "extend-last" => {
                    state = state_for_action(
                        &layout,
                        state,
                        CodeViewAction::ExtendLast,
                        fixture_u32(record.field("step")),
                    );
                    None
                }
                "horizontal-previous" => {
                    state = state_for_action(
                        &layout,
                        state,
                        CodeViewAction::HorizontalPrevious,
                        fixture_u32(record.field("step")),
                    );
                    None
                }
                "horizontal-next" => {
                    state = state_for_action(
                        &layout,
                        state,
                        CodeViewAction::HorizontalNext,
                        fixture_u32(record.field("step")),
                    );
                    None
                }
                "select-all" => {
                    state = state_for_action(
                        &layout,
                        state,
                        CodeViewAction::SelectAll,
                        fixture_u32(record.field("step")),
                    );
                    None
                }
                "copy-selection" => Some(CodeCopyKind::Selection),
                "copy-document" => Some(CodeCopyKind::Document),
                value => panic!("case {} has unknown action {value}", record.id),
            };
            state = normalize_state(&layout, state);
            assert_eq!(
                state.cursor(),
                fixture_usize(record.field("expected-cursor")),
                "case {} cursor",
                record.id
            );
            let actual_anchor = state
                .selection_anchor()
                .map_or_else(|| "-".to_owned(), |value| value.to_string());
            assert_eq!(
                actual_anchor,
                record.field("expected-anchor"),
                "case {} anchor",
                record.id
            );
            assert_eq!(
                state.horizontal_offset(),
                fixture_u32(record.field("expected-offset")),
                "case {} offset",
                record.id
            );
            let lines = state.selected_lines();
            assert_eq!(
                format!("{}:{}", lines.start, lines.end),
                record.field("expected-lines"),
                "case {} lines",
                record.id
            );
            if let Some(kind) = copy_kind {
                let context = CodeViewActionContext {
                    id: NodeId::new("code"),
                    layout,
                    state,
                    horizontal_step: 4,
                    on_change: Arc::new(|_| ()),
                    on_copy: None,
                };
                let request = code_view_copy_request(kind, &context).expect("fixture copy");
                assert_eq!(
                    request.text(),
                    record.text("expected-copy"),
                    "case {} copy",
                    record.id
                );
                let request_lines = request.lines();
                assert_eq!(
                    format!("{}:{}", request_lines.start, request_lines.end),
                    record.field("expected-copy-lines"),
                    "case {} copy lines",
                    record.id
                );
                let bytes = request.bytes();
                assert_eq!(
                    format!("{}:{}", bytes.start, bytes.end),
                    record.field("expected-bytes"),
                    "case {} bytes",
                    record.id
                );
            } else {
                assert_eq!(
                    record.field("expected-copy"),
                    "-",
                    "case {} unexpected copy",
                    record.id
                );
                assert_eq!(
                    record.field("expected-copy-lines"),
                    "-",
                    "case {} unexpected copy lines",
                    record.id
                );
                assert_eq!(
                    record.field("expected-bytes"),
                    "-",
                    "case {} unexpected bytes",
                    record.id
                );
            }
        }
    }

    fn fixture_bool(value: &str) -> bool {
        match value {
            "true" => true,
            "false" => false,
            _ => panic!("invalid fixture Boolean {value}"),
        }
    }

    fn fixture_u32(value: &str) -> u32 {
        value.parse().unwrap()
    }

    fn fixture_usize(value: &str) -> usize {
        value.parse().unwrap()
    }
}
