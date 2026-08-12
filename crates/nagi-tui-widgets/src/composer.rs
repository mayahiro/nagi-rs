use std::sync::{Arc, LazyLock};

use nagi_text::{WidthProfile, graphemes};
use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, BindingSupport, EventResult, KeyBinding, KeyCode,
    KeyStroke, Length, Modifiers, Node, NodeId, RepeatPolicy, Style, TEXT_CURSOR_DOWN_ACTION_ID,
    TEXT_CURSOR_UP_ACTION_ID, TEXT_INSERT_LINE_BREAK_ACTION_ID,
};

use crate::action::{
    COMPOSER_SUBMIT_ACTION_ID, HISTORY_NEXT_ACTION_ID, HISTORY_PREVIOUS_ACTION_ID,
};
use crate::text_area::{
    TextAreaInsertionDecision, TextAreaInsertionPolicy, text_area_action_descriptors,
    text_area_visual_line_count,
};
use crate::{TextArea, TextAreaBoundaryNavigation, TextAreaState, TextAreaStyle};

const DEFAULT_MIN_ROWS: u32 = 1;
const DEFAULT_MAX_ROWS: u32 = 6;
const TEXT_ACTION_COUNT: usize = 18;
const COMPOSER_ACTION_COUNT: usize = 3 + TEXT_ACTION_COUNT;

/// Application-owned editor, history position, and draft for a [`Composer`]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ComposerState {
    text_area: TextAreaState,
    history_index: Option<usize>,
    draft: Option<TextAreaState>,
}

impl ComposerState {
    /// Creates state outside history browsing
    #[must_use]
    pub const fn new(text_area: TextAreaState) -> Self {
        Self {
            text_area,
            history_index: None,
            draft: None,
        }
    }

    /// Creates state with the cursor at the end of `value`
    #[must_use]
    pub fn at_end(value: impl Into<String>) -> Self {
        Self::new(TextAreaState::at_end(value))
    }

    /// Returns the controlled TextArea state
    #[must_use]
    pub const fn text_area(&self) -> &TextAreaState {
        &self.text_area
    }

    /// Returns the oldest-to-newest history index currently being browsed
    #[must_use]
    pub const fn history_index(&self) -> Option<usize> {
        self.history_index
    }

    /// Returns the TextArea state restored after browsing past newest history
    #[must_use]
    pub const fn draft(&self) -> Option<&TextAreaState> {
        self.draft.as_ref()
    }

    /// Replaces editor state and leaves history browsing
    #[must_use]
    pub fn with_text_area(mut self, text_area: TextAreaState) -> Self {
        self.text_area = text_area;
        self.history_index = None;
        self.draft = None;
        self
    }
}

/// Behavior when inserted text would exceed a Composer maximum length
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ComposerOverflowPolicy {
    /// Reject the complete Text or Paste edit
    #[default]
    Reject,
    /// Insert the longest grapheme-aligned prefix that fits
    Truncate,
}

#[derive(Clone, Copy)]
enum ComposerLengthUnit {
    Utf8Bytes,
    Graphemes,
}

#[derive(Clone, Copy)]
struct ComposerLengthLimit {
    maximum: usize,
    unit: ComposerLengthUnit,
    overflow: ComposerOverflowPolicy,
}

/// A controlled multiline message editor with submit and recall actions
///
/// Composer owns no persistence or submit meaning. History entries are supplied
/// oldest-to-newest by the application. Its root, viewport, and caret IDs must
/// be distinct and stable
pub struct Composer<Message> {
    id: NodeId,
    viewport_id: NodeId,
    caret_id: NodeId,
    state: ComposerState,
    history: Vec<String>,
    enabled: bool,
    submit_enabled: bool,
    placeholder: String,
    style: TextAreaStyle,
    selection_style: Style,
    width_profile: WidthProfile<'static>,
    wrap_width: Option<usize>,
    min_rows: u32,
    max_rows: u32,
    length_limit: Option<ComposerLengthLimit>,
    validation: Option<Node<Message>>,
    on_change: Arc<dyn Fn(ComposerState) -> Message>,
    on_submit: Arc<dyn Fn() -> Message>,
    on_undo: Option<Arc<dyn Fn() -> Message>>,
    on_redo: Option<Arc<dyn Fn() -> Message>>,
}

