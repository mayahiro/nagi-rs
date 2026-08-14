use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, LazyLock};

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, AnchoredOverlayOptions, EventResult, KeyBinding,
    KeyCode, KeyMap, KeyScope, KeyScopePropagation, KeyStroke, Modifiers, MouseButton, MouseKind,
    Node, NodeId, RepeatPolicy, Style, TEXT_CURSOR_DOWN_ACTION_ID, TEXT_CURSOR_UP_ACTION_ID,
    TEXT_INSERT_LINE_BREAK_ACTION_ID,
};

use crate::action::{
    COMPOSER_SUBMIT_ACTION_ID, HISTORY_NEXT_ACTION_ID, HISTORY_PREVIOUS_ACTION_ID,
    SELECTION_NEXT_ACTION_ID, SELECTION_NEXT_ACTION_LABEL, SELECTION_PREVIOUS_ACTION_ID,
    SELECTION_PREVIOUS_ACTION_LABEL, SUGGESTION_ACCEPT_ACTION_ID, SUGGESTION_DISMISS_ACTION_ID,
};
use crate::navigation::{Navigation, navigate};

/// One stable candidate identity supplied to a [`SuggestionPopup`]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SuggestionId(Arc<str>);

impl SuggestionId {
    /// Creates an opaque candidate identity
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(Arc::from(value.into()))
    }

    /// Returns the application-defined identity
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SuggestionId {
    fn from(value: &str) -> Self {
        Self(Arc::from(value))
    }
}

impl From<String> for SuggestionId {
    fn from(value: String) -> Self {
        Self(Arc::from(value))
    }
}

/// Error returned when a suggestion order contains the same stable ID twice
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateSuggestionId {
    id: SuggestionId,
}

impl DuplicateSuggestionId {
    /// Returns the duplicated candidate identity
    #[must_use]
    pub const fn id(&self) -> &SuggestionId {
        &self.id
    }
}

impl fmt::Display for DuplicateSuggestionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "duplicate suggestion ID {}", self.id.as_str())
    }
}

impl Error for DuplicateSuggestionId {}

/// Immutable unique candidate order shared across controlled view rebuilds
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SuggestionItems {
    inner: Arc<SuggestionItemsData>,
}

#[derive(Debug, Default, Eq, PartialEq)]
struct SuggestionItemsData {
    items: Box<[SuggestionId]>,
    positions: HashMap<SuggestionId, usize>,
}

impl SuggestionItems {
    /// Creates a validated immutable candidate order
    pub fn new(
        candidates: impl IntoIterator<Item = SuggestionId>,
    ) -> Result<Self, DuplicateSuggestionId> {
        let candidates: Vec<SuggestionId> = candidates.into_iter().collect();
        let mut positions = HashMap::with_capacity(candidates.len());
        for (index, candidate) in candidates.iter().enumerate() {
            if positions.insert(candidate.clone(), index).is_some() {
                return Err(DuplicateSuggestionId {
                    id: candidate.clone(),
                });
            }
        }
        Ok(Self {
            inner: Arc::new(SuggestionItemsData {
                items: candidates.into_boxed_slice(),
                positions,
            }),
        })
    }

    /// Returns the number of candidates
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.items.len()
    }

    /// Reports whether this candidate order is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.items.is_empty()
    }

    /// Returns one candidate by current index
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&SuggestionId> {
        self.inner.items.get(index)
    }

    /// Returns the immutable ordered candidates
    #[must_use]
    pub fn as_slice(&self) -> &[SuggestionId] {
        &self.inner.items
    }

    fn position(&self, id: &SuggestionId) -> Option<usize> {
        self.inner.positions.get(id).copied()
    }

    #[cfg(test)]
    fn shares_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

/// Application-owned asynchronous display state for a suggestion popup
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SuggestionPopupStatus {
    /// Candidate data is current for the application request
    #[default]
    Ready,
    /// A newer application request is still running
    Loading,
}

/// Context passed to an application suggestion row builder
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuggestionRowContext {
    index: usize,
    id: SuggestionId,
    selected: bool,
}

impl SuggestionRowContext {
    /// Returns the candidate index in application order
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }

    /// Returns the stable application candidate identity
    #[must_use]
    pub const fn id(&self) -> &SuggestionId {
        &self.id
    }

    /// Reports whether this candidate is the normalized selection
    #[must_use]
    pub const fn is_selected(&self) -> bool {
        self.selected
    }
}

/// Visual styles applied by a [`SuggestionPopup`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SuggestionPopupStyle {
    /// Style used by the popup border
    pub border: Style,
    /// Style used by the default loading and empty notices
    pub notice: Style,
}

