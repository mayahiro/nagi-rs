use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, LazyLock};

use nagi_text::graphemes;
use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, Event, EventResult, KeyAction, KeyBinding,
    KeyCode, KeyStroke, Length, Modifiers, Node, NodeId, ParagraphOptions, Style, TextSpan,
    WrapMode,
};

use crate::action::{
    ACTIVATE_ACTION_ID, COLLAPSE_ACTION_ID, EXPAND_ACTION_ID, INSPECTOR_COPY_ACTION_ID,
    SELECTION_FIRST_ACTION_ID, SELECTION_LAST_ACTION_ID, SELECTION_NEXT_ACTION_ID,
    SELECTION_PREVIOUS_ACTION_ID, repeatable_action_binding,
};
use crate::event::is_pointer_activation_event;
use crate::json::{JsonDocument, JsonDocumentNode, JsonKind, JsonNodeLabel, JsonPointer};

/// Default maximum decoded grapheme count shown for a String or Number
pub const DEFAULT_JSON_INSPECTOR_MAX_SCALAR_GRAPHEMES: usize = 80;

/// Application-owned selection and expanded branch identities
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonInspectorState {
    selected: JsonPointer,
    expanded: Arc<BTreeSet<JsonPointer>>,
}

impl JsonInspectorState {
    /// Creates state from one selected pointer and ordered unique expanded pointers
    #[must_use]
    pub fn new(selected: JsonPointer, expanded: impl IntoIterator<Item = JsonPointer>) -> Self {
        Self {
            selected,
            expanded: Arc::new(expanded.into_iter().collect()),
        }
    }

    /// Returns the selected complete JSON Pointer
    #[must_use]
    pub const fn selected(&self) -> &JsonPointer {
        &self.selected
    }

    /// Iterates expanded pointers in deterministic lexical order
    #[must_use]
    pub fn expanded(&self) -> impl ExactSizeIterator<Item = &JsonPointer> {
        self.expanded.iter()
    }

    /// Reports whether one pointer is expanded
    #[must_use]
    pub fn is_expanded(&self, pointer: &JsonPointer) -> bool {
        self.expanded.contains(pointer)
    }

    /// Returns state with a replacement selected pointer
    #[must_use]
    pub fn with_selected(mut self, selected: JsonPointer) -> Self {
        self.selected = selected;
        self
    }

    /// Returns state with one pointer inserted into or removed from expansion
    #[must_use]
    pub fn with_expanded(mut self, pointer: JsonPointer, expanded: bool) -> Self {
        let values = Arc::make_mut(&mut self.expanded);
        if expanded {
            values.insert(pointer);
        } else {
            values.remove(&pointer);
        }
        self
    }
}

impl Default for JsonInspectorState {
    fn default() -> Self {
        let root = JsonPointer::root();
        Self::new(root.clone(), [root])
    }
}

/// Application-handled request that independently owns one complete JSON value
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonInspectorCopyRequest {
    source: NodeId,
    path: JsonPointer,
    kind: JsonKind,
    text: String,
}

impl JsonInspectorCopyRequest {
    /// Returns the stable inspector root Node ID
    #[must_use]
    pub const fn source(&self) -> &NodeId {
        &self.source
    }

    /// Returns the selected complete JSON Pointer
    #[must_use]
    pub const fn path(&self) -> &JsonPointer {
        &self.path
    }

    /// Returns the selected JSON value category
    #[must_use]
    pub const fn kind(&self) -> JsonKind {
        self.kind
    }

