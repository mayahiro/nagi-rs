//! Shared runtime vertical-slice conformance fixtures

mod support;

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, ActionId, App, BindingConflictKind, Capabilities,
    Effect, Event, EventResult, Insets, KeyAction, KeyBinding, KeyCode, KeyEvent, KeyMap,
    KeyProtocol, KeyScope, KeyScopePropagation, KeyStroke, Modifiers, Node, NodeId, Runtime,
    RuntimeError, RuntimeEventError, Size, Style, Subscription, VirtualClock, encode,
};

#[derive(Default)]
struct Echo {
    text: String,
}

impl App for Echo {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.text.push_str(&message);
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::border(Node::text(&self.text), Style::default())
    }
}

#[test]
fn input_update_surface_and_vt_output_match_shared_fixtures() {
    let Some(records) = support::load(
        "runtime/roundtrip.txt",
        "runtime-roundtrip",
        &["width", "height", "input", "expected"],
    ) else {
        return;
    };

    for record in records {
        let width = number(record.field("width"));
        let height = number(record.field("height"));
        let input = record.decoded("input");
        let expected = record.text("expected");
        let mut runtime = Runtime::with_clock(
            Echo::default(),
            nagi_tui::RuntimeConfig::new(Size::new(width, height)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap().unwrap();
        let mut decoder = nagi_tui::TimedInputDecoder::new(
            VirtualClock::new(),
            std::time::Duration::from_millis(25),
        );

        for event in decoder.feed(&input) {
            if let Event::Text(text) = event {
                runtime.enqueue(text).unwrap();
            }
        }
        let frame = runtime.step().unwrap().unwrap();

        assert_eq!(frame.surface().snapshot(), expected, "case {}", record.id);
        let output = encode(frame.operations(), Capabilities::BASELINE);
        assert!(
            output.windows(input.len()).any(|window| window == input),
            "case {} did not reach VT output",
            record.id
        );
    }
}

#[derive(Default)]
struct TextInputApp {
    value: String,
}

impl App for TextInputApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.value = message;
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::text_input("input", &self.value, |value| value)
    }
}

