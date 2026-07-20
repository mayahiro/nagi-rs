//! Asynchronous search with latest-result supervision

use std::thread;
use std::time::Duration;

use nagi_tui::{
    App, CancelToken, Effect, Event, EventAction, Length, MouseTracking, Node, Style,
    TerminalOptions, run_terminal,
};

const SEARCH_DELAY: Duration = Duration::from_millis(150);
const SEARCH_POLL_INTERVAL: Duration = Duration::from_millis(5);
const ITEMS: &[&str] = &[
    "Application runtime",
    "Cell surface",
    "Effect supervision",
    "Grapheme-aware text",
    "Interaction state",
    "Unix terminal session",
    "VT codec",
];

enum Message {
    QueryChanged(String),
    SearchFinished {
        query: String,
        results: Vec<&'static str>,
    },
}

#[derive(Default)]
struct AsyncSearch {
    query: String,
    results: Vec<&'static str>,
    searching: bool,
}

impl AsyncSearch {
    fn search(query: String) -> Effect<Message> {
        Effect::latest("search", move |cancel| {
            cooperative_delay(&cancel, SEARCH_DELAY);
            let normalized = query.to_ascii_lowercase();
            let results = ITEMS
                .iter()
                .copied()
                .filter(|item| item.to_ascii_lowercase().contains(&normalized))
                .collect();
            Message::SearchFinished { query, results }
        })
    }
}

impl App for AsyncSearch {
    type Message = Message;

    fn init(&mut self) -> Effect<Self::Message> {
        self.searching = true;
        Self::search(String::new())
    }

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::QueryChanged(query) => {
                self.query = query.clone();
                self.searching = true;
                Self::search(query)
            }
            Message::SearchFinished { query, results } => {
                self.query = query;
                self.results = results;
                self.searching = false;
                Effect::none()
            }
        }
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let status = if self.searching {
            "Searching..."
        } else if self.results.is_empty() {
            "No matches"
        } else {
            "Results"
        };
        let mut results: Vec<_> = self
            .results
            .iter()
            .map(|item| Node::text(format!("  {item}")))
            .collect();
        if results.is_empty() {
            results.push(Node::text("  --"));
        }

        Node::border(
            Node::column([
                Node::styled_text(
                    "Async Search",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                )
                .with_length(Length::Fixed(1)),
                Node::text_input("search-input", &self.query, Message::QueryChanged)
                    .with_length(Length::Fixed(1)),
                Node::text(status).with_length(Length::Fixed(1)),
                Node::column(results).with_length(Length::Flex(1)),
                Node::text("Type to search, click or Tab to restore focus, Esc to exit")
                    .with_length(Length::Fixed(1)),
            ]),
            Style::default(),
        )
    }
}

fn cooperative_delay(cancel: &CancelToken, delay: Duration) {
    let mut remaining = delay;
    while !cancel.is_cancelled() && !remaining.is_zero() {
        let interval = remaining.min(SEARCH_POLL_INTERVAL);
        thread::sleep(interval);
        remaining = remaining.saturating_sub(interval);
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    let options = TerminalOptions {
        mouse_tracking: Some(MouseTracking::Press),
        focus_first: true,
        ..TerminalOptions::default()
    };
    run_terminal(AsyncSearch::default(), options, |event| match event {
        Event::Key(key) if key.code == nagi_tui::KeyCode::Escape => EventAction::Exit,
        Event::Key(key)
            if key.modifiers.control && key.code == nagi_tui::KeyCode::Character('c') =>
        {
            EventAction::Exit
        }
        _ => EventAction::Ignore,
    })?;
    Ok(())
}
