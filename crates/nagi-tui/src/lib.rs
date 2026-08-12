//! Terminal Presentation Rules, bounded Content-to-Node projection,
//! application, semantic view, scoped key maps, interaction, effects,
//! subscriptions, and terminal runtime facade for Nagi TUI

#![deny(unsafe_code)]

mod action_routing;
mod ansi_text;
mod app;
mod clock;
mod content_projection;
mod core_action;
mod effect;
mod identity;
mod input;
mod interaction;
mod keymap;
mod layout;
mod node;
mod panel;
mod presentation;
mod renderer;
mod rich_text;
mod routing;
mod runtime;
mod runtime_notice;
mod subscription;
mod subscription_supervisor;
mod supervisor;
mod terminal;
#[allow(dead_code, unsafe_code)]
mod terminal_unix;
mod text_edit;
mod virtual_flow;
mod wake;

#[cfg(test)]
mod fixture_support;

pub use ansi_text::AnsiTextOptions;
pub use app::{App, ViewContext};
pub use clock::{Clock, SystemClock, Timestamp, VirtualClock};
pub use content_projection::{
    ContentProjectionError, ContentProjectionErrorKind, ContentProjectionLimits,
    ContentProjectionOptions, DEFAULT_CONTENT_PROJECTION_MAX_CONTENT_NODES,
    DEFAULT_CONTENT_PROJECTION_MAX_DEPTH, DEFAULT_CONTENT_PROJECTION_MAX_OUTPUT_NODES,
    DEFAULT_CONTENT_PROJECTION_MAX_SPANS, DEFAULT_CONTENT_PROJECTION_MAX_VISUAL_BYTES,
    MAX_CONTENT_PROJECTION_DEPTH, project_content, project_content_with_states,
};
pub use effect::{CancelToken, ClipboardRequest, Effect, ScopeId, Task, TaskKey};
pub use identity::NodeId;
pub use input::{EventAction, TimedInputDecoder};
pub use interaction::{InteractionState, ScrollAxis, ScrollOffset, ScrollState, TextInputState};
pub use keymap::{
    Action, ActionAvailability, ActionDescriptor, ActionEvent, ActionId, BindingConflict,
    BindingConflictKind, BindingSupport, FOCUS_NEXT_ACTION_ID, FOCUS_PREVIOUS_ACTION_ID,
    KeyBinding, KeyMap, KeyMapError, KeyScope, KeyScopePropagation, KeyStroke, RepeatPolicy,
    ResolvedAction, ResolvedActions, SCROLL_END_ACTION_ID, SCROLL_PAGE_DOWN_ACTION_ID,
    SCROLL_PAGE_UP_ACTION_ID, SCROLL_START_ACTION_ID, TEXT_COPY_DOCUMENT_ACTION_ID,
    TEXT_COPY_SELECTION_ACTION_ID, TEXT_CURSOR_DOCUMENT_END_ACTION_ID,
    TEXT_CURSOR_DOCUMENT_START_ACTION_ID, TEXT_CURSOR_DOWN_ACTION_ID, TEXT_CURSOR_LEFT_ACTION_ID,
    TEXT_CURSOR_LINE_END_ACTION_ID, TEXT_CURSOR_LINE_START_ACTION_ID, TEXT_CURSOR_RIGHT_ACTION_ID,
    TEXT_CURSOR_UP_ACTION_ID, TEXT_CURSOR_WORD_LEFT_ACTION_ID, TEXT_CURSOR_WORD_RIGHT_ACTION_ID,
    TEXT_DELETE_BACKWARD_ACTION_ID, TEXT_DELETE_FORWARD_ACTION_ID,
    TEXT_INSERT_LINE_BREAK_ACTION_ID, TEXT_REDO_ACTION_ID, TEXT_SELECT_ALL_ACTION_ID,
    TEXT_SELECTION_EXTEND_DOCUMENT_END_ACTION_ID, TEXT_SELECTION_EXTEND_DOCUMENT_START_ACTION_ID,
    TEXT_SELECTION_EXTEND_DOWN_ACTION_ID, TEXT_SELECTION_EXTEND_LEFT_ACTION_ID,
    TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID, TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID,
    TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID, TEXT_SELECTION_EXTEND_UP_ACTION_ID,
    TEXT_SELECTION_EXTEND_WORD_LEFT_ACTION_ID, TEXT_SELECTION_EXTEND_WORD_RIGHT_ACTION_ID,
    TEXT_UNDO_ACTION_ID, resolve_actions,
};
pub use layout::Length;
pub use nagi_surface::{Point, Rect, Size, Surface};
pub use nagi_vt::{
    Attributes, Capabilities, Color, CursorShape, EraseMode, Event, KeyAction, KeyCode, KeyEvent,
    KeyProtocol, Modifiers, MouseButton, MouseEvent, MouseKind, MouseTracking, SgrColor, SgrStyle,
    Style, TerminalOp, encode,
};
pub use node::{
    HorizontalAlignment, Insets, ModalFocusOptions, ModalInitialFocus, ModalReturnFocus, Node,
    ScrollViewportOptions, VerticalAlignment, VirtualFragment, VirtualViewport,
};
pub use panel::{BorderKind, PanelOptions, PanelStyle};
pub use presentation::{
    ComputedPresentation, DeclarationValue, DuplicatePresentationState, PresentationDeclaration,
    PresentationDisplay, PresentationRule, PresentationSelector, PresentationSheet,
    PresentationState, TextStyleDeclaration,
};
pub use rich_text::{ParagraphOptions, TextSpan, WrapMode};
pub use routing::{EventDispatch, EventResult, PointerEventContext, PointerViewport, TextHit};
pub use runtime::{
    DEFAULT_QUEUE_CAPACITY, DEFAULT_RUNTIME_NOTICE_CAPACITY, DEFAULT_SUBSCRIPTION_CAPACITY,
    DEFAULT_TASK_LIMIT, Frame, QueueFull, Runtime, RuntimeConfig, RuntimeError, RuntimeEventError,
};
pub use runtime_notice::{RuntimeNotice, RuntimeNoticeDiagnostics, RuntimeNoticeKind};
pub use subscription::{
    DeliveryPolicy, Subscription, SubscriptionClosed, SubscriptionKey, SubscriptionSink,
};
pub use subscription_supervisor::SubscriptionDiagnostics;
pub use supervisor::EffectDiagnostics;
pub use terminal::{
    RunError, TerminalClipboard, TerminalOptions, run_terminal, run_terminal_with_notice_handler,
};
pub use virtual_flow::{
    DuplicateVirtualFlowItemKey, VirtualFlowAnchor, VirtualFlowAnchorAffinity, VirtualFlowItem,
    VirtualFlowItemContext, VirtualFlowItems, VirtualFlowOptions, VirtualFlowSource,
    VirtualFlowState, VirtualFlowUpdate,
};

#[cfg(test)]
mod tests {
    #[test]
    fn uses_canonical_package_name() {
        assert_eq!(env!("CARGO_PKG_NAME"), "nagi-tui");
    }
}