impl<Message: 'static> Composer<Message> {
    /// Creates an enabled Composer using application-owned state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        viewport_id: impl Into<NodeId>,
        caret_id: impl Into<NodeId>,
        state: ComposerState,
        on_change: impl Fn(ComposerState) -> Message + 'static,
        on_submit: impl Fn() -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            viewport_id: viewport_id.into(),
            caret_id: caret_id.into(),
            state,
            history: Vec::new(),
            enabled: true,
            submit_enabled: true,
            placeholder: String::new(),
            style: TextAreaStyle::default(),
            selection_style: Style {
                reverse: true,
                ..Style::default()
            },
            width_profile: WidthProfile::MODERN,
            wrap_width: None,
            min_rows: DEFAULT_MIN_ROWS,
            max_rows: DEFAULT_MAX_ROWS,
            length_limit: None,
            validation: None,
            on_change: Arc::new(on_change),
            on_submit: Arc::new(on_submit),
            on_undo: None,
            on_redo: None,
        }
    }

    /// Sets whether the Composer can receive focus and edit or submit its value
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets whether submit is currently valid while leaving editing enabled
    #[must_use]
    pub const fn submit_enabled(mut self, enabled: bool) -> Self {
        self.submit_enabled = enabled;
        self
    }

    /// Sets placeholder text shown when the value is empty
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Replaces TextArea visual styles
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

    /// Sets the terminal cell-width policy used by editing and layout
    ///
    /// Pass `ViewContext::width_profile` to keep the widget aligned with its Runtime
    #[must_use]
    pub const fn width_profile(mut self, profile: WidthProfile<'static>) -> Self {
        self.width_profile = profile;
        self
    }

    /// Uses logical lines without soft wrapping
    #[must_use]
    pub const fn no_wrap(mut self) -> Self {
        self.wrap_width = None;
        self
    }

    /// Soft-wraps visual lines at a terminal-cell width
    ///
    /// Zero is normalized to one Cell. Applications should recompute the width
    /// from `ViewContext` after a resize
    #[must_use]
    pub fn soft_wrap(mut self, width: u32) -> Self {
        self.wrap_width = Some(width.max(1) as usize);
        self
    }

    /// Clamps the automatic editor height between normalized row bounds
    ///
    /// `min_rows` is normalized to at least one and `max_rows` to at least the
    /// normalized minimum
    #[must_use]
    pub const fn rows(mut self, min_rows: u32, max_rows: u32) -> Self {
        self.min_rows = if min_rows == 0 { 1 } else { min_rows };
        self.max_rows = if max_rows < self.min_rows {
            self.min_rows
        } else {
            max_rows
        };
        self
    }

    /// Limits future insertion by UTF-8 bytes without rewriting existing text
    #[must_use]
    pub const fn maximum_utf8_bytes(
        mut self,
        maximum: usize,
        overflow: ComposerOverflowPolicy,
    ) -> Self {
        self.length_limit = Some(ComposerLengthLimit {
            maximum,
            unit: ComposerLengthUnit::Utf8Bytes,
            overflow,
        });
        self
    }

    /// Limits future insertion by grapheme count without rewriting existing text
    #[must_use]
    pub const fn maximum_graphemes(
        mut self,
        maximum: usize,
        overflow: ComposerOverflowPolicy,
    ) -> Self {
        self.length_limit = Some(ComposerLengthLimit {
            maximum,
            unit: ComposerLengthUnit::Graphemes,
            overflow,
        });
        self
    }

    /// Replaces oldest-to-newest recall entries without taking persistence ownership
    #[must_use]
    pub fn history<I, S>(mut self, entries: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.history = entries.into_iter().map(Into::into).collect();
        self
    }

    /// Renders an application-provided validation Node below the editor viewport
    #[must_use]
    pub fn validation(mut self, validation: Node<Message>) -> Self {
        self.validation = Some(validation);
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

    /// Returns the current editor viewport height before parent layout clamping
    #[must_use]
    pub fn visible_rows(&self) -> u32 {
        composer_visible_rows(
            &self.state.text_area,
            self.wrap_width,
            self.min_rows,
            self.max_rows,
            self.width_profile,
        )
    }

    /// Returns Composer actions followed by its inherited TextArea actions
    ///
    /// The first three actions are submit, history previous, and history next.
    /// The remaining 18 preserve TextArea order with Composer newline defaults
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; COMPOSER_ACTION_COUNT] {
        let text = composer_text_descriptors(
            self.enabled,
            self.on_undo.is_some(),
            self.on_redo.is_some(),
            &self.state.text_area,
            self.wrap_width,
            self.width_profile,
        );
        let (has_up, has_down) = text_directions(&text);
        let leading = composer_leading_descriptors(
            self.enabled,
            self.submit_enabled,
            !has_up && composer_has_history_previous(&self.state, self.history.len()),
            !has_down && self.state.history_index.is_some(),
        );
        std::array::from_fn(|index| {
            if index < leading.len() {
                leading[index].clone()
            } else {
                text[index - leading.len()].clone()
            }
        })
    }

    /// Builds the public semantic Node for this Composer
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let rows = self.visible_rows();
        let has_undo = self.on_undo.is_some();
        let has_redo = self.on_redo.is_some();
        let current_for_change = self.state.clone();
        let on_change = Arc::clone(&self.on_change);
        let mut text_area = TextArea::new(
            self.id.clone(),
            self.state.text_area.clone(),
            move |text_area| on_change(composer_changed_state(&current_for_change, text_area)),
        )
        .enabled(self.enabled)
        .placeholder(self.placeholder)
        .style(self.style)
        .selection_style(self.selection_style)
        .width_profile(self.width_profile)
        .boundary_navigation(TextAreaBoundaryNavigation::Bubble)
        .viewport(self.viewport_id, self.caret_id, Length::Fixed(rows));
        if let Some(width) = self.wrap_width {
            text_area = text_area.soft_wrap(width as u32);
        }
        if let Some(on_undo) = self.on_undo {
            text_area = text_area.on_undo(move || on_undo());
        }
        if let Some(on_redo) = self.on_redo {
            text_area = text_area.on_redo(move || on_redo());
        }

        let text_descriptors = composer_text_descriptors(
            self.enabled,
            has_undo,
            has_redo,
            &self.state.text_area,
            self.wrap_width,
            self.width_profile,
        );
        let (has_up, has_down) = text_directions(&text_descriptors);
        let previous = (!has_up)
            .then(|| composer_history_previous(&self.state, &self.history))
            .flatten();
        let next = (!has_down)
            .then(|| composer_history_next(&self.state, &self.history))
            .flatten();
        let leading_descriptors = composer_leading_descriptors(
            self.enabled,
            self.submit_enabled,
            previous.is_some(),
            next.is_some(),
        );
        let leading_actions = composer_leading_actions(
            leading_descriptors,
            self.id,
            Arc::clone(&self.on_change),
            self.on_submit,
            previous,
            next,
        );
        let insertion_policy = self.length_limit.map(composer_insertion_policy);
        let editor =
            text_area.into_node_with_actions(text_descriptors, leading_actions, insertion_policy);
        let mut children = vec![editor];
        if let Some(validation) = self.validation {
            children.push(validation);
        }
        Node::column(children)
    }
}

