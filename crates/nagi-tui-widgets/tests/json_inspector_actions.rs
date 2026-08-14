//! JSON inspector semantic-action and pointer integration tests

use nagi_tui::{
    App, Effect, Event, KeyAction, KeyBinding, KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope,
    KeyStroke, Modifiers, MouseButton, MouseEvent, MouseKind, Node, NodeId, Runtime, RuntimeConfig,
    Size, VirtualClock,
};
use nagi_tui_widgets::{
    JsonDocument, JsonInspector, JsonInspectorCopyRequest, JsonInspectorState, JsonMember,
    JsonNumber, JsonPointer, JsonValue, SELECTION_NEXT_ACTION_ID,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Message {
    State(JsonInspectorState),
    Copy(JsonInspectorCopyRequest),
}

struct InspectorApp {
    document: JsonDocument,
    state: JsonInspectorState,
    key_map: KeyMap,
    messages: Vec<Message>,
}

impl App for InspectorApp {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if let Message::State(state) = &message {
            self.state = state.clone();
        }
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::column([JsonInspector::new(
            "inspector",
            self.document.clone(),
            self.state.clone(),
            Message::State,
        )
        .maximum_scalar_graphemes(5)
        .on_copy(Message::Copy)
        .into_node()])
        .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn json_inspector_routes_actions_pointer_and_complete_copy() {
    let key_map = KeyMap::new()
        .rebind(
            SELECTION_NEXT_ACTION_ID,
            [KeyBinding::new(KeyStroke::character('j', Modifiers::NONE))],
        )
        .expect("valid inspector rebind");
    let mut runtime = Runtime::with_clock(
        InspectorApp {
            document: inspector_document(),
            state: JsonInspectorState::default(),
            key_map,
            messages: Vec::new(),
        },
        RuntimeConfig::new(Size::new(60, 10)),
        VirtualClock::new(),
    )
    .expect("runtime");
    let frame = runtime
        .render_if_dirty()
        .expect("initial render")
        .expect("initial frame");
    assert!(surface_row(frame.surface(), 2).contains("\"long\": \"abcde…\""));
    assert!(
        runtime
            .request_focus(&NodeId::from("inspector"))
            .expect("focus")
    );

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
    assert_eq!(runtime.app().state.selected().as_str(), "/long");
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
    assert_eq!(request.path().as_str(), "/long");
    assert_eq!(request.text(), "\"abcdef日ghi\"");
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
            y: 3,
            modifiers: Modifiers::NONE,
        }),
        1,
    );
    assert_eq!(runtime.app().state.selected().as_str(), "/nested");
    assert!(
        runtime
            .app()
            .state
            .is_expanded(&JsonPointer::new("/nested").unwrap())
    );
    dispatch_key(
        &mut runtime,
        KeyCode::Right,
        Modifiers::NONE,
        KeyAction::Press,
        1,
    );
    assert_eq!(runtime.app().state.selected().as_str(), "/nested/flag");
}

fn inspector_document() -> JsonDocument {
    let nested = JsonValue::object([
        JsonMember::new("flag", JsonValue::boolean(true)),
        JsonMember::new(
            "items",
            JsonValue::array([
                JsonValue::number(JsonNumber::new("1").unwrap()),
                JsonValue::number(JsonNumber::new("2").unwrap()),
            ]),
        ),
    ])
    .unwrap();
    let root = JsonValue::object([
        JsonMember::new("short", JsonValue::string("ok")),
        JsonMember::new("long", JsonValue::string("abcdef日ghi")),
        JsonMember::new("nested", nested),
        JsonMember::new("empty", JsonValue::array([])),
    ])
    .unwrap();
    JsonDocument::new(root).unwrap()
}

fn dispatch_key(
    runtime: &mut Runtime<InspectorApp, VirtualClock>,
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
    runtime: &mut Runtime<InspectorApp, VirtualClock>,
    event: &Event,
    expected_messages: usize,
) {
    runtime.render_if_dirty().expect("render before event");
    let dispatch = runtime.dispatch_event(event).expect("dispatch event");
    assert!(dispatch.consumed());
    assert_eq!(dispatch.messages(), expected_messages);
    assert_eq!(
        runtime.process_pending().expect("process pending"),
        expected_messages
    );
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
