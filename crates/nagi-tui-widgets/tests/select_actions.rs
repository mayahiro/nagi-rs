//! Shared Select semantic-action integration tests

mod support;

use nagi_tui::{
    ActionAvailability, App, Effect, Event, Insets, KeyAction, KeyBinding, KeyCode, KeyEvent,
    KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers, MouseButton, MouseEvent, MouseKind, Node,
    NodeId, Runtime, Size, VirtualClock, resolve_actions,
};
use nagi_tui_widgets::{
    ACTIVATE_ACTION_ID, SELECTION_FIRST_ACTION_ID, SELECTION_LAST_ACTION_ID,
    SELECTION_NEXT_ACTION_ID, SELECTION_PREVIOUS_ACTION_ID, Select,
};

struct SelectActionApp {
    count: usize,
    selected: usize,
    enabled: bool,
    key_map: KeyMap,
    messages: Vec<usize>,
}

impl App for SelectActionApp {
    type Message = usize;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let select = Select::new(
            "select",
            (0..self.count).map(|index| format!("Option {index}")),
            self.selected,
            |index| index,
        )
        .enabled(self.enabled)
        .into_node();
        Node::padding(select, Insets::all(0))
            .with_key_scope(KeyScope::new("root", self.key_map.clone()))
    }
}

#[test]
fn select_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/select-action.txt",
        "widget-select-action",
        &[
            "count",
            "selected",
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
        let count = fixture_usize(record.field("count"));
        let selected = fixture_usize(record.field("selected"));
        let enabled = fixture_boolean(record.field("enabled"));
        let key_map = select_key_map(record.field("mode"));
        let mut runtime = Runtime::with_clock(
            SelectActionApp {
                count,
                selected,
                enabled,
                key_map: key_map.clone(),
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(24, 2)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        let interactive = enabled && count != 0;
        let resolved = if interactive {
            assert!(
                runtime.request_focus(&NodeId::from("select")).unwrap(),
                "case {}",
                record.id
            );
            runtime
                .active_action_groups()
                .unwrap()
                .into_iter()
                .find(|group| group.owner().as_str() == "select")
                .unwrap_or_else(|| panic!("case {} has no Select action group", record.id))
        } else {
            assert!(
                !runtime.request_focus(&NodeId::from("select")).unwrap(),
                "case {}",
                record.id
            );
            resolve_actions(
                &NodeId::from("select"),
                &select_descriptors(count, selected, enabled),
                &[KeyScope::new("root", key_map)],
            )
            .unwrap()
        };

        let expected_ids = [
            ACTIVATE_ACTION_ID,
            SELECTION_PREVIOUS_ACTION_ID,
            SELECTION_NEXT_ACTION_ID,
            SELECTION_FIRST_ACTION_ID,
            SELECTION_LAST_ACTION_ID,
        ];
        let expected_labels = ["Activate", "Previous", "Next", "First", "Last"];
        let expected_keys = fixture_key_groups(record.field("keys"));
        assert_eq!(
            resolved.actions().len(),
            expected_ids.len(),
            "case {}",
            record.id
        );
        assert_eq!(
            expected_keys.len(),
            expected_ids.len(),
            "case {}",
            record.id
        );
        for (index, action) in resolved.actions().iter().enumerate() {
            assert_eq!(
                action.id().as_str(),
                expected_ids[index],
                "case {}",
                record.id
            );
            assert_eq!(action.label(), expected_labels[index], "case {}", record.id);
            assert_eq!(
                action
                    .bindings()
                    .iter()
                    .map(|binding| binding.stroke().notation())
                    .collect::<Vec<_>>(),
                expected_keys[index],
                "case {} action {}",
                record.id,
                expected_ids[index]
            );
            assert_eq!(
                action.availability() == ActionAvailability::Enabled,
                fixture_boolean(record.field("available")),
                "case {} action {}",
                record.id,
                expected_ids[index]
            );
        }

        let pointer_event = record.field("event") == "mouse-left-press";
        if pointer_event && interactive {
            runtime.clear_focus();
        }
        let dispatch = runtime
            .dispatch_event(&select_event(record.field("event")))
            .unwrap();
        runtime.process_pending().unwrap();
        let expected_messages = fixture_usize_list(record.field("message"));
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
        if pointer_event {
            assert_eq!(
                runtime.interaction().focused() == Some(&NodeId::from("select")),
                interactive,
                "case {}",
                record.id
            );
        }
    }
}

fn select_descriptors(
    count: usize,
    selected: usize,
    enabled: bool,
) -> [nagi_tui::ActionDescriptor; 5] {
    Select::new(
        "select",
        (0..count).map(|index| format!("Option {index}")),
        selected,
        |index| index,
    )
    .enabled(enabled)
    .action_descriptors()
}

fn select_key_map(mode: &str) -> KeyMap {
    match mode {
        "default" => KeyMap::new(),
        "activate-x" => KeyMap::new()
            .rebind(
                ACTIVATE_ACTION_ID,
                [KeyBinding::new(KeyStroke::character('x', Modifiers::NONE))],
            )
            .unwrap(),
        "previous-k" => KeyMap::new()
            .rebind(
                SELECTION_PREVIOUS_ACTION_ID,
                [KeyBinding::new(KeyStroke::character('k', Modifiers::NONE))],
            )
            .unwrap(),
        "unbind-next" => KeyMap::new()
            .rebind(SELECTION_NEXT_ACTION_ID, std::iter::empty())
            .unwrap(),
        _ => panic!("unknown Select action mode {mode}"),
    }
}

fn select_event(value: &str) -> Event {
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
        "left" => keyboard(KeyCode::Left, Modifiers::NONE, KeyAction::Press),
        "up" => keyboard(KeyCode::Up, Modifiers::NONE, KeyAction::Press),
        "right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Press),
        "down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Press),
        "home" => keyboard(KeyCode::Home, Modifiers::NONE, KeyAction::Press),
        "end" => keyboard(KeyCode::End, Modifiers::NONE, KeyAction::Press),
        "repeat-down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Repeat),
        "release-down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Release),
        "shift-down" => keyboard(
            KeyCode::Down,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "control-enter" => keyboard(
            KeyCode::Enter,
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "x" => keyboard(KeyCode::Character('x'), Modifiers::NONE, KeyAction::Press),
        "k" => keyboard(KeyCode::Character('k'), Modifiers::NONE, KeyAction::Press),
        "mouse-left-press" => Event::Mouse(MouseEvent {
            kind: MouseKind::Press,
            button: MouseButton::Left,
            x: 0,
            y: 0,
            modifiers: Modifiers::NONE,
        }),
        _ => panic!("unknown Select action event {value}"),
    }
}

fn fixture_boolean(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        _ => panic!("invalid fixture Boolean {value}"),
    }
}

fn fixture_usize(value: &str) -> usize {
    value.parse().unwrap()
}

fn fixture_usize_list(value: &str) -> Vec<usize> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').map(fixture_usize).collect()
    }
}

fn fixture_key_groups(value: &str) -> Vec<Vec<String>> {
    value
        .split('|')
        .map(|group| {
            if group == "-" {
                Vec::new()
            } else {
                group.split(',').map(str::to_owned).collect()
            }
        })
        .collect()
}