impl Default for SuggestionPopupStyle {
    fn default() -> Self {
        Self {
            border: Style::default(),
            notice: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// A controlled, anchored suggestion list that leaves focus in its base content
///
/// The application owns query parsing, asynchronous work, ranking, candidates,
/// selection, and acceptance meaning. This widget only provides placement,
/// bounded row construction, keyboard routing, pointer activation, and status
/// presentation
pub struct SuggestionPopup<Message> {
    id: NodeId,
    base: Node<Message>,
    anchor: NodeId,
    focus_owner: NodeId,
    candidates: SuggestionItems,
    selected: Option<SuggestionId>,
    status: SuggestionPopupStatus,
    open: bool,
    enabled: bool,
    visible_rows: usize,
    placement: AnchoredOverlayOptions,
    style: SuggestionPopupStyle,
    loading: Option<Node<Message>>,
    empty: Option<Node<Message>>,
    row: Arc<dyn Fn(SuggestionRowContext) -> Node<Message>>,
    on_select: Arc<dyn Fn(SuggestionId) -> Message>,
    on_accept: Arc<dyn Fn(SuggestionId) -> Message>,
    on_dismiss: Arc<dyn Fn() -> Message>,
}

impl<Message: 'static> SuggestionPopup<Message> {
    /// Creates an open controlled popup with application-owned callbacks
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<NodeId>,
        base: Node<Message>,
        anchor: impl Into<NodeId>,
        focus_owner: impl Into<NodeId>,
        candidates: SuggestionItems,
        selected: Option<SuggestionId>,
        row: impl Fn(SuggestionRowContext) -> Node<Message> + 'static,
        on_select: impl Fn(SuggestionId) -> Message + 'static,
        on_accept: impl Fn(SuggestionId) -> Message + 'static,
        on_dismiss: impl Fn() -> Message + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            base,
            anchor: anchor.into(),
            focus_owner: focus_owner.into(),
            candidates,
            selected,
            status: SuggestionPopupStatus::Ready,
            open: true,
            enabled: true,
            visible_rows: 8,
            placement: AnchoredOverlayOptions::default(),
            style: SuggestionPopupStyle::default(),
            loading: None,
            empty: None,
            row: Arc::new(row),
            on_select: Arc::new(on_select),
            on_accept: Arc::new(on_accept),
            on_dismiss: Arc::new(on_dismiss),
        }
    }

    /// Sets whether the popup layer and its scoped actions are present
    #[must_use]
    pub const fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Sets whether candidate interaction is available while retaining status content
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets the application-owned asynchronous display state
    #[must_use]
    pub const fn status(mut self, status: SuggestionPopupStatus) -> Self {
        self.status = status;
        self
    }

    /// Limits constructed candidate rows to a positive visible window
    #[must_use]
    pub const fn visible_rows(mut self, rows: usize) -> Self {
        self.visible_rows = if rows == 0 { 1 } else { rows };
        self
    }

    /// Replaces anchored overlay placement and size limits
    #[must_use]
    pub const fn placement(mut self, placement: AnchoredOverlayOptions) -> Self {
        self.placement = placement;
        self
    }

    /// Replaces popup styles
    #[must_use]
    pub const fn style(mut self, style: SuggestionPopupStyle) -> Self {
        self.style = style;
        self
    }

    /// Replaces the loading notice with an application-provided Node
    #[must_use]
    pub fn loading(mut self, node: Node<Message>) -> Self {
        self.loading = Some(node);
        self
    }

    /// Replaces the empty-result notice with an application-provided Node
    #[must_use]
    pub fn empty(mut self, node: Node<Message>) -> Self {
        self.empty = Some(node);
        self
    }

    /// Returns accept, previous, next, and dismiss descriptors in dispatch order
    #[must_use]
    pub fn action_descriptors(&self) -> [ActionDescriptor; 4] {
        suggestion_action_descriptors(
            self.open,
            self.enabled,
            self.status == SuggestionPopupStatus::Ready && !self.candidates.is_empty(),
        )
    }