    /// Returns the complete deterministic compact serialization
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Independent semantic style slots used by a [`JsonInspector`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JsonInspectorStyle {
    /// Style for object key contents
    pub key: Style,
    /// Style for decoded String previews
    pub string: Style,
    /// Style for Number previews
    pub number: Style,
    /// Style for Boolean values
    pub boolean: Style,
    /// Style for null values
    pub null: Style,
    /// Style for delimiters, indentation, and disclosure markers
    pub punctuation: Style,
    /// Style for array indexes
    pub index: Style,
    /// Style for branch child counts
    pub summary: Style,
    /// Style merged over every span in the selected row
    pub selected: Style,
    /// Style merged over the selected row while the inspector owns focus
    pub focused: Style,
    /// Style merged over every span while the inspector is disabled
    pub disabled: Style,
}

impl Default for JsonInspectorStyle {
    fn default() -> Self {
        Self {
            key: Style {
                bold: true,
                ..Style::default()
            },
            string: Style::default(),
            number: Style::default(),
            boolean: Style {
                bold: true,
                ..Style::default()
            },
            null: Style {
                dim: true,
                ..Style::default()
            },
            punctuation: Style {
                dim: true,
                ..Style::default()
            },
            index: Style {
                dim: true,
                ..Style::default()
            },
            summary: Style {
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
        }
    }
}

/// Controlled, bounded hierarchy view over one immutable JSON document
///
/// Selection and expansion updates are returned through the application
/// callback. Copy requests contain complete source values and never access an
/// operating-system or terminal clipboard directly
pub struct JsonInspector<Message> {
    id: NodeId,
    document: JsonDocument,
    state: JsonInspectorState,
    viewport_height: usize,
    maximum_scalar_graphemes: usize,
    enabled: bool,
    style: JsonInspectorStyle,
    on_change: Arc<dyn Fn(JsonInspectorState) -> Message>,
    on_copy: Option<Arc<dyn Fn(JsonInspectorCopyRequest) -> Message>>,
}

impl<Message: 'static> JsonInspector<Message> {
    /// Creates an enabled inspector using application-owned controlled state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        document: JsonDocument,
        state: JsonInspectorState,
        on_change: impl Fn(JsonInspectorState) -> Message + 'static,
    ) -> Self {
        let state = normalize_state(&document, &state);
        Self {
            id: id.into(),
            document,
            state,
            viewport_height: 0,
            maximum_scalar_graphemes: DEFAULT_JSON_INSPECTOR_MAX_SCALAR_GRAPHEMES,
            enabled: true,
            style: JsonInspectorStyle::default(),
            on_change: Arc::new(on_change),
            on_copy: None,
        }
    }

    /// Returns the visually normalized controlled state
    #[must_use]
    pub const fn state(&self) -> &JsonInspectorState {
        &self.state
    }

    /// Sets whether the inspector can receive focus and emit messages
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Limits constructed rows to a deterministic window following selection
    ///
    /// Zero constructs every visible row
    #[must_use]
    pub const fn viewport(mut self, height: usize) -> Self {
        self.viewport_height = height;
        self
    }

    /// Sets the positive decoded grapheme limit for String and Number previews
    ///
    /// Zero restores [`DEFAULT_JSON_INSPECTOR_MAX_SCALAR_GRAPHEMES`]
    #[must_use]
    pub const fn maximum_scalar_graphemes(mut self, maximum: usize) -> Self {
        self.maximum_scalar_graphemes = if maximum == 0 {
            DEFAULT_JSON_INSPECTOR_MAX_SCALAR_GRAPHEMES
        } else {
            maximum
        };
        self
    }

    /// Replaces every semantic style slot
    #[must_use]
    pub const fn style(mut self, style: JsonInspectorStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the application callback for complete selected-value copy requests
    #[must_use]
    pub fn on_copy(
        mut self,
        handler: impl Fn(JsonInspectorCopyRequest) -> Message + 'static,
    ) -> Self {
        self.on_copy = Some(Arc::new(handler));
        self
    }

    /// Returns the eight ordered semantic action descriptors
    ///
    /// The order is activate, previous, next, first, last, collapse, expand,
    /// and copy
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; 8] {
        json_inspector_action_descriptors(self.enabled, self.on_copy.is_some())
    }

    /// Builds the public semantic node for this inspector
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        build_inspector_node(self)
    }
}

static JSON_INSPECTOR_ACTION_DESCRIPTORS: LazyLock<[ActionDescriptor; 8]> = LazyLock::new(|| {
    let control = Modifiers {
        control: true,
        ..Modifiers::NONE
    };
    [
        ActionDescriptor::new(
            ACTIVATE_ACTION_ID,
            "Activate",
            [
                KeyBinding::new(KeyStroke::new(KeyCode::Enter, Modifiers::NONE)),
                KeyBinding::new(KeyStroke::character(' ', Modifiers::NONE)),
            ],
        ),
        ActionDescriptor::new(
            SELECTION_PREVIOUS_ACTION_ID,
            "Previous",
            [repeatable_action_binding(KeyCode::Up)],
        ),
        ActionDescriptor::new(
            SELECTION_NEXT_ACTION_ID,
            "Next",
            [repeatable_action_binding(KeyCode::Down)],
        ),
        ActionDescriptor::new(
            SELECTION_FIRST_ACTION_ID,
            "First",
            [repeatable_action_binding(KeyCode::Home)],
        ),
        ActionDescriptor::new(
            SELECTION_LAST_ACTION_ID,
            "Last",
            [repeatable_action_binding(KeyCode::End)],
        ),
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
        ActionDescriptor::new(
            INSPECTOR_COPY_ACTION_ID,
            "Copy value",
            [KeyBinding::new(KeyStroke::character('c', control))],
        ),
    ]
});

#[derive(Clone, Copy)]
enum JsonInspectorAction {
    Activate,
    Previous,
    Next,
    First,
    Last,
    Collapse,
    Expand,
    Copy,
}

