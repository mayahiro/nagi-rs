use std::ops::Range;
use std::sync::Arc;

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, Color, Event, EventResult, Length, Node, NodeId,
    ParagraphOptions, Style, TextSpan, WrapMode,
};

use crate::code::CodeVisualRow;
use crate::code_view::{
    CODE_VIEW_ACTION_COUNT, CODE_VIEW_ACTION_DESCRIPTORS, CODE_VIEW_ACTIONS, CodeViewAction,
    CodeViewState, is_blocked_copy_repeat, normalize_state, slice_spans_by_cells, state_for_action,
    visible_row_window,
};
use crate::diff::{DiffDocument, DiffLayout, DiffLineKind};
use crate::event::is_pointer_activation_event;

/// Application-owned logical-line selection and horizontal diff offset
pub type DiffViewState = CodeViewState;

/// Semantic source range copied from a [`DiffView`]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiffCopyKind {
    /// The selected complete logical lines
    Selection,
    /// The complete diff document
    Document,
}

/// Independently owned unified-text copy request emitted by a [`DiffView`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffCopyRequest {
    source: NodeId,
    kind: DiffCopyKind,
    text: String,
    lines: Range<usize>,
    bytes: Range<usize>,
}

impl DiffCopyRequest {
    /// Returns the source DiffView Node ID
    #[must_use]
    pub fn source(&self) -> &NodeId {
        &self.source
    }

    /// Returns whether the selection or complete document was copied
    #[must_use]
    pub const fn kind(&self) -> DiffCopyKind {
        self.kind
    }

    /// Returns independently owned unified UTF-8 text
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the ordered logical-line range in the original document
    #[must_use]
    pub fn lines(&self) -> Range<usize> {
        self.lines.clone()
    }

    /// Returns the conceptual UTF-8 byte range in the original document
    #[must_use]
    pub fn bytes(&self) -> Range<usize> {
        self.bytes.clone()
    }
}

/// Independent semantic style slots used by a [`DiffView`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiffViewStyle {
    /// Base semantic style for metadata content and gutter
    pub metadata: Style,
    /// Base semantic style for hunk content and gutter
    pub hunk: Style,
    /// Base semantic style for context content and gutter
    pub context: Style,
    /// Base semantic style for addition content and gutter
    pub addition: Style,
    /// Base semantic style for deletion content and gutter
    pub deletion: Style,
    /// Style merged over line-kind styles for old and new numbers
    pub line_number: Style,
    /// Style merged over line-kind styles for the marker and following space
    pub marker: Style,
    /// Style merged over the marker in a wrapped continuation row
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

