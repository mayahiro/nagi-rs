//! Shared List and Table semantic-action integration tests

mod support;

use nagi_tui::{
    ActionAvailability, App, Effect, Event, Insets, KeyAction, KeyBinding, KeyCode, KeyEvent,
    KeyMap, KeyProtocol, KeyScope, KeyStroke, Length, Modifiers, MouseButton, MouseEvent,
    MouseKind, Node, NodeId, ResolvedActions, Runtime, Size, VirtualClock, resolve_actions,
};
use nagi_tui_widgets::{
    ACTIVATE_ACTION_ID, List, ListItem, SELECTION_FIRST_ACTION_ID, SELECTION_LAST_ACTION_ID,
    SELECTION_NEXT_ACTION_ID, SELECTION_PREVIOUS_ACTION_ID, Table, TableColumn, TableRow,
};

struct CollectionActionApp {
    widget: String,
    layout: String,
    view: String,
    count: usize,
    selected: usize,
    enabled: bool,
    key_map: KeyMap,
    messages: Vec<usize>,
}

impl App for CollectionActionApp {
    type Message = usize;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::padding(
            collection_node(
                &self.widget,
                &self.layout,
                &self.view,
                self.count,
                self.selected,
                self.enabled,
            ),
            Insets::all(0),
        )
        .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn list_and_table_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/list-table-action.txt",
        "widget-list-table-action",
        &[
            "widget",
            "layout",
            "view",
            "count",
            "selected",
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
        let widget = record.field("widget");
        let layout = record.field("layout");
        let view = record.field("view");
        let count = fixture_usize(record.field("count"));
        let selected = fixture_usize(record.field("selected"));
        let enabled = fixture_boolean(record.field("enabled"));
        let available = fixture_boolean(record.field("available"));
        let key_map = collection_key_map(record.field("mode"));
        let mut runtime = Runtime::with_clock(
            CollectionActionApp {
                widget: widget.to_owned(),
                layout: layout.to_owned(),
                view: view.to_owned(),
                count,
                selected,
                enabled,
                key_map: key_map.clone(),
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(30, 6)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        let focused = runtime.request_focus(&NodeId::from("root")).unwrap();
        assert_eq!(focused, available, "case {}", record.id);
        let resolved = if available {
            let groups = runtime.active_action_groups().unwrap();
            assert_eq!(groups.len(), 1, "case {}", record.id);
            assert_eq!(groups[0].owner().as_str(), "root", "case {}", record.id);
            groups[0].clone()
        } else {
            resolve_actions(
                &NodeId::from("root"),
                &collection_descriptors(widget, layout, view, count, selected, enabled),
                &[KeyScope::new("scope", key_map)],
            )
            .unwrap()
        };
        assert_collection_actions(
            &record.id,
            &resolved,
            &fixture_key_groups(record.field("keys")),
            available,
        );

        let pointer = record.field("event").starts_with("pointer-");
        if pointer {
            runtime.clear_focus();
        }
        let dispatch = runtime
            .dispatch_event(&collection_event(
                widget,
                view,
                count,
                record.field("event"),
            ))
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

fn collection_node(
    widget: &str,
    layout: &str,
    view: &str,
    count: usize,
    selected: usize,
    enabled: bool,
) -> Node<usize> {
    match widget {
        "list" => configured_list(layout, view, count, selected, enabled).into_node(),
        "table" => configured_table(layout, count, selected, enabled).into_node(),
        _ => panic!("unknown collection widget {widget}"),
    }
}

fn collection_descriptors(
    widget: &str,
    layout: &str,
    view: &str,
    count: usize,
    selected: usize,
    enabled: bool,
) -> [nagi_tui::ActionDescriptor; 5] {
    match widget {
        "list" => configured_list(layout, view, count, selected, enabled).action_descriptors(),
        "table" => configured_table(layout, count, selected, enabled).action_descriptors(),
        _ => panic!("unknown collection widget {widget}"),
    }
}

fn configured_list(
    layout: &str,
    view: &str,
    count: usize,
    selected: usize,
    enabled: bool,
) -> List<usize> {
    let mut list = List::new(
        "root",
        (0..count).map(|index| ListItem::new(format!("item-{index}"), fixture_label(index))),
        selected,
        |index| index,
    )
    .enabled(enabled);
    list = match view {
        "all" => list,
        "filter-p" => list.filter("p"),
        "filter-z" => list.filter("z"),
        "window-1-2" => list.window(1, 2),
        "window-empty" => list.window(0, 0),
        _ => panic!("unknown List view {view}"),
    };
    match layout {
        "eager" => list,
        "virtual" => list.viewport("viewport", Length::Fixed(2)),
        _ => panic!("unknown collection layout {layout}"),
    }
}

fn configured_table(layout: &str, count: usize, selected: usize, enabled: bool) -> Table<usize> {
    let table = Table::new(
        "root",
        [TableColumn::new("Value", Length::Fixed(8))],
        (0..count).map(|index| TableRow::new(format!("row-{index}"), [fixture_label(index)])),
        selected,
        |index| index,
    )
    .enabled(enabled);
    match layout {
        "eager" => table,
        "virtual" => table.viewport("viewport", Length::Fixed(2)),
        _ => panic!("unknown collection layout {layout}"),
    }
}

fn fixture_label(index: usize) -> String {
    ["Alpha", "Beta", "Alpine", "Gamma"]
        .get(index)
        .map_or_else(|| format!("Item {index}"), |label| (*label).to_owned())
}

fn collection_key_map(mode: &str) -> KeyMap {
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
        _ => panic!("unknown collection action mode {mode}"),
    }
}

fn collection_event(widget: &str, view: &str, count: usize, value: &str) -> Event {
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
        "up" => keyboard(KeyCode::Up, Modifiers::NONE, KeyAction::Press),
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
        pointer if pointer.starts_with("pointer-") => {
            let original = fixture_usize(pointer.trim_start_matches("pointer-"));
            let position = if widget == "list" {
                fixture_visible_indices(view, count)
                    .iter()
                    .position(|index| *index == original)
                    .unwrap_or(0)
            } else {
                original
            };
            Event::Mouse(MouseEvent {
                kind: MouseKind::Press,
                button: MouseButton::Left,
                x: 0,
                y: u32::try_from(position + usize::from(widget == "table")).unwrap(),
                modifiers: Modifiers::NONE,
            })
        }
        _ => panic!("unknown collection action event {value}"),
    }
}

fn fixture_visible_indices(view: &str, count: usize) -> Vec<usize> {
    match view {
        "all" => (0..count).collect(),
        "filter-p" => [0, 2].into_iter().filter(|index| *index < count).collect(),
        "filter-z" | "window-empty" => Vec::new(),
        "window-1-2" => (1..count.min(3)).collect(),
        _ => panic!("unknown List view {view}"),
    }
}

fn assert_collection_actions(
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
    ];
    let expected_labels = ["Activate", "Previous", "Next", "First", "Last"];
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
