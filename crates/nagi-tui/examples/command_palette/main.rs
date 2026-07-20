//! Searchable command palette using the application runtime

use nagi_text::previous_grapheme_boundary;
use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Length, Node, Style, TerminalOptions, run_terminal,
};

const COMMANDS: &[&str] = &[
    "Open file",
    "Save file",
    "Close editor",
    "Toggle sidebar",
    "Show shortcuts",
];

enum Message {
    Insert(String),
    Backspace,
    MoveUp,
    MoveDown,
}

#[derive(Default)]
struct CommandPalette {
    query: String,
    selected: usize,
}

impl CommandPalette {
    fn filtered(&self) -> Vec<&'static str> {
        let query = self.query.to_ascii_lowercase();
        COMMANDS
            .iter()
            .copied()
            .filter(|command| command.to_ascii_lowercase().contains(&query))
            .collect()
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.filtered().len().saturating_sub(1));
    }
}

impl App for CommandPalette {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Insert(text) => {
                self.query.push_str(&text);
                self.clamp_selection();
            }
            Message::Backspace => {
                if let Some(boundary) = previous_grapheme_boundary(&self.query, self.query.len()) {
                    self.query.truncate(boundary);
                    self.clamp_selection();
                }
            }
            Message::MoveUp => self.selected = self.selected.saturating_sub(1),
            Message::MoveDown => {
                self.selected = (self.selected + 1).min(self.filtered().len().saturating_sub(1));
            }
        }
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let mut commands = Vec::new();
        for (index, command) in self.filtered().into_iter().enumerate() {
            let marker = if index == self.selected { "> " } else { "  " };
            commands.push(
                Node::styled_text(
                    format!("{marker}{command}"),
                    Style {
                        reverse: index == self.selected,
                        ..Style::default()
                    },
                )
                .with_length(Length::Fixed(1)),
            );
        }
        if commands.is_empty() {
            commands.push(Node::text("  No matching commands"));
        }

        Node::border(
            Node::column([
                Node::styled_text(
                    "Command Palette",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                )
                .with_length(Length::Fixed(1)),
                Node::text(format!("> {}", self.query)).with_length(Length::Fixed(1)),
                Node::column(commands).with_length(Length::Flex(1)),
                Node::text("Type to filter, arrows to move, Enter/Esc to close")
                    .with_length(Length::Fixed(1)),
            ]),
            Style::default(),
        )
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        CommandPalette::default(),
        TerminalOptions::default(),
        |event| match event {
            Event::Text(text) => EventAction::Message(Message::Insert(text)),
            Event::Paste(text) => EventAction::Message(Message::Insert(text)),
            Event::Key(key) if key.code == KeyCode::Backspace => {
                EventAction::Message(Message::Backspace)
            }
            Event::Key(key) if key.code == KeyCode::Up => EventAction::Message(Message::MoveUp),
            Event::Key(key) if key.code == KeyCode::Down => EventAction::Message(Message::MoveDown),
            Event::Key(key) if matches!(key.code, KeyCode::Enter | KeyCode::Escape) => {
                EventAction::Exit
            }
            Event::Key(key) if key.modifiers.control && key.code == KeyCode::Character('c') => {
                EventAction::Exit
            }
            _ => EventAction::Ignore,
        },
    )?;
    Ok(())
}
