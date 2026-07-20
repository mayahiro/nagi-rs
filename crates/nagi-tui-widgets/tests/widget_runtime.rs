//! Public standard-widget runtime integration tests

use nagi_tui::{
    App, Effect, Event, KeyAction, KeyCode, KeyEvent, KeyProtocol, Length, Modifiers, MouseButton,
    MouseEvent, MouseKind, Node, NodeId, Runtime, Size, Subscription, VirtualClock,
};
use nagi_tui_widgets::{
    Button, List, ListItem, Modal, Progress, Spinner, Table, TableColumn, TableRow,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Message {
    Press,
    Select(usize),
    CloseModal,
}

struct WidgetApp {
    presses: usize,
    selected: usize,
    modal: bool,
}

impl App for WidgetApp {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Press => self.presses += 1,
            Message::Select(selected) => self.selected = selected,
            Message::CloseModal => self.modal = false,
        }
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let base = Node::column([
            Button::new("save", "Save", || Message::Press).into_node(),
            List::new(
                "list",
                [
                    ListItem::new("item-one", "One"),
                    ListItem::new("item-two", "Two"),
                ],
                self.selected,
                Message::Select,
            )
            .into_node(),
            Progress::<Message>::new(1, 4, 4).into_node(),
            Spinner::<Message>::new(1).label("Work").into_node(),
        ]);
        if self.modal {
            Node::stack([
                base,
                Modal::new(
                    "modal",
                    Button::new("inside", "OK", || Message::CloseModal).into_node(),
                )
                .title("Confirm")
                .on_escape(|| Message::CloseModal)
                .into_node(),
            ])
        } else {
            base
        }
    }
}

#[test]
fn public_widget_nodes_render_and_route_events() {
    let mut runtime = Runtime::with_clock(
        WidgetApp {
            presses: 0,
            selected: 0,
            modal: false,
        },
        nagi_tui::RuntimeConfig::new(Size::new(20, 10)),
        VirtualClock::new(),
    )
    .expect("runtime");

    let initial = runtime
        .render_if_dirty()
        .expect("render")
        .expect("initial frame");
    assert_eq!(row_text(initial.surface(), 0), "[ Save ]            ");
    assert_eq!(row_text(initial.surface(), 1), "> One               ");
    assert_eq!(row_text(initial.surface(), 2), "  Two               ");
    assert_eq!(row_text(initial.surface(), 3), "█░░░                ");
    assert_eq!(row_text(initial.surface(), 4), "⠙ Work              ");

    assert!(runtime.request_focus(&NodeId::from("save")).expect("focus"));
    let focused = runtime
        .render_if_dirty()
        .expect("render")
        .expect("focus frame");
    assert!(
        focused
            .surface()
            .cell(0, 0)
            .expect("button cell")
            .style()
            .reverse
    );
    runtime
        .dispatch_event(&key(KeyCode::Enter))
        .expect("dispatch Enter");
    assert_eq!(runtime.process_pending().expect("update"), 1);
    assert_eq!(runtime.app().presses, 1);

    assert!(
        runtime
            .request_focus(&NodeId::from("list"))
            .expect("focus item")
    );
    runtime
        .dispatch_event(&key(KeyCode::Down))
        .expect("dispatch Down");
    assert_eq!(runtime.process_pending().expect("select"), 1);
    assert_eq!(runtime.app().selected, 1);
    assert_eq!(runtime.interaction().focused(), Some(&NodeId::from("list")));

    runtime.app_mut().modal = true;
    runtime.render_if_dirty().expect("modal render");
    assert!(
        !runtime
            .request_focus(&NodeId::from("save"))
            .expect("outside focus")
    );
    assert!(
        runtime
            .request_focus(&NodeId::from("inside"))
            .expect("inside focus")
    );
    runtime
        .dispatch_event(&key(KeyCode::Escape))
        .expect("dispatch Escape");
    assert_eq!(runtime.process_pending().expect("close modal"), 1);
    assert!(!runtime.app().modal);
}

enum CompositeMessage {
    Select(usize),
    After,
}

struct CompositeApp {
    selected: usize,
}

impl App for CompositeApp {
    type Message = CompositeMessage;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if let CompositeMessage::Select(selected) = message {
            self.selected = selected;
        }
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let rows = (0..5).map(|index| {
            TableRow::new(
                format!("row-{index}"),
                [index.to_string(), format!("value-{index}")],
            )
        });
        Node::column([
            Table::new(
                "table",
                [
                    TableColumn::new("ID", Length::Fixed(4)),
                    TableColumn::new("Value", Length::Flex(1)),
                ],
                rows,
                self.selected,
                CompositeMessage::Select,
            )
            .viewport("table-body", Length::Fixed(2))
            .into_node(),
            Button::new("after", "After", || CompositeMessage::After).into_node(),
        ])
    }
}

