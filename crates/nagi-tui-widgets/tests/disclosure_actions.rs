//! Shared Disclosure semantic-action and lazy-body integration tests

mod support;

use std::cell::Cell;
use std::rc::Rc;

use nagi_tui::{
    ActionAvailability, App, Effect, Event, EventDispatch, Insets, KeyAction, KeyBinding, KeyCode,
    KeyEvent, KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers, MouseButton, MouseEvent,
    MouseKind, Node, NodeId, Runtime, Size, VirtualClock,
};
use nagi_tui_widgets::{ACTIVATE_ACTION_ID, COLLAPSE_ACTION_ID, Disclosure, EXPAND_ACTION_ID};

struct DisclosureActionApp {
    expanded: bool,
    enabled: bool,
    body: bool,
    key_map: KeyMap,
    outer: bool,
    body_builds: Rc<Cell<usize>>,
    messages: Vec<String>,
}

impl App for DisclosureActionApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if message == "true" || message == "false" {
            self.expanded = message == "true";
        }
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let mut disclosure = Disclosure::new(
            "disclosure",
            Node::text("Summary"),
            self.expanded,
            |expanded| expanded.to_string(),
        )
        .enabled(self.enabled);
        if self.body {
            let builds = Rc::clone(&self.body_builds);
            disclosure = disclosure.body(move || {
                builds.set(builds.get() + 1);
                Node::text("Details").focusable("body")
            });
        }
        let scoped = Node::padding(disclosure.into_node(), Insets::all(0))
            .with_key_scope(KeyScope::new("scope", self.key_map.clone()));
        if self.outer {
            Node::padding(scoped, Insets::all(0))
                .on_event("root", |_| nagi_tui::EventResult::message("raw".to_owned()))
        } else {
            scoped
        }
    }
}

#[test]
fn disclosure_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/disclosure.txt",
        "widget-disclosure",
        &[
            "expanded",
            "enabled",
            "body",
            "event",
            "mode",
            "outer",
            "message",
            "consumed",
            "focus",
            "expected-expanded",
            "builds",
            "marker",
            "availability",
        ],
    ) else {
        return;
    };

    for record in records {
        let expanded = fixture_boolean(record.field("expanded"));
        let enabled = fixture_boolean(record.field("enabled"));
        let descriptors = Disclosure::new(
            "disclosure",
            Node::<String>::text("Summary"),
            expanded,
            |next| next.to_string(),
        )
        .enabled(enabled)
        .action_descriptors();
        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.id().as_str())
                .collect::<Vec<_>>(),
            [ACTIVATE_ACTION_ID, COLLAPSE_ACTION_ID, EXPAND_ACTION_ID],
            "case {} action IDs",
            record.id
        );
        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| fixture_availability(descriptor.availability()))
                .collect::<Vec<_>>(),
            fixture_list(record.field("availability")),
            "case {} availability",
            record.id
        );

        let body_builds = Rc::new(Cell::new(0));
        let mut runtime = Runtime::with_clock(
            DisclosureActionApp {
                expanded,
                enabled,
                body: fixture_boolean(record.field("body")),
                key_map: disclosure_key_map(record.field("mode")),
                outer: record.field("outer") == "raw",
                body_builds: Rc::clone(&body_builds),
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(20, 3)),
            VirtualClock::new(),
        )
        .unwrap();
        let mut frame = runtime.render_if_dirty().unwrap().unwrap();
        let mut dispatch = None;

        match record.field("event") {
            "none" => {}
            "external-collapse" => {
                assert!(runtime.request_focus(&NodeId::from("body")).unwrap());
                runtime.app_mut().expanded = false;
                runtime.request_frame();
                frame = runtime.render_if_dirty().unwrap().unwrap();
            }
            event => {
                if event != "pointer" && enabled {
                    assert!(runtime.request_focus(&NodeId::from("disclosure")).unwrap());
                }
                let event_dispatch = runtime.dispatch_event(&disclosure_event(event)).unwrap();
                runtime.process_pending().unwrap();
                if let Some(rendered) = runtime.render_if_dirty().unwrap() {
                    frame = rendered;
                }
                dispatch = Some(event_dispatch);
            }
        }

        let expected_messages = fixture_list(record.field("message"));
        assert_eq!(
            runtime.app().messages,
            expected_messages,
            "case {} messages",
            record.id
        );
        assert_dispatch(
            dispatch.as_ref(),
            expected_messages.len(),
            fixture_boolean(record.field("consumed")),
            &record.id,
        );
        assert_eq!(
            runtime.app().expanded,
            fixture_boolean(record.field("expected-expanded")),
            "case {} expanded",
            record.id
        );
        assert_eq!(
            body_builds.get(),
            fixture_usize(record.field("builds")),
            "case {} body builds",
            record.id
        );
        let marker = frame
            .surface()
            .cell(0, 0)
            .expect("Disclosure marker cell")
            .content();
        assert_eq!(marker, record.field("marker"), "case {} marker", record.id);
        let actual_focus = runtime.interaction().focused().map(NodeId::as_str);
        let expected_focus = match record.field("focus") {
            "none" => None,
            focus => Some(focus),
        };
        assert_eq!(actual_focus, expected_focus, "case {} focus", record.id);
    }
}

