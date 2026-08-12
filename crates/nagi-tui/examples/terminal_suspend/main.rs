//! Full-screen terminal suspension around an application-owned child process

use std::process::Command;

use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Node, TerminalOptions, ViewContext, run_terminal,
};

enum Message {
    OpenShell,
    ShellReturned(String),
    Quit,
}

struct TerminalSuspendApp {
    status: String,
    exiting: bool,
}

impl Default for TerminalSuspendApp {
    fn default() -> Self {
        Self {
            status: "Shell has not run".to_owned(),
            exiting: false,
        }
    }
}

impl App for TerminalSuspendApp {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::OpenShell => {
                self.status = "Opening the application-owned shell".to_owned();
                return Effect::suspend_terminal(|_| {
                    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
                    let status = Command::new(shell).status();
                    let summary = match status {
                        Ok(status) => format!("Shell returned with {status}"),
                        Err(error) => format!("Shell failed: {error}"),
                    };
                    Message::ShellReturned(summary)
                });
            }
            Message::ShellReturned(status) => self.status = status,
            Message::Quit => {
                self.exiting = true;
                return Effect::exit();
            }
        }
        Effect::none()
    }

    fn view(&self, _context: ViewContext) -> Node<Self::Message> {
        let lifecycle = if self.exiting { "Stopping" } else { "Running" };
        Node::panel(
            Node::column([
                Node::text(format!("Status: {}", self.status)),
                Node::text(format!("Lifecycle: {lifecycle}")),
                Node::text("Press S to open $SHELL, then exit the shell to resume"),
                Node::text("Press Escape to exit"),
            ]),
            "Terminal suspend / resume",
        )
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        TerminalSuspendApp::default(),
        TerminalOptions::default(),
        |event| match event {
            Event::Text(text) if matches!(text.as_str(), "s" | "S") => {
                EventAction::Message(Message::OpenShell)
            }
            Event::Key(key) if key.code == KeyCode::Escape => EventAction::Message(Message::Quit),
            _ => EventAction::Ignore,
        },
    )?;
    Ok(())
}
