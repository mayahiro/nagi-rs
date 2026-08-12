use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};

use nagi_vt::{Event, KeyAction, KeyCode, Modifiers};

use crate::{EventResult, NodeId};

static EMPTY_SCOPE_PATH: LazyLock<Arc<[NodeId]>> = LazyLock::new(Arc::default);

/// Stable Action ID for moving focus to the next focusable node
pub const FOCUS_NEXT_ACTION_ID: &str = "nagi.focus.next";

/// Stable Action ID for moving focus to the previous focusable node
pub const FOCUS_PREVIOUS_ACTION_ID: &str = "nagi.focus.previous";

/// Stable Action ID for scrolling one visible page toward the start
pub const SCROLL_PAGE_UP_ACTION_ID: &str = "nagi.scroll.page-up";

/// Stable Action ID for scrolling one visible page toward the end
pub const SCROLL_PAGE_DOWN_ACTION_ID: &str = "nagi.scroll.page-down";

/// Stable Action ID for scrolling to the beginning of the enabled axis
pub const SCROLL_START_ACTION_ID: &str = "nagi.scroll.start";

/// Stable Action ID for scrolling to the end of the enabled axis
pub const SCROLL_END_ACTION_ID: &str = "nagi.scroll.end";

/// Stable Action ID for moving a text cursor left
pub const TEXT_CURSOR_LEFT_ACTION_ID: &str = "nagi.text.cursor.left";

/// Stable Action ID for moving a text cursor right
pub const TEXT_CURSOR_RIGHT_ACTION_ID: &str = "nagi.text.cursor.right";

/// Stable Action ID for moving a text cursor to the previous word
pub const TEXT_CURSOR_WORD_LEFT_ACTION_ID: &str = "nagi.text.cursor.word-left";

/// Stable Action ID for moving a text cursor to the next word
pub const TEXT_CURSOR_WORD_RIGHT_ACTION_ID: &str = "nagi.text.cursor.word-right";

/// Stable Action ID for moving a text cursor up
pub const TEXT_CURSOR_UP_ACTION_ID: &str = "nagi.text.cursor.up";

/// Stable Action ID for moving a text cursor down
pub const TEXT_CURSOR_DOWN_ACTION_ID: &str = "nagi.text.cursor.down";

/// Stable Action ID for moving a text cursor to the current line start
pub const TEXT_CURSOR_LINE_START_ACTION_ID: &str = "nagi.text.cursor.line-start";

/// Stable Action ID for moving a text cursor to the current line end
pub const TEXT_CURSOR_LINE_END_ACTION_ID: &str = "nagi.text.cursor.line-end";

/// Stable Action ID for moving a text cursor to the document start
pub const TEXT_CURSOR_DOCUMENT_START_ACTION_ID: &str = "nagi.text.cursor.document-start";

/// Stable Action ID for moving a text cursor to the document end
pub const TEXT_CURSOR_DOCUMENT_END_ACTION_ID: &str = "nagi.text.cursor.document-end";

/// Stable Action ID for extending text selection left
pub const TEXT_SELECTION_EXTEND_LEFT_ACTION_ID: &str = "nagi.text.selection.extend-left";

/// Stable Action ID for extending text selection right
pub const TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID: &str = "nagi.text.selection.extend-right";

/// Stable Action ID for extending text selection to the previous word
pub const TEXT_SELECTION_EXTEND_WORD_LEFT_ACTION_ID: &str = "nagi.text.selection.extend-word-left";

/// Stable Action ID for extending text selection to the next word
pub const TEXT_SELECTION_EXTEND_WORD_RIGHT_ACTION_ID: &str =
    "nagi.text.selection.extend-word-right";

/// Stable Action ID for extending text selection up
pub const TEXT_SELECTION_EXTEND_UP_ACTION_ID: &str = "nagi.text.selection.extend-up";

/// Stable Action ID for extending text selection down
pub const TEXT_SELECTION_EXTEND_DOWN_ACTION_ID: &str = "nagi.text.selection.extend-down";

/// Stable Action ID for extending text selection to the current line start
pub const TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID: &str =
    "nagi.text.selection.extend-line-start";