#[test]
fn table_is_one_tab_stop_and_selection_follows_its_viewport() {
    let mut runtime = Runtime::with_clock(
        CompositeApp { selected: 0 },
        nagi_tui::RuntimeConfig::new(Size::new(24, 6)),
        VirtualClock::new(),
    )
    .expect("runtime");
    runtime.render_if_dirty().expect("initial render");
    assert!(
        runtime
            .request_focus(&NodeId::from("table"))
            .expect("focus table")
    );

    runtime.dispatch_event(&key(KeyCode::Tab)).expect("Tab");
    assert_eq!(
        runtime.interaction().focused(),
        Some(&NodeId::from("after"))
    );

    assert!(
        runtime
            .request_focus(&NodeId::from("table"))
            .expect("refocus table")
    );
    runtime.dispatch_event(&key(KeyCode::End)).expect("End");
    assert_eq!(runtime.process_pending().expect("select last"), 1);
    runtime.render_if_dirty().expect("follow selection");
    let state = runtime
        .interaction()
        .scroll_state(&NodeId::from("table-body"))
        .expect("table scroll state");
    assert_eq!(state.offset.y, 3);
    assert_eq!(state.maximum.y, 3);

    runtime.app_mut().selected = 0;
    runtime.render_if_dirty().expect("return to start");
    runtime
        .dispatch_event(&Event::Mouse(MouseEvent {
            kind: MouseKind::Press,
            button: MouseButton::Left,
            x: 0,
            y: 2,
            modifiers: Modifiers::NONE,
        }))
        .expect("click second row");
    assert_eq!(runtime.process_pending().expect("mouse selection"), 1);
    assert_eq!(runtime.app().selected, 1);
    assert_eq!(
        runtime.interaction().focused(),
        Some(&NodeId::from("table"))
    );
}

struct ListViewportApp {
    selected: usize,
}

impl App for ListViewportApp {
    type Message = usize;

    fn update(&mut self, selected: Self::Message) -> Effect<Self::Message> {
        self.selected = selected;
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        List::new(
            "large-list",
            (0..5).map(|index| ListItem::new(format!("list-row-{index}"), index.to_string())),
            self.selected,
            |selected| selected,
        )
        .viewport("list-body", Length::Fixed(2))
        .into_node()
    }
}

#[test]
fn list_selection_follows_its_virtual_viewport() {
    let mut runtime = Runtime::with_clock(
        ListViewportApp { selected: 0 },
        nagi_tui::RuntimeConfig::new(Size::new(12, 2)),
        VirtualClock::new(),
    )
    .expect("runtime");
    runtime.render_if_dirty().expect("initial render");
    assert!(
        runtime
            .request_focus(&NodeId::from("large-list"))
            .expect("focus list")
    );

    runtime.dispatch_event(&key(KeyCode::End)).expect("End");
    assert_eq!(runtime.process_pending().expect("select last"), 1);
    runtime.render_if_dirty().expect("follow selection");
    let state = runtime
        .interaction()
        .scroll_state(&NodeId::from("list-body"))
        .expect("list scroll state");
    assert_eq!(state.offset.y, 3);
    assert_eq!(state.maximum.y, 3);

    runtime.app_mut().selected = 0;
    runtime.render_if_dirty().expect("return to start");
    runtime
        .dispatch_event(&Event::Mouse(MouseEvent {
            kind: MouseKind::Press,
            button: MouseButton::Left,
            x: 0,
            y: 1,
            modifiers: Modifiers::NONE,
        }))
        .expect("click second row");
    assert_eq!(runtime.process_pending().expect("mouse selection"), 1);
    assert_eq!(runtime.app().selected, 1);
    assert_eq!(
        runtime.interaction().focused(),
        Some(&NodeId::from("large-list"))
    );
}

struct WrappedListViewportApp;

impl App for WrappedListViewportApp {
    type Message = usize;

    fn update(&mut self, _selected: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        List::new(
            "wrapped-list",
            [
                ListItem::new("wrapped-first", "ABCDEFGHIJ"),
                ListItem::new("wrapped-second", "B"),
            ],
            0,
            |selected| selected,
        )
        .viewport("wrapped-list-body", Length::Fixed(2))
        .into_node()
    }
}

#[test]
fn virtual_list_rows_are_one_cell_high() {
    let mut runtime = Runtime::with_clock(
        WrappedListViewportApp,
        nagi_tui::RuntimeConfig::new(Size::new(6, 2)),
        VirtualClock::new(),
    )
    .expect("runtime");

    let frame = runtime
        .render_if_dirty()
        .expect("render")
        .expect("initial frame");

    assert_eq!(row_text(frame.surface(), 0), "> ABCD");
    assert_eq!(row_text(frame.surface(), 1), "  B   ");
}

struct MultilineTableViewportApp;

impl App for MultilineTableViewportApp {
    type Message = usize;

    fn update(&mut self, _selected: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Table::new(
            "multiline-table",
            [TableColumn::new("Value", Length::Fixed(4))],
            [
                TableRow::new("multiline-first", ["A\nX"]),
                TableRow::new("multiline-second", ["B"]),
            ],
            0,
            |selected| selected,
        )
        .viewport("multiline-table-body", Length::Fixed(2))
        .into_node()
    }
}

#[test]
fn virtual_table_rows_are_one_cell_high() {
    let mut runtime = Runtime::with_clock(
        MultilineTableViewportApp,
        nagi_tui::RuntimeConfig::new(Size::new(6, 3)),
        VirtualClock::new(),
    )
    .expect("runtime");

    let frame = runtime
        .render_if_dirty()
        .expect("render")
        .expect("initial frame");

    assert_eq!(frame.surface().cell(2, 2).unwrap().content(), "B");
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
