//! Shared Tabs semantic-action integration tests

mod support;

use nagi_tui::{
    ActionAvailability, App, Effect, Event, Insets, KeyAction, KeyBinding, KeyCode, KeyEvent,
    KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers, MouseButton, MouseEvent, MouseKind, Node,
    NodeId, ResolvedActions, Runtime, Size, VirtualClock, resolve_actions,
};
use nagi_tui_widgets::{
    ACTIVATE_ACTION_ID, SELECTION_FIRST_ACTION_ID, SELECTION_LAST_ACTION_ID,
    SELECTION_NEXT_ACTION_ID, SELECTION_PREVIOUS_ACTION_ID, TabItem, Tabs,
};

struct TabsActionApp {
    count: usize,
    selected: usize,
    enabled: bool,
    key_map: KeyMap,
    messages: Vec<usize>,
}

impl App for TabsActionApp {
    type Message = usize;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let tabs = fixture_tabs(self.count, self.selected, self.enabled).into_node();
        Node::padding(tabs, Insets::all(0))
            .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn tabs_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/tabs-action.txt",
        "widget-tabs-action",
        &[
            "count",
            "selected",
            "focus",
            "mode",
            "enabled",
            "event",
            "message",
            "consumed",
            "focus-after",
            "item-keys",
            "root-keys",
            "available",
        ],
    ) else {
        return;
    };

    for record in records {
        let count = fixture_usize(record.field("count"));
        let selected = fixture_usize(record.field("selected"));
        let enabled = fixture_boolean(record.field("enabled"));
        let key_map = tabs_key_map(record.field("mode"));
        let mut runtime = Runtime::with_clock(
            TabsActionApp {
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
        let (item_actions, root_actions) = if interactive {
            let inspection_index = fixture_optional_usize(record.field("focus"))
                .unwrap_or_else(|| selected.min(count - 1));
            assert!(
                runtime.request_focus(&tab_id(inspection_index)).unwrap(),
                "case {}",
                record.id
            );
            let groups = runtime.active_action_groups().unwrap();
            assert_eq!(groups.len(), 2, "case {}", record.id);
            assert_eq!(
                groups[0].owner(),
                &tab_id(inspection_index),
                "case {}",
                record.id
            );
            assert_eq!(
                groups[1].owner(),
                &NodeId::from("tabs"),
                "case {}",
                record.id
            );
            (groups[0].clone(), groups[1].clone())
        } else {
            let tabs = fixture_tabs(count, selected, enabled);
            let item_descriptor = tabs.item_action_descriptor();
            let navigation_descriptors = tabs.navigation_action_descriptors();
            let scopes = [KeyScope::new("scope", key_map.clone())];
            (
                resolve_actions(&NodeId::from("tab-0"), &[item_descriptor], &scopes).unwrap(),
                resolve_actions(&NodeId::from("tabs"), &navigation_descriptors, &scopes).unwrap(),
            )
        };

        assert_action_group(
            &record.id,
            &item_actions,
            &[ACTIVATE_ACTION_ID],
            &["Activate"],
            &fixture_key_groups(record.field("item-keys")),
            fixture_boolean(record.field("available")),
        );
        assert_action_group(
            &record.id,
            &root_actions,
            &[
                SELECTION_PREVIOUS_ACTION_ID,
                SELECTION_NEXT_ACTION_ID,
                SELECTION_FIRST_ACTION_ID,
                SELECTION_LAST_ACTION_ID,
            ],
            &["Previous", "Next", "First", "Last"],
            &fixture_key_groups(record.field("root-keys")),
            fixture_boolean(record.field("available")),
        );

        match fixture_optional_usize(record.field("focus")) {
            Some(index) if interactive => {
                assert!(
                    runtime.request_focus(&tab_id(index)).unwrap(),
                    "case {}",
                    record.id
                );
            }
            _ => runtime.clear_focus(),
        }

        let dispatch = runtime
            .dispatch_event(&tabs_event(record.field("event")))
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
        let expected_focus = fixture_optional_usize(record.field("focus-after")).map(tab_id);
        assert_eq!(
            runtime.interaction().focused(),
            expected_focus.as_ref(),
            "case {}",
            record.id
        );
    }
}

fn fixture_tabs(count: usize, selected: usize, enabled: bool) -> Tabs<usize> {
    Tabs::new(
        "tabs",
        (0..count).map(|index| TabItem::new(tab_id(index), fixture_label(index))),
        selected,
        |index| index,
    )
    .enabled(enabled)
}

fn fixture_label(index: usize) -> String {
    char::from(b'A' + u8::try_from(index).unwrap()).to_string()
}

fn tab_id(index: usize) -> NodeId {
    NodeId::from(format!("tab-{index}"))
}

fn tabs_key_map(mode: &str) -> KeyMap {
    let character_binding = |character| {
        [KeyBinding::new(KeyStroke::character(
            character,
            Modifiers::NONE,
        ))]
    };
    match mode {
        "default" => KeyMap::new(),
        "activate-x" => KeyMap::new()
            .rebind(ACTIVATE_ACTION_ID, character_binding('x'))
            .unwrap(),
        "previous-k" => KeyMap::new()
            .rebind(SELECTION_PREVIOUS_ACTION_ID, character_binding('k'))
            .unwrap(),
        "unbind-next" => KeyMap::new()
            .rebind(SELECTION_NEXT_ACTION_ID, std::iter::empty())
            .unwrap(),
        "shadow-x" => KeyMap::new()
            .rebind(ACTIVATE_ACTION_ID, character_binding('x'))
            .unwrap()
            .rebind(SELECTION_PREVIOUS_ACTION_ID, character_binding('x'))
            .unwrap(),
        _ => panic!("unknown Tabs action mode {mode}"),
    }
}

fn tabs_event(value: &str) -> Event {
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
        "repeat-enter" => keyboard(KeyCode::Enter, Modifiers::NONE, KeyAction::Repeat),
        "release-enter" => keyboard(KeyCode::Enter, Modifiers::NONE, KeyAction::Release),
        "control-enter" => keyboard(
            KeyCode::Enter,
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "left" => keyboard(KeyCode::Left, Modifiers::NONE, KeyAction::Press),
        "right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Press),
        "home" => keyboard(KeyCode::Home, Modifiers::NONE, KeyAction::Press),
        "end" => keyboard(KeyCode::End, Modifiers::NONE, KeyAction::Press),
        "repeat-right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Repeat),
        "shift-right" => keyboard(
            KeyCode::Right,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "x" => keyboard(KeyCode::Character('x'), Modifiers::NONE, KeyAction::Press),
        "k" => keyboard(KeyCode::Character('k'), Modifiers::NONE, KeyAction::Press),
        pointer if pointer.starts_with("mouse-left-press-") => {
            let index = fixture_usize(pointer.trim_start_matches("mouse-left-press-"));
            Event::Mouse(MouseEvent {
                kind: MouseKind::Press,
                button: MouseButton::Left,
                x: u32::try_from(index * 3).unwrap(),
                y: 0,
                modifiers: Modifiers::NONE,
            })
        }
        _ => panic!("unknown Tabs action event {value}"),
    }
}

fn assert_action_group(
    case: &str,
    resolved: &ResolvedActions,
    expected_ids: &[&str],
    expected_labels: &[&str],
    expected_keys: &[Vec<String>],
    available: bool,
) {
    assert_eq!(resolved.actions().len(), expected_ids.len(), "case {case}");
    assert_eq!(expected_keys.len(), expected_ids.len(), "case {case}");
    for (index, action) in resolved.actions().iter().enumerate() {
        assert_eq!(action.id().as_str(), expected_ids[index], "case {case}");
        assert_eq!(action.label(), expected_labels[index], "case {case}");
        assert_eq!(
            action
                .bindings()
                .iter()
                .map(|binding| binding.stroke().notation())
                .collect::<Vec<_>>(),
            expected_keys[index],
            "case {case} action {}",
            expected_ids[index]
        );
        assert_eq!(
            action.availability() == ActionAvailability::Enabled,
            available,
            "case {case} action {}",
            expected_ids[index]
        );
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

fn fixture_optional_usize(value: &str) -> Option<usize> {
    (value != "none").then(|| fixture_usize(value))
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