/// Stable Action ID for extending text selection to the current line end
pub const TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID: &str = "nagi.text.selection.extend-line-end";

/// Stable Action ID for extending text selection to the document start
pub const TEXT_SELECTION_EXTEND_DOCUMENT_START_ACTION_ID: &str =
    "nagi.text.selection.extend-document-start";

/// Stable Action ID for extending text selection to the document end
pub const TEXT_SELECTION_EXTEND_DOCUMENT_END_ACTION_ID: &str =
    "nagi.text.selection.extend-document-end";

/// Stable Action ID for selecting one complete semantic text document
pub const TEXT_SELECT_ALL_ACTION_ID: &str = "nagi.text.select-all";

/// Stable Action ID for copying the current semantic text selection
pub const TEXT_COPY_SELECTION_ACTION_ID: &str = "nagi.text.copy-selection";

/// Stable Action ID for copying one complete semantic text document
pub const TEXT_COPY_DOCUMENT_ACTION_ID: &str = "nagi.text.copy-document";

/// Stable Action ID for deleting text backward
pub const TEXT_DELETE_BACKWARD_ACTION_ID: &str = "nagi.text.delete.backward";

/// Stable Action ID for deleting text forward
pub const TEXT_DELETE_FORWARD_ACTION_ID: &str = "nagi.text.delete.forward";

/// Stable Action ID for inserting a line break
pub const TEXT_INSERT_LINE_BREAK_ACTION_ID: &str = "nagi.text.insert-line-break";

/// Stable Action ID for undoing a text edit
pub const TEXT_UNDO_ACTION_ID: &str = "nagi.text.undo";

/// Stable Action ID for redoing a text edit
pub const TEXT_REDO_ACTION_ID: &str = "nagi.text.redo";

/// A stable, key-independent action identity
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActionId(Arc<str>);

impl ActionId {
    /// Creates an Action ID from an application-defined stable value
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(Arc::from(value.into()))
    }

    /// Returns the application-defined identity
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(formatter)
    }
}

impl From<&str> for ActionId {
    fn from(value: &str) -> Self {
        Self(Arc::from(value))
    }
}

impl From<String> for ActionId {
    fn from(value: String) -> Self {
        Self(Arc::from(value))
    }
}

/// Whether an action participates in key matching
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ActionAvailability {
    /// The action is an active candidate
    #[default]
    Enabled,
    /// The action is unavailable and matching continues to another candidate
    DisabledPassThrough,
    /// The action is unavailable but remains an active blocking candidate
    DisabledConsume,
}

impl ActionAvailability {
    fn is_candidate(self) -> bool {
        matches!(self, Self::Enabled | Self::DisabledConsume)
    }
}

/// Whether an explicit key-repeat event may match a binding
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum RepeatPolicy {
    /// Accept an initial press or a legacy action of unknown kind
    #[default]
    InitialOnly,
    /// Also accept an explicit repeat event
    AllowRepeat,
}

/// Terminal capability metadata for one key binding
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BindingSupport {
    /// The terminal capability is not known
    #[default]
    Unknown,
    /// The terminal is known to report the stroke distinctly
    Supported,
    /// The terminal is not known to report the stroke distinctly
    Unsupported,
}

/// One normalized logical key and exact modifier set
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KeyStroke {
    code: KeyCode,
    modifiers: Modifiers,
}

impl KeyStroke {
    /// Creates a stroke from a normalized logical key and modifiers
    #[must_use]
    pub const fn new(code: KeyCode, modifiers: Modifiers) -> Self {
        Self { code, modifiers }
    }

    /// Creates a Character stroke
    #[must_use]
    pub const fn character(character: char, modifiers: Modifiers) -> Self {
        Self::new(KeyCode::Character(character), modifiers)
    }

    /// Creates a Function-key stroke
    #[must_use]
    pub const fn function(number: u8, modifiers: Modifiers) -> Self {
        Self::new(KeyCode::Function(number), modifiers)
    }

    /// Returns the logical key
    #[must_use]
    pub const fn code(self) -> KeyCode {
        self.code
    }

