//! Public Runtime event and resize coverage for modern widgets

use nagi_tui::{
    App, Effect, Event, KeyAction, KeyCode, KeyEvent, KeyProtocol, Modifiers, Node, NodeId,
    Runtime, RuntimeConfig, Size, VirtualClock,
};
use nagi_tui_widgets::{
    Calendar, CalendarDate, FilePicker, FilePickerEntry, Paginator, TextArea, TextAreaHistory,
    TextAreaState, Tree, TreeItem,
};

#[derive(Clone)]
enum Message {
    Page(usize),
    File(usize),
    Open(usize),
    Back,
    Date(CalendarDate),
    Tree(usize),
    Edit(TextAreaState),
    Undo,
    Redo,
}

struct ModernApp {
    page: usize,
    file: usize,
    opened: Option<usize>,
    back: bool,
    date: CalendarDate,
    tree: usize,
    text: TextAreaState,
    history: TextAreaHistory,
}

impl ModernApp {
    fn new() -> Self {
        let text = TextAreaState::at_end("ab");
        Self {
            page: 1,
            file: 0,
            opened: None,
            back: false,
            date: CalendarDate::new(2024, 2, 28),
            tree: 0,
            text: text.clone(),
            history: TextAreaHistory::new(text),
        }
    }
}

impl App for ModernApp {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Page(page) => self.page = page,
            Message::File(file) => self.file = file,
            Message::Open(file) => self.opened = Some(file),
            Message::Back => self.back = true,
            Message::Date(date) => self.date = date,
            Message::Tree(tree) => self.tree = tree,
            Message::Edit(text) => {
                self.history.record(text.clone());
                self.text = text;
            }
            Message::Undo => {
                if let Some(text) = self.history.undo() {
                    self.text = text;
                }
            }
            Message::Redo => {
                if let Some(text) = self.history.redo() {
                    self.text = text;
                }
            }
        }
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let files = [
            FilePickerEntry::file("file-a", "A", "a"),
            FilePickerEntry::directory("file-b", "B", "b"),
            FilePickerEntry::file("file-c", "C", "c"),
            FilePickerEntry::file("file-d", "D", "d"),
        ];
        let tree_items = (0..5).map(|index| {
            TreeItem::leaf(
                char::from(b'a' + u8::try_from(index).unwrap()).to_string(),
                format!("Item{index}"),
                0,
            )
        });
        Node::column([
            Paginator::new("pages", self.page, 5, Message::Page).into_node(),
            FilePicker::new("files", files, self.file, Message::File)
                .viewport(2)
                .on_open(Message::Open)
                .on_back(|| Message::Back)
                .into_node(),
            Calendar::new(
                "calendar",
                self.date.year,
                i32::from(self.date.month),
                self.date,
                Message::Date,
            )
            .into_node(),
            Tree::new("tree", tree_items, self.tree, Message::Tree)
                .viewport(2)
                .into_node(),
            TextArea::new("area", self.text.clone(), Message::Edit)
                .on_undo(|| Message::Undo)
                .on_redo(|| Message::Redo)
                .into_node(),
        ])
    }
}

#[test]
fn modern_widgets_route_events_and_survive_resize() {
    let mut runtime = Runtime::with_clock(
        ModernApp::new(),
        RuntimeConfig::new(Size::new(40, 20)),
        VirtualClock::new(),
    )
    .expect("runtime");
    runtime.render_if_dirty().expect("initial render");

    dispatch_key(&mut runtime, "pages", KeyCode::Right, Modifiers::NONE);
    assert_eq!(runtime.app().page, 2);

    dispatch_key(&mut runtime, "files", KeyCode::PageDown, Modifiers::NONE);
    assert_eq!(runtime.app().file, 2);
    dispatch_key(&mut runtime, "files", KeyCode::Enter, Modifiers::NONE);
    assert_eq!(runtime.app().opened, Some(2));
    dispatch_key(&mut runtime, "files", KeyCode::Backspace, Modifiers::NONE);
    assert!(runtime.app().back);

    dispatch_key(&mut runtime, "calendar", KeyCode::Right, Modifiers::NONE);
    assert_eq!(runtime.app().date, CalendarDate::new(2024, 2, 29));
    dispatch_key(&mut runtime, "calendar", KeyCode::PageDown, Modifiers::NONE);
    assert_eq!(runtime.app().date, CalendarDate::new(2024, 3, 29));

    dispatch_key(&mut runtime, "tree", KeyCode::End, Modifiers::NONE);
    assert_eq!(runtime.app().tree, 4);
    assert_eq!(runtime.interaction().focused(), Some(&NodeId::from("tree")));

    dispatch_key(
        &mut runtime,
        "area",
        KeyCode::Character('a'),
        Modifiers {
            control: true,
            ..Modifiers::NONE
        },
    );
    assert!(runtime.app().text.selection().is_some());
    dispatch_event(&mut runtime, &Event::Text("X".to_owned()));
    assert_eq!(runtime.app().text.value(), "X");
    dispatch_key(
        &mut runtime,
        "area",
        KeyCode::Character('z'),
        Modifiers {
            control: true,
            ..Modifiers::NONE
        },
    );
    assert_eq!(runtime.app().text.value(), "ab");
    dispatch_key(
        &mut runtime,
        "area",
        KeyCode::Character('y'),
        Modifiers {
            control: true,
            ..Modifiers::NONE
        },
    );
    assert_eq!(runtime.app().text.value(), "X");

    for size in [Size::new(12, 10), Size::new(60, 24)] {
        runtime.resize(size);
        let frame = runtime
            .render_if_dirty()
            .expect("resize render")
            .expect("dirty resize frame");
        assert_eq!(frame.surface().width(), size.width);
        assert_eq!(frame.surface().height(), size.height);
    }
}

fn dispatch_key(
    runtime: &mut Runtime<ModernApp, VirtualClock>,
    id: &str,
    code: KeyCode,
    modifiers: Modifiers,
) {
    runtime.render_if_dirty().expect("render before focus");
    assert!(
        runtime
            .request_focus(&NodeId::from(id))
            .expect("request focus")
    );
    dispatch_event(
        runtime,
        &Event::Key(KeyEvent {
            code,
            modifiers,
            action: KeyAction::Press,
            text: None,
            protocol: KeyProtocol::Legacy,
        }),
    );
}

fn dispatch_event(runtime: &mut Runtime<ModernApp, VirtualClock>, event: &Event) {
    runtime.render_if_dirty().expect("render before event");
    runtime.dispatch_event(event).expect("dispatch event");
    assert_eq!(runtime.process_pending().expect("process message"), 1);
}