#[test]
fn text_input_cursor_snapshot_matches_shared_fixture() {
    let Some(records) = support::load(
        "interaction/text-input-runtime.txt",
        "text-input-runtime",
        &["width", "height", "input", "expected"],
    ) else {
        return;
    };
    for record in records {
        let mut runtime = Runtime::with_clock(
            TextInputApp::default(),
            nagi_tui::RuntimeConfig::new(Size::new(
                number(record.field("width")),
                number(record.field("height")),
            )),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        runtime
            .request_focus(&nagi_tui::NodeId::from("input"))
            .unwrap();
        let mut decoder = nagi_tui::TimedInputDecoder::new(
            VirtualClock::new(),
            std::time::Duration::from_millis(25),
        );
        for event in decoder.feed(&record.decoded("input")) {
            runtime.dispatch_event(&event).unwrap();
        }

        let frame = runtime.step().unwrap().unwrap();

        assert_eq!(
            frame.surface().snapshot(),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

struct KeyRoutingApp {
    scenario: String,
    updates: Vec<String>,
}

impl App for KeyRoutingApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.updates.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        match self.scenario.as_str() {
            "child-precedence" => {
                let child = Node::text("child")
                    .focusable("child")
                    .on_event("child", |_| {
                        EventResult::message("unexpected-raw".to_owned())
                    })
                    .on_actions("child", [message_action("app.child", 'x', "child-action")]);
                Node::padding(child, Insets::all(0))
                    .on_event("root", |_| {
                        EventResult::message("unexpected-root".to_owned())
                    })
                    .on_actions("root", [message_action("app.root", 'x', "root-action")])
            }
            "ignored-action-routing" => {
                let child = Node::text("child")
                    .focusable("child")
                    .on_event("child", |_| {
                        EventResult::ignored().emit("child-raw".to_owned())
                    })
                    .on_actions("child", [ignored_action("app.child", 'x', "child-action")]);
                Node::padding(child, Insets::all(0))
                    .on_event("root", |_| {
                        EventResult::message("unexpected-root".to_owned())
                    })
                    .on_actions("root", [message_action("app.root", 'x', "root-action")])
            }
            "stop-action-propagation" => {
                let outer_map = KeyMap::new()
                    .rebind(ActionId::from("app.child"), [binding('y')])
                    .unwrap();
                let child = Node::text("child").focusable("child").on_actions(
                    "child",
                    [message_action("app.child", 'x', "unexpected-child-action")],
                );
                let scope = Node::padding(child, Insets::all(0))
                    .with_key_scope(
                        KeyScope::new("scope", KeyMap::new())
                            .with_propagation(KeyScopePropagation::StopAtScope),
                    )
                    .on_event("scope", |_| {
                        EventResult::ignored().emit("scope-raw".to_owned())
                    });
                Node::padding(scope, Insets::all(0))
                    .with_key_scope(KeyScope::new("root", outer_map))
                    .on_event("root", |_| EventResult::message("root-raw".to_owned()))
                    .on_actions(
                        "root",
                        [message_action("app.root", 'x', "unexpected-action")],
                    )
            }
            "scope-rebind" => {
                let map = KeyMap::new()
                    .rebind(ActionId::from("app.child"), [binding('y')])
                    .unwrap();
                let child = Node::text("child").focusable("child").on_actions(
                    "child",
                    [routed_action("app.child", 'x', 'y', "child-action")],
                );
                Node::padding(child, Insets::all(0)).with_key_scope(KeyScope::new("scope", map))
            }
            "disabled-consume" => {
                let action = Action::new(
                    descriptor("app.child", 'x')
                        .with_availability(ActionAvailability::DisabledConsume),
                    |_| EventResult::message("unexpected-action".to_owned()),
                );
                Node::text("child")
                    .focusable("child")
                    .on_event("child", |_| {
                        EventResult::message("unexpected-raw".to_owned())
                    })
                    .on_actions("child", [action])
            }
            "paste-bypasses-actions" => Node::text("child")
                .focusable("child")
                .on_event("child", |_| EventResult::message("child-raw".to_owned()))
                .on_actions(
                    "child",
                    [message_action("app.child", 'x', "unexpected-action")],
                ),
            "text-input-local-action" => {
                Node::text_input("input", "", |_| "input-change".to_owned())
                    .on_actions("input", [message_action("app.input", 'x', "input-action")])
            }
            "text-input-core-before-ancestor" => Node::padding(
                Node::text_input("input", "", |_| "input-change".to_owned()),
                Insets::all(0),
            )
            .on_actions(
                "root",
                [message_action("app.root", 'x', "unexpected-action")],
            ),
            scenario => panic!("unknown key routing scenario {scenario}"),
        }
    }
}

#[test]
fn scoped_key_routing_matches_shared_fixtures() {
    let Some(records) = support::load(
        "interaction/key-routing-runtime.txt",
        "key-routing-runtime",
        &["event", "expected", "consumed", "groups"],
    ) else {
        return;
    };
    for record in records {
        let scenario = record.id.clone();
        let mut runtime = Runtime::with_clock(
            KeyRoutingApp {
                scenario,
                updates: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(20, 3)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        let target = if record.id.starts_with("text-input") {
            "input"
        } else {
            "child"
        };
        assert!(
            runtime.request_focus(&target.into()).unwrap(),
            "case {}",
            record.id
        );

        let resolved_groups = runtime.active_action_groups().unwrap();
        let groups: Vec<_> = resolved_groups
            .iter()
            .map(|group| group.owner().as_str().to_owned())
            .collect();
        assert_eq!(
            groups,
            fixture_list(record.field("groups")),
            "case {}",
            record.id
        );
        if record.id == "stop-action-propagation" {
            assert_eq!(
                resolved_groups[0]
                    .scope_path()
                    .iter()
                    .map(NodeId::as_str)
                    .collect::<Vec<_>>(),
                ["root", "scope"]
            );
            assert_eq!(resolved_groups[0].actions()[0].bindings(), [binding('y')]);
        }

        let dispatch = runtime
            .dispatch_event(&key_routing_event(record.field("event")))
            .unwrap();
        runtime.process_pending().unwrap();
        assert_eq!(
            dispatch.consumed(),
            record.field("consumed") == "true",
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().updates,
            fixture_list(record.field("expected")),
            "case {}",
            record.id
        );
    }
}

struct ConflictingActions;

impl App for ConflictingActions {
    type Message = ();

    fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::text("conflict").on_actions(
            "owner",
            [
                Action::new(descriptor("app.first", 'x'), |_| EventResult::consumed()),
                Action::new(descriptor("app.second", 'x'), |_| EventResult::consumed()),
            ],
        )
    }
}

#[test]
fn runtime_rejects_action_conflicts_before_publishing_a_frame() {
    let mut runtime = Runtime::with_clock(
        ConflictingActions,
        nagi_tui::RuntimeConfig::new(Size::new(20, 1)),
        VirtualClock::new(),
    )
    .unwrap();
    let error = runtime.render_if_dirty().unwrap_err();
    let RuntimeError::BindingConflict(conflict) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.kind(), BindingConflictKind::AmbiguousBinding);
    assert_eq!(conflict.owner().as_str(), "owner");
    assert_eq!(
        conflict
            .actions()
            .iter()
            .map(ActionId::as_str)
            .collect::<Vec<_>>(),
        ["app.first", "app.second"]
    );
}

struct RouteConflictingActions;

impl App for RouteConflictingActions {
    type Message = ();

    fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let map = KeyMap::new()
            .rebind(ActionId::from("app.second"), [binding('x')])
            .unwrap();
        let child = Node::text("child")
            .focusable("child")
            .with_key_scope(KeyScope::new("child", map))
            .on_event("child", |_| EventResult::message(()));
        Node::padding(child, Insets::all(0))
            .on_event("root", |_| EventResult::message(()))
            .on_actions(
                "root",
                [
                    Action::new(descriptor("app.first", 'x'), |_| EventResult::consumed()),
                    Action::new(descriptor("app.second", 'y'), |_| EventResult::consumed()),
                ],
            )
    }
}

#[test]
fn runtime_rejects_route_specific_conflicts_before_any_handler() {
    let mut runtime = Runtime::with_clock(
        RouteConflictingActions,
        nagi_tui::RuntimeConfig::new(Size::new(20, 1)),
        VirtualClock::new(),
    )
    .unwrap();
    runtime.render_if_dirty().unwrap();
    runtime.request_focus(&NodeId::from("child")).unwrap();

    let error = runtime
        .dispatch_event(&key_routing_event("key/z"))
        .unwrap_err();

    let RuntimeEventError::Runtime(RuntimeError::BindingConflict(conflict)) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.owner().as_str(), "root");
    assert_eq!(
        conflict
            .scope_path()
            .iter()
            .map(NodeId::as_str)
            .collect::<Vec<_>>(),
        ["child"]
    );
    assert_eq!(runtime.queued_messages(), 0);
}

