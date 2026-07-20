use crate::{Effect, Node, Size, Subscription};

/// Environment information available while rebuilding an application view
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewContext {
    /// Current terminal size in cells
    pub size: Size,
}

impl ViewContext {
    /// Creates view environment information for a terminal size
    #[must_use]
    pub const fn new(size: Size) -> Self {
        Self { size }
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