const JSON_INSPECTOR_ACTIONS: [JsonInspectorAction; 8] = [
    JsonInspectorAction::Activate,
    JsonInspectorAction::Previous,
    JsonInspectorAction::Next,
    JsonInspectorAction::First,
    JsonInspectorAction::Last,
    JsonInspectorAction::Collapse,
    JsonInspectorAction::Expand,
    JsonInspectorAction::Copy,
];

fn json_inspector_action_descriptors(
    enabled: bool,
    has_copy_handler: bool,
) -> [ActionDescriptor; 8] {
    std::array::from_fn(|index| {
        let available = enabled
            && (has_copy_handler
                || !matches!(JSON_INSPECTOR_ACTIONS[index], JsonInspectorAction::Copy));
        JSON_INSPECTOR_ACTION_DESCRIPTORS[index]
            .clone()
            .with_availability(if available {
                ActionAvailability::Enabled
            } else {
                ActionAvailability::DisabledPassThrough
            })
    })
}

struct JsonInspectorActionContext<Message> {
    id: NodeId,
    document: JsonDocument,
    state: JsonInspectorState,
    selected: usize,
    on_change: Arc<dyn Fn(JsonInspectorState) -> Message>,
    on_copy: Option<Arc<dyn Fn(JsonInspectorCopyRequest) -> Message>>,
}

fn build_inspector_node<Message: 'static>(inspector: JsonInspector<Message>) -> Node<Message> {
    let selected = inspector
        .document
        .index_of(inspector.state.selected())
        .unwrap_or(0);
    let indices = visible_window(
        &inspector.document,
        &inspector.state,
        selected,
        inspector.viewport_height,
    );
    let descriptors =
        json_inspector_action_descriptors(inspector.enabled, inspector.on_copy.is_some());
    let context = Arc::new(JsonInspectorActionContext {
        id: inspector.id.clone(),
        document: inspector.document.clone(),
        state: inspector.state.clone(),
        selected,
        on_change: Arc::clone(&inspector.on_change),
        on_copy: inspector.on_copy.as_ref().map(Arc::clone),
    });
    let mut rows = Vec::with_capacity(indices.len());
    for index in indices {
        let is_selected = index == selected;
        let record = inspector.document.record(index);
        let expanded = is_expanded_branch(record, &inspector.state);
        let spans = json_row_spans(
            record,
            inspector.document.node_at(index).serialized(),
            expanded,
            inspector.maximum_scalar_graphemes,
            inspector.style,
            inspector.enabled,
            is_selected,
        );
        let row_id = json_inspector_row_id(&inspector.id, &record.path);
        let mut row = Node::paragraph(
            spans,
            ParagraphOptions {
                wrap: WrapMode::None,
                ..ParagraphOptions::default()
            },
        )
        .with_id(row_id.clone());
        if inspector.enabled {
            let pointer_context = Arc::clone(&context);
            row = row.on_event(row_id, move |event| {
                json_inspector_pointer_result(event, index, pointer_context.as_ref())
            });
        }
        if is_selected && inspector.enabled {
            rows.push(json_inspector_action_target(
                Node::column([row]),
                Arc::clone(&context),
                descriptors.clone(),
                inspector.style.focused,
            ));
        } else {
            rows.push(row);
        }
    }

    let mut root = Node::column(rows);
    if inspector.viewport_height > 0 {
        root = root.with_length(Length::Fixed(
            u32::try_from(inspector.viewport_height).unwrap_or(u32::MAX),
        ));
    }
    if inspector.enabled {
        root
    } else {
        let id = inspector.id;
        root.with_id(id.clone()).on_actions(
            id,
            descriptors
                .into_iter()
                .map(|descriptor| Action::new(descriptor, |_| EventResult::ignored())),
        )
    }
}

fn json_inspector_action_target<Message: 'static>(
    node: Node<Message>,
    context: Arc<JsonInspectorActionContext<Message>>,
    descriptors: [ActionDescriptor; 8],
    focused_style: Style,
) -> Node<Message> {
    let id = context.id.clone();
    let actions_context = Arc::clone(&context);
    let actions =
        descriptors
            .into_iter()
            .zip(JSON_INSPECTOR_ACTIONS)
            .map(move |(descriptor, action)| {
                let context = Arc::clone(&actions_context);
                Action::new(descriptor, move |_| {
                    json_inspector_action_result(action, context.as_ref())
                })
            });
    node.focusable(id.clone())
        .with_focused_style(focused_style)
        .on_actions(id.clone(), actions)
        .on_event(id, move |event| {
            if is_blocked_initial_only_repeat(event, context.on_copy.is_some()) {
                EventResult::consumed()
            } else {
                EventResult::ignored()
            }
        })
}

