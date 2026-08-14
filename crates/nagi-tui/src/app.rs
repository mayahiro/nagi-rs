use nagi_text::WidthProfile;

use crate::{Effect, Node, Size, Subscription, TerminalCapabilityProfile};

/// Environment information available while rebuilding an application view
#[derive(Clone, Copy, Debug)]
pub struct ViewContext {
    /// Current terminal size in cells
    pub size: Size,
    /// Runtime terminal cell-width policy
    pub width_profile: WidthProfile<'static>,
    /// Detected terminal features and active keyboard protocol
    pub terminal_capabilities: TerminalCapabilityProfile,
}

impl ViewContext {
    /// Creates view environment information for a terminal size
    #[must_use]
    pub const fn new(size: Size) -> Self {
        Self {
            size,
            width_profile: WidthProfile::MODERN,
            terminal_capabilities: TerminalCapabilityProfile::UNKNOWN,
        }
    }

    /// Creates view environment information with an explicit width profile
    #[must_use]
    pub const fn with_width_profile(size: Size, width_profile: WidthProfile<'static>) -> Self {
        Self {
            size,
            width_profile,
            terminal_capabilities: TerminalCapabilityProfile::UNKNOWN,
        }
    }

    /// Creates view environment information with complete terminal context
    #[must_use]
    pub const fn with_terminal_capabilities(
        size: Size,
        width_profile: WidthProfile<'static>,
        terminal_capabilities: TerminalCapabilityProfile,
    ) -> Self {
        Self {
            size,
            width_profile,
            terminal_capabilities,
        }
    }
}

/// One application whose state is updated by sequential messages
pub trait App {
    /// A message that can update application state
    type Message: Send + 'static;

    /// Initializes application state and returns startup work
    fn init(&mut self) -> Effect<Self::Message> {
        Effect::none()
    }

    /// Applies one message and returns follow-up work
    fn update(&mut self, message: Self::Message) -> Effect<Self::Message>;

    /// Describes long-lived message sources for the current state
    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    /// Rebuilds the semantic view for the current state
    fn view(&self, context: ViewContext) -> Node<Self::Message>;
}
