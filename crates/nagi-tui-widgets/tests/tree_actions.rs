//! Shared Tree semantic-action integration tests

mod action_support;
mod support;

use nagi_tui::{
    ActionAvailability, App, Effect, Event, Insets, KeyAction, KeyBinding, KeyCode, KeyEvent,
    KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers, MouseButton, MouseEvent, MouseKind, Node,
    NodeId, ResolvedActions, Runtime, Size, VirtualClock, resolve_actions,
};
use nagi_tui_widgets::{
    ACTIVATE_ACTION_ID, COLLAPSE_ACTION_ID, EXPAND_ACTION_ID, SELECTION_FIRST_ACTION_ID,
    SELECTION_LAST_ACTION_ID, SELECTION_NEXT_ACTION_ID, SELECTION_PREVIOUS_ACTION_ID, Tree,
    TreeItem,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Message {
    Select(usize),
    Toggle(usize, bool),
}

struct TreeActionApp {
    layout: String,
    model: String,
    selected: usize,
    toggle: bool,
    enabled: bool,
    key_map: KeyMap,
    messages: Vec<Message>,
}

impl App for TreeActionApp {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::padding(
            configured_tree(
                &self.layout,
                &self.model,
                self.selected,
                self.toggle,
                self.enabled,
            )
            .into_node(),
            Insets::all(0),
        )
        .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn tree_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/tree-action.txt",
        "widget-tree-action",
        &[
            "layout",
            "model",
            "selected",
            "toggle",
            "mode",
            "enabled",
            "event",
            "message",
            "consumed",
            "focus",
            "keys",
            "available",
        ],
    ) else {
        return;
    };

    for record in records {
        let layout = record.field("layout");
        let model = record.field("model");
        let selected = fixture_usize(record.field("selected"));
        let toggle = fixture_boolean(record.field("toggle"));
        let enabled = fixture_boolean(record.field("enabled"));
        let available = fixture_boolean(record.field("available"));
        let key_map = tree_key_map(record.field("mode"));
        let mut runtime = Runtime::with_clock(
            TreeActionApp {
                layout: layout.to_owned(),
                model: model.to_owned(),
                selected,
                toggle,
                enabled,
                key_map: key_map.clone(),
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(30, 8)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        let focused = runtime.request_focus(&NodeId::from("root")).unwrap();
        assert_eq!(focused, available, "case {}", record.id);
        let resolved = if available {
            let groups =
                action_support::node_declared_groups(runtime.active_action_groups().unwrap());
            assert_eq!(groups.len(), 1, "case {}", record.id);
            assert_eq!(groups[0].owner().as_str(), "root", "case {}", record.id);
            groups[0].clone()
        } else {
            resolve_actions(
                &NodeId::from("root"),
                &configured_tree(layout, model, selected, toggle, enabled).action_descriptors(),
                &[KeyScope::new("scope", key_map)],
            )
            .unwrap()
        };
        assert_tree_actions(
            &record.id,
            &resolved,
            &fixture_key_groups(record.field("keys")),
            available,
        );

        let event_name = record.field("event");
        if event_name.starts_with("pointer-") {
            runtime.clear_focus();
        }
        let dispatch = runtime
            .dispatch_event(&tree_event(layout, model, selected, event_name))
            .unwrap();
        runtime.process_pending().unwrap();

        let expected_messages = fixture_messages(record.field("message"));
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
        let expected_focus = match record.field("focus") {
            "root" => Some(NodeId::from("root")),
            "none" => None,
            value => panic!("invalid focus {value}"),
        };
        assert_eq!(
            runtime.interaction().focused(),
            expected_focus.as_ref(),
            "case {}",
            record.id
        );
    }
}

fn configured_tree(
    layout: &str,
    model: &str,
    selected: usize,
    toggle: bool,
    enabled: bool,
) -> Tree<Message> {
    let mut tree = Tree::new("root", tree_items(model), selected, Message::Select).enabled(enabled);
    if toggle {
        tree = tree.on_toggle(Message::Toggle);
    }
    match layout {
        "full" => tree,
        "viewport" => tree.viewport(2),
        _ => panic!("unknown Tree layout {layout}"),
    }
}

fn tree_items(model: &str) -> Vec<TreeItem> {
    match model {
        "nested" => nested_tree(true),
        "nested-group-collapsed" => nested_tree(false),
        "root-collapsed" => vec![
            TreeItem::branch("item-0", "Root", 0, false),
            TreeItem::branch("item-1", "Group", 1, true),
            TreeItem::leaf("item-2", "Leaf", 2),
            TreeItem::leaf("item-3", "Sibling", 1),
            TreeItem::branch("item-4", "Peer", 0, false),
            TreeItem::leaf("item-5", "Hidden", 1),
            TreeItem::leaf("item-6", "Tail", 0),
        ],
        "flat" => (0..4)
            .map(|index| TreeItem::leaf(format!("item-{index}"), format!("Item {index}"), 0))
            .collect(),
        "empty" => Vec::new(),
        _ => panic!("unknown Tree model {model}"),
    }
}

fn nested_tree(group_expanded: bool) -> Vec<TreeItem> {
    vec![
        TreeItem::branch("item-0", "Root", 0, true),
        TreeItem::branch("item-1", "Group", 1, group_expanded),
        TreeItem::leaf("item-2", "Leaf", 2),
        TreeItem::leaf("item-3", "Sibling", 1),
        TreeItem::branch("item-4", "Peer", 0, false),
        TreeItem::leaf("item-5", "Hidden", 1),
        TreeItem::leaf("item-6", "Tail", 0),
    ]
}

fn tree_key_map(mode: &str) -> KeyMap {
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
        "collapse-h" => KeyMap::new()
            .rebind(COLLAPSE_ACTION_ID, character_binding('h'))
            .unwrap(),
        "expand-l" => KeyMap::new()
            .rebind(EXPAND_ACTION_ID, character_binding('l'))
            .unwrap(),
        "unbind-next" => KeyMap::new()
            .rebind(SELECTION_NEXT_ACTION_ID, std::iter::empty())
            .unwrap(),
        _ => panic!("unknown Tree action mode {mode}"),
    }
}

fn tree_event(layout: &str, model: &str, selected: usize, value: &str) -> Event {
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
        "control-enter" => keyboard(
            KeyCode::Enter,
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "up" => keyboard(KeyCode::Up, Modifiers::NONE, KeyAction::Press),
        "down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Press),
        "home" => keyboard(KeyCode::Home, Modifiers::NONE, KeyAction::Press),
        "end" => keyboard(KeyCode::End, Modifiers::NONE, KeyAction::Press),
        "left" => keyboard(KeyCode::Left, Modifiers::NONE, KeyAction::Press),
        "right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Press),
        "repeat-down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Repeat),
        "repeat-right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Repeat),
        "release-down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Release),
        "release-right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Release),
        "shift-down" => keyboard(
            KeyCode::Down,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "shift-right" => keyboard(
            KeyCode::Right,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "x" => keyboard(KeyCode::Character('x'), Modifiers::NONE, KeyAction::Press),
        "h" => keyboard(KeyCode::Character('h'), Modifiers::NONE, KeyAction::Press),
        "l" => keyboard(KeyCode::Character('l'), Modifiers::NONE, KeyAction::Press),
        pointer if pointer.starts_with("pointer-") => {
            let original = fixture_usize(pointer.trim_start_matches("pointer-"));
            let visible = fixture_visible_indices(model);
            let position = visible
                .iter()
                .position(|index| *index == original)
                .expect("visible pointer target");
            let selected_position = visible
                .iter()
                .rposition(|index| *index <= selected)
                .unwrap_or(0);
            let start = if layout == "viewport" {
                fixture_viewport_start(visible.len(), selected_position, 2)
            } else {
                0
            };
            assert!((start..start.saturating_add(2)).contains(&position) || layout == "full");
            Event::Mouse(MouseEvent {
                kind: MouseKind::Press,
                button: MouseButton::Left,
                x: 0,
                y: u32::try_from(position - start).unwrap(),
                modifiers: Modifiers::NONE,
            })
        }
        _ => panic!("unknown Tree action event {value}"),
    }
}

fn fixture_visible_indices(model: &str) -> Vec<usize> {
    match model {
        "nested" => vec![0, 1, 2, 3, 4, 6],
        "nested-group-collapsed" => vec![0, 1, 3, 4, 6],
        "root-collapsed" => vec![0, 4, 6],
        "flat" => vec![0, 1, 2, 3],
        "empty" => Vec::new(),
        _ => panic!("unknown Tree model {model}"),
    }
}

fn fixture_viewport_start(count: usize, selected: usize, height: usize) -> usize {
    let height = height.min(count);
    selected
        .saturating_sub(height / 2)
        .min(count.saturating_sub(height))
}

fn assert_tree_actions(
    case: &str,
    resolved: &ResolvedActions,
    expected_keys: &[Vec<String>],
    available: bool,
) {
    let expected_ids = [
        ACTIVATE_ACTION_ID,
        SELECTION_PREVIOUS_ACTION_ID,
        SELECTION_NEXT_ACTION_ID,
        SELECTION_FIRST_ACTION_ID,
        SELECTION_LAST_ACTION_ID,
        COLLAPSE_ACTION_ID,
        EXPAND_ACTION_ID,
    ];
    let expected_labels = [
        "Activate", "Previous", "Next", "First", "Last", "Collapse", "Expand",
    ];
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

fn fixture_messages(value: &str) -> Vec<Message> {
    if value == "-" {
        return Vec::new();
    }
    value
        .split(',')
        .map(|message| {
            let fields: Vec<_> = message.split(':').collect();
            match fields.as_slice() {
                ["select", index] => Message::Select(fixture_usize(index)),
                ["toggle", index, expanded] => {
                    Message::Toggle(fixture_usize(index), fixture_boolean(expanded))
                }
                _ => panic!("invalid Tree fixture message {message}"),
            }
        })
        .collect()
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