fn json_inspector_action_result<Message>(
    action: JsonInspectorAction,
    context: &JsonInspectorActionContext<Message>,
) -> EventResult<Message> {
    if matches!(action, JsonInspectorAction::Copy) {
        return json_inspector_copy_result(context);
    }
    let next = state_for_action(&context.document, &context.state, context.selected, action);
    emit_json_inspector_change(
        EventResult::consumed().focus(context.id.clone()),
        next,
        context,
    )
}

fn json_inspector_copy_result<Message>(
    context: &JsonInspectorActionContext<Message>,
) -> EventResult<Message> {
    let Some(handler) = context.on_copy.as_ref() else {
        return EventResult::ignored();
    };
    let node = context.document.node_at(context.selected);
    let request = JsonInspectorCopyRequest {
        source: context.id.clone(),
        path: node.path().clone(),
        kind: node.kind(),
        text: node.serialized().to_owned(),
    };
    EventResult::consumed()
        .focus(context.id.clone())
        .emit(handler(request))
}

fn json_inspector_pointer_result<Message>(
    event: &Event,
    index: usize,
    context: &JsonInspectorActionContext<Message>,
) -> EventResult<Message> {
    if !is_pointer_activation_event(event) {
        return EventResult::ignored();
    }
    let next = state_for_pointer(&context.document, &context.state, index);
    emit_json_inspector_change(
        EventResult::consumed().focus(context.id.clone()),
        next,
        context,
    )
}

fn state_for_pointer(
    document: &JsonDocument,
    state: &JsonInspectorState,
    index: usize,
) -> JsonInspectorState {
    let record = document.record(index);
    let mut next = state.clone().with_selected(record.path.clone());
    if is_branch(record) {
        next = next.with_expanded(record.path.clone(), !is_expanded_branch(record, state));
    }
    next
}

fn emit_json_inspector_change<Message>(
    result: EventResult<Message>,
    next: JsonInspectorState,
    context: &JsonInspectorActionContext<Message>,
) -> EventResult<Message> {
    if next == context.state {
        result
    } else {
        result.emit((context.on_change)(next))
    }
}

fn is_blocked_initial_only_repeat(event: &Event, has_copy_handler: bool) -> bool {
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
    matches!(key.code, KeyCode::Enter) && key.modifiers == Modifiers::NONE
        || matches!(key.code, KeyCode::Character(' ')) && key.modifiers == Modifiers::NONE
        || has_copy_handler
            && matches!(key.code, KeyCode::Character('c'))
            && key.modifiers == control
}

fn state_for_action(
    document: &JsonDocument,
    state: &JsonInspectorState,
    selected: usize,
    action: JsonInspectorAction,
) -> JsonInspectorState {
    let record = document.record(selected);
    match action {
        JsonInspectorAction::Activate if is_branch(record) => state
            .clone()
            .with_expanded(record.path.clone(), !is_expanded_branch(record, state)),
        JsonInspectorAction::Previous => previous_visible(document, state, selected).map_or_else(
            || state.clone(),
            |index| select_index(document, state, index),
        ),
        JsonInspectorAction::Next => next_visible(document, state, selected).map_or_else(
            || state.clone(),
            |index| select_index(document, state, index),
        ),
        JsonInspectorAction::First => select_index(document, state, 0),
        JsonInspectorAction::Last => select_index(document, state, last_visible(document, state)),
        JsonInspectorAction::Collapse if is_expanded_branch(record, state) => {
            state.clone().with_expanded(record.path.clone(), false)
        }
        JsonInspectorAction::Collapse => record.parent.map_or_else(
            || state.clone(),
            |index| select_index(document, state, index),
        ),
        JsonInspectorAction::Expand if is_branch(record) && !is_expanded_branch(record, state) => {
            state.clone().with_expanded(record.path.clone(), true)
        }
        JsonInspectorAction::Expand if is_expanded_branch(record, state) => {
            select_index(document, state, selected.saturating_add(1))
        }
        JsonInspectorAction::Activate | JsonInspectorAction::Expand | JsonInspectorAction::Copy => {
            state.clone()
        }
    }
}

fn select_index(
    document: &JsonDocument,
    state: &JsonInspectorState,
    index: usize,
) -> JsonInspectorState {
    state
        .clone()
        .with_selected(document.record(index).path.clone())
}

fn normalize_state(document: &JsonDocument, state: &JsonInspectorState) -> JsonInspectorState {
    let selected = document.index_of(state.selected()).unwrap_or(0);
    let selected = normalize_visible_index(document, state, selected);
    select_index(document, state, selected)
}