    /// Returns the exact modifier set
    #[must_use]
    pub const fn modifiers(self) -> Modifiers {
        self.modifiers
    }

    /// Normalizes one Key or single-scalar Text event into a stroke
    ///
    /// Release, Paste, multi-scalar Text, and non-key events return `None`
    #[must_use]
    pub fn from_event(event: &Event) -> Option<Self> {
        match event {
            Event::Key(key) if key.action != KeyAction::Release => {
                Some(Self::new(key.code, key.modifiers))
            }
            Event::Text(text) => {
                let mut characters = text.chars();
                let character = characters.next()?;
                characters
                    .next()
                    .is_none()
                    .then(|| Self::character(character, Modifiers::NONE))
            }
            _ => None,
        }
    }

    /// Returns deterministic user-facing English notation
    #[must_use]
    pub fn notation(self) -> String {
        self.to_string()
    }
}

impl fmt::Display for KeyStroke {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.control {
            formatter.write_str("Ctrl+")?;
        }
        if self.modifiers.alt {
            formatter.write_str("Alt+")?;
        }
        if self.modifiers.shift {
            formatter.write_str("Shift+")?;
        }
        if self.modifiers.meta {
            formatter.write_str("Meta+")?;
        }
        match self.code {
            KeyCode::Character(' ') => formatter.write_str("Space"),
            KeyCode::Character(character) if character.is_control() => {
                write!(formatter, "U+{:04X}", u32::from(character))
            }
            KeyCode::Character(character) => character.fmt(formatter),
            KeyCode::Enter => formatter.write_str("Enter"),
            KeyCode::Tab => formatter.write_str("Tab"),
            KeyCode::Backspace => formatter.write_str("Backspace"),
            KeyCode::Escape => formatter.write_str("Escape"),
            KeyCode::Up => formatter.write_str("Up"),
            KeyCode::Down => formatter.write_str("Down"),
            KeyCode::Right => formatter.write_str("Right"),
            KeyCode::Left => formatter.write_str("Left"),
            KeyCode::Home => formatter.write_str("Home"),
            KeyCode::End => formatter.write_str("End"),
            KeyCode::Insert => formatter.write_str("Insert"),
            KeyCode::Delete => formatter.write_str("Delete"),
            KeyCode::PageUp => formatter.write_str("PageUp"),
            KeyCode::PageDown => formatter.write_str("PageDown"),
            KeyCode::Function(number) => write!(formatter, "F{number}"),
            KeyCode::Unknown => formatter.write_str("Unknown"),
        }
    }
}

/// One stroke with repeat and terminal-support metadata
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KeyBinding {
    stroke: KeyStroke,
    repeat_policy: RepeatPolicy,
    support: BindingSupport,
}

impl KeyBinding {
    /// Creates an initial-only binding with unknown terminal support
    #[must_use]
    pub const fn new(stroke: KeyStroke) -> Self {
        Self {
            stroke,
            repeat_policy: RepeatPolicy::InitialOnly,
            support: BindingSupport::Unknown,
        }
    }

    /// Replaces the repeat policy
    #[must_use]
    pub const fn with_repeat_policy(mut self, repeat_policy: RepeatPolicy) -> Self {
        self.repeat_policy = repeat_policy;
        self
    }

    /// Replaces terminal-support metadata
    ///
    /// Support metadata affects presentation but never event matching
    #[must_use]
    pub const fn with_support(mut self, support: BindingSupport) -> Self {
        self.support = support;
        self
    }

    /// Returns the normalized stroke
    #[must_use]
    pub const fn stroke(self) -> KeyStroke {
        self.stroke
    }

    /// Returns the repeat policy
    #[must_use]
    pub const fn repeat_policy(self) -> RepeatPolicy {
        self.repeat_policy
    }

    /// Returns terminal-support metadata
    #[must_use]
    pub const fn support(self) -> BindingSupport {
        self.support
    }

