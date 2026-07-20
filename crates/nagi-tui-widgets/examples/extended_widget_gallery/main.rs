//! Interactive gallery for the extended Nagi TUI widgets

use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Length, MouseTracking, Node, Style, Subscription,
    TerminalOptions, run_terminal,
};
use nagi_tui_widgets::{
    Checkbox, Command, CommandPalette, Radio, Scrollbar, ScrollbarOrientation, Select, TabItem,
    Table, TableColumn, TableRow, Tabs, TextArea, TextAreaState, Tree, TreeItem,
};

enum Message {
    SelectPage(usize),
    SetFeature(bool),
    SelectMode(usize),
    SelectTheme(usize),
    EditNotes(TextAreaState),
    SelectRow(usize),
    SelectTree(usize),
    ToggleTree(usize, bool),
    QueryChanged(String),
    SelectCommand(usize),
    ActivateCommand(usize),
}

struct Gallery {
    page: usize,
    feature: bool,
    mode: usize,
    theme: usize,
    notes: TextAreaState,
    row: usize,
    tree: usize,
    tree_expanded: bool,
    query: String,
    command: usize,
    last_action: String,
}

impl Default for Gallery {
    fn default() -> Self {
        Self {
            page: 0,
            feature: true,
            mode: 0,
            theme: 0,
            notes: TextAreaState::at_end("Multiline notes\nremain application state"),
            row: 0,
            tree: 0,
            tree_expanded: true,
            query: String::new(),
            command: 0,
            last_action: "None".to_owned(),
        }
    }
}

impl App for Gallery {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::SelectPage(index) => self.page = index,
            Message::SetFeature(enabled) => self.feature = enabled,
            Message::SelectMode(index) => self.mode = index,
            Message::SelectTheme(index) => self.theme = index,
            Message::EditNotes(state) => self.notes = state,
            Message::SelectRow(index) => self.row = index,
            Message::SelectTree(index) => self.tree = index,
            Message::ToggleTree(index, expanded) => {
                if index == 0 {
                    self.tree_expanded = expanded;
                }
            }
            Message::QueryChanged(query) => self.query = query,
            Message::SelectCommand(index) => self.command = index,
            Message::ActivateCommand(index) => {
                self.last_action = ["Open file", "Save file", "Toggle sidebar"]
                    .get(index)
                    .copied()
                    .unwrap_or("Unknown")
                    .to_owned();
            }
        }
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let tabs = Tabs::new(
            "gallery-tabs",
            [
                TabItem::new("page-inputs", "Inputs"),
                TabItem::new("page-data", "Data"),
                TabItem::new("page-commands", "Commands"),
            ],
            self.page,
            Message::SelectPage,
        )
        .into_node()
        .with_length(Length::Fixed(1));

        let page = match self.page {
            0 => self.inputs_page(),
            1 => self.data_page(),
            _ => self.commands_page(),
        };
        Node::border(
            Node::column([
                Node::styled_text(
                    "Extended Widget Gallery",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                )
                .with_length(Length::Fixed(1)),
                tabs,
                page.with_length(Length::Flex(1)),
                Node::text("Tab changes focus, arrows navigate, Enter/Space activate, Esc exits")
                    .with_length(Length::Fixed(1)),
            ]),
            Style::default(),
        )
    }
}

impl Gallery {
    fn inputs_page(&self) -> Node<Message> {
        Node::column([
            Checkbox::new(
                "feature",
                "Enable feature",
                self.feature,
                Message::SetFeature,
            )
            .into_node(),
            Node::row([
                Radio::new("mode-safe", "Safe", self.mode == 0, || {
                    Message::SelectMode(0)
                })
                .into_node(),
                Node::text("  "),
                Radio::new("mode-fast", "Fast", self.mode == 1, || {
                    Message::SelectMode(1)
                })
                .into_node(),
            ]),
            Select::new(
                "theme",
                ["System", "Light", "Dark"],
                self.theme,
                Message::SelectTheme,
            )
            .into_node(),
            Node::text("TextArea:"),
            Node::border(
                TextArea::new("notes", self.notes.clone(), Message::EditNotes)
                    .placeholder("Enter notes")
                    .into_node(),
                Style::default(),
            ),
        ])
    }

    fn data_page(&self) -> Node<Message> {
        let table = Table::new(
            "process-table",
            [
                TableColumn::new("Process", Length::Flex(1)),
                TableColumn::new("State", Length::Fixed(8)),
                TableColumn::new("CPU", Length::Fixed(6)),
            ],
            [
                TableRow::new("process-api", ["api", "Ready", "12%"]),
                TableRow::new("process-worker", ["worker", "Busy", "48%"]),
                TableRow::new("process-index", ["indexer", "Idle", "2%"]),
            ],
            self.row,
            Message::SelectRow,
        )
        .into_node();
        let tree = Tree::new(
            "file-tree",
            [
                TreeItem::branch("tree-src", "src", 0, self.tree_expanded),
                TreeItem::leaf("tree-main", "main.rs", 1),
                TreeItem::leaf("tree-lib", "lib.rs", 1),
                TreeItem::leaf("tree-readme", "README.md", 0),
            ],
            self.tree,
            Message::SelectTree,
        )
        .on_toggle(Message::ToggleTree)
        .into_node();
        let offset = u64::try_from(self.row)
            .unwrap_or(u64::MAX)
            .saturating_mul(35);
        Node::column([
            table,
            Node::text("Tree:"),
            tree,
            Node::row([
                Node::text("Viewport: "),
                Scrollbar::<Message>::new(100, 30, offset, 24)
                    .orientation(ScrollbarOrientation::Horizontal)
                    .into_node(),
            ]),
        ])
    }

    fn commands_page(&self) -> Node<Message> {
        Node::column([
            CommandPalette::new(
                "command-palette",
                "command-query",
                self.query.clone(),
                [
                    Command::new("command-open", "Open file").keywords(["read"]),
                    Command::new("command-save", "Save file").keywords(["write"]),
                    Command::new("command-sidebar", "Toggle sidebar").keywords(["panel"]),
                ],
                self.command,
                Message::QueryChanged,
                Message::SelectCommand,
                Message::ActivateCommand,
            )
            .title("Command Palette")
            .into_node(),
            Node::text(format!("Last action: {}", self.last_action)),
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
        Event::Key(key) if key.code == KeyCode::Escape => EventAction::Exit,
        Event::Key(key) if key.modifiers.control && key.code == KeyCode::Character('c') => {
            EventAction::Exit
        }
        _ => EventAction::Ignore,
    })?;
    Ok(())
}
