//! Interactive gallery for standard Nagi TUI widgets

use std::time::Duration;

use nagi_tui::{
    App, DeliveryPolicy, Effect, Event, EventAction, KeyCode, Length, MouseTracking, Node, Style,
    Subscription, TerminalOptions, run_terminal,
};
use nagi_tui_widgets::{Button, List, ListItem, Modal, Progress, Spinner};

const SPINNER_INTERVAL: Duration = Duration::from_millis(80);

enum Message {
    Select(usize),
    Advance,
    Tick,
    OpenModal,
    CloseModal,
}

#[derive(Default)]
struct Gallery {
    selected: usize,
    progress: u64,
    tick: u64,
    modal: bool,
}

impl App for Gallery {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Select(selected) => self.selected = selected,
            Message::Advance => self.progress = (self.progress + 1) % 11,
            Message::Tick => self.tick = self.tick.wrapping_add(1),
            Message::OpenModal => self.modal = true,
            Message::CloseModal => self.modal = false,
        }
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::every(
            "gallery-spinner",
            SPINNER_INTERVAL,
            DeliveryPolicy::latest(),
            || Message::Tick,
        )
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let content = Node::border(
            Node::column([
                Node::styled_text(
                    "Standard Widget Gallery",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                )
                .with_length(Length::Fixed(1)),
                List::new(
                    "gallery-list",
                    [
                        ListItem::new("list-alpha", "Alpha"),
                        ListItem::new("list-beta", "Beta"),
                        ListItem::new("list-gamma", "Gamma"),
                    ],
                    self.selected,
                    Message::Select,
                )
                .into_node(),
                Progress::<Message>::new(self.progress, 10, 20)
                    .into_node()
                    .with_length(Length::Fixed(1)),
                Spinner::<Message>::new(self.tick)
                    .label("Clock-driven spinner")
                    .into_node()
                    .with_length(Length::Fixed(1)),
                Node::row([
                    Button::new("advance", "Advance", || Message::Advance).into_node(),
                    Node::text(" "),
                    Button::new("open-modal", "Open modal", || Message::OpenModal).into_node(),
                ])
                .with_length(Length::Fixed(1)),
                Node::text("Tab/Shift-Tab focus, arrows select, Enter/Space activate, q exits")
                    .with_length(Length::Fixed(1)),
            ]),
            Style::default(),
        );
        if !self.modal {
            return content;
        }
        Node::stack([
            content,
            Modal::new(
                "gallery-modal",
                Node::column([
                    Node::text("This panel owns focus and routing"),
                    Button::new("close-modal", "Close", || Message::CloseModal).into_node(),
                ]),
            )
            .title("Modal")
            .on_escape(|| Message::CloseModal)
            .into_node(),
        ])
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    let options = TerminalOptions {
        mouse_tracking: Some(MouseTracking::Press),
        focus_first: true,
        ..TerminalOptions::default()
    };
    run_terminal(Gallery::default(), options, |event| match event {
        Event::Text(text) if text == "q" => EventAction::Exit,
        Event::Key(key) if key.code == KeyCode::Escape => EventAction::Exit,
        Event::Key(key) if key.modifiers.control && key.code == KeyCode::Character('c') => {
            EventAction::Exit
        }
        _ => EventAction::Ignore,
    })?;
    Ok(())
}
