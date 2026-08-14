//! Shared Checkbox and Radio semantic-action integration tests

mod support;

use nagi_tui::{
    ActionAvailability, App, Effect, Event, Insets, KeyAction, KeyBinding, KeyCode, KeyEvent,
    KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers, MouseButton, MouseEvent, MouseKind, Node,
    NodeId, Runtime, Size, VirtualClock, resolve_actions,
};
use nagi_tui_widgets::{ACTIVATE_ACTION_ID, Checkbox, Radio};

struct ChoiceActionApp {
    widget: String,
    state: bool,
    enabled: bool,
    key_map: KeyMap,
    messages: Vec<String>,
}

impl App for ChoiceActionApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let choice = match self.widget.as_str() {
            "checkbox" => Checkbox::new("choice", "Choice", self.state, |value| value.to_string())
                .enabled(self.enabled)
                .into_node(),
            "radio" => Radio::new("choice", "Choice", self.state, || "select".to_owned())
                .enabled(self.enabled)
                .into_node(),
            widget => panic!("unknown choice widget {widget}"),
        };
        Node::padding(choice, Insets::all(0))
            .with_key_scope(KeyScope::new("root", self.key_map.clone()))
    }
}

#[test]
fn choice_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/choice-action.txt",
        "widget-choice-action",
        &[
            "widget",
            "state",
            "mode",
            "enabled",
            "event",
            "message",
            "consumed",
            "keys",
            "available",
        ],
    ) else {
        return;
    };

    for record in records {
        let enabled = fixture_boolean(record.field("enabled"));
        let state = fixture_boolean(record.field("state"));
        let key_map = activation_key_map(record.field("mode"));
        let mut runtime = Runtime::with_clock(
            ChoiceActionApp {
                widget: record.field("widget").to_owned(),
                state,
                enabled,
                key_map: key_map.clone(),
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(20, 2)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        let resolved = if enabled {
            assert!(
                runtime.request_focus(&NodeId::from("choice")).unwrap(),
                "case {}",
                record.id
            );
            runtime
                .active_action_groups()
                .unwrap()
                .into_iter()
                .find(|group| group.owner().as_str() == "choice")
                .unwrap_or_else(|| panic!("case {} has no choice action group", record.id))
        } else {
            assert!(
                !runtime.request_focus(&NodeId::from("choice")).unwrap(),
                "case {}",
                record.id
            );
            let descriptor = choice_descriptor(record.field("widget"), state);
            resolve_actions(
                &NodeId::from("choice"),
                &[descriptor],
                &[KeyScope::new("root", key_map)],
            )
            .unwrap()
        };
        assert_eq!(resolved.actions().len(), 1, "case {}", record.id);
        let action = &resolved.actions()[0];
        assert_eq!(
            action.id().as_str(),
            ACTIVATE_ACTION_ID,
            "case {}",
            record.id
        );
        assert_eq!(
            action
                .bindings()
                .iter()
                .map(|binding| binding.stroke().notation())
                .collect::<Vec<_>>(),
            fixture_list(record.field("keys")),
            "case {}",
            record.id
        );
        assert_eq!(
            action.availability() == ActionAvailability::Enabled,
            fixture_boolean(record.field("available")),
            "case {}",
            record.id
        );

        let dispatch = runtime
            .dispatch_event(&activation_event(record.field("event")))
            .unwrap();
        runtime.process_pending().unwrap();
        let expected_messages = fixture_list(record.field("message"));
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

fn choice_descriptor(widget: &str, state: bool) -> nagi_tui::ActionDescriptor {
    match widget {
        "checkbox" => Checkbox::new("choice", "Choice", state, |value| value.to_string())
            .enabled(false)
            .action_descriptor(),
        "radio" => Radio::new("choice", "Choice", state, || "select".to_owned())
            .enabled(false)
            .action_descriptor(),
        _ => panic!("unknown choice widget {widget}"),
    }
}

fn activation_key_map(mode: &str) -> KeyMap {
    match mode {
        "default" => KeyMap::new(),
        "rebind-x" => KeyMap::new()
            .rebind(
                ACTIVATE_ACTION_ID,
                [KeyBinding::new(KeyStroke::character('x', Modifiers::NONE))],
            )
            .unwrap(),
        "unbound" => KeyMap::new()
            .rebind(ACTIVATE_ACTION_ID, std::iter::empty())
            .unwrap(),
        _ => panic!("unknown activation mode {mode}"),
    }
}

fn activation_event(value: &str) -> Event {
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
        "enter" => keyboard(KeyCode::Enter, Modifiers::NONE, KeyAction::Press),
        "space-text" => Event::Text(" ".to_owned()),
        "space-key" => keyboard(KeyCode::Character(' '), Modifiers::NONE, KeyAction::Press),
        "repeat-enter" => keyboard(KeyCode::Enter, Modifiers::NONE, KeyAction::Repeat),
        "repeat-space" => keyboard(KeyCode::Character(' '), Modifiers::NONE, KeyAction::Repeat),
        "release-enter" => keyboard(KeyCode::Enter, Modifiers::NONE, KeyAction::Release),
        "control-enter" => keyboard(
            KeyCode::Enter,
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "control-space" => keyboard(
            KeyCode::Character(' '),
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "x" => keyboard(KeyCode::Character('x'), Modifiers::NONE, KeyAction::Press),
        "mouse-left-press" => Event::Mouse(MouseEvent {
            kind: MouseKind::Press,
            button: MouseButton::Left,
            x: 0,
            y: 0,
            modifiers: Modifiers::NONE,
        }),
        _ => panic!("unknown activation event {value}"),
    }
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
