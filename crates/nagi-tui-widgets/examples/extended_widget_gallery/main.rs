//! Interactive gallery for the extended Nagi TUI widgets

use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Length, MouseTracking, Node, Style, Subscription,
    TerminalOptions, TextSpan, run_terminal,
};
use nagi_tui_widgets::{
    Button, Checkbox, Command, CommandPalette, Composer, ComposerOverflowPolicy, ComposerState,
    Dialog, DialogAction, Disclosure, Radio, Scrollbar, ScrollbarOrientation, Select,
    SelectableText, SelectableTextContent, SelectableTextState, TabItem, Table, TableColumn,
    TableRow, Tabs, TextArea, TextAreaState, TextCopyKind, TextCopyRequest, Tree, TreeItem,
};

enum Message {
    SelectPage(usize),
    SetFeature(bool),
    SelectMode(usize),
    SelectTheme(usize),
    EditNotes(TextAreaState),
    EditComposer(ComposerState),
    SubmitComposer,
    SelectRow(usize),
    SelectTree(usize),
    ToggleTree(usize, bool),
    ToggleDetails(bool),
    SelectText(SelectableTextState),
    CopyText(TextCopyRequest),
    QueryChanged(String),
    SelectCommand(usize),
    ActivateCommand(usize),
    OpenDialog,
    ChooseDialog(usize),
    CloseDialog,
}

struct Gallery {
    page: usize,
    feature: bool,
    mode: usize,
    theme: usize,
    notes: TextAreaState,
    composer: ComposerState,
    composer_history: Vec<String>,
    row: usize,
    tree: usize,
    tree_expanded: bool,
    details_expanded: bool,
    selectable_content: SelectableTextContent,
    selectable: SelectableTextState,
    query: String,
    command: usize,
    last_action: String,
    dialog_open: bool,
}

impl Default for Gallery {
    fn default() -> Self {
        Self {
            page: 0,
            feature: true,
            mode: 0,
            theme: 0,
            notes: TextAreaState::at_end("Multiline notes\nremain application state"),
            composer: ComposerState::at_end("Draft message"),
            composer_history: vec!["Earlier message".to_owned()],
            row: 0,
            tree: 0,
            tree_expanded: true,
            details_expanded: false,
            selectable_content: SelectableTextContent::styled([
                TextSpan::new(
                    "Selectable",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                ),
                TextSpan::new(" text keeps application-owned selection.", Style::default()),
            ]),
            selectable: SelectableTextState::with_selection(10, 0),
            query: String::new(),
            command: 0,
            last_action: "None".to_owned(),
            dialog_open: false,
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
            Message::EditComposer(state) => self.composer = state,
            Message::SubmitComposer => {
                let value = self.composer.text_area().value().to_owned();
                if !value.trim().is_empty() {
                    if self.composer_history.len() == 8 {
                        self.composer_history.remove(0);
                    }
                    self.composer_history.push(value.clone());
                    self.composer = ComposerState::at_end("");
                    self.last_action = format!("Submitted: {}", value.replace('\n', " / "));
                }
            }
            Message::SelectRow(index) => self.row = index,
            Message::SelectTree(index) => self.tree = index,
            Message::ToggleTree(index, expanded) => {
                if index == 0 {
                    self.tree_expanded = expanded;
                }
            }
            Message::ToggleDetails(expanded) => self.details_expanded = expanded,
            Message::SelectText(state) => self.selectable = state,
            Message::CopyText(request) => {
                let kind = match request.kind() {
                    TextCopyKind::Selection => "selection",
                    TextCopyKind::Document => "document",
                };
                self.last_action =
                    format!("Copied {kind}: {}", request.text().replace('\n', " / "));
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
            Message::OpenDialog => self.dialog_open = true,
            Message::ChooseDialog(index) => {
                self.last_action = ["Open from dialog", "Save from dialog"]
                    .get(index)
                    .copied()
                    .unwrap_or("Unknown dialog action")
                    .to_owned();
                self.dialog_open = false;
            }
            Message::CloseDialog => self.dialog_open = false,
        }
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    fn view(&self, context: nagi_tui::ViewContext) -> Node<Self::Message> {
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
            0 => self.inputs_page(context.size.width.saturating_sub(4).max(1)),
            1 => self.data_page(),
            _ => self.commands_page(),
        };
        let content = Node::border(
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
        );
        if !self.dialog_open {
            return content;
        }
        let dialog = Dialog::new(
            "choice-dialog",
            Node::text("Choose one application-defined command"),
            [
                DialogAction::new("dialog-open", "Open", || Message::ChooseDialog(0)),
                DialogAction::new("dialog-save", "Save", || Message::ChooseDialog(1)),
                DialogAction::new("dialog-cancel", "Cancel", || Message::CloseDialog),
            ],
        )
        .title(Node::styled_text(
            "Generic dialog",
            Style {
                bold: true,
                ..Style::default()
            },
        ))
        .default_action("dialog-cancel")
        .cancel_action("dialog-cancel")
        .action_wrap_width(context.size.width.saturating_sub(4).max(1))
        .into_node();
        Node::stack([content, dialog])
    }
}

impl Gallery {
    fn inputs_page(&self, composer_width: u32) -> Node<Message> {
        let composer_valid = !self.composer.text_area().value().trim().is_empty();
        let mut composer = Composer::new(
            "composer",
            "composer-viewport",
            "composer-caret",
            self.composer.clone(),
            Message::EditComposer,
            || Message::SubmitComposer,
        )
        .placeholder("Enter a message")
        .soft_wrap(composer_width)
        .rows(1, 3)
        .history(self.composer_history.clone())
        .maximum_graphemes(240, ComposerOverflowPolicy::Truncate)
        .submit_enabled(composer_valid);
        if !composer_valid {
            composer = composer.validation(Node::text("A message is required"));
        }
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
            Node::text("Composer: Enter submits, Shift-Enter inserts a line"),
            Node::border(composer.into_node(), Style::default()),
        ])
    }

    fn data_page(&self) -> Node<Message> {
        let details = Disclosure::new(
            "process-details",
            Node::text("Process details"),
            self.details_expanded,
            Message::ToggleDetails,
        )
        .body(|| Node::text("Metrics are application-owned detail content"))
        .into_node();
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
            details,
            table,
            Node::text("Tree:"),
            tree,
            Node::row([
                Node::text("Viewport: "),
                Scrollbar::<Message>::new(100, 30, offset, 24)
                    .orientation(ScrollbarOrientation::Horizontal)
                    .into_node(),
            ]),
            Node::text(
                "SelectableText: Shift-arrows select, Ctrl-C copies, Ctrl-Shift-C copies all",
            ),
            Node::border(
                SelectableText::new(
                    "selectable-text",
                    self.selectable_content.clone(),
                    self.selectable,
                    Message::SelectText,
                )
                .on_copy(Message::CopyText)
                .into_node(),
                Style::default(),
            ),
            Node::text(format!("Last action: {}", self.last_action)),
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
            Button::new("open-dialog", "Open generic dialog", || Message::OpenDialog).into_node(),
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