    /// Reports whether a normalized event matches this binding
    #[must_use]
    pub fn matches(self, event: &Event) -> bool {
        if !self.matches_stroke(event) {
            return false;
        }
        match event {
            Event::Key(key) => match key.action {
                KeyAction::Press | KeyAction::Unknown => true,
                KeyAction::Repeat => self.repeat_policy == RepeatPolicy::AllowRepeat,
                KeyAction::Release => false,
            },
            Event::Text(_) => true,
            _ => false,
        }
    }

    /// Reports whether an event has this binding's normalized stroke
    ///
    /// Unlike [`Self::matches`], this ignores repeat policy. Disabled-consume
    /// actions use it to keep repeated input inside the same semantic boundary
    /// without invoking a handler that is initial-only
    #[must_use]
    pub fn matches_stroke(self, event: &Event) -> bool {
        KeyStroke::from_event(event) == Some(self.stroke)
    }
}

/// A handler-independent action definition
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionDescriptor {
    id: ActionId,
    label: Arc<str>,
    default_bindings: Arc<[KeyBinding]>,
    availability: ActionAvailability,
    help_visible: bool,
}

impl ActionDescriptor {
    /// Creates an enabled, Help-visible action descriptor
    #[must_use]
    pub fn new(
        id: impl Into<ActionId>,
        label: impl Into<String>,
        bindings: impl IntoIterator<Item = KeyBinding>,
    ) -> Self {
        Self {
            id: id.into(),
            label: Arc::from(label.into()),
            default_bindings: bindings.into_iter().collect(),
            availability: ActionAvailability::Enabled,
            help_visible: true,
        }
    }

    /// Replaces whether and how the action participates in matching
    #[must_use]
    pub const fn with_availability(mut self, availability: ActionAvailability) -> Self {
        self.availability = availability;
        self
    }

    /// Replaces whether Help projections normally include the action
    #[must_use]
    pub const fn with_help_visible(mut self, visible: bool) -> Self {
        self.help_visible = visible;
        self
    }

    /// Returns the stable Action ID
    #[must_use]
    pub fn id(&self) -> &ActionId {
        &self.id
    }

    /// Returns the user-facing label
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the ordered default bindings
    #[must_use]
    pub fn default_bindings(&self) -> &[KeyBinding] {
        self.default_bindings.as_ref()
    }

    /// Returns current availability
    #[must_use]
    pub const fn availability(&self) -> ActionAvailability {
        self.availability
    }

    /// Reports whether Help projections normally include the action
    #[must_use]
    pub const fn is_help_visible(&self) -> bool {
        self.help_visible
    }
}

/// One semantic action invocation after key resolution
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionEvent {
    action: ActionId,
    stroke: KeyStroke,
}

impl ActionEvent {
    pub(crate) const fn new(action: ActionId, stroke: KeyStroke) -> Self {
        Self { action, stroke }
    }

    /// Returns the resolved Action ID
    #[must_use]
    pub const fn action(&self) -> &ActionId {
        &self.action
    }

    /// Returns the normalized stroke that triggered the action
    #[must_use]
    pub const fn stroke(&self) -> KeyStroke {
        self.stroke
    }
}

/// A handler-independent descriptor paired with one Node-local handler
pub struct Action<Message> {
    descriptor: ActionDescriptor,
    handler: Rc<ActionHandler<Message>>,
}

type ActionHandler<Message> = dyn Fn(&ActionEvent) -> EventResult<Message>;

impl<Message> Action<Message> {
    /// Creates an action from a descriptor and semantic handler
    #[must_use]
    pub fn new(
        descriptor: ActionDescriptor,
        handler: impl Fn(&ActionEvent) -> EventResult<Message> + 'static,
    ) -> Self {
        Self {
            descriptor,
            handler: Rc::new(handler),
        }
    }

    /// Returns the handler-independent action descriptor
    #[must_use]
    pub const fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    pub(crate) fn invoke(&self, event: &ActionEvent) -> EventResult<Message> {
        (self.handler)(event)
    }
}

