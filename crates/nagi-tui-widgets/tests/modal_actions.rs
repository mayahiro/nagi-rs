//! Shared Modal semantic-action integration tests

mod action_support;
mod support;

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, App, Effect, Event, EventResult, Insets,
    KeyAction, KeyBinding, KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope, KeyScopePropagation,
    KeyStroke, Modifiers, Node, NodeId, Runtime, Size, VirtualClock,
};
use nagi_tui_widgets::{DISMISS_ACTION_ID, Modal};

struct ModalActionApp {
    handler: bool,
    child: String,
    outer: String,
    key_map: KeyMap,
    propagation: KeyScopePropagation,
    messages: Vec<String>,
}

impl App for ModalActionApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let child = modal_child(self.child.as_str());
        let modal = Modal::new("modal", child);
        let modal = if self.handler {
            modal.on_escape(|| "dismiss".to_owned())
        } else {
            modal
        };
        let modal = modal.into_node().with_key_scope(
            KeyScope::new("modal", self.key_map.clone()).with_propagation(self.propagation),
        );
        let root = Node::padding(modal, Insets::all(0));
        match self.outer.as_str() {
            "none" => root,
            "action" => root.on_actions(
                "root",
                [Action::new(
                    fixture_action_descriptor("app.outer", "Outer"),
                    |_| EventResult::message("outer-action".to_owned()),
                )],
            ),
            "raw" => root.on_event("root", |_| EventResult::message("outer-raw".to_owned())),
            outer => panic!("unknown Modal outer behavior {outer}"),
        }
    }
}

#[test]
fn modal_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/modal-action.txt",
        "widget-modal-action",
        &[
            "handler",
            "focus",
            "mode",
            "propagation",
            "child",
            "outer",
            "event",
            "messages",
            "consumed",
            "owners",
            "keys",
            "available",
        ],
    ) else {
        return;
    };

    for record in records {
        let key_map = modal_key_map(record.field("mode"));
        let mut runtime = Runtime::with_clock(
            ModalActionApp {
                handler: fixture_boolean(record.field("handler")),
                child: record.field("child").to_owned(),
                outer: record.field("outer").to_owned(),
                key_map,
                propagation: fixture_propagation(record.field("propagation")),
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(24, 4)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        if record.field("focus") == "child" {
            assert!(
                runtime.request_focus(&NodeId::from("child")).unwrap(),
                "case {}",
                record.id
            );
        }

        let groups = action_support::node_declared_groups(runtime.active_action_groups().unwrap());
        assert_eq!(
            groups
                .iter()
                .map(|group| group.owner().as_str().to_owned())
                .collect::<Vec<_>>(),
            fixture_list(record.field("owners")),
            "case {}",
            record.id
        );
        let modal = groups
            .iter()
            .find(|group| group.owner() == &NodeId::from("modal"))
            .unwrap_or_else(|| panic!("case {} has no Modal action group", record.id));
        assert_eq!(modal.actions().len(), 1, "case {}", record.id);
        let dismiss = &modal.actions()[0];
        assert_eq!(
            dismiss.id().as_str(),
            DISMISS_ACTION_ID,
            "case {}",
            record.id
        );
        assert_eq!(dismiss.label(), "Dismiss", "case {}", record.id);
        assert_eq!(
            dismiss
                .bindings()
                .iter()
                .map(|binding| binding.stroke().notation())
                .collect::<Vec<_>>(),
            fixture_list(record.field("keys")),
            "case {}",
            record.id
        );
        assert_eq!(
            dismiss.availability() == ActionAvailability::Enabled,
            fixture_boolean(record.field("available")),
            "case {}",
            record.id
        );

        let dispatch = runtime
            .dispatch_event(&modal_event(record.field("event")))
            .unwrap();
        runtime.process_pending().unwrap();
        let expected_messages = fixture_list(record.field("messages"));
        assert_eq!(
            runtime.app().messages,
            expected_messages,
            "case {}",
            record.id
        );
        assert_eq!(
            dispatch.messages(),
            expected_messages.len(),
            "case {}",
            record.id
        );
        assert_eq!(
            dispatch.consumed(),
            fixture_boolean(record.field("consumed")),
            "case {}",
            record.id
        );
    }
}

fn modal_child(behavior: &str) -> Node<String> {
    let child = Node::text("child").focusable("child");
    match behavior {
        "none" => child,
        "action-consume" => child.on_actions(
            "child",
            [Action::new(
                fixture_action_descriptor("app.child", "Child"),
                |_| EventResult::message("child-action".to_owned()),
            )],
        ),
        "action-ignore" => child.on_actions(
            "child",
            [Action::new(
                fixture_action_descriptor("app.child", "Child"),
                |_| EventResult::ignored().emit("child-action".to_owned()),
            )],
        ),
        "raw-consume" => child.on_event("child", |_| EventResult::message("child-raw".to_owned())),
        "raw-ignore" => child.on_event("child", |_| {
            EventResult::ignored().emit("child-raw".to_owned())
        }),
        child => panic!("unknown Modal child behavior {child}"),
    }
}

fn fixture_action_descriptor(id: &str, label: &str) -> ActionDescriptor {
    ActionDescriptor::new(id, label, [modal_binding(KeyCode::Escape)])
}

fn modal_binding(code: KeyCode) -> KeyBinding {
    KeyBinding::new(KeyStroke::new(code, Modifiers::NONE))
}

fn modal_key_map(mode: &str) -> KeyMap {
    match mode {
        "default" => KeyMap::new(),
        "rebind-x" => KeyMap::new()
            .rebind(
                DISMISS_ACTION_ID,
                [KeyBinding::new(KeyStroke::character('x', Modifiers::NONE))],
            )
            .unwrap(),
        "unbound" => KeyMap::new()
            .rebind(DISMISS_ACTION_ID, std::iter::empty())
            .unwrap(),
        mode => panic!("unknown Modal action mode {mode}"),
    }
}

fn fixture_propagation(value: &str) -> KeyScopePropagation {
    match value {
        "continue" => KeyScopePropagation::Continue,
        "stop" => KeyScopePropagation::StopAtScope,
        value => panic!("unknown Modal propagation {value}"),
    }
}

fn modal_event(value: &str) -> Event {
    let keyboard = |code, modifiers, action| {
        Event::Key(KeyEvent {
            code,
            modifiers,
            action,
            text: None,
            protocol: KeyProtocol::Legacy,
        })
    };
    match value {
        "escape" => keyboard(KeyCode::Escape, Modifiers::NONE, KeyAction::Press),
        "repeat-escape" => keyboard(KeyCode::Escape, Modifiers::NONE, KeyAction::Repeat),
        "unknown-escape" => keyboard(KeyCode::Escape, Modifiers::NONE, KeyAction::Unknown),
        "release-escape" => keyboard(KeyCode::Escape, Modifiers::NONE, KeyAction::Release),
        "shift-escape" => keyboard(
            KeyCode::Escape,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "control-escape" => keyboard(
            KeyCode::Escape,
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "alt-escape" => keyboard(
            KeyCode::Escape,
            Modifiers {
                alt: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "meta-escape" => keyboard(
            KeyCode::Escape,
            Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "x" => keyboard(KeyCode::Character('x'), Modifiers::NONE, KeyAction::Press),
        event => panic!("unknown Modal action event {event}"),
    }
}

fn fixture_boolean(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        value => panic!("invalid fixture Boolean {value}"),
    }
}

fn fixture_list(value: &str) -> Vec<String> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').map(str::to_owned).collect()
    }
}