fn composer_text_descriptors(
    enabled: bool,
    has_undo: bool,
    has_redo: bool,
    state: &TextAreaState,
    wrap_width: Option<usize>,
    profile: WidthProfile<'static>,
) -> [ActionDescriptor; TEXT_ACTION_COUNT] {
    let mut descriptors = text_area_action_descriptors(
        enabled,
        has_undo,
        has_redo,
        TextAreaBoundaryNavigation::Bubble,
        state,
        wrap_width,
        profile,
    );
    let line_break = descriptors
        .iter_mut()
        .find(|descriptor| descriptor.id().as_str() == TEXT_INSERT_LINE_BREAK_ACTION_ID)
        .expect("TextArea line-break descriptor");
    *line_break = COMPOSER_LINE_BREAK_DESCRIPTOR
        .clone()
        .with_availability(line_break.availability());
    descriptors
}

fn text_directions(descriptors: &[ActionDescriptor; TEXT_ACTION_COUNT]) -> (bool, bool) {
    let available = |id: &str| {
        descriptors.iter().any(|descriptor| {
            descriptor.id().as_str() == id
                && descriptor.availability() == ActionAvailability::Enabled
        })
    };
    (
        available(TEXT_CURSOR_UP_ACTION_ID),
        available(TEXT_CURSOR_DOWN_ACTION_ID),
    )
}

