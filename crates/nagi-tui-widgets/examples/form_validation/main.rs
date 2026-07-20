//! Controlled registration form with live validation

use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Length, MouseTracking, Node, Style, TerminalOptions,
    run_terminal,
};
use nagi_tui_widgets::{Button, Calendar, CalendarDate, Checkbox, Help, HelpBinding, Select};

enum Message {
    Name(String),
    Email(String),
    Role(usize),
    AcceptTerms(bool),
    Date(CalendarDate),
    Submit,
}

struct RegistrationForm {
    name: String,
    email: String,
    role: usize,
    accepted: bool,
    date: CalendarDate,
    submitted: bool,
}

impl Default for RegistrationForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            email: String::new(),
            role: 0,
            accepted: false,
            date: CalendarDate::new(2026, 7, 18),
            submitted: false,
        }
    }
}

impl App for RegistrationForm {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Name(value) => {
                self.name = value;
                self.submitted = false;
            }
            Message::Email(value) => {
                self.email = value;
                self.submitted = false;
            }
            Message::Role(index) => {
                self.role = index;
                self.submitted = false;
            }
            Message::AcceptTerms(value) => {
                self.accepted = value;
                self.submitted = false;
            }
            Message::Date(date) => {
                self.date = date;
                self.submitted = false;
            }
            Message::Submit => self.submitted = self.validation_errors().is_empty(),
        }
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let errors = self.validation_errors();
        let mut error_nodes = Vec::with_capacity(errors.len().max(1) + 1);
        if errors.is_empty() {
            error_nodes.push(Node::styled_text(
                "All fields are valid",
                Style {
                    bold: true,
                    ..Style::default()
                },
            ));
        } else {
            error_nodes.extend(errors.iter().map(|error| Node::text(format!("- {error}"))));
        }
        if self.submitted {
            error_nodes.push(Node::styled_text(
                "Registration submitted",
                Style {
                    bold: true,
                    ..Style::default()
                },
            ));
        }

        let fields = Node::column([
            field_row(
                "Name",
                Node::text_input_styled(
                    "name",
                    self.name.clone(),
                    "Ada Lovelace",
                    Style::default(),
                    Style {
                        dim: true,
                        ..Style::default()
                    },
                    Message::Name,
                ),
            ),
            field_row(
                "Email",
                Node::text_input_styled(
                    "email",
                    self.email.clone(),
                    "ada@example.com",
                    Style::default(),
                    Style {
                        dim: true,
                        ..Style::default()
                    },
                    Message::Email,
                ),
            ),
            Node::row([
                Node::text("Role: ").with_length(Length::Fixed(10)),
                Select::new(
                    "role",
                    ["Viewer", "Operator", "Administrator"],
                    self.role,
                    Message::Role,
                )
                .into_node(),
            ])
            .with_length(Length::Fixed(1)),
            Checkbox::new(
                "terms",
                "I accept the usage policy",
                self.accepted,
                Message::AcceptTerms,
            )
            .into_node()
            .with_length(Length::Fixed(1)),
            Node::gap(1),
            Button::new("submit", "Submit", || Message::Submit)
                .enabled(errors.is_empty())
                .into_node(),
        ]);
        let calendar = Calendar::new(
            "start-date",
            self.date.year,
            i32::from(self.date.month),
            self.date,
            Message::Date,
        )
        .show_adjacent(true)
        .into_node();

        Node::panel(
            Node::column([
                Node::row([
                    Node::panel(fields, "Account").with_length(Length::Flex(1)),
                    Node::panel(
                        Node::column([
                            calendar,
                            Node::text(format!(
                                "Selected: {:04}-{:02}-{:02}",
                                self.date.year, self.date.month, self.date.day
                            )),
                        ]),
                        "Start date",
                    )
                    .with_length(Length::Fixed(27)),
                ])
                .with_length(Length::Flex(1)),
                Node::panel(Node::column(error_nodes), "Validation").with_length(Length::Fixed(6)),
                Help::new([
                    HelpBinding::new("Tab", "next field"),
                    HelpBinding::new("Shift-Tab", "previous field"),
                    HelpBinding::new("Arrows", "select"),
                    HelpBinding::new("Enter/Space", "activate"),
                    HelpBinding::new("Esc", "exit"),
                ])
                .into_node()
                .with_length(Length::Fixed(1)),
            ]),
            "Registration form",
        )
    }
}

fn field_row(label: &str, input: Node<Message>) -> Node<Message> {
    Node::row([
        Node::text(format!("{label}: ")).with_length(Length::Fixed(10)),
        input.with_length(Length::Flex(1)),
    ])
    .with_length(Length::Fixed(1))
}

impl RegistrationForm {
    fn validation_errors(&self) -> Vec<&'static str> {
        let mut errors = Vec::with_capacity(3);
        if self.name.trim().is_empty() {
            errors.push("Name is required");
        }
        if !valid_email(&self.email) {
            errors.push("Email must contain a local part, @, and domain");
        }
        if !self.accepted {
            errors.push("Usage policy acceptance is required");
        }
        errors
    }
}

fn valid_email(value: &str) -> bool {
    let value = value.trim();
    value.find('@').is_some_and(|at| {
        at > 0 && at < value.len().saturating_sub(1) && value[at + 1..].contains('.')
    })
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        RegistrationForm::default(),
        TerminalOptions {
            mouse_tracking: Some(MouseTracking::Press),
            focus_first: true,
            ..TerminalOptions::default()
        },
        |event| match event {
            Event::Key(key) if key.code == KeyCode::Escape => EventAction::Exit,
            Event::Key(key) if key.modifiers.control && key.code == KeyCode::Character('c') => {
                EventAction::Exit
            }
            _ => EventAction::Ignore,
        },
    )?;
    Ok(())
}