    /// Builds the public semantic Node for this suggestion popup
    #[must_use]
    pub fn into_node(mut self) -> Node<Message> {
        if !self.open {
            return self.base;
        }
        let selected = (self.status == SuggestionPopupStatus::Ready)
            .then(|| normalized_suggestion_selection(&self.candidates, self.selected.as_ref()))
            .flatten();
        let action_descriptors =
            suggestion_action_descriptors(true, self.enabled, selected.is_some());
        let loading = self.loading.take();
        let empty = self.empty.take();
        let popup = suggestion_layer(&self, selected, loading, empty);
        let scope = suggestion_key_scope(&self.id, self.enabled && selected.is_some());
        let repeat_focus = self.focus_owner.clone();
        let block_repeats = self.enabled;
        Node::anchored_overlay_with_options(self.base, self.anchor, popup, self.placement)
            .with_key_scope(scope)
            .on_actions(
                self.id.clone(),
                suggestion_actions(
                    action_descriptors,
                    self.candidates.clone(),
                    selected,
                    self.focus_owner.clone(),
                    Arc::clone(&self.on_select),
                    Arc::clone(&self.on_accept),
                    Arc::clone(&self.on_dismiss),
                ),
            )
            .on_event(self.id.clone(), move |event| {
                if block_repeats && suggestion_blocked_repeat(event) {
                    EventResult::consumed().focus(repeat_focus.clone())
                } else {
                    EventResult::ignored()
                }
            })
    }
}

fn suggestion_layer<Message: 'static>(
    popup: &SuggestionPopup<Message>,
    selected: Option<usize>,
    loading: Option<Node<Message>>,
    empty: Option<Node<Message>>,
) -> Node<Message> {
    let body = if popup.status == SuggestionPopupStatus::Loading {
        loading.unwrap_or_else(|| Node::styled_text("Loading...", popup.style.notice))
    } else if popup.candidates.is_empty() {
        empty.unwrap_or_else(|| Node::styled_text("No suggestions", popup.style.notice))
    } else {
        let selected = selected.expect("a non-empty suggestion collection has a selection");
        let (start, end) = suggestion_window(popup.candidates.len(), selected, popup.visible_rows);
        let rows = (start..end).map(|index| {
            let id = popup
                .candidates
                .get(index)
                .expect("visible candidate index is in range")
                .clone();
            let is_selected = index == selected;
            let node = (popup.row)(SuggestionRowContext {
                index,
                id: id.clone(),
                selected: is_selected,
            });
            let row_id = suggestion_row_node_id(&popup.id, &id);
            if !popup.enabled {
                return node.with_id(row_id);
            }
            let focus_owner = popup.focus_owner.clone();
            let on_select = Arc::clone(&popup.on_select);
            let on_accept = Arc::clone(&popup.on_accept);
            let pointer_id = id.clone();
            node.on_event(row_id, move |event| {
                if !matches!(
                    event,
                    nagi_tui::Event::Mouse(mouse)
                        if mouse.kind == MouseKind::Press && mouse.button == MouseButton::Left
                ) {
                    return EventResult::ignored();
                }
                let mut result = EventResult::consumed().focus(focus_owner.clone());
                if !is_selected {
                    result = result.emit(on_select(pointer_id.clone()));
                }
                result.emit(on_accept(pointer_id.clone()))
            })
        });
        Node::column(rows)
    };
    Node::border(body, popup.style.border)
}

fn suggestion_actions<Message: 'static>(
    descriptors: [ActionDescriptor; 4],
    candidates: SuggestionItems,
    selected: Option<usize>,
    focus_owner: NodeId,
    on_select: Arc<dyn Fn(SuggestionId) -> Message>,
    on_accept: Arc<dyn Fn(SuggestionId) -> Message>,
    on_dismiss: Arc<dyn Fn() -> Message>,
) -> [Action<Message>; 4] {
    std::array::from_fn(|action| {
        let candidates = candidates.clone();
        let focus_owner = focus_owner.clone();
        let on_select = Arc::clone(&on_select);
        let on_accept = Arc::clone(&on_accept);
        let on_dismiss = Arc::clone(&on_dismiss);
        Action::new(descriptors[action].clone(), move |_| {
            let result = EventResult::consumed().focus(focus_owner.clone());
            if action == 3 {
                return result.emit(on_dismiss());
            }
            let Some(selected) = selected else {
                return result;
            };
            if action == 0 {
                return result.emit(on_accept(
                    candidates
                        .get(selected)
                        .expect("selected suggestion index is in range")
                        .clone(),
                ));
            }
            let direction = if action == 1 {
                Navigation::Up
            } else {
                Navigation::Down
            };
            let next = navigate(candidates.len(), selected, direction).unwrap_or(selected);
            if next == selected {
                result
            } else {
                result.emit(on_select(
                    candidates
                        .get(next)
                        .expect("next suggestion index is in range")
                        .clone(),
                ))
            }
        })
    })
}