fn composer_leading_descriptors(
    enabled: bool,
    submit_enabled: bool,
    has_previous: bool,
    has_next: bool,
) -> [ActionDescriptor; 3] {
    let submit_availability = if !enabled {
        ActionAvailability::DisabledPassThrough
    } else if submit_enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledConsume
    };
    [
        COMPOSER_SUBMIT_DESCRIPTOR
            .clone()
            .with_availability(submit_availability),
        HISTORY_PREVIOUS_DESCRIPTOR
            .clone()
            .with_availability(history_availability(enabled && has_previous)),
        HISTORY_NEXT_DESCRIPTOR
            .clone()
            .with_availability(history_availability(enabled && has_next)),
    ]
}

const fn history_availability(enabled: bool) -> ActionAvailability {
    if enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    }
}

fn composer_leading_actions<Message: 'static>(
    descriptors: [ActionDescriptor; 3],
    id: NodeId,
    on_change: Arc<dyn Fn(ComposerState) -> Message>,
    on_submit: Arc<dyn Fn() -> Message>,
    previous: Option<ComposerState>,
    next: Option<ComposerState>,
) -> [Action<Message>; 3] {
    let submit_id = id.clone();
    let previous_id = id.clone();
    let previous_change = Arc::clone(&on_change);
    let next_change = on_change;
    [
        Action::new(descriptors[0].clone(), move |_| {
            EventResult::consumed()
                .focus(submit_id.clone())
                .emit(on_submit())
        }),
        Action::new(descriptors[1].clone(), move |_| {
            let result = EventResult::consumed().focus(previous_id.clone());
            match previous.clone() {
                Some(state) => result.emit(previous_change(state)),
                None => result,
            }
        }),
        Action::new(descriptors[2].clone(), move |_| {
            let result = EventResult::consumed().focus(id.clone());
            match next.clone() {
                Some(state) => result.emit(next_change(state)),
                None => result,
            }
        }),
    ]
}

fn composer_changed_state(current: &ComposerState, text_area: TextAreaState) -> ComposerState {
    let mut next = current.clone();
    let value_changed = next.text_area.value() != text_area.value();
    next.text_area = text_area;
    if value_changed {
        next.history_index = None;
        next.draft = None;
    }
    next
}

fn composer_history_previous(state: &ComposerState, history: &[String]) -> Option<ComposerState> {
    if !composer_has_history_previous(state, history.len()) {
        return None;
    }
    let target = state
        .history_index
        .map_or(history.len().saturating_sub(1), |index| {
            index.min(history.len()).saturating_sub(1)
        });
    let mut next = state.clone();
    if next.draft.is_none() {
        next.draft = Some(next.text_area.clone());
    }
    next.history_index = Some(target);
    next.text_area = TextAreaState::at_end(history[target].clone());
    Some(next)
}

const fn composer_has_history_previous(state: &ComposerState, history_len: usize) -> bool {
    history_len != 0 && !matches!(state.history_index, Some(0))
}

fn composer_history_next(state: &ComposerState, history: &[String]) -> Option<ComposerState> {
    let index = state.history_index?;
    let mut next = state.clone();
    if index.saturating_add(1) < history.len() {
        let target = index + 1;
        next.history_index = Some(target);
        next.text_area = TextAreaState::at_end(history[target].clone());
        return Some(next);
    }
    next.text_area = next.draft.take().unwrap_or_else(|| state.text_area.clone());
    next.history_index = None;
    Some(next)
}

