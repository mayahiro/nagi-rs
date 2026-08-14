//! Shared CommandPalette semantic-action integration tests

mod action_support;
mod support;

use nagi_tui::{
    ActionAvailability, App, BindingConflictKind, Effect, Event, Insets, KeyAction, KeyBinding,
    KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers, MouseButton,
    MouseEvent, MouseKind, Node, NodeId, ResolvedActions, Runtime, RuntimeError, Size,
    VirtualClock, resolve_actions,
};
use nagi_tui_widgets::{
    ACTIVATE_ACTION_ID, Command, CommandPalette, SELECTION_FIRST_ACTION_ID,
    SELECTION_LAST_ACTION_ID, SELECTION_NEXT_ACTION_ID, SELECTION_PREVIOUS_ACTION_ID,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum CommandPaletteActionMessage {
    Query(String),
    Select(usize),
    Activate(usize),
}

struct CommandPaletteActionApp {
    query: String,
    selected: usize,
    enabled: bool,
    key_map: KeyMap,
    messages: Vec<CommandPaletteActionMessage>,
}

impl App for CommandPaletteActionApp {
    type Message = CommandPaletteActionMessage;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match &message {
            CommandPaletteActionMessage::Query(query) => self.query.clone_from(query),
            CommandPaletteActionMessage::Select(index) => self.selected = *index,
            CommandPaletteActionMessage::Activate(_) => {}
        }
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let palette = fixture_palette(&self.query, self.selected, self.enabled).into_node();
        Node::padding(palette, Insets::all(0))
            .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn command_palette_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/command-palette-action.txt",
        "widget-command-palette-action",
        &[
            "query",
            "selected",
            "focus",
            "mode",
            "enabled",
            "event",
            "messages",
            "consumed",
            "focus-after",
            "query-after",
            "selected-after",
            "command-keys",
            "root-keys",
            "available",
            "owners",
            "cursor-after",
        ],
    ) else {
        return;
    };

    for record in records {
        let query = fixture_query(&record.text("query"));
        let selected = fixture_usize(record.field("selected"));
        let enabled = fixture_boolean(record.field("enabled"));
        let key_map = command_palette_key_map(record.field("mode"));
        let palette = fixture_palette(&query, selected, enabled);
        let command_descriptor = palette.command_action_descriptor();
        let root_descriptors = palette.action_descriptors();
        let scopes = [KeyScope::new("scope", key_map.clone())];
        let command_actions = resolve_actions(
            &NodeId::from("command-inspection"),
            &[command_descriptor],
            &scopes,
        )
        .unwrap();
        let root_actions =
            resolve_actions(&NodeId::from("palette"), &root_descriptors, &scopes).unwrap();
        assert_action_group(
            &record.id,
            &command_actions,
            &[ACTIVATE_ACTION_ID],
            &["Activate"],
            &[fixture_key_list(record.field("command-keys"))],
            fixture_boolean(record.field("available")),
        );
        assert_action_group(
            &record.id,
            &root_actions,
            &[
                ACTIVATE_ACTION_ID,
                SELECTION_PREVIOUS_ACTION_ID,
                SELECTION_NEXT_ACTION_ID,
                SELECTION_FIRST_ACTION_ID,
                SELECTION_LAST_ACTION_ID,
            ],
            &["Activate", "Previous", "Next", "First", "Last"],
            &fixture_key_groups(record.field("root-keys")),
            fixture_boolean(record.field("available")),
        );

        let mut runtime = Runtime::with_clock(
            CommandPaletteActionApp {
                query,
                selected,
                enabled,
                key_map,
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(32, 8)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        match record.field("focus") {
            "none" => runtime.clear_focus(),
            value => assert!(
                runtime.request_focus(&fixture_focus_id(value)).unwrap(),
                "case {}",
                record.id
            ),
        }
        let groups = action_support::node_declared_groups(runtime.active_action_groups().unwrap());
        assert_eq!(
            groups
                .iter()
                .map(|group| group.owner().as_str())
                .collect::<Vec<_>>(),
            fixture_string_list(record.field("owners")),
            "case {}",
            record.id
        );

        let dispatch = runtime
            .dispatch_event(&command_palette_event(record.field("event")))
            .unwrap();
        runtime.process_pending().unwrap();

        assert_eq!(
            runtime
                .app()
                .messages
                .iter()
                .map(command_palette_message_name)
                .collect::<Vec<_>>(),
            fixture_string_list(&record.text("messages")),
            "case {}",
            record.id
        );
        assert_eq!(
            dispatch.messages(),
            fixture_string_list(&record.text("messages")).len(),
            "case {}",
            record.id
        );
        assert_eq!(
            dispatch.consumed(),
            fixture_boolean(record.field("consumed")),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.interaction().focused(),
            fixture_optional_focus_id(record.field("focus-after")).as_ref(),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().query,
            fixture_query(&record.text("query-after")),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().selected,
            fixture_usize(record.field("selected-after")),
            "case {}",
            record.id
        );
        if record.field("cursor-after") != "-" {
            assert_eq!(
                runtime
                    .interaction()
                    .text_input(&NodeId::from("query"))
                    .unwrap()
                    .cursor(),
                fixture_usize(record.field("cursor-after")),
                "case {}",
                record.id
            );
        }
    }
}

#[test]
fn command_palette_root_conflict_is_reported_before_dispatch() {
    let binding = || {
        KeyBinding::new(KeyStroke::character(
            'k',
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
        ))
    };
    let key_map = KeyMap::new()
        .rebind(ACTIVATE_ACTION_ID, [binding()])
        .unwrap()
        .rebind(SELECTION_PREVIOUS_ACTION_ID, [binding()])
        .unwrap();
    let mut runtime = Runtime::with_clock(
        CommandPaletteActionApp {
            query: String::new(),
            selected: 0,
            enabled: true,
            key_map,
            messages: Vec::new(),
        },
        nagi_tui::RuntimeConfig::new(Size::new(32, 8)),
        VirtualClock::new(),
    )
    .unwrap();

    let error = runtime.render_if_dirty().unwrap_err();
    let RuntimeError::BindingConflict(conflict) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.kind(), BindingConflictKind::AmbiguousBinding);
    assert_eq!(conflict.owner().as_str(), "palette");
    assert_eq!(
        conflict
            .actions()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>(),
        [ACTIVATE_ACTION_ID, SELECTION_PREVIOUS_ACTION_ID]
    );
    assert!(runtime.app().messages.is_empty());
}

fn fixture_palette(
    query: &str,
    selected: usize,
    enabled: bool,
) -> CommandPalette<CommandPaletteActionMessage> {
    CommandPalette::new(
        "palette",
        "query",
        query,
        [
            Command::new("command-0", "Alpha").keywords(["first"]),
            Command::new("command-1", "Beta").keywords(["second"]),
            Command::new("command-2", "Alpine").keywords(["peak"]),
            Command::new("command-3", "Gamma").keywords(["third"]),
        ],
        selected,
        CommandPaletteActionMessage::Query,
        CommandPaletteActionMessage::Select,
        CommandPaletteActionMessage::Activate,
    )
    .enabled(enabled)
}

fn command_palette_key_map(mode: &str) -> KeyMap {
    let control = Modifiers {
        control: true,
        ..Modifiers::NONE
    };
    match mode {
        "default" => KeyMap::new(),
        "activate-control-enter" => KeyMap::new()
            .rebind(
                ACTIVATE_ACTION_ID,
                [KeyBinding::new(KeyStroke::new(KeyCode::Enter, control))],
            )
            .unwrap(),
        "activate-x" => KeyMap::new()
            .rebind(
                ACTIVATE_ACTION_ID,
                [KeyBinding::new(KeyStroke::character('x', Modifiers::NONE))],
            )
            .unwrap(),
        "next-control-j" => KeyMap::new()
            .rebind(
                SELECTION_NEXT_ACTION_ID,
                [KeyBinding::new(KeyStroke::character('j', control))],
            )
            .unwrap(),
        "unbind-next" => KeyMap::new()
            .rebind(SELECTION_NEXT_ACTION_ID, std::iter::empty())
            .unwrap(),
        _ => panic!("unknown CommandPalette action mode {mode}"),
    }
}

fn command_palette_event(value: &str) -> Event {
    let keyboard = |code, modifiers, action| {
        Event::Key(KeyEvent {
            code,
            modifiers,
            action,
            text: None,
            protocol: KeyProtocol::Legacy,
        })
    };
    let control = Modifiers {
        control: true,
        ..Modifiers::NONE
    };
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    match value {
        "enter" => keyboard(KeyCode::Enter, Modifiers::NONE, KeyAction::Press),
        "repeat-enter" => keyboard(KeyCode::Enter, Modifiers::NONE, KeyAction::Repeat),
        "release-enter" => keyboard(KeyCode::Enter, Modifiers::NONE, KeyAction::Release),
        "shift-enter" => keyboard(KeyCode::Enter, shift, KeyAction::Press),
        "control-enter" => keyboard(KeyCode::Enter, control, KeyAction::Press),
        "up" => keyboard(KeyCode::Up, Modifiers::NONE, KeyAction::Press),
        "down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Press),
        "repeat-down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Repeat),
        "shift-down" => keyboard(KeyCode::Down, shift, KeyAction::Press),
        "home" => keyboard(KeyCode::Home, Modifiers::NONE, KeyAction::Press),
        "end" => keyboard(KeyCode::End, Modifiers::NONE, KeyAction::Press),
        "control-j" => keyboard(KeyCode::Character('j'), control, KeyAction::Press),
        "space-text" => Event::Text(" ".to_owned()),
        "text-x" => Event::Text("x".to_owned()),
        pointer if pointer.starts_with("mouse-left-press-") => {
            let position = fixture_usize(pointer.trim_start_matches("mouse-left-press-"));
            Event::Mouse(MouseEvent {
                kind: MouseKind::Press,
                button: MouseButton::Left,
                x: 2,
                y: u32::try_from(position + 2).unwrap(),
                modifiers: Modifiers::NONE,
            })
        }
        _ => panic!("unknown CommandPalette action event {value}"),
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
        let expected_availability = if available {
            ActionAvailability::Enabled
        } else {
            ActionAvailability::DisabledPassThrough
        };
        assert_eq!(
            action.availability(),
            expected_availability,
            "case {case} action {}",
            expected_ids[index]
        );
    }
}

fn command_palette_message_name(message: &CommandPaletteActionMessage) -> String {
    match message {
        CommandPaletteActionMessage::Query(query) => format!("query:{query}"),
        CommandPaletteActionMessage::Select(index) => format!("select:{index}"),
        CommandPaletteActionMessage::Activate(index) => format!("activate:{index}"),
    }
}

fn fixture_focus_id(value: &str) -> NodeId {
    match value {
        "query" => NodeId::from("query"),
        value if value.starts_with("command-") => NodeId::from(value.to_owned()),
        _ => panic!("invalid focus {value}"),
    }
}

fn fixture_optional_focus_id(value: &str) -> Option<NodeId> {
    (value != "none").then(|| fixture_focus_id(value))
}

fn fixture_query(value: &str) -> String {
    if value == "-" {
        String::new()
    } else {
        value.to_owned()
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

fn fixture_string_list(value: &str) -> Vec<String> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').map(str::to_owned).collect()
    }
}

fn fixture_key_list(value: &str) -> Vec<String> {
    fixture_string_list(value)
}

fn fixture_key_groups(value: &str) -> Vec<Vec<String>> {
    value.split('|').map(fixture_key_list).collect()
}
