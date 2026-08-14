//! Inspect detected terminal capabilities and enhanced keyboard input

use nagi_tui::{
    App, Effect, Event, EventAction, KeyAction, KeyCode, Node, TerminalCapabilityDetection,
    TerminalOptions, ViewContext, run_terminal,
};

enum Message {
    Input(String),
    Quit,
}

struct CapabilityDemo {
    last_input: String,
    exiting: bool,
}

impl Default for CapabilityDemo {
    fn default() -> Self {
        Self {
            last_input: "None".to_owned(),
            exiting: false,
        }
    }
}

impl App for CapabilityDemo {
    type Message = Message;

    fn update(&mut self, message: Message) -> Effect<Message> {
        match message {
            Message::Input(input) => self.last_input = input,
            Message::Quit => {
                self.exiting = true;
                return Effect::exit();
            }
        }
        Effect::none()
    }

    fn view(&self, context: ViewContext) -> Node<Message> {
        let profile = context.terminal_capabilities;
        let status = if self.exiting { "Stopping" } else { "Running" };
        Node::panel(
            Node::column([
                Node::text(format!("Color: {:?}", profile.color_level())),
                Node::text(format!(
                    "NO_COLOR preference: {}",
                    profile.prefers_no_color()
                )),
                Node::text(format!("Hyperlinks: {:?}", profile.hyperlinks())),
                Node::text(format!("Clipboard: {:?}", profile.clipboard())),
                Node::text(format!(
                    "Extended keyboard: {:?}",
                    profile.extended_keyboard()
                )),
                Node::text(format!(
                    "Keyboard protocol: {:?}",
                    profile.keyboard_protocol()
                )),
                Node::text(format!("Last input: {}", self.last_input)),
                Node::text(format!("Status: {status}")),
                Node::text("Press Enter or Shift+Enter, Escape to exit"),
            ]),
            "Terminal capabilities",
        )
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    let options = TerminalOptions {
        capability_detection: TerminalCapabilityDetection::Enabled,
        ..TerminalOptions::default()
    };
    run_terminal(CapabilityDemo::default(), options, |event| match event {
        Event::Key(key)
            if key.code == KeyCode::Enter
                && key.modifiers.shift
                && key.action != KeyAction::Release =>
        {
            EventAction::Message(Message::Input(format!(
                "Shift+Enter via {:?}",
                key.protocol
            )))
        }
        Event::Key(key) if key.code == KeyCode::Enter && key.action != KeyAction::Release => {
            EventAction::Message(Message::Input(format!("Enter via {:?}", key.protocol)))
        }
        Event::Key(key) if key.code == KeyCode::Escape && key.action != KeyAction::Release => {
            EventAction::Message(Message::Quit)
        }
        _ => EventAction::Ignore,
    })?;
    Ok(())
}
