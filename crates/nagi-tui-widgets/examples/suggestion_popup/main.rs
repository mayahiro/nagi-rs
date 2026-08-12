//! Application-owned asynchronous candidates shown by SuggestionPopup

use std::thread;
use std::time::Duration;

use nagi_tui::{
    App, CancelToken, Effect, Event, EventAction, Length, MouseTracking, Node, Style,
    TerminalOptions, run_terminal,
};
use nagi_tui_widgets::{
    Composer, ComposerState, SuggestionId, SuggestionItems, SuggestionPopup, SuggestionPopupStatus,
    SuggestionRowContext,
};

const SEARCH_DELAY: Duration = Duration::from_millis(120);
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
    Edit(ComposerState),
    SearchFinished {
        query: String,
        candidates: SuggestionItems,
    },
    Select(SuggestionId),
    Accept(SuggestionId),
    Dismiss,
    Submit,
}

#[derive(Default)]
struct SuggestionExample {
    composer: ComposerState,
    candidates: SuggestionItems,
    selected: Option<SuggestionId>,
    searching: bool,
    popup_open: bool,
    accepted: Option<String>,
}

impl SuggestionExample {
    fn search(query: String) -> Effect<Message> {
        Effect::latest("suggestions", move |cancel| {
            cooperative_delay(&cancel, SEARCH_DELAY);
            let normalized = query.to_ascii_lowercase();
            let candidates = SuggestionItems::new(
                ITEMS
                    .iter()
                    .copied()
                    .filter(|item| item.to_ascii_lowercase().contains(&normalized))
                    .map(SuggestionId::new),
            )
            .expect("the static catalog contains unique values");
            Message::SearchFinished { query, candidates }
        })
    }
}

impl App for SuggestionExample {
    type Message = Message;

    fn init(&mut self) -> Effect<Self::Message> {
        self.searching = true;
        self.popup_open = true;
        Self::search(String::new())
    }

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Edit(state) => {
                let query = state.text_area().value().to_owned();
                self.composer = state;
                self.selected = None;
                self.searching = true;
                self.popup_open = true;
                Self::search(query)
            }
            Message::SearchFinished { query, candidates } => {
                debug_assert_eq!(query, self.composer.text_area().value());
                self.candidates = candidates;
                self.searching = false;
                Effect::none()
            }
            Message::Select(id) => {
                self.selected = Some(id);
                Effect::none()
            }
            Message::Accept(id) => {
                self.composer = ComposerState::at_end(id.as_str());
                self.accepted = Some(id.as_str().to_owned());
                self.popup_open = false;
                Effect::none()
            }
            Message::Dismiss => {
                self.popup_open = false;
                Effect::none()
            }
            Message::Submit => {
                self.accepted = Some(self.composer.text_area().value().to_owned());
                self.popup_open = false;
                Effect::none()
            }
        }
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let composer = Composer::new(
            "composer",
            "composer-viewport",
            "composer-caret",
            self.composer.clone(),
            Message::Edit,
            || Message::Submit,
        )
        .rows(1, 3)
        .into_node();
        let status = if self.searching {
            SuggestionPopupStatus::Loading
        } else {
            SuggestionPopupStatus::Ready
        };
        let suggestions = SuggestionPopup::new(
            "suggestions",
            composer,
            "composer-caret",
            "composer",
            self.candidates.clone(),
            self.selected.clone(),
            |context: SuggestionRowContext| {
                let prefix = if context.is_selected() { "> " } else { "  " };
                Node::text(format!("{prefix}{}", context.id().as_str()))
            },
            Message::Select,
            Message::Accept,
            || Message::Dismiss,
        )
        .status(status)
        .open(self.popup_open)
        .visible_rows(5)
        .into_node()
        .with_length(Length::Flex(1));

        let accepted = self.accepted.as_deref().map_or_else(
            || "Accepted: --".to_owned(),
            |value| format!("Accepted: {value}"),
        );
        Node::border(
            Node::column([
                Node::styled_text(
                    "SuggestionPopup",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                )
                .with_length(Length::Fixed(1)),
                suggestions,
                Node::text(accepted).with_length(Length::Fixed(1)),
                Node::text("Type to search, arrows select, Enter accepts, Esc closes")
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
    run_terminal(SuggestionExample::default(), options, |event| match event {
        Event::Key(key)
            if key.modifiers.control && key.code == nagi_tui::KeyCode::Character('c') =>
        {
            EventAction::Exit
        }
        _ => EventAction::Ignore,
    })?;
    Ok(())
}
