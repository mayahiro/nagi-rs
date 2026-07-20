//! Public integration tests for the extended standard widgets

use nagi_tui::{
    App, Effect, Event, KeyAction, KeyCode, KeyEvent, KeyProtocol, Modifiers, Node, NodeId,
    Runtime, Size, Subscription, VirtualClock,
};
use nagi_tui_widgets::{
    Checkbox, Command, CommandPalette, Radio, Scrollbar, ScrollbarOrientation, Select, TabItem,
    Table, TableColumn, TableRow, Tabs, TextArea, TextAreaState, Tree, TreeItem,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Message {
    Check(bool),
    ChooseRadio,
    SelectTab(usize),
    SelectOption(usize),
    SelectRow(usize),
    SelectTree(usize),
    ToggleTree(usize, bool),
    Edit(TextAreaState),
    Query(String),
    SelectCommand(usize),
    ActivateCommand(usize),
}

#[derive(Default)]
struct ExtendedApp {
    checked: bool,
    radio: bool,
    tab: usize,
    option: usize,
    row: usize,
    tree: usize,
    tree_expanded: bool,
    text: TextAreaState,
    query: String,
    command: usize,
    activated: Option<usize>,
}

impl App for ExtendedApp {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Check(value) => self.checked = value,
            Message::ChooseRadio => self.radio = true,
            Message::SelectTab(index) => self.tab = index,
            Message::SelectOption(index) => self.option = index,
            Message::SelectRow(index) => self.row = index,
            Message::SelectTree(index) => self.tree = index,
            Message::ToggleTree(index, expanded) => {
                assert_eq!(index, 0);
                self.tree_expanded = expanded;
            }
            Message::Edit(state) => self.text = state,
            Message::Query(query) => self.query = query,
            Message::SelectCommand(index) => self.command = index,
            Message::ActivateCommand(index) => self.activated = Some(index),
        }
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::column([
            Scrollbar::<Message>::new(100, 20, 40, 10)
                .orientation(ScrollbarOrientation::Horizontal)
                .into_node(),
            Checkbox::new("check", "Enabled", self.checked, Message::Check).into_node(),
            Radio::new("radio", "Primary", self.radio, || Message::ChooseRadio).into_node(),
            Tabs::new(
                "tabs",
                [TabItem::new("tab-a", "A"), TabItem::new("tab-b", "B")],
                self.tab,
                Message::SelectTab,
            )
            .into_node(),
            Select::new(
                "select",
                ["First", "Second"],
                self.option,
                Message::SelectOption,
            )
            .into_node(),
            Table::new(
                "table",
                [
                    TableColumn::new("Name", nagi_tui::Length::Fixed(8)),
                    TableColumn::new("State", nagi_tui::Length::Fixed(6)),
                ],
                [
                    TableRow::new("row-a", ["Alpha", "Ready"]),
                    TableRow::new("row-b", ["Beta", "Busy"]),
                ],
                self.row,
                Message::SelectRow,
            )
            .into_node(),
            Tree::new(
                "tree",
                [
                    TreeItem::branch("tree-root", "Root", 0, self.tree_expanded),
                    TreeItem::leaf("tree-child", "Child", 1),
                    TreeItem::leaf("tree-peer", "Peer", 0),
                ],
                self.tree,
                Message::SelectTree,
            )
            .on_toggle(Message::ToggleTree)
            .into_node(),
            TextArea::new("text-area", self.text.clone(), Message::Edit)
                .placeholder("Notes")
                .into_node(),
            CommandPalette::new(
                "palette",
                "palette-input",
                self.query.clone(),
                [
                    Command::new("command-open", "Open"),
                    Command::new("command-save", "Save"),
                ],
                self.command,
                Message::Query,
                Message::SelectCommand,
                Message::ActivateCommand,
            )
            .title("Commands")
            .into_node(),
        ])
    }
}

#[test]
fn extended_widgets_render_and_route_public_events() {
    let mut runtime = Runtime::with_clock(
        ExtendedApp::default(),
        nagi_tui::RuntimeConfig::new(Size::new(60, 30)),
        VirtualClock::new(),
    )
    .expect("runtime");

    let initial = runtime
        .render_if_dirty()
        .expect("render")
        .expect("initial frame");
    assert!(row_text(initial.surface(), 0).starts_with("────██────"));
    assert!(row_text(initial.surface(), 1).starts_with("[ ] Enabled"));
    assert!(row_text(initial.surface(), 5).starts_with("  Name"));

    activate(&mut runtime, "check", KeyCode::Enter);
    assert!(runtime.app().checked);

    activate(&mut runtime, "radio", KeyCode::Enter);
    assert!(runtime.app().radio);

    activate(&mut runtime, "tab-a", KeyCode::Right);
    assert_eq!(runtime.app().tab, 1);

    activate(&mut runtime, "select", KeyCode::Enter);
    assert_eq!(runtime.app().option, 1);

    activate(&mut runtime, "table", KeyCode::Down);
    assert_eq!(runtime.app().row, 1);

    activate(&mut runtime, "tree", KeyCode::Right);
    assert!(runtime.app().tree_expanded);

    assert!(
        runtime
            .request_focus(&NodeId::from("text-area"))
            .expect("text focus")
    );
    runtime
        .dispatch_event(&Event::Text("日".to_owned()))
        .expect("text event");
    assert_eq!(runtime.process_pending().expect("text update"), 1);
    assert_eq!(runtime.app().text, TextAreaState::at_end("日"));

    assert!(
        runtime
            .request_focus(&NodeId::from("palette-input"))
            .expect("palette focus")
    );
    runtime
        .dispatch_event(&Event::Text("s".to_owned()))
        .expect("query event");
    assert_eq!(runtime.process_pending().expect("query update"), 1);
    assert_eq!(runtime.app().query, "s");
    runtime.render_if_dirty().expect("query render");
    runtime
        .dispatch_event(&key(KeyCode::Enter))
        .expect("activate command");
    assert_eq!(runtime.process_pending().expect("command update"), 1);
    assert_eq!(runtime.app().activated, Some(1));
}

fn activate(runtime: &mut Runtime<ExtendedApp, VirtualClock>, id: &str, code: KeyCode) {
    assert!(
        runtime
            .request_focus(&NodeId::from(id))
            .expect("request focus")
    );
    runtime.dispatch_event(&key(code)).expect("dispatch key");
    assert_eq!(runtime.process_pending().expect("process message"), 1);
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: Modifiers::NONE,
        action: KeyAction::Press,
        text: None,
        protocol: KeyProtocol::Legacy,
    })
}

fn row_text(surface: &nagi_tui::Surface, row: i32) -> String {
    (0..surface.width())
        .map(|column| {
            surface
                .cell(column as i32, row)
                .expect("surface cell")
                .content()
        })
        .collect()
}
