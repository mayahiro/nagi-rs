//! Application, semantic view, interaction, effects, subscriptions, and
//! terminal runtime facade for Nagi TUI

#![deny(unsafe_code)]

mod ansi_text;
mod app;
mod clock;
mod effect;
mod identity;
mod input;
mod interaction;
mod layout;
mod node;
mod panel;
mod renderer;
mod rich_text;
mod routing;
mod runtime;
mod subscription;
mod subscription_supervisor;
mod supervisor;
mod terminal;
#[allow(dead_code, unsafe_code)]
mod terminal_unix;
mod text_edit;

#[cfg(test)]
mod fixture_support;

pub use ansi_text::AnsiTextOptions;
pub use app::{App, ViewContext};
pub use clock::{Clock, SystemClock, Timestamp, VirtualClock};
pub use effect::{CancelToken, Effect, ScopeId, Task, TaskKey};
pub use identity::NodeId;
pub use input::{EventAction, TimedInputDecoder};
pub use interaction::{InteractionState, ScrollAxis, ScrollOffset, ScrollState, TextInputState};
pub use layout::Length;
pub use nagi_surface::{Point, Rect, Size, Surface};
pub use nagi_vt::{
    Attributes, Capabilities, Color, CursorShape, EraseMode, Event, KeyAction, KeyCode, KeyEvent,
    KeyProtocol, Modifiers, MouseButton, MouseEvent, MouseKind, MouseTracking, SgrColor, SgrStyle,
    Style, TerminalOp, encode,
};
pub use node::{
    HorizontalAlignment, Insets, Node, ScrollViewportOptions, VerticalAlignment, VirtualFragment,
    VirtualViewport,
};
pub use panel::{BorderKind, PanelOptions, PanelStyle};
pub use rich_text::{ParagraphOptions, TextSpan, WrapMode};
pub use routing::{EventDispatch, EventResult};
pub use runtime::{
    DEFAULT_QUEUE_CAPACITY, DEFAULT_SUBSCRIPTION_CAPACITY, DEFAULT_TASK_LIMIT, Frame, QueueFull,
    Runtime, RuntimeConfig, RuntimeError, RuntimeEventError,
};
pub use subscription::{
    DeliveryPolicy, Subscription, SubscriptionClosed, SubscriptionKey, SubscriptionSink,
};
pub use subscription_supervisor::SubscriptionDiagnostics;
pub use supervisor::EffectDiagnostics;
pub use terminal::{RunError, TerminalOptions, run_terminal};

#[cfg(test)]
mod tests {
    #[test]
    fn uses_canonical_package_name() {
        assert_eq!(env!("CARGO_PKG_NAME"), "nagi-tui");
    }
}