fn normalize_visible_index(
    document: &JsonDocument,
    state: &JsonInspectorState,
    index: usize,
) -> usize {
    let mut normalized = index;
    let mut parent = document.record(index).parent;
    while let Some(index) = parent {
        let record = document.record(index);
        if is_branch(record) && !is_expanded_branch(record, state) {
            normalized = index;
        }
        parent = record.parent;
    }
    normalized
}

fn is_branch(record: &JsonDocumentNode) -> bool {
    record.child_count > 0 && matches!(record.kind, JsonKind::Array | JsonKind::Object)
}

fn is_expanded_branch(record: &JsonDocumentNode, state: &JsonInspectorState) -> bool {
    is_branch(record) && state.is_expanded(&record.path)
}

fn next_visible(
    document: &JsonDocument,
    state: &JsonInspectorState,
    index: usize,
) -> Option<usize> {
    let record = document.record(index);
    let next = if is_expanded_branch(record, state) {
        index.saturating_add(1)
    } else {
        record.subtree_end
    };
    (next < document.len()).then_some(next)
}

fn previous_visible(
    document: &JsonDocument,
    state: &JsonInspectorState,
    index: usize,
) -> Option<usize> {
    index
        .checked_sub(1)
        .map(|previous| normalize_visible_index(document, state, previous))
}

fn last_visible(document: &JsonDocument, state: &JsonInspectorState) -> usize {
    let mut index = 0;
    while let Some(next) = next_visible(document, state, index) {
        index = next;
    }
    index
}

fn visible_window(
    document: &JsonDocument,
    state: &JsonInspectorState,
    selected: usize,
    height: usize,
) -> Vec<usize> {
    if height == 0 {
        let mut visible = Vec::new();
        let mut current = Some(0);
        while let Some(index) = current {
            visible.push(index);
            current = next_visible(document, state, index);
        }
        return visible;
    }

    let mut start = selected;
    for _ in 0..height / 2 {
        let Some(previous) = previous_visible(document, state, start) else {
            break;
        };
        start = previous;
    }
    let mut visible = VecDeque::with_capacity(height);
    let mut current = Some(start);
    while visible.len() < height {
        let Some(index) = current else {
            break;
        };
        visible.push_back(index);
        current = next_visible(document, state, index);
    }
    while visible.len() < height {
        let Some(first) = visible.front().copied() else {
            break;
        };
        let Some(previous) = previous_visible(document, state, first) else {
            break;
        };
        visible.push_front(previous);
    }
    visible.into_iter().collect()
}

fn json_inspector_row_id(root: &NodeId, path: &JsonPointer) -> NodeId {
    NodeId::new(format!(
        "nagi.json-inspector.row:{}:{}:{}:{}",
        root.as_str().len(),
        root.as_str(),
        path.as_str().len(),
        path.as_str()
    ))
}

fn json_row_spans(
    record: &JsonDocumentNode,
    serialized_value: &str,
    expanded: bool,
    maximum_scalar_graphemes: usize,
    style: JsonInspectorStyle,
    enabled: bool,
    selected: bool,
) -> Vec<TextSpan> {
    let overlay = if !enabled {
        style.disabled
    } else if selected {
        style.selected
    } else {
        Style::default()
    };
    let mut spans = Vec::with_capacity(12);
    push_span(
        &mut spans,
        "  ".repeat(usize::try_from(record.depth).unwrap_or(usize::MAX)),
        style.punctuation,
        overlay,
    );
    let disclosure = if is_branch(record) {
        if expanded { "▼ " } else { "▶ " }
    } else {
        "  "
    };
    push_span(
        &mut spans,
        disclosure.to_owned(),
        style.punctuation,
        overlay,
    );
    match &record.label {
        JsonNodeLabel::Root => {}
        JsonNodeLabel::ObjectKey(key) => {
            push_span(&mut spans, "\"".to_owned(), style.punctuation, overlay);
            push_span(
                &mut spans,
                escape_json_string_content(key),
                style.key,
                overlay,
            );
            push_span(&mut spans, "\": ".to_owned(), style.punctuation, overlay);
        }
        JsonNodeLabel::ArrayIndex(index) => {
            push_span(&mut spans, "[".to_owned(), style.punctuation, overlay);
            push_span(&mut spans, index.to_string(), style.index, overlay);
            push_span(&mut spans, "]: ".to_owned(), style.punctuation, overlay);
        }
    }
    push_json_value_spans(
        &mut spans,
        record,
        serialized_value,
        maximum_scalar_graphemes,
        style,
        overlay,
    );
    spans
}