fn composer_visible_rows(
    state: &TextAreaState,
    wrap_width: Option<usize>,
    min_rows: u32,
    max_rows: u32,
    profile: WidthProfile<'static>,
) -> u32 {
    let minimum = min_rows.max(1);
    let maximum = max_rows.max(minimum);
    let content =
        u32::try_from(text_area_visual_line_count(state, wrap_width, profile)).unwrap_or(u32::MAX);
    content.clamp(minimum, maximum)
}

fn composer_insertion_policy(limit: ComposerLengthLimit) -> TextAreaInsertionPolicy {
    Arc::new(move |state, inserted| composer_insertion_decision(limit, state, inserted))
}

fn composer_insertion_decision(
    limit: ComposerLengthLimit,
    state: &TextAreaState,
    inserted: &str,
) -> TextAreaInsertionDecision {
    let selection = state.selection();
    let selected = selection
        .as_ref()
        .map_or("", |range| &state.value()[range.clone()]);
    let base = measured_length(state.value(), limit.unit)
        .saturating_sub(measured_length(selected, limit.unit));
    let inserted_length = measured_length(inserted, limit.unit);
    if base.saturating_add(inserted_length) <= limit.maximum {
        return TextAreaInsertionDecision::Accept;
    }
    if limit.overflow == ComposerOverflowPolicy::Reject {
        return TextAreaInsertionDecision::Reject;
    }
    let available = limit.maximum.saturating_sub(base);
    let mut boundary = 0;
    match limit.unit {
        ComposerLengthUnit::Utf8Bytes => {
            for grapheme in graphemes(inserted) {
                if grapheme.end() > available {
                    break;
                }
                boundary = grapheme.end();
            }
        }
        ComposerLengthUnit::Graphemes => {
            for grapheme in graphemes(inserted).take(available) {
                boundary = grapheme.end();
            }
        }
    }
    if boundary == inserted.len() {
        TextAreaInsertionDecision::Accept
    } else {
        TextAreaInsertionDecision::Replace(inserted[..boundary].to_owned())
    }
}

fn measured_length(value: &str, unit: ComposerLengthUnit) -> usize {
    match unit {
        ComposerLengthUnit::Utf8Bytes => value.len(),
        ComposerLengthUnit::Graphemes => graphemes(value).count(),
    }
}

static COMPOSER_SUBMIT_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        COMPOSER_SUBMIT_ACTION_ID,
        "Submit",
        [KeyBinding::new(KeyStroke::new(
            KeyCode::Enter,
            Modifiers::NONE,
        ))],
    )
});

static HISTORY_PREVIOUS_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        HISTORY_PREVIOUS_ACTION_ID,
        "Previous history entry",
        [repeatable_binding(KeyCode::Up, Modifiers::NONE)],
    )
});

static HISTORY_NEXT_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        HISTORY_NEXT_ACTION_ID,
        "Next history entry",
        [repeatable_binding(KeyCode::Down, Modifiers::NONE)],
    )
});

static COMPOSER_LINE_BREAK_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        TEXT_INSERT_LINE_BREAK_ACTION_ID,
        "Insert line break",
        [
            repeatable_binding(
                KeyCode::Enter,
                Modifiers {
                    shift: true,
                    ..Modifiers::NONE
                },
            ),
            repeatable_binding(
                KeyCode::Enter,
                Modifiers {
                    alt: true,
                    ..Modifiers::NONE
                },
            ),
            KeyBinding::new(KeyStroke::character(
                'o',
                Modifiers {
                    control: true,
                    ..Modifiers::NONE
                },
            ))
            .with_repeat_policy(RepeatPolicy::AllowRepeat)
            .with_support(BindingSupport::Supported),
        ],
    )
});

const fn repeatable_binding(code: KeyCode, modifiers: Modifiers) -> KeyBinding {
    KeyBinding::new(KeyStroke::new(code, modifiers)).with_repeat_policy(RepeatPolicy::AllowRepeat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_descriptor_clones_reuse_immutable_storage() {
        let enabled = composer_leading_descriptors(true, true, true, true);
        let unavailable = composer_leading_descriptors(false, false, false, false);

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
        }
    }
}
