//! Minimal stateful Nagi TUI application

use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Node, TerminalOptions, ViewContext, run_terminal,
};

enum Message {
    Increment,
    Quit,
}

#[derive(Default)]
struct Counter {
    count: u64,
    exiting: bool,
}

impl App for Counter {
    type Message = Message;

    fn update(&mut self, message: Message) -> Effect<Message> {
        match message {
            Message::Increment => self.count += 1,
            Message::Quit => {
                self.exiting = true;
                return Effect::exit();
            }
        }
        Effect::none()
    }

    fn view(&self, _context: ViewContext) -> Node<Message> {
        let status = if self.exiting { "Stopping" } else { "Running" };
        Node::panel(
            Node::column([
                Node::text(format!("Count: {}", self.count)),
                Node::text(format!("Status: {status}")),
                Node::text("Press Enter to increment, Escape to exit"),
            ]),
            "Counter",
        )
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        Counter::default(),
        TerminalOptions::default(),
        |event| match event {
            Event::Key(key) if key.code == KeyCode::Enter => {
                EventAction::Message(Message::Increment)
            }
            Event::Key(key) if key.code == KeyCode::Escape => EventAction::Message(Message::Quit),
            _ => EventAction::Ignore,
        },
    )?;
    Ok(())
}