fn push_json_value_spans(
    spans: &mut Vec<TextSpan>,
    record: &JsonDocumentNode,
    serialized_value: &str,
    maximum_scalar_graphemes: usize,
    style: JsonInspectorStyle,
    overlay: Style,
) {
    match record.kind {
        JsonKind::Null => {
            push_span(spans, "null".to_owned(), style.null, overlay);
        }
        JsonKind::Boolean => {
            push_span(spans, serialized_value.to_owned(), style.boolean, overlay);
        }
        JsonKind::Number => {
            let (preview, truncated) = scalar_preview(serialized_value, maximum_scalar_graphemes);
            push_span(spans, preview.to_owned(), style.number, overlay);
            if truncated {
                push_span(spans, "…".to_owned(), style.number, overlay);
            }
        }
        JsonKind::String => {
            let value = record
                .decoded_string
                .as_deref()
                .expect("String document node has decoded content");
            let (preview, truncated) = scalar_preview(value, maximum_scalar_graphemes);
            push_span(spans, "\"".to_owned(), style.punctuation, overlay);
            push_span(
                spans,
                escape_json_string_content(preview),
                style.string,
                overlay,
            );
            if truncated {
                push_span(spans, "…".to_owned(), style.string, overlay);
            }
            push_span(spans, "\"".to_owned(), style.punctuation, overlay);
        }
        JsonKind::Array => {
            push_branch_summary(spans, '[', ']', record.child_count, style, overlay);
        }
        JsonKind::Object => {
            push_branch_summary(spans, '{', '}', record.child_count, style, overlay);
        }
    }
}

fn push_branch_summary(
    spans: &mut Vec<TextSpan>,
    opening: char,
    closing: char,
    child_count: usize,
    style: JsonInspectorStyle,
    overlay: Style,
) {
    push_span(spans, opening.to_string(), style.punctuation, overlay);
    if child_count > 0 {
        push_span(spans, child_count.to_string(), style.summary, overlay);
    }
    push_span(spans, closing.to_string(), style.punctuation, overlay);
}

fn push_span(spans: &mut Vec<TextSpan>, text: String, style: Style, overlay: Style) {
    if !text.is_empty() {
        spans.push(TextSpan::new(text, style.merged(overlay)));
    }
}

fn scalar_preview(value: &str, maximum: usize) -> (&str, bool) {
    let maximum = maximum.max(1);
    let mut clusters = graphemes(value);
    let mut end = 0;
    for _ in 0..maximum {
        let Some(cluster) = clusters.next() else {
            return (value, false);
        };
        end = cluster.end();
    }
    if clusters.next().is_some() {
        (&value[..end], true)
    } else {
        (value, false)
    }
}