fn suggestion_action_descriptors(
    open: bool,
    enabled: bool,
    has_candidates: bool,
) -> [ActionDescriptor; 4] {
    let candidates = if open && enabled && has_candidates {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    };
    let dismiss = if open && enabled {
        ActionAvailability::Enabled
    } else {
        ActionAvailability::DisabledPassThrough
    };
    [
        SUGGESTION_ACCEPT_DESCRIPTOR
            .clone()
            .with_availability(candidates),
        SUGGESTION_PREVIOUS_DESCRIPTOR
            .clone()
            .with_availability(candidates),
        SUGGESTION_NEXT_DESCRIPTOR
            .clone()
            .with_availability(candidates),
        SUGGESTION_DISMISS_DESCRIPTOR
            .clone()
            .with_availability(dismiss),
    ]
}

fn suggestion_key_scope(id: &NodeId, intercept_candidates: bool) -> KeyScope {
    let mut map = KeyMap::new();
    if intercept_candidates {
        for action in [
            TEXT_CURSOR_UP_ACTION_ID,
            TEXT_CURSOR_DOWN_ACTION_ID,
            TEXT_INSERT_LINE_BREAK_ACTION_ID,
            COMPOSER_SUBMIT_ACTION_ID,
            HISTORY_PREVIOUS_ACTION_ID,
            HISTORY_NEXT_ACTION_ID,
        ] {
            map = map
                .rebind(action, [])
                .expect("suggestion scope Action IDs are unique");
        }
    }
    KeyScope::new(id.clone(), map).with_propagation(KeyScopePropagation::Continue)
}

fn normalized_suggestion_selection(
    candidates: &SuggestionItems,
    selected: Option<&SuggestionId>,
) -> Option<usize> {
    if candidates.is_empty() {
        return None;
    }
    selected
        .and_then(|selected| candidates.position(selected))
        .or(Some(0))
}

fn suggestion_window(count: usize, selected: usize, rows: usize) -> (usize, usize) {
    let limit = rows.max(1).min(count);
    let start = selected
        .saturating_add(1)
        .saturating_sub(limit)
        .min(count.saturating_sub(limit));
    (start, start.saturating_add(limit))
}

fn suggestion_row_node_id(popup: &NodeId, candidate: &SuggestionId) -> NodeId {
    NodeId::new(format!(
        "suggestion-row:{}:{}:{}:{}",
        popup.as_str().len(),
        popup.as_str(),
        candidate.as_str().len(),
        candidate.as_str()
    ))
}

fn suggestion_blocked_repeat(event: &nagi_tui::Event) -> bool {
    matches!(
        event,
        nagi_tui::Event::Key(key)
            if key.action == nagi_tui::KeyAction::Repeat
                && key.modifiers == Modifiers::NONE
                && matches!(key.code, KeyCode::Enter | KeyCode::Escape)
    )
}

static SUGGESTION_ACCEPT_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        SUGGESTION_ACCEPT_ACTION_ID,
        "Accept suggestion",
        [KeyBinding::new(KeyStroke::new(
            KeyCode::Enter,
            Modifiers::NONE,
        ))],
    )
});

static SUGGESTION_PREVIOUS_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        SELECTION_PREVIOUS_ACTION_ID,
        SELECTION_PREVIOUS_ACTION_LABEL,
        [
            KeyBinding::new(KeyStroke::new(KeyCode::Up, Modifiers::NONE))
                .with_repeat_policy(RepeatPolicy::AllowRepeat),
        ],
    )
});

static SUGGESTION_NEXT_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        SELECTION_NEXT_ACTION_ID,
        SELECTION_NEXT_ACTION_LABEL,
        [
            KeyBinding::new(KeyStroke::new(KeyCode::Down, Modifiers::NONE))
                .with_repeat_policy(RepeatPolicy::AllowRepeat),
        ],
    )
});