impl<Message> Clone for Action<Message> {
    fn clone(&self) -> Self {
        Self {
            descriptor: self.descriptor.clone(),
            handler: self.handler.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct KeyOverride {
    action: ActionId,
    bindings: Arc<[KeyBinding]>,
}

/// One immutable Action-ID-to-binding override layer
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeyMap {
    overrides: Arc<[KeyOverride]>,
}

impl KeyMap {
    /// Creates an empty override layer
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a new layer with one complete binding-list replacement
    ///
    /// An empty replacement unbinds the action. Rebinding an Action ID already
    /// present in this layer returns [`KeyMapError::DuplicateActionOverride`]
    pub fn rebind(
        &self,
        action: impl Into<ActionId>,
        bindings: impl IntoIterator<Item = KeyBinding>,
    ) -> Result<Self, KeyMapError> {
        let action = action.into();
        if self
            .overrides
            .iter()
            .any(|binding_override| binding_override.action == action)
        {
            return Err(KeyMapError::DuplicateActionOverride(action));
        }
        let mut overrides = self.overrides.to_vec();
        overrides.push(KeyOverride {
            action,
            bindings: bindings.into_iter().collect(),
        });
        Ok(Self {
            overrides: overrides.into(),
        })
    }

    /// Returns the replacement for an Action ID when this layer names it
    #[must_use]
    pub fn bindings(&self, action: &ActionId) -> Option<&[KeyBinding]> {
        self.binding_storage(action).map(AsRef::as_ref)
    }

    fn binding_storage(&self, action: &ActionId) -> Option<&Arc<[KeyBinding]>> {
        self.overrides
            .iter()
            .find(|binding_override| &binding_override.action == action)
            .map(|binding_override| &binding_override.bindings)
    }

    /// Reports whether the layer contains no overrides
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }
}

/// An invalid immutable KeyMap layer
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyMapError {
    /// One layer attempted to override the same Action ID twice
    DuplicateActionOverride(ActionId),
}

impl KeyMapError {
    /// Returns the Action ID associated with the error
    #[must_use]
    pub const fn action_id(&self) -> &ActionId {
        match self {
            Self::DuplicateActionOverride(action) => action,
        }
    }
}

impl fmt::Display for KeyMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateActionOverride(action) => {
                write!(formatter, "duplicate key override for ActionId {action}")
            }
        }
    }
}

impl Error for KeyMapError {}

/// Runtime ancestor-action propagation behavior at one active Key scope
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum KeyScopePropagation {
    /// Continue resolving actions on ancestor Nodes
    #[default]
    Continue,
    /// Skip actions outside this scope while preserving raw event routing
    StopAtScope,
}

/// One active semantic scope and its immutable KeyMap layer
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyScope {
    id: NodeId,
    key_map: KeyMap,
    propagation: KeyScopePropagation,
}

impl KeyScope {
    /// Creates a scope identified by a stable semantic Node ID
    #[must_use]
    pub fn new(id: impl Into<NodeId>, key_map: KeyMap) -> Self {
        Self {
            id: id.into(),
            key_map,
            propagation: KeyScopePropagation::Continue,
        }
    }

    /// Replaces action propagation behavior for this scope
    #[must_use]
    pub const fn with_propagation(mut self, propagation: KeyScopePropagation) -> Self {
        self.propagation = propagation;
        self
    }

    /// Returns the semantic scope identity
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns this scope's immutable override layer
    #[must_use]
    pub const fn key_map(&self) -> &KeyMap {
        &self.key_map
    }

    /// Returns action propagation behavior after this scope is processed
    #[must_use]
    pub const fn propagation(&self) -> KeyScopePropagation {
        self.propagation
    }
}

/// One action after active KeyMap layers have been applied
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAction {
    id: ActionId,
    label: Arc<str>,
    bindings: Arc<[KeyBinding]>,
    availability: ActionAvailability,
    help_visible: bool,
}

impl ResolvedAction {
    pub(crate) fn from_descriptor_defaults(descriptor: &ActionDescriptor) -> Self {
        Self {
            id: descriptor.id.clone(),
            label: descriptor.label.clone(),
            bindings: descriptor.default_bindings.clone(),
            availability: descriptor.availability,
            help_visible: descriptor.help_visible,
        }
    }