fn escape_json_string_content(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{0008}' => output.push_str("\\b"),
            '\u{000C}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{0000}'..='\u{001F}' => {
                let value = u32::from(character) as usize;
                output.push_str("\\u00");
                output.push(char::from(HEX[(value >> 4) & 0x0F]));
                output.push(char::from(HEX[value & 0x0F]));
            }
            _ => output.push(character),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use nagi_tui::{Color, KeyEvent, KeyProtocol};

    use super::*;
    use crate::json::{JsonMember, JsonNumber, JsonValue};

    #[test]
    fn interactions_match_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/json-inspector.txt",
            "widget-json-inspector",
            &[
                "selected",
                "expanded",
                "event",
                "max-graphemes",
                "expected-selected",
                "expected-expanded",
                "expected-message",
                "expected-copy",
                "consumed",
            ],
        ) else {
            return;
        };
        let document = inspector_document();
        for record in records {
            let initial = inspector_state(record.field("selected"), record.field("expanded"));
            let initial = normalize_state(&document, &initial);
            let event = record.field("event");
            let mut next = initial.clone();
            let mut message = String::from("-");
            let mut copy = String::from("-");
            let consumed = match event {
                "-" => None,
                "repeat-enter" => {
                    assert!(is_blocked_initial_only_repeat(
                        &repeat_key(KeyCode::Enter, Modifiers::NONE),
                        true,
                    ));
                    Some(true)
                }
                "repeat-copy" => {
                    assert!(is_blocked_initial_only_repeat(
                        &repeat_key(
                            KeyCode::Character('c'),
                            Modifiers {
                                control: true,
                                ..Modifiers::NONE
                            }
                        ),
                        true,
                    ));
                    Some(true)
                }
                "copy" => {
                    let selected = document.index_of(initial.selected()).unwrap();
                    let node = document.node_at(selected);
                    message = format!("copy:{}", pointer_text(node.path()));
                    copy = node.serialized().to_owned();
                    Some(true)
                }
                "pointer-nested" => {
                    let nested = document.index_of(&json_pointer("/nested")).unwrap();
                    next = state_for_pointer(&document, &initial, nested);
                    if next != initial {
                        message = state_message(&next);
                    }
                    Some(true)
                }
                value => {
                    let action = fixture_action(value);
                    let selected = document.index_of(initial.selected()).unwrap();
                    next = state_for_action(&document, &initial, selected, action);
                    if next != initial {
                        message = state_message(&next);
                    }
                    Some(true)
                }
            };

            assert_eq!(
                pointer_text(next.selected()),
                record.field("expected-selected"),
                "case {} selected",
                record.id
            );
            assert_eq!(
                expanded_text(&next),
                record.field("expected-expanded"),
                "case {} expanded",
                record.id
            );
            assert_eq!(
                message,
                record.field("expected-message"),
                "case {}",
                record.id
            );
            assert_eq!(copy, record.text("expected-copy"), "case {}", record.id);
            assert_eq!(
                consumed.map_or("-", |value| if value { "true" } else { "false" }),
                record.field("consumed"),
                "case {} consumed",
                record.id
            );
            let maximum: usize = record.field("max-graphemes").parse().unwrap();
            assert_eq!(scalar_preview("abcdef日ghi", maximum), ("abcde", true));
        }
    }

    #[test]
    fn bounded_viewport_constructs_only_the_requested_window() {
        let values =
            (0..100).map(|value| JsonValue::number(JsonNumber::new(value.to_string()).unwrap()));
        let document = JsonDocument::new(JsonValue::array(values)).unwrap();
        let selected = json_pointer("/50");
        let state = JsonInspectorState::new(selected.clone(), [JsonPointer::root()]);
        let selected = document.index_of(&selected).unwrap();

        let window = visible_window(&document, &state, selected, 7);

        assert_eq!(window.len(), 7);
        assert!(window.contains(&selected));
        assert_eq!(document.node_at(window[0]).path().as_str(), "/47");
        assert_eq!(document.node_at(window[6]).path().as_str(), "/53");
    }

    #[test]
    fn row_preview_is_grapheme_safe_and_complete_copy_stays_unchanged() {
        let document = inspector_document();
        let node = document.get(&json_pointer("/long")).unwrap();
        let record = document.record(document.index_of(node.path()).unwrap());
        let spans = json_row_spans(
            record,
            node.serialized(),
            false,
            7,
            JsonInspectorStyle::default(),
            true,
            false,
        );
        let display = spans.iter().map(TextSpan::text).collect::<String>();

        assert!(display.contains("\"abcdef日…\""));
        assert_eq!(node.serialized(), "\"abcdef日ghi\"");
    }

    #[test]
    fn semantic_styles_remain_independent_and_overlays_apply_to_every_span() {
        let document = style_document();
        let style = JsonInspectorStyle {
            key: foreground_style(1),
            string: foreground_style(2),
            number: foreground_style(3),
            boolean: foreground_style(4),
            null: foreground_style(5),
            punctuation: foreground_style(6),
            index: foreground_style(7),
            summary: foreground_style(8),
            selected: Style {
                background: Color::Indexed(9),
                ..Style::default()
            },
            focused: Style::default(),
            disabled: Style {
                background: Color::Indexed(10),
                ..Style::default()
            },
        };

        let root = document.record(0);
        let root_spans = json_row_spans(
            root,
            document.root().serialized(),
            true,
            80,
            style,
            true,
            false,
        );
        assert_span_foreground(&root_spans, "▼ ", 6);
        assert_span_foreground(&root_spans, "5", 8);

        assert_value_styles(&document, &style);

        let text_index = document.index_of(&json_pointer("/text")).unwrap();
        let text_record = document.record(text_index);
        let selected = json_row_spans(
            text_record,
            document.node_at(text_index).serialized(),
            false,
            80,
            style,
            true,
            true,
        );
        assert!(
            selected
                .iter()
                .all(|span| span.style().background == Color::Indexed(9))
        );
        let disabled = json_row_spans(
            text_record,
            document.node_at(text_index).serialized(),
            false,
            80,
            style,
            false,
            true,
        );
        assert!(
            disabled
                .iter()
                .all(|span| span.style().background == Color::Indexed(10))
        );
    }

    #[test]
    fn state_clones_share_expansion_until_changed() {
        let state = JsonInspectorState::default();
        let clone = state.clone();
        assert!(Arc::ptr_eq(&state.expanded, &clone.expanded));

        let changed = clone.with_expanded(json_pointer("/nested"), true);
        assert!(!Arc::ptr_eq(&state.expanded, &changed.expanded));
        assert!(changed.is_expanded(&json_pointer("/nested")));
    }

    #[test]
    fn copy_repeat_passes_through_without_a_copy_handler() {
        let event = repeat_key(
            KeyCode::Character('c'),
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
        );
        assert!(!is_blocked_initial_only_repeat(&event, false));
    }

    fn inspector_document() -> JsonDocument {
        JsonDocument::new(
            JsonValue::object([
                JsonMember::new("short", JsonValue::string("ok")),
                JsonMember::new("long", JsonValue::string("abcdef日ghi")),
                JsonMember::new(
                    "nested",
                    JsonValue::object([
                        JsonMember::new("flag", JsonValue::boolean(true)),
                        JsonMember::new(
                            "items",
                            JsonValue::array([
                                JsonValue::number(JsonNumber::new("1").unwrap()),
                                JsonValue::number(JsonNumber::new("2").unwrap()),
                            ]),
                        ),
                    ])
                    .unwrap(),
                ),
                JsonMember::new("empty", JsonValue::array([])),
            ])
            .unwrap(),
        )
        .unwrap()
    }

    fn style_document() -> JsonDocument {
        JsonDocument::new(
            JsonValue::object([
                JsonMember::new("text", JsonValue::string("x")),
                JsonMember::new("number", JsonValue::number(JsonNumber::new("1").unwrap())),
                JsonMember::new("boolean", JsonValue::boolean(true)),
                JsonMember::new("null", JsonValue::null()),
                JsonMember::new(
                    "array",
                    JsonValue::array([JsonValue::number(JsonNumber::new("2").unwrap())]),
                ),
            ])
            .unwrap(),
        )
        .unwrap()
    }

    fn foreground_style(index: u8) -> Style {
        Style {
            foreground: Color::Indexed(index),
            ..Style::default()
        }
    }

    fn assert_value_styles(document: &JsonDocument, style: &JsonInspectorStyle) {
        for (path, text, color) in [
            ("/text", "x", 2),
            ("/number", "1", 3),
            ("/boolean", "true", 4),
            ("/null", "null", 5),
        ] {
            let index = document.index_of(&json_pointer(path)).unwrap();
            let record = document.record(index);
            let spans = json_row_spans(
                record,
                document.node_at(index).serialized(),
                false,
                80,
                *style,
                true,
                false,
            );
            assert_span_foreground(&spans, path.trim_start_matches('/'), 1);
            assert_span_foreground(&spans, text, color);
        }
        let index = document.index_of(&json_pointer("/array/0")).unwrap();
        let record = document.record(index);
        let spans = json_row_spans(
            record,
            document.node_at(index).serialized(),
            false,
            80,
            *style,
            true,
            false,
        );
        assert_span_foreground(&spans, "0", 7);
        assert_span_foreground(&spans, "2", 3);
    }

    fn assert_span_foreground(spans: &[TextSpan], text: &str, color: u8) {
        assert!(
            spans.iter().any(|span| {
                span.text() == text && span.style().foreground == Color::Indexed(color)
            }),
            "missing span {text:?} with indexed foreground {color}"
        );
    }

    fn fixture_action(value: &str) -> JsonInspectorAction {
        match value {
            "enter" => JsonInspectorAction::Activate,
            "up" => JsonInspectorAction::Previous,
            "down" => JsonInspectorAction::Next,
            "home" => JsonInspectorAction::First,
            "end" => JsonInspectorAction::Last,
            "left" => JsonInspectorAction::Collapse,
            "right" => JsonInspectorAction::Expand,
            _ => panic!("unknown fixture action {value}"),
        }
    }

    fn inspector_state(selected: &str, expanded: &str) -> JsonInspectorState {
        JsonInspectorState::new(
            json_pointer_text(selected),
            if expanded == "-" {
                Vec::new()
            } else {
                expanded.split(',').map(json_pointer_text).collect()
            },
        )
    }

    fn json_pointer_text(value: &str) -> JsonPointer {
        if value == "$" {
            JsonPointer::root()
        } else {
            json_pointer(value)
        }
    }

    fn json_pointer(value: &str) -> JsonPointer {
        JsonPointer::new(value).unwrap()
    }

    fn pointer_text(pointer: &JsonPointer) -> &str {
        if pointer.as_str().is_empty() {
            "$"
        } else {
            pointer.as_str()
        }
    }

    fn expanded_text(state: &JsonInspectorState) -> String {
        let values = state.expanded().map(pointer_text).collect::<Vec<_>>();
        if values.is_empty() {
            "-".to_owned()
        } else {
            values.join(",")
        }
    }

    fn state_message(state: &JsonInspectorState) -> String {
        format!(
            "state:{}:{}",
            pointer_text(state.selected()),
            expanded_text(state)
        )
    }

    fn repeat_key(code: KeyCode, modifiers: Modifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            action: KeyAction::Repeat,
            text: None,
            protocol: KeyProtocol::Legacy,
        })
    }
}