static SUGGESTION_DISMISS_DESCRIPTOR: LazyLock<ActionDescriptor> = LazyLock::new(|| {
    ActionDescriptor::new(
        SUGGESTION_DISMISS_ACTION_ID,
        "Dismiss suggestions",
        [KeyBinding::new(KeyStroke::new(
            KeyCode::Escape,
            Modifiers::NONE,
        ))],
    )
});

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn selection_uses_stable_identity_and_falls_back_to_first() {
        let candidates = SuggestionItems::new([SuggestionId::from("a"), SuggestionId::from("b")])
            .expect("unique candidates");
        assert_eq!(
            normalized_suggestion_selection(&candidates, Some(&SuggestionId::from("b"))),
            Some(1)
        );
        assert_eq!(
            normalized_suggestion_selection(&candidates, Some(&SuggestionId::from("missing"))),
            Some(0)
        );
    }

    #[test]
    fn visible_window_follows_selection() {
        assert_eq!(suggestion_window(10, 0, 3), (0, 3));
        assert_eq!(suggestion_window(10, 5, 3), (3, 6));
        assert_eq!(suggestion_window(10, 9, 3), (7, 10));
    }

    #[test]
    fn row_construction_is_bounded_and_skipped_for_status_notices() {
        let builds = Arc::new(AtomicUsize::new(0));
        let row_builds = Arc::clone(&builds);
        let candidates = (0..100)
            .map(|index| SuggestionId::new(format!("candidate-{index}")))
            .collect::<Vec<_>>();
        let items = SuggestionItems::new(candidates.clone()).expect("unique candidates");
        let _ = SuggestionPopup::new(
            "popup",
            Node::text("base").with_id("anchor"),
            "anchor",
            "anchor",
            items,
            Some(candidates[50].clone()),
            move |_| {
                row_builds.fetch_add(1, Ordering::Relaxed);
                Node::text("row")
            },
            |_| (),
            |_| (),
            || (),
        )
        .visible_rows(3)
        .into_node();
        assert_eq!(builds.load(Ordering::Relaxed), 3);

        for status in [SuggestionPopupStatus::Loading, SuggestionPopupStatus::Ready] {
            let candidates = if status == SuggestionPopupStatus::Ready {
                Vec::new()
            } else {
                vec![SuggestionId::from("candidate")]
            };
            let row_builds = Arc::clone(&builds);
            let _ = SuggestionPopup::new(
                "popup",
                Node::text("base").with_id("anchor"),
                "anchor",
                "anchor",
                SuggestionItems::new(candidates).expect("unique candidates"),
                None,
                move |_| {
                    row_builds.fetch_add(1, Ordering::Relaxed);
                    Node::text("row")
                },
                |_| (),
                |_| (),
                || (),
            )
            .status(status)
            .into_node();
        }
        assert_eq!(builds.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn descriptors_have_stable_order_repeat_policy_and_availability() {
        let popup = SuggestionPopup::new(
            "popup",
            Node::text("base").with_id("anchor"),
            "anchor",
            "anchor",
            SuggestionItems::new([SuggestionId::from("candidate")]).expect("unique candidates"),
            None,
            |_| Node::text("row"),
            |_| (),
            |_| (),
            || (),
        );
        let descriptors = popup.action_descriptors();
        assert_eq!(
            std::array::from_fn(|index| descriptors[index].id().as_str().to_owned()),
            [
                SUGGESTION_ACCEPT_ACTION_ID.to_owned(),
                SELECTION_PREVIOUS_ACTION_ID.to_owned(),
                SELECTION_NEXT_ACTION_ID.to_owned(),
                SUGGESTION_DISMISS_ACTION_ID.to_owned(),
            ]
        );
        assert_eq!(
            std::array::from_fn(|index| {
                descriptors[index].default_bindings()[0].repeat_policy()
            }),
            [
                RepeatPolicy::InitialOnly,
                RepeatPolicy::AllowRepeat,
                RepeatPolicy::AllowRepeat,
                RepeatPolicy::InitialOnly,
            ]
        );
        assert!(
            descriptors
                .iter()
                .all(|descriptor| descriptor.availability() == ActionAvailability::Enabled)
        );
    }

    #[test]
    fn immutable_items_reject_duplicates_and_share_storage() {
        let items = SuggestionItems::new([SuggestionId::from("a"), SuggestionId::from("b")])
            .expect("unique candidates");
        let clone = items.clone();
        assert!(items.shares_storage(&clone));
        assert_eq!(items.len(), 2);
        assert_eq!(items.get(1).map(SuggestionId::as_str), Some("b"));

        let duplicate =
            SuggestionItems::new([SuggestionId::from("same"), SuggestionId::from("same")])
                .expect_err("duplicate must fail");
        assert_eq!(duplicate.id().as_str(), "same");
    }

    #[test]
    fn derived_row_ids_distinguish_popup_and_candidate_boundaries() {
        let first = suggestion_row_node_id(
            &NodeId::from("x"),
            &SuggestionId::from("y:suggestion-row:1:z"),
        );
        let second = suggestion_row_node_id(
            &NodeId::from("x:suggestion-row:20:y"),
            &SuggestionId::from("z"),
        );
        assert_ne!(first, second);
    }
}
