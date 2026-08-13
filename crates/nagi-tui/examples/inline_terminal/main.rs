//! A bounded main-screen TUI that leaves its final frame in terminal history

use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Node, TerminalOptions, TerminalViewport, ViewContext,
    run_terminal,
};

enum Message {
    Increment,
    Quit,
}

#[derive(Default)]
struct InlineApp {
    count: u64,
    exiting: bool,
}

impl App for InlineApp {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Increment => self.count = self.count.saturating_add(1),
            Message::Quit => {
                self.exiting = true;
                return Effect::exit();
            }
        }
        Effect::none()
    }

    fn view(&self, context: ViewContext) -> Node<Self::Message> {
        Node::column([
            Node::text(format!("Inline count: {}", self.count)),
            Node::text("Press Enter to increment"),
            Node::text(if self.exiting {
                "Stopped; this frame remains in terminal history"
            } else {
                "Press Escape to stop"
            }),
            Node::text(format!(
                "Viewport: {} x {}",
                context.size.width, context.size.height
            )),
        ])
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = TerminalOptions {
        viewport: TerminalViewport::inline(4)?,
        ..TerminalOptions::default()
    };
    run_terminal(InlineApp::default(), options, |event| match event {
        Event::Key(key) if key.code == KeyCode::Enter => EventAction::Message(Message::Increment),
        Event::Key(key) if key.code == KeyCode::Escape => EventAction::Message(Message::Quit),
        _ => EventAction::Ignore,
    })?;
    Ok(())
}
