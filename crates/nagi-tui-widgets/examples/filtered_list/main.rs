//! Bubbles-style filtered and paginated controlled list

use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Length, Node, Style, TerminalOptions, run_terminal,
};
use nagi_tui_widgets::{Help, HelpBinding, List, ListItem, Paginator};

const PAGE_SIZE: usize = 6;
const PACKAGES: &[&str] = &[
    "calendar",
    "chart",
    "command palette",
    "file picker",
    "help",
    "list",
    "modal",
    "paginator",
    "progress",
    "scrollbar",
    "select",
    "sparkline",
    "table",
    "text area",
    "tree",
];

enum Message {
    Query(String),
    Select(usize),
    Page(usize),
}

#[derive(Default)]
struct FilteredList {
    query: String,
    selected: usize,
    page: usize,
}

impl App for FilteredList {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Query(query) => {
                self.query = query;
                self.page = 0;
                if let Some(first) = matching_indices(&self.query).first() {
                    self.selected = *first;
                }
            }
            Message::Select(index) => self.selected = index,
            Message::Page(page) => {
                self.page = page;
                if let Some(first) = matching_indices(&self.query).get(page * PAGE_SIZE) {
                    self.selected = *first;
                }
            }
        }
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let items = PACKAGES
            .iter()
            .enumerate()
            .map(|(index, name)| ListItem::new(format!("package-{index}"), *name));
        let matches = matching_indices(&self.query);
        let total_pages = matches.len().div_ceil(PAGE_SIZE);
        let results = if matches.is_empty() {
            Node::text("No matching widgets").with_length(Length::Fixed(PAGE_SIZE as u32))
        } else {
            List::new("results", items, self.selected, Message::Select)
                .filter(self.query.clone())
                .paginate(self.page, PAGE_SIZE)
                .viewport("results-viewport", Length::Fixed(PAGE_SIZE as u32))
                .into_node()
        };
        let selection = matches
            .is_empty()
            .then_some("None")
            .or_else(|| PACKAGES.get(self.selected).copied())
            .unwrap_or("None");
        let filter = Node::row([
            Node::text("Filter: ").with_length(Length::Fixed(8)),
            Node::text_input_styled(
                "query",
                self.query.clone(),
                "type to filter",
                Style::default(),
                Style {
                    dim: true,
                    ..Style::default()
                },
                Message::Query,
            )
            .with_length(Length::Flex(1)),
        ]);

        Node::panel(
            Node::column([
                filter.with_length(Length::Fixed(1)),
                Node::gap(1),
                results.with_length(Length::Fixed(PAGE_SIZE as u32)),
                Node::row([
                    Paginator::new("pages", self.page, total_pages, Message::Page).into_node(),
                    Node::text(format!(
                        "  {} matches  selected: {selection}",
                        matches.len()
                    )),
                ])
                .with_length(Length::Fixed(1)),
                Help::new([
                    HelpBinding::new("Tab", "focus"),
                    HelpBinding::new("Up/Down", "select"),
                    HelpBinding::new("Left/Right", "page"),
                    HelpBinding::new("Esc", "exit"),
                ])
                .into_node()
                .with_length(Length::Fixed(1)),
            ]),
            "Filtered widget catalog",
        )
    }
}

fn matching_indices(query: &str) -> Vec<usize> {
    let query = query.to_ascii_lowercase();
    PACKAGES
        .iter()
        .enumerate()
        .filter_map(|(index, name)| name.to_ascii_lowercase().contains(&query).then_some(index))
        .collect()
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        FilteredList::default(),
        TerminalOptions {
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
