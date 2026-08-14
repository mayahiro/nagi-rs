//! DiffView semantic-action, rendering, pointer, and copy integration tests

use nagi_tui::{
    App, Effect, Event, KeyAction, KeyBinding, KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope,
    KeyStroke, Modifiers, MouseButton, MouseEvent, MouseKind, Node, NodeId, Runtime, RuntimeConfig,
    Size, VirtualClock,
};
use nagi_tui_widgets::{
    CodeLine, DiffCopyRequest, DiffDocument, DiffHunk, DiffLayout, DiffLayoutOptions, DiffLine,
    DiffRange, DiffView, DiffViewState, SELECTION_NEXT_ACTION_ID,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Message {
    State(DiffViewState),
    Copy(DiffCopyRequest),
}

struct DiffViewApp {
    layout: DiffLayout,
    state: DiffViewState,
    key_map: KeyMap,
    messages: Vec<Message>,
}

impl App for DiffViewApp {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if let Message::State(state) = &message {
            self.state = *state;
        }
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::column([
            DiffView::new("diff", self.layout.clone(), self.state, Message::State)
                .viewport(4)
                .on_copy(Message::Copy)
                .into_node(),
        ])
        .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn diff_view_routes_actions_renders_and_copies_unified_source() {
    let hunk = DiffHunk::new(DiffRange::new(1, 2).unwrap(), DiffRange::new(1, 2).unwrap());
    let document = DiffDocument::new(
        [
            DiffLine::metadata(CodeLine::plain("diff --git a/a b/a").unwrap()),
            DiffLine::hunk(hunk, CodeLine::plain("@@ -1,2 +1,2 @@").unwrap()),
            DiffLine::context(1, 1, CodeLine::plain("same").unwrap()).unwrap(),
            DiffLine::deletion(2, CodeLine::plain("old").unwrap()).unwrap(),
            DiffLine::addition(2, CodeLine::plain("new").unwrap()).unwrap(),
        ],
        true,
    )
    .unwrap();
    let layout = DiffLayout::new(
        document,
        DiffLayoutOptions::default().with_viewport_width(16),
    )
    .unwrap();
    let key_map = KeyMap::new()
        .rebind(
            SELECTION_NEXT_ACTION_ID,
            [KeyBinding::new(KeyStroke::character('j', Modifiers::NONE))],
        )
        .unwrap();
    let mut runtime = Runtime::with_clock(
        DiffViewApp {
            layout,
            state: DiffViewState::default(),
            key_map,
            messages: Vec::new(),
        },
        RuntimeConfig::new(Size::new(16, 4)),
        VirtualClock::new(),
    )
    .unwrap();
    let frame = runtime.render_if_dirty().unwrap().unwrap();
    assert_eq!(surface_row(frame.surface(), 2), "1 1   same      ");
    assert_eq!(surface_row(frame.surface(), 3), "2   - old       ");
    assert!(runtime.request_focus(&NodeId::from("diff")).unwrap());

    for _ in 0..3 {
        dispatch_key(
            &mut runtime,
            KeyCode::Character('j'),
            Modifiers::NONE,
            KeyAction::Press,
            1,
        );
    }
    assert_eq!(runtime.app().state.cursor(), 3);

    dispatch_key(
        &mut runtime,
        KeyCode::Character('c'),
        Modifiers {
            control: true,
            ..Modifiers::NONE
        },
        KeyAction::Press,
        1,
    );
    let Some(Message::Copy(request)) = runtime.app().messages.last() else {
        panic!("missing copy request");
    };
    assert_eq!(request.text(), "-old");
    assert_eq!(request.lines(), 3..4);

    dispatch_key(
        &mut runtime,
        KeyCode::Character('c'),
        Modifiers {
            control: true,
            ..Modifiers::NONE
        },
        KeyAction::Repeat,
        0,
    );
    dispatch_event(
        &mut runtime,
        &Event::Mouse(MouseEvent {
            kind: MouseKind::Press,
            button: MouseButton::Left,
            x: 0,
            y: 0,
            modifiers: Modifiers::NONE,
        }),
        1,
    );
    assert_eq!(runtime.app().state.cursor(), 1);
}

fn dispatch_key(
    runtime: &mut Runtime<DiffViewApp, VirtualClock>,
    code: KeyCode,
    modifiers: Modifiers,
    action: KeyAction,
    expected_messages: usize,
) {
    dispatch_event(
        runtime,
        &Event::Key(KeyEvent {
            code,
            modifiers,
            action,
            text: None,
            protocol: KeyProtocol::Legacy,
        }),
        expected_messages,
    );
}

fn dispatch_event(
    runtime: &mut Runtime<DiffViewApp, VirtualClock>,
    event: &Event,
    expected_messages: usize,
) {
    runtime.render_if_dirty().unwrap();
    let dispatch = runtime.dispatch_event(event).unwrap();
    assert!(dispatch.consumed());
    assert_eq!(dispatch.messages(), expected_messages);
    assert_eq!(runtime.process_pending().unwrap(), expected_messages);
}

fn surface_row(surface: &nagi_tui::Surface, y: i32) -> String {
    let mut row = String::new();
    for x in 0..i32::try_from(surface.width()).unwrap_or(i32::MAX) {
        if let Some(cell) = surface.cell(x, y) {
            row.push_str(cell.content());
        }
    }
    row
}
