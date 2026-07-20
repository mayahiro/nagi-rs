//! Controlled file browser over application-supplied metadata

use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Length, Node, Style, TerminalOptions, TextSpan,
    run_terminal,
};
use nagi_tui_widgets::{Checkbox, FilePicker, FilePickerEntry, Help, HelpBinding};

enum Message {
    Select(usize),
    Open(usize),
    Back,
    ShowHidden(bool),
}

struct FileBrowser {
    cwd: String,
    selected: usize,
    show_hidden: bool,
    status: String,
}

impl Default for FileBrowser {
    fn default() -> Self {
        Self {
            cwd: "/".to_owned(),
            selected: 0,
            show_hidden: false,
            status: "Select an entry".to_owned(),
        }
    }
}

impl App for FileBrowser {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Select(index) => self.selected = index,
            Message::Open(index) => {
                if let Some(entry) = entries_for(&self.cwd).get(index) {
                    let path = entry.path().to_owned();
                    if entry.is_directory() {
                        self.cwd.clone_from(&path);
                        self.selected = 0;
                        self.status = format!("Entered {path}");
                    } else {
                        self.status = format!("Opened {path}");
                    }
                }
            }
            Message::Back => {
                if self.cwd == "/" {
                    self.status = "Already at root".to_owned();
                } else {
                    self.cwd = parent_directory(&self.cwd).to_owned();
                    self.selected = 0;
                    self.status = format!("Returned to {}", self.cwd);
                }
            }
            Message::ShowHidden(show) => {
                self.show_hidden = show;
                self.selected = 0;
            }
        }
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let entries = entries_for(&self.cwd);
        let picker = FilePicker::new("files", entries.clone(), self.selected, Message::Select)
            .on_open(Message::Open)
            .on_back(|| Message::Back)
            .show_hidden(self.show_hidden)
            .viewport(10)
            .into_node();

        let (name, kind, path) = entries
            .get(self.selected)
            .map_or(("None", "-", "-"), |entry| {
                (
                    entry.name(),
                    if entry.is_directory() {
                        "directory"
                    } else {
                        "file"
                    },
                    entry.path(),
                )
            });
        let details = Node::column([
            Node::rich_text([
                TextSpan::new(
                    "Name: ",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                ),
                TextSpan::new(name, Style::default()),
            ]),
            Node::text(format!("Kind: {kind}")),
            Node::text(format!("Path: {path}")),
            Node::gap(1),
            Node::text(&self.status),
        ]);

        Node::column([
            Node::styled_text(
                format!("File browser  {}", self.cwd),
                Style {
                    bold: true,
                    ..Style::default()
                },
            )
            .with_length(Length::Fixed(1)),
            Checkbox::new(
                "show-hidden",
                "Show hidden entries",
                self.show_hidden,
                Message::ShowHidden,
            )
            .into_node()
            .with_length(Length::Fixed(1)),
            Node::row([
                Node::panel(picker, "Entries").with_length(Length::Fixed(36)),
                Node::panel(details, "Details").with_length(Length::Flex(1)),
            ])
            .with_length(Length::Flex(1)),
            Help::new([
                HelpBinding::new("Up/Down", "select"),
                HelpBinding::new("Enter", "open"),
                HelpBinding::new("Left/Backspace", "parent"),
                HelpBinding::new("Tab", "focus"),
                HelpBinding::new("Esc", "exit"),
            ])
            .into_node()
            .with_length(Length::Fixed(1)),
        ])
    }
}

fn entries_for(directory: &str) -> Vec<FilePickerEntry> {
    match directory {
        "/src" => vec![
            FilePickerEntry::directory("src-widget", "widget", "/src/widget"),
            FilePickerEntry::file("src-main", "main.rs", "/src/main.rs"),
            FilePickerEntry::file("src-runtime", "runtime.rs", "/src/runtime.rs"),
        ],
        "/src/widget" => vec![
            FilePickerEntry::file("widget-list", "list.rs", "/src/widget/list.rs"),
            FilePickerEntry::file("widget-table", "table.rs", "/src/widget/table.rs"),
            FilePickerEntry::file("widget-tree", "tree.rs", "/src/widget/tree.rs"),
        ],
        "/docs" => vec![
            FilePickerEntry::file("docs-api", "API.md", "/docs/API.md"),
            FilePickerEntry::file(
                "docs-authoring",
                "WIDGET_AUTHORING.md",
                "/docs/WIDGET_AUTHORING.md",
            ),
        ],
        _ => vec![
            FilePickerEntry::directory("root-src", "src", "/src"),
            FilePickerEntry::directory("root-docs", "docs", "/docs"),
            FilePickerEntry::file("root-readme", "README.md", "/README.md"),
            FilePickerEntry::file("root-env", ".env", "/.env").hidden(true),
        ],
    }
}

fn parent_directory(directory: &str) -> &str {
    if directory == "/src/widget" {
        "/src"
    } else {
        "/"
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        FileBrowser::default(),
        TerminalOptions {
            mouse_tracking: Some(nagi_tui::MouseTracking::Press),
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
