//! CodeView semantic-action, rendering, pointer, and copy integration tests

use nagi_tui::{
    App, Effect, Event, KeyAction, KeyBinding, KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope,
    KeyStroke, Modifiers, MouseButton, MouseEvent, MouseKind, Node, NodeId, Runtime, RuntimeConfig,
    Size, VirtualClock,
};
use nagi_tui_widgets::{
    CodeCopyRequest, CodeDocument, CodeLayout, CodeLayoutOptions, CodeLine, CodeView,
    CodeViewState, SELECTION_NEXT_ACTION_ID,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Message {
    State(CodeViewState),
    Copy(CodeCopyRequest),
}

struct CodeViewApp {
    layout: CodeLayout,
    state: CodeViewState,
    key_map: KeyMap,
    messages: Vec<Message>,
}

impl App for CodeViewApp {
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
            CodeView::new("code", self.layout.clone(), self.state, Message::State)
                .viewport(3)
                .on_copy(Message::Copy)
                .into_node(),
        ])
        .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn code_view_routes_actions_renders_and_copies_complete_source() {
    let document = CodeDocument::new(
        ["a\t日", "abcdefghij", "last"]
            .into_iter()
            .map(CodeLine::plain)
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        true,
    )
    .unwrap();
    let layout = CodeLayout::new(
        document,
        CodeLayoutOptions::default().with_viewport_width(12),
    )
    .unwrap();
    let key_map = KeyMap::new()
        .rebind(
            SELECTION_NEXT_ACTION_ID,
            [KeyBinding::new(KeyStroke::character('j', Modifiers::NONE))],
        )
        .unwrap();
    let mut runtime = Runtime::with_clock(
        CodeViewApp {
            layout,
            state: CodeViewState::default(),
            key_map,
            messages: Vec::new(),
        },
        RuntimeConfig::new(Size::new(12, 3)),
        VirtualClock::new(),
    )
    .unwrap();
    let frame = runtime.render_if_dirty().unwrap().unwrap();
    assert!(surface_row(frame.surface(), 0).contains("1 | a   日"));
    assert_eq!(surface_row(frame.surface(), 1), "2 | abcdefgh");
    assert!(runtime.request_focus(&NodeId::from("code")).unwrap());

    dispatch_key(
        &mut runtime,
        KeyCode::Character('j'),
        Modifiers::NONE,
        KeyAction::Press,
        1,
    );
    dispatch_key(
        &mut runtime,
        KeyCode::Character('j'),
        Modifiers::NONE,
        KeyAction::Press,
        1,
    );
    assert_eq!(runtime.app().state.cursor(), 2);

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
    assert_eq!(request.text(), "last\n");
    assert_eq!(request.lines(), 2..3);

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
    assert_eq!(runtime.app().state.cursor(), 0);
}

fn dispatch_key(
    runtime: &mut Runtime<CodeViewApp, VirtualClock>,
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
    runtime: &mut Runtime<CodeViewApp, VirtualClock>,
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