    /// Returns the stable Action ID
    #[must_use]
    pub const fn id(&self) -> &ActionId {
        &self.id
    }

    /// Returns the user-facing label
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the ordered effective bindings
    #[must_use]
    pub fn bindings(&self) -> &[KeyBinding] {
        self.bindings.as_ref()
    }

    /// Returns current availability
    #[must_use]
    pub const fn availability(&self) -> ActionAvailability {
        self.availability
    }

    /// Reports whether Help projections normally include the action
    #[must_use]
    pub const fn is_help_visible(&self) -> bool {
        self.help_visible
    }
}

/// Ordered actions resolved for one semantic owner and active scope path
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedActions {
    owner: NodeId,
    scope_path: Arc<[NodeId]>,
    actions: Arc<[ResolvedAction]>,
}

impl ResolvedActions {
    pub(crate) fn from_shared_defaults(
        owner: &NodeId,
        scopes: &[KeyScope],
        actions: Arc<[ResolvedAction]>,
    ) -> Self {
        let scope_path = if scopes.is_empty() {
            Arc::clone(&EMPTY_SCOPE_PATH)
        } else {
            scopes.iter().map(|scope| scope.id.clone()).collect()
        };
        Self {
            owner: owner.clone(),
            scope_path,
            actions,
        }
    }

    /// Returns the semantic owner identity
    #[must_use]
    pub const fn owner(&self) -> &NodeId {
        &self.owner
    }

    /// Returns active scope IDs in root-to-target order
    #[must_use]
    pub fn scope_path(&self) -> &[NodeId] {
        self.scope_path.as_ref()
    }

    /// Returns actions in declaration order
    #[must_use]
    pub fn actions(&self) -> &[ResolvedAction] {
        self.actions.as_ref()
    }

    /// Iterates actions normally included in Help projections
    pub fn help_actions(&self) -> impl Iterator<Item = &ResolvedAction> {
        self.actions.iter().filter(|action| action.help_visible)
    }
}

/// The category of one action-group binding conflict
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BindingConflictKind {
    /// One action group declared the same Action ID more than once
    DuplicateAction,
    /// One resolved action contained the same stroke more than once
    DuplicateBinding,
    /// Two different active candidates contained the same stroke
    AmbiguousBinding,
}

/// A deterministic conflict while resolving one action owner
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingConflict {
    kind: BindingConflictKind,
    owner: NodeId,
    scope_path: Arc<[NodeId]>,
    actions: Arc<[ActionId]>,
    stroke: Option<KeyStroke>,
}

impl BindingConflict {
    fn new(
        kind: BindingConflictKind,
        owner: &NodeId,
        scope_path: &Arc<[NodeId]>,
        actions: impl IntoIterator<Item = ActionId>,
        stroke: Option<KeyStroke>,
    ) -> Self {
        Self {
            kind,
            owner: owner.clone(),
            scope_path: scope_path.clone(),
            actions: actions.into_iter().collect(),
            stroke,
        }
    }

    /// Returns the conflict category
    #[must_use]
    pub const fn kind(&self) -> BindingConflictKind {
        self.kind
    }

    /// Returns the semantic owner identity
    #[must_use]
    pub const fn owner(&self) -> &NodeId {
        &self.owner
    }

    /// Returns active scope IDs in root-to-target order
    #[must_use]
    pub fn scope_path(&self) -> &[NodeId] {
        self.scope_path.as_ref()
    }

    /// Returns involved Action IDs in declaration order
    #[must_use]
    pub fn actions(&self) -> &[ActionId] {
        self.actions.as_ref()
    }

    /// Returns the conflicting stroke when the category has one
    #[must_use]
    pub const fn stroke(&self) -> Option<KeyStroke> {
        self.stroke
    }
}