fn descriptor(id: &'static str, character: char) -> ActionDescriptor {
    ActionDescriptor::new(id, id, [binding(character)])
}

fn binding(character: char) -> KeyBinding {
    KeyBinding::new(KeyStroke::character(character, Modifiers::NONE))
}

fn message_action(id: &'static str, character: char, message: &'static str) -> Action<String> {
    routed_action(id, character, character, message)
}

fn routed_action(
    id: &'static str,
    default_character: char,
    event_character: char,
    message: &'static str,
) -> Action<String> {
    Action::new(descriptor(id, default_character), move |event| {
        assert_eq!(event.action().as_str(), id);
        assert_eq!(
            event.stroke(),
            KeyStroke::character(event_character, Modifiers::NONE)
        );
        EventResult::message(message.to_owned())
    })
}

fn ignored_action(id: &'static str, character: char, message: &'static str) -> Action<String> {
    Action::new(descriptor(id, character), move |event| {
        assert_eq!(event.action().as_str(), id);
        assert_eq!(
            event.stroke(),
            KeyStroke::character(character, Modifiers::NONE)
        );
        EventResult::ignored().emit(message.to_owned())
    })
}

fn key_routing_event(value: &str) -> Event {
    let (kind, scalar) = value.split_once('/').expect("fixture event has a slash");
    let character = scalar.chars().next().expect("fixture event has a scalar");
    match kind {
        "key" => Event::Key(KeyEvent {
            code: KeyCode::Character(character),
            modifiers: Modifiers::NONE,
            action: KeyAction::Press,
            text: Some(character.to_string()),
            protocol: KeyProtocol::Legacy,
        }),
        "text" => Event::Text(character.to_string()),
        "paste" => Event::Paste(character.to_string()),
        _ => panic!("unknown fixture event kind {kind}"),
    }
}

fn fixture_list(value: &str) -> Vec<String> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').map(str::to_owned).collect()
    }
}

fn number(value: &str) -> u32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
}