impl Default for DiffViewStyle {
    fn default() -> Self {
        Self {
            metadata: Style {
                dim: true,
                ..Style::default()
            },
            hunk: Style {
                foreground: Color::Indexed(6),
                bold: true,
                ..Style::default()
            },
            context: Style::default(),
            addition: Style {
                foreground: Color::Indexed(2),
                ..Style::default()
            },
            deletion: Style {
                foreground: Color::Indexed(1),
                ..Style::default()
            },
            line_number: Style {
                dim: true,
                ..Style::default()
            },
            marker: Style {
                bold: true,
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

/// Controlled bounded unified terminal view over one immutable [`DiffLayout`]
///
/// Navigation and copy requests are returned to the application. The view
/// does not parse diffs, read repositories, apply patches, or access a
/// clipboard backend
pub struct DiffView<Message> {
    id: NodeId,
    layout: DiffLayout,
    state: DiffViewState,
    viewport_height: usize,
    horizontal_step: u32,
    enabled: bool,
    style: DiffViewStyle,
    on_change: Arc<dyn Fn(DiffViewState) -> Message>,
    on_copy: Option<Arc<dyn Fn(DiffCopyRequest) -> Message>>,
}

impl<Message: 'static> DiffView<Message> {
    /// Creates an enabled view using application-owned controlled state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        layout: DiffLayout,
        state: DiffViewState,
        on_change: impl Fn(DiffViewState) -> Message + 'static,
    ) -> Self {
        let state = normalize_state(layout.code_layout(), state);
        Self {
            id: id.into(),
            layout,
            state,
            viewport_height: 0,
            horizontal_step: 4,
            enabled: true,
            style: DiffViewStyle::default(),
            on_change: Arc::new(on_change),
            on_copy: None,
        }
    }

    /// Returns the visually normalized controlled state
    #[must_use]
    pub const fn state(&self) -> DiffViewState {
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
    pub const fn style(mut self, value: DiffViewStyle) -> Self {
        self.style = value;
        self
    }

    /// Sets the application callback for unified source copy requests
    #[must_use]
    pub fn on_copy(mut self, handler: impl Fn(DiffCopyRequest) -> Message + 'static) -> Self {
        self.on_copy = Some(Arc::new(handler));
        self
    }

    /// Returns the thirteen ordered semantic actions
    ///
    /// The order matches CodeView: four movement, four selection extension,
    /// two horizontal movement, select all, copy selection, and copy document
    #[must_use]
    pub fn action_descriptors(&self) -> Vec<ActionDescriptor> {
        diff_view_action_descriptors(
            self.enabled,
            self.on_copy.is_some(),
            &self.layout,
            self.state,
        )
        .into_iter()
        .collect()
    }

    /// Builds the public semantic Node for this view
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        build_diff_view_node(self)
    }
}

fn diff_view_action_descriptors(
    enabled: bool,
    has_copy_handler: bool,
    layout: &DiffLayout,
    state: DiffViewState,
) -> [ActionDescriptor; CODE_VIEW_ACTION_COUNT] {
    let document = layout.document();
    let lines = document.line_count();
    let state = normalize_state(layout.code_layout(), state);
    std::array::from_fn(|index| {
        let action = CODE_VIEW_ACTIONS[index];
        let available = enabled
            && lines > 0
            && match action {
                CodeViewAction::HorizontalPrevious | CodeViewAction::HorizontalNext => {
                    !layout.options().wraps() && layout.maximum_row_width() > layout.code_width()
                }
                CodeViewAction::CopySelection => {
                    has_copy_handler && document.is_range_copyable(state.selected_lines())
                }
                CodeViewAction::CopyDocument => {
                    has_copy_handler && document.is_range_copyable(0..lines)
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

struct DiffViewActionContext<Message> {
    id: NodeId,
    layout: DiffLayout,
    state: DiffViewState,
    horizontal_step: u32,
    on_change: Arc<dyn Fn(DiffViewState) -> Message>,
    on_copy: Option<Arc<dyn Fn(DiffCopyRequest) -> Message>>,
}

fn build_diff_view_node<Message: 'static>(view: DiffView<Message>) -> Node<Message> {
    let state = normalize_state(view.layout.code_layout(), view.state);
    let descriptors =
        diff_view_action_descriptors(view.enabled, view.on_copy.is_some(), &view.layout, state);
    let context = Arc::new(DiffViewActionContext {
        id: view.id.clone(),
        layout: view.layout.clone(),
        state,
        horizontal_step: view.horizontal_step,
        on_change: Arc::clone(&view.on_change),
        on_copy: view.on_copy.as_ref().map(Arc::clone),
    });
    let rows = visible_row_window(
        view.layout.code_layout(),
        state.cursor(),
        view.viewport_height,
    );
    let selected = state.selected_lines();
    let mut nodes = Vec::with_capacity(rows.len());
    for row_index in rows {
        let record = view
            .layout
            .code_layout()
            .row(row_index)
            .expect("known visual row");
        let line = view
            .layout
            .document()
            .line(record.line)
            .expect("known diff line");
        let mut spans = diff_view_row_spans(
            &view.layout,
            record,
            line.kind(),
            line.old_line(),
            line.new_line(),
            state.horizontal_offset(),
            selected.contains(&record.line),
            record.line == state.cursor(),
            view.enabled,
            view.style,
        );
        if spans.is_empty() {
            spans.push(TextSpan::new("", Style::default()));
        }
        let row_id = diff_view_row_id(&view.id, row_index);
        let mut row = Node::paragraph(
            spans,
            ParagraphOptions {
                wrap: WrapMode::None,
                ..ParagraphOptions::default()
            },
        )
        .with_id(row_id.clone());
        if view.enabled {
            let context = Arc::clone(&context);
            let line = record.line;
            row = row.on_event(row_id, move |event| {
                diff_view_pointer_result(event, line, context.as_ref())
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
                    diff_view_action_result(action, context.as_ref())
                })
            });
    if !view.enabled {
        return root.with_id(id.clone()).on_actions(id, actions);
    }
    root.focusable(id.clone())
        .with_focused_style(view.style.focused)
        .on_actions(id.clone(), actions)
        .on_event(id, move |event| {
            let (selection_copy, document_copy) = diff_copy_actions_enabled(
                context.layout.document(),
                context.state,
                context.on_copy.is_some(),
            );
            if is_blocked_copy_repeat(event, selection_copy, document_copy) {
                EventResult::consumed()
            } else {
                EventResult::ignored()
            }
        })
}

fn diff_view_action_result<Message>(
    action: CodeViewAction,
    context: &DiffViewActionContext<Message>,
) -> EventResult<Message> {
    if matches!(action, CodeViewAction::CopySelection) {
        return diff_view_copy_result(DiffCopyKind::Selection, context);
    }
    if matches!(action, CodeViewAction::CopyDocument) {
        return diff_view_copy_result(DiffCopyKind::Document, context);
    }
    let next = state_for_action(
        context.layout.code_layout(),
        context.state,
        action,
        context.horizontal_step,
    );
    emit_diff_view_change(
        EventResult::consumed().focus(context.id.clone()),
        next,
        context,
    )
}

fn diff_view_copy_result<Message>(
    kind: DiffCopyKind,
    context: &DiffViewActionContext<Message>,
) -> EventResult<Message> {
    let Some(handler) = context.on_copy.as_ref() else {
        return EventResult::ignored();
    };
    let Some(request) = diff_view_copy_request(kind, context) else {
        return EventResult::ignored();
    };
    EventResult::consumed()
        .focus(context.id.clone())
        .emit(handler(request))
}

fn diff_view_copy_request<Message>(
    kind: DiffCopyKind,
    context: &DiffViewActionContext<Message>,
) -> Option<DiffCopyRequest> {
    let document = context.layout.document();
    let lines = match kind {
        DiffCopyKind::Selection => context.state.selected_lines(),
        DiffCopyKind::Document => 0..document.line_count(),
    };
    let bytes = document.byte_range_for_lines(lines.clone())?;
    let text = document.copy_text_for_lines(lines.clone())?;
    Some(DiffCopyRequest {
        source: context.id.clone(),
        kind,
        text,
        lines,
        bytes,
    })
}

fn diff_view_pointer_result<Message>(
    event: &Event,
    line: usize,
    context: &DiffViewActionContext<Message>,
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
            .unwrap_or(context.state.cursor());
        DiffViewState::with_selection(line, anchor)
            .with_horizontal_offset(context.state.horizontal_offset())
    } else {
        DiffViewState::new(line).with_horizontal_offset(context.state.horizontal_offset())
    };
    emit_diff_view_change(
        EventResult::consumed().focus(context.id.clone()),
        next,
        context,
    )
}

fn emit_diff_view_change<Message>(
    result: EventResult<Message>,
    next: DiffViewState,
    context: &DiffViewActionContext<Message>,
) -> EventResult<Message> {
    let next = normalize_state(context.layout.code_layout(), next);
    if next == context.state {
        result
    } else {
        result.emit((context.on_change)(next))
    }
}

#[allow(clippy::too_many_arguments)]
fn diff_view_row_spans(
    layout: &DiffLayout,
    row: &CodeVisualRow,
    kind: DiffLineKind,
    old_line: Option<u64>,
    new_line: Option<u64>,
    horizontal_offset: u32,
    selected: bool,
    current: bool,
    enabled: bool,
    style: DiffViewStyle,
) -> Vec<TextSpan> {
    let mut output = Vec::with_capacity(row.spans.len().saturating_add(2));
    if layout.gutter_width() > 0 {
        if layout.old_number_width() > 0 {
            let old = diff_line_number(old_line, layout.old_number_width(), row.continuation);
            let new = diff_line_number(new_line, layout.new_number_width(), row.continuation);
            output.push(TextSpan::new(
                format!("{old} {new} "),
                diff_row_overlay(
                    diff_kind_overlay(style.line_number, kind, style),
                    selected,
                    current,
                    enabled,
                    style,
                ),
            ));
        }
        let marker = if row.continuation {
            '>'
        } else {
            kind.marker().unwrap_or(' ')
        };
        let marker_style = if row.continuation {
            style.marker.merged(style.continuation)
        } else {
            style.marker
        };
        output.push(TextSpan::new(
            format!("{marker} "),
            diff_row_overlay(
                diff_kind_overlay(marker_style, kind, style),
                selected,
                current,
                enabled,
                style,
            ),
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
            diff_row_overlay(
                diff_kind_overlay(span.style(), kind, style),
                selected,
                current,
                enabled,
                style,
            ),
        )
    }));
    output
}

fn diff_line_number(value: Option<u64>, width: u32, continuation: bool) -> String {
    let width = usize::try_from(width).unwrap_or(usize::MAX);
    if continuation {
        return " ".repeat(width);
    }
    value.map_or_else(|| " ".repeat(width), |value| format!("{value:>width$}"))
}

fn diff_kind_overlay(base: Style, kind: DiffLineKind, style: DiffViewStyle) -> Style {
    let semantic = match kind {
        DiffLineKind::Metadata => style.metadata,
        DiffLineKind::Hunk => style.hunk,
        DiffLineKind::Context => style.context,
        DiffLineKind::Addition => style.addition,
        DiffLineKind::Deletion => style.deletion,
    };
    semantic.merged(base)
}

fn diff_row_overlay(
    base: Style,
    selected: bool,
    current: bool,
    enabled: bool,
    style: DiffViewStyle,
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

fn diff_view_row_id(root: &NodeId, row: usize) -> NodeId {
    NodeId::new(format!("{}:diff-visual-row:{row}", root.as_str()))
}

fn diff_copy_actions_enabled(
    document: &DiffDocument,
    state: DiffViewState,
    has_copy_handler: bool,
) -> (bool, bool) {
    (
        has_copy_handler && document.is_range_copyable(state.selected_lines()),
        has_copy_handler && document.is_range_copyable(0..document.line_count()),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use nagi_tui::{
        Modifiers, MouseButton, MouseKind, TEXT_COPY_DOCUMENT_ACTION_ID,
        TEXT_COPY_SELECTION_ACTION_ID,
    };

    use super::*;
    use crate::{CodeLine, DiffDocument, DiffHunk, DiffLayoutOptions, DiffLine, DiffRange};

    fn layout(wrap: bool) -> DiffLayout {
        let document = DiffDocument::new(
            [
                DiffLine::metadata(CodeLine::plain("header").unwrap()),
                DiffLine::deletion(1, CodeLine::plain("old").unwrap()).unwrap(),
                DiffLine::addition(1, CodeLine::plain("new").unwrap()).unwrap(),
            ],
            true,
        )
        .unwrap();
        DiffLayout::new(
            document,
            DiffLayoutOptions::default()
                .with_viewport_width(10)
                .with_wrap(wrap),
        )
        .unwrap()
    }

    fn sample_layout(wrap: bool) -> DiffLayout {
        let hunk = DiffHunk::new(DiffRange::new(1, 3).unwrap(), DiffRange::new(1, 4).unwrap());
        let document = DiffDocument::new(
            [
                DiffLine::metadata(CodeLine::plain("diff --git a/a.txt b/a.txt").unwrap()),
                DiffLine::hunk(hunk, CodeLine::plain("@@ -1,3 +1,4 @@").unwrap()),
                DiffLine::context(1, 1, CodeLine::plain("same").unwrap()).unwrap(),
                DiffLine::deletion(2, CodeLine::plain("old").unwrap()).unwrap(),
                DiffLine::addition(2, CodeLine::plain("new").unwrap()).unwrap(),
                DiffLine::addition(3, CodeLine::plain("more").unwrap()).unwrap(),
                DiffLine::context(3, 4, CodeLine::plain("last").unwrap()).unwrap(),
            ],
            true,
        )
        .unwrap();
        DiffLayout::new(
            document,
            DiffLayoutOptions::default()
                .with_viewport_width(12)
                .with_wrap(wrap),
        )
        .unwrap()
    }

    #[test]
    fn gutter_formats_old_new_markers_and_continuations() {
        let unwrapped = layout(false);
        let record = unwrapped.code_layout().row(1).unwrap();
        let line = unwrapped.document().line(1).unwrap();
        let spans = diff_view_row_spans(
            &unwrapped,
            record,
            line.kind(),
            line.old_line(),
            line.new_line(),
            0,
            false,
            false,
            true,
            DiffViewStyle::default(),
        );
        assert_eq!(
            spans.iter().map(TextSpan::text).collect::<String>(),
            "1   - old"
        );
        assert_eq!(spans[1].style().foreground, Color::Indexed(1));

        let wrapped = layout(true);
        let continuation = wrapped.code_layout().row(1).expect("metadata continuation");
        assert!(continuation.continuation);
        let line = wrapped.document().line(0).unwrap();
        let spans = diff_view_row_spans(
            &wrapped,
            continuation,
            line.kind(),
            line.old_line(),
            line.new_line(),
            0,
            false,
            false,
            true,
            DiffViewStyle::default(),
        );
        assert!(
            spans
                .iter()
                .map(TextSpan::text)
                .collect::<String>()
                .starts_with("    > ")
        );
    }

    #[test]
    fn descriptors_disable_copy_without_handler() {
        let layout = layout(false);
        let descriptors =
            diff_view_action_descriptors(true, false, &layout, DiffViewState::default());
        assert_eq!(
            descriptors[11].availability(),
            ActionAvailability::DisabledPassThrough
        );
        assert_eq!(
            descriptors[12].availability(),
            ActionAvailability::DisabledPassThrough
        );
        assert_eq!(descriptors[11].id().as_str(), TEXT_COPY_SELECTION_ACTION_ID);
        assert_eq!(descriptors[12].id().as_str(), TEXT_COPY_DOCUMENT_ACTION_ID);
    }

    #[test]
    fn source_style_overrides_kind_color_while_semantic_attributes_remain() {
        let source = Style {
            foreground: Color::Indexed(5),
            italic: true,
            ..Style::default()
        };
        let resolved = diff_kind_overlay(source, DiffLineKind::Addition, DiffViewStyle::default());
        assert_eq!(resolved.foreground, Color::Indexed(5));
        assert!(resolved.italic);
    }

    #[test]
    fn pointer_shift_extends_the_controlled_selection() {
        let received = Arc::new(Mutex::new(DiffViewState::default()));
        let callback_received = Arc::clone(&received);
        let context = DiffViewActionContext {
            id: NodeId::new("diff"),
            layout: layout(false),
            state: DiffViewState::new(1),
            horizontal_step: 4,
            on_change: Arc::new(move |state| {
                *callback_received.lock().expect("callback state") = state;
                state
            }),
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
        let _result = diff_view_pointer_result(&event, 2, &context);
        assert_eq!(
            received.lock().expect("received state").selected_lines(),
            1..3
        );
    }

    #[test]
    fn bounded_window_depends_on_viewport_not_document_size() {
        let layout = sample_layout(false);
        assert_eq!(visible_row_window(layout.code_layout(), 4, 3).len(), 3);
    }

    #[test]
    fn diff_view_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/diff-view.txt",
            "widget-diff-view",
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
                "expected-lines",
                "expected-copy",
                "expected-copy-lines",
                "expected-bytes",
            ],
        ) else {
            return;
        };
        for record in records {
            let layout = sample_layout(fixture_bool(record.field("wrap")));
            let cursor = fixture_usize(record.field("cursor"));
            let mut state = if record.field("anchor") == "-" {
                DiffViewState::new(cursor)
            } else {
                DiffViewState::with_selection(cursor, fixture_usize(record.field("anchor")))
            }
            .with_horizontal_offset(fixture_u32(record.field("offset")));
            let copy_kind = match record.field("action") {
                "normalize" => None,
                "previous" => {
                    state = fixture_action(&layout, state, CodeViewAction::Previous, &record);
                    None
                }
                "next" => {
                    state = fixture_action(&layout, state, CodeViewAction::Next, &record);
                    None
                }
                "first" => {
                    state = fixture_action(&layout, state, CodeViewAction::First, &record);
                    None
                }
                "last" => {
                    state = fixture_action(&layout, state, CodeViewAction::Last, &record);
                    None
                }
                "extend-previous" => {
                    state = fixture_action(&layout, state, CodeViewAction::ExtendPrevious, &record);
                    None
                }
                "extend-next" => {
                    state = fixture_action(&layout, state, CodeViewAction::ExtendNext, &record);
                    None
                }
                "extend-first" => {
                    state = fixture_action(&layout, state, CodeViewAction::ExtendFirst, &record);
                    None
                }
                "extend-last" => {
                    state = fixture_action(&layout, state, CodeViewAction::ExtendLast, &record);
                    None
                }
                "horizontal-previous" => {
                    state =
                        fixture_action(&layout, state, CodeViewAction::HorizontalPrevious, &record);
                    None
                }
                "horizontal-next" => {
                    state = fixture_action(&layout, state, CodeViewAction::HorizontalNext, &record);
                    None
                }
                "select-all" => {
                    state = fixture_action(&layout, state, CodeViewAction::SelectAll, &record);
                    None
                }
                "copy-selection" => Some(DiffCopyKind::Selection),
                "copy-document" => Some(DiffCopyKind::Document),
                value => panic!("case {} has unknown action {value}", record.id),
            };
            state = normalize_state(layout.code_layout(), state);
            assert_eq!(
                state.cursor(),
                fixture_usize(record.field("expected-cursor")),
                "case {} cursor",
                record.id
            );
            let anchor = state
                .selection_anchor()
                .map_or_else(|| "-".to_owned(), |value| value.to_string());
            assert_eq!(
                anchor,
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
                let context = DiffViewActionContext {
                    id: NodeId::new("diff"),
                    layout,
                    state,
                    horizontal_step: 4,
                    on_change: Arc::new(|_| ()),
                    on_copy: None,
                };
                let request = diff_view_copy_request(kind, &context).expect("fixture copy");
                assert_eq!(
                    request.text(),
                    record.text("expected-copy"),
                    "case {} copy",
                    record.id
                );
                let lines = request.lines();
                assert_eq!(
                    format!("{}:{}", lines.start, lines.end),
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
                assert_eq!(record.field("expected-copy"), "-");
                assert_eq!(record.field("expected-copy-lines"), "-");
                assert_eq!(record.field("expected-bytes"), "-");
            }
        }
    }

    fn fixture_action(
        layout: &DiffLayout,
        state: DiffViewState,
        action: CodeViewAction,
        record: &crate::fixture_support::Record,
    ) -> DiffViewState {
        state_for_action(
            layout.code_layout(),
            state,
            action,
            fixture_u32(record.field("step")),
        )
    }

    fn fixture_usize(value: &str) -> usize {
        value.parse().expect("fixture usize")
    }

    fn fixture_u32(value: &str) -> u32 {
        value.parse().expect("fixture u32")
    }

    fn fixture_bool(value: &str) -> bool {
        value.parse().expect("fixture bool")
    }
}