impl fmt::Display for BindingConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            BindingConflictKind::DuplicateAction => write!(
                formatter,
                "duplicate ActionId {} on NodeId {}",
                self.actions[0], self.owner
            ),
            BindingConflictKind::DuplicateBinding => write!(
                formatter,
                "duplicate key binding {} for ActionId {} on NodeId {}",
                self.stroke.expect("duplicate binding has a stroke"),
                self.actions[0],
                self.owner
            ),
            BindingConflictKind::AmbiguousBinding => write!(
                formatter,
                "ambiguous key binding {} for ActionIds {} and {} on NodeId {}",
                self.stroke.expect("ambiguous binding has a stroke"),
                self.actions[0],
                self.actions[1],
                self.owner
            ),
        }
    }
}

impl Error for BindingConflict {}

/// Applies active root-to-target KeyMap layers to one action owner
///
/// The resolver is pure and does not install actions into a Runtime. It
/// preserves action, binding, and scope order and returns the first conflict in
/// that stable order
pub fn resolve_actions(
    owner: &NodeId,
    actions: &[ActionDescriptor],
    scopes: &[KeyScope],
) -> Result<ResolvedActions, BindingConflict> {
    let scope_path: Arc<[NodeId]> = scopes.iter().map(|scope| scope.id.clone()).collect();
    let mut seen_actions = HashSet::with_capacity(actions.len());
    let mut active_bindings = HashMap::<KeyStroke, ActionId>::new();
    let mut resolved = Vec::with_capacity(actions.len());

    for action in actions {
        if !seen_actions.insert(action.id.clone()) {
            return Err(BindingConflict::new(
                BindingConflictKind::DuplicateAction,
                owner,
                &scope_path,
                [action.id.clone()],
                None,
            ));
        }

        let mut bindings = action.default_bindings.clone();
        for scope in scopes {
            if let Some(replacement) = scope.key_map.binding_storage(&action.id) {
                bindings = replacement.clone();
            }
        }

        let mut seen_strokes = HashSet::with_capacity(bindings.len());
        for binding in bindings.iter() {
            if !seen_strokes.insert(binding.stroke) {
                return Err(BindingConflict::new(
                    BindingConflictKind::DuplicateBinding,
                    owner,
                    &scope_path,
                    [action.id.clone()],
                    Some(binding.stroke),
                ));
            }
        }

        if action.availability.is_candidate() {
            for binding in bindings.iter() {
                if let Some(existing) = active_bindings.get(&binding.stroke) {
                    return Err(BindingConflict::new(
                        BindingConflictKind::AmbiguousBinding,
                        owner,
                        &scope_path,
                        [existing.clone(), action.id.clone()],
                        Some(binding.stroke),
                    ));
                }
                active_bindings.insert(binding.stroke, action.id.clone());
            }
        }

        resolved.push(ResolvedAction {
            id: action.id.clone(),
            label: action.label.clone(),
            bindings,
            availability: action.availability,
            help_visible: action.help_visible,
        });
    }

    Ok(ResolvedActions {
        owner: owner.clone(),
        scope_path,
        actions: resolved.into(),
    })
}

#[cfg(test)]
mod tests {
    use nagi_vt::{KeyEvent, KeyProtocol};

    use super::*;

    #[test]
    fn associated_text_protocol_and_support_do_not_change_matching() {
        let stroke = KeyStroke::character(
            'a',
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
        );
        let binding = KeyBinding::new(stroke).with_support(BindingSupport::Unsupported);
        let event = Event::Key(KeyEvent {
            code: KeyCode::Character('a'),
            modifiers: stroke.modifiers(),
            action: KeyAction::Press,
            text: Some("different".to_owned()),
            protocol: KeyProtocol::Unknown,
        });

        assert!(binding.matches(&event));
    }

    #[test]
    fn duplicate_override_does_not_mutate_the_original_map() {
        let original = KeyMap::new()
            .rebind(
                "app.submit",
                [KeyBinding::new(KeyStroke::new(
                    KeyCode::Enter,
                    Modifiers::NONE,
                ))],
            )
            .unwrap();
        let error = original
            .rebind("app.submit", std::iter::empty())
            .unwrap_err();

        assert_eq!(error.action_id().as_str(), "app.submit");
        assert_eq!(
            original
                .bindings(&ActionId::from("app.submit"))
                .unwrap()
                .len(),
            1
        );
    }
}
