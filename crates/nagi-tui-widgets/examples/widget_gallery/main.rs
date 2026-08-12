//! Interactive gallery for standard Nagi TUI widgets

use std::time::Duration;

use nagi_tui::{
    App, DeliveryPolicy, Effect, Event, EventAction, KeyCode, Length, MouseTracking, Node, Style,
    Subscription, TerminalOptions, run_terminal,
};
use nagi_tui_widgets::{
    Button, ConfirmDialog, ConfirmDialogDefault, DialogAction, Disclosure, List, ListItem,
    Progress, Spinner,
};

const SPINNER_INTERVAL: Duration = Duration::from_millis(80);

enum Message {
    Select(usize),
    Advance,
    Tick,
    OpenModal,
    ConfirmModal,
    CloseModal,
    ToggleModalDetails(bool),
}

#[derive(Default)]
struct Gallery {
    selected: usize,
    progress: u64,
    tick: u64,
    modal: bool,
    modal_details: bool,
}

impl App for Gallery {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Select(selected) => self.selected = selected,
            Message::Advance => self.progress = (self.progress + 1) % 11,
            Message::Tick => self.tick = self.tick.wrapping_add(1),
            Message::OpenModal => {
                self.modal = true;
                self.modal_details = false;
            }
            Message::ConfirmModal => {
                self.progress = (self.progress + 1) % 11;
                self.modal = false;
            }
            Message::CloseModal => self.modal = false,
            Message::ToggleModalDetails(expanded) => self.modal_details = expanded,
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

    fn view(&self, context: nagi_tui::ViewContext) -> Node<Self::Message> {
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
                    Button::new("open-modal", "Open confirm", || Message::OpenModal).into_node(),
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
        let details = Disclosure::new(
            "confirm-details",
            Node::text("What changes?"),
            self.modal_details,
            Message::ToggleModalDetails,
        )
        .body(|| Node::text("Confirming advances progress by one step"));
        let dialog = ConfirmDialog::new(
            "gallery-dialog",
            Node::text("Advance the progress indicator?"),
            DialogAction::new("confirm-advance", "Advance", || Message::ConfirmModal),
            DialogAction::new("cancel-advance", "Cancel", || Message::CloseModal),
            ConfirmDialogDefault::Cancel,
        )
        .title(Node::styled_text(
            "Confirm action",
            Style {
                bold: true,
                ..Style::default()
            },
        ))
        .details(details)
        .width_profile(context.width_profile)
        .action_wrap_width(context.size.width.saturating_sub(4).max(1))
        .into_node();
        Node::stack([content, dialog])
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