struct NestedDisclosureApp {
    outer: bool,
    inner: bool,
}

impl App for NestedDisclosureApp {
    type Message = (bool, bool);

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.outer = message.0;
        self.inner = message.1;
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let inner = self.inner;
        Disclosure::new("outer", Node::text("Outer"), self.outer, move |expanded| {
            (expanded, inner)
        })
        .body({
            let outer = self.outer;
            let inner = self.inner;
            move || {
                Disclosure::new("inner", Node::text("Inner"), inner, move |expanded| {
                    (outer, expanded)
                })
                .body(|| Node::text("Nested details").focusable("inner-body"))
                .into_node()
            }
        })
        .into_node()
    }
}

#[test]
fn nested_disclosure_uses_nearest_focus_fallback() {
    let mut runtime = Runtime::with_clock(
        NestedDisclosureApp {
            outer: true,
            inner: true,
        },
        nagi_tui::RuntimeConfig::new(Size::new(20, 4)),
        VirtualClock::new(),
    )
    .unwrap();
    runtime.render_if_dirty().unwrap();
    assert!(runtime.request_focus(&NodeId::from("inner-body")).unwrap());

    runtime.app_mut().inner = false;
    runtime.request_frame();
    runtime.render_if_dirty().unwrap();
    assert_eq!(
        runtime.interaction().focused(),
        Some(&NodeId::from("inner"))
    );

    runtime.app_mut().outer = false;
    runtime.request_frame();
    runtime.render_if_dirty().unwrap();
    assert_eq!(
        runtime.interaction().focused(),
        Some(&NodeId::from("outer"))
    );
}

fn disclosure_key_map(mode: &str) -> KeyMap {
    match mode {
        "default" => KeyMap::new(),
        "rebind" => KeyMap::new()
            .rebind(
                ACTIVATE_ACTION_ID,
                [KeyBinding::new(KeyStroke::character('x', Modifiers::NONE))],
            )
            .unwrap(),
        "unbind" => KeyMap::new()
            .rebind(ACTIVATE_ACTION_ID, std::iter::empty())
            .unwrap(),
        _ => panic!("unknown Disclosure key mode {mode}"),
    }
}

fn disclosure_event(value: &str) -> Event {
    let key = |code| {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers::NONE,
            action: KeyAction::Press,
            text: None,
            protocol: KeyProtocol::Legacy,
        })
    };
    match value {
        "enter" => key(KeyCode::Enter),
        "space" => key(KeyCode::Character(' ')),
        "left" => key(KeyCode::Left),
        "right" => key(KeyCode::Right),
        "x" => key(KeyCode::Character('x')),
        "pointer" => Event::Mouse(MouseEvent {
            kind: MouseKind::Press,
            button: MouseButton::Left,
            x: 0,
            y: 0,
            modifiers: Modifiers::NONE,
        }),
        _ => panic!("unknown Disclosure event {value}"),
    }
}

fn assert_dispatch(dispatch: Option<&EventDispatch>, messages: usize, consumed: bool, case: &str) {
    match dispatch {
        Some(dispatch) => {
            assert_eq!(
                dispatch.messages(),
                messages,
                "case {case} dispatch messages"
            );
            assert_eq!(dispatch.consumed(), consumed, "case {case} consumed");
        }
        None => {
            assert_eq!(messages, 0, "case {case} missing dispatch messages");
            assert!(!consumed, "case {case} missing dispatch consumed");
        }
    }
}

fn fixture_availability(value: ActionAvailability) -> String {
    match value {
        ActionAvailability::Enabled => "enabled",
        ActionAvailability::DisabledPassThrough => "pass",
        ActionAvailability::DisabledConsume => "consume",
    }
    .to_owned()
}

fn fixture_boolean(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        _ => panic!("invalid fixture Boolean {value}"),
    }
}

fn fixture_list(value: &str) -> Vec<String> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').map(str::to_owned).collect()
    }
}

fn fixture_usize(value: &str) -> usize {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid fixture usize {value}: {error}"))
}
