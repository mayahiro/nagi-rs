//! Shared TextArea semantic-action integration tests

mod action_support;
mod support;

use nagi_tui::{
    ActionAvailability, App, BindingConflictKind, Effect, Event, Insets, KeyAction, KeyBinding,
    KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers, Node, NodeId,
    ResolvedActions, Runtime, RuntimeError, Size, TEXT_CURSOR_DOWN_ACTION_ID,
    TEXT_CURSOR_LEFT_ACTION_ID, TEXT_CURSOR_LINE_END_ACTION_ID, TEXT_CURSOR_LINE_START_ACTION_ID,
    TEXT_CURSOR_RIGHT_ACTION_ID, TEXT_CURSOR_UP_ACTION_ID, TEXT_DELETE_BACKWARD_ACTION_ID,
    TEXT_DELETE_FORWARD_ACTION_ID, TEXT_INSERT_LINE_BREAK_ACTION_ID, TEXT_REDO_ACTION_ID,
    TEXT_SELECT_ALL_ACTION_ID, TEXT_SELECTION_EXTEND_DOWN_ACTION_ID,
    TEXT_SELECTION_EXTEND_LEFT_ACTION_ID, TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID,
    TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID, TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID,
    TEXT_SELECTION_EXTEND_UP_ACTION_ID, TEXT_UNDO_ACTION_ID, VirtualClock, resolve_actions,
};
use nagi_tui_widgets::{TextArea, TextAreaState};

#[derive(Clone, Debug, Eq, PartialEq)]
enum TextAreaActionMessage {
    Change(TextAreaState),
    Undo,
    Redo,
}

struct TextAreaActionApp {
    state: TextAreaState,
    handlers: String,
    enabled: bool,
    key_map: KeyMap,
    messages: Vec<TextAreaActionMessage>,
}

impl App for TextAreaActionApp {
    type Message = TextAreaActionMessage;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if let TextAreaActionMessage::Change(state) = &message {
            self.state = state.clone();
        }
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let mut area = TextArea::new("area", self.state.clone(), TextAreaActionMessage::Change)
            .enabled(self.enabled);
        if has_handler(&self.handlers, "undo") {
            area = area.on_undo(|| TextAreaActionMessage::Undo);
        }
        if has_handler(&self.handlers, "redo") {
            area = area.on_redo(|| TextAreaActionMessage::Redo);
        }
        Node::padding(area.into_node(), Insets::all(0))
            .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn text_area_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/text-area-action.txt",
        "widget-text-area-action",
        &[
            "initial",
            "cursor",
            "anchor",
            "handlers",
            "mode",
            "enabled",
            "event",
            "message",
            "expected",
            "expected-cursor",
            "expected-anchor",
            "consumed",
            "focus",
        ],
    ) else {
        return;
    };

    for record in records {
        let handlers = record.field("handlers").to_owned();
        let enabled = fixture_boolean(record.field("enabled"));
        let key_map = text_area_key_map(record.field("mode"));
        let mut runtime = Runtime::with_clock(
            TextAreaActionApp {
                state: fixture_state(
                    record.text("initial"),
                    record.field("cursor"),
                    record.field("anchor"),
                ),
                handlers: handlers.clone(),
                enabled,
                key_map: key_map.clone(),
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(24, 6)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        let resolved = if enabled {
            assert!(
                runtime.request_focus(&NodeId::from("area")).unwrap(),
                "case {}",
                record.id
            );
            let groups =
                action_support::node_declared_groups(runtime.active_action_groups().unwrap());
            assert_eq!(groups.len(), 1, "case {}", record.id);
            assert_eq!(groups[0].owner().as_str(), "area", "case {}", record.id);
            groups[0].clone()
        } else {
            assert!(
                !runtime.request_focus(&NodeId::from("area")).unwrap(),
                "case {}",
                record.id
            );
            resolve_actions(
                &NodeId::from("area"),
                &text_area_descriptors(enabled, &handlers),
                &[KeyScope::new("scope", key_map)],
            )
            .unwrap()
        };
        assert_text_area_actions(
            &resolved,
            enabled,
            &handlers,
            record.field("mode"),
            &record.id,
        );

        let dispatch = runtime
            .dispatch_event(&text_area_event(record.field("event")))
            .unwrap();
        runtime.process_pending().unwrap();

        assert_eq!(
            runtime
                .app()
                .messages
                .iter()
                .map(text_area_message_name)
                .collect::<Vec<_>>(),
            fixture_message_names(record.field("message")),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().state,
            fixture_state(
                record.text("expected"),
                record.field("expected-cursor"),
                record.field("expected-anchor"),
            ),
            "case {}",
            record.id
        );
        assert_eq!(
            dispatch.messages(),
            fixture_message_names(record.field("message")).len(),
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
            runtime.interaction().focused() == Some(&NodeId::from("area")),
            record.field("focus") == "area",
            "case {}",
            record.id
        );
    }
}

#[test]
fn text_area_same_owner_conflict_is_reported_before_editing() {
    let binding = || KeyBinding::new(KeyStroke::character('x', Modifiers::NONE));
    let key_map = KeyMap::new()
        .rebind(TEXT_CURSOR_LEFT_ACTION_ID, [binding()])
        .unwrap()
        .rebind(TEXT_CURSOR_RIGHT_ACTION_ID, [binding()])
        .unwrap();
    let mut runtime = Runtime::with_clock(
        TextAreaActionApp {
            state: TextAreaState::new("ab", 1),
            handlers: "both".to_owned(),
            enabled: true,
            key_map,
            messages: Vec::new(),
        },
        nagi_tui::RuntimeConfig::new(Size::new(12, 2)),
        VirtualClock::new(),
    )
    .unwrap();
    let error = runtime.render_if_dirty().unwrap_err();
    let RuntimeError::BindingConflict(conflict) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.kind(), BindingConflictKind::AmbiguousBinding);
    assert_eq!(conflict.owner().as_str(), "area");
    assert_eq!(
        conflict
            .actions()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>(),
        [TEXT_CURSOR_LEFT_ACTION_ID, TEXT_CURSOR_RIGHT_ACTION_ID]
    );
    assert!(runtime.app().messages.is_empty());
    assert_eq!(runtime.app().state, TextAreaState::new("ab", 1));
}

#[test]
fn text_area_history_availability_controls_conflicts() {
    let key_map = KeyMap::new()
        .rebind(
            TEXT_UNDO_ACTION_ID,
            [KeyBinding::new(KeyStroke::new(
                KeyCode::Left,
                Modifiers::NONE,
            ))],
        )
        .unwrap();
    let mut available_runtime = Runtime::with_clock(
        TextAreaActionApp {
            state: TextAreaState::new("ab", 1),
            handlers: "redo".to_owned(),
            enabled: true,
            key_map: key_map.clone(),
            messages: Vec::new(),
        },
        nagi_tui::RuntimeConfig::new(Size::new(12, 2)),
        VirtualClock::new(),
    )
    .unwrap();
    available_runtime.render_if_dirty().unwrap();
    available_runtime
        .request_focus(&NodeId::from("area"))
        .unwrap();
    available_runtime
        .dispatch_event(&text_area_event("left"))
        .unwrap();
    available_runtime.process_pending().unwrap();
    assert_eq!(available_runtime.app().state, TextAreaState::new("ab", 0));

    let mut conflicting_runtime = Runtime::with_clock(
        TextAreaActionApp {
            state: TextAreaState::new("ab", 1),
            handlers: "both".to_owned(),
            enabled: true,
            key_map,
            messages: Vec::new(),
        },
        nagi_tui::RuntimeConfig::new(Size::new(12, 2)),
        VirtualClock::new(),
    )
    .unwrap();
    let error = conflicting_runtime.render_if_dirty().unwrap_err();
    let RuntimeError::BindingConflict(conflict) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.kind(), BindingConflictKind::AmbiguousBinding);
    assert_eq!(
        conflict
            .actions()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>(),
        [TEXT_CURSOR_LEFT_ACTION_ID, TEXT_UNDO_ACTION_ID]
    );
}

fn text_area_descriptors(enabled: bool, handlers: &str) -> [nagi_tui::ActionDescriptor; 18] {
    let mut area = TextArea::new(
        "area",
        TextAreaState::default(),
        TextAreaActionMessage::Change,
    )
    .enabled(enabled);
    if has_handler(handlers, "undo") {
        area = area.on_undo(|| TextAreaActionMessage::Undo);
    }
    if has_handler(handlers, "redo") {
        area = area.on_redo(|| TextAreaActionMessage::Redo);
    }
    area.action_descriptors()
}

fn text_area_key_map(mode: &str) -> KeyMap {
    match mode {
        "default" => KeyMap::new(),
        "line-break-x" => KeyMap::new()
            .rebind(
                TEXT_INSERT_LINE_BREAK_ACTION_ID,
                [KeyBinding::new(KeyStroke::character('x', Modifiers::NONE))],
            )
            .unwrap(),
        "unbind-delete-forward" => KeyMap::new()
            .rebind(TEXT_DELETE_FORWARD_ACTION_ID, std::iter::empty())
            .unwrap(),
        _ => panic!("unknown TextArea action mode {mode}"),
    }
}

fn assert_text_area_actions(
    resolved: &ResolvedActions,
    enabled: bool,
    handlers: &str,
    mode: &str,
    case: &str,
) {
    let expected_ids = [
        TEXT_CURSOR_LEFT_ACTION_ID,
        TEXT_CURSOR_RIGHT_ACTION_ID,
        TEXT_CURSOR_UP_ACTION_ID,
        TEXT_CURSOR_DOWN_ACTION_ID,
        TEXT_CURSOR_LINE_START_ACTION_ID,
        TEXT_CURSOR_LINE_END_ACTION_ID,
        TEXT_SELECTION_EXTEND_LEFT_ACTION_ID,
        TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID,
        TEXT_SELECTION_EXTEND_UP_ACTION_ID,
        TEXT_SELECTION_EXTEND_DOWN_ACTION_ID,
        TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID,
        TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID,
        TEXT_SELECT_ALL_ACTION_ID,
        TEXT_DELETE_BACKWARD_ACTION_ID,
        TEXT_DELETE_FORWARD_ACTION_ID,
        TEXT_INSERT_LINE_BREAK_ACTION_ID,
        TEXT_UNDO_ACTION_ID,
        TEXT_REDO_ACTION_ID,
    ];
    let expected_labels = [
        "Move cursor left",
        "Move cursor right",
        "Move cursor up",
        "Move cursor down",
        "Move to line start",
        "Move to line end",
        "Extend selection left",
        "Extend selection right",
        "Extend selection up",
        "Extend selection down",
        "Extend selection to line start",
        "Extend selection to line end",
        "Select all",
        "Delete backward",
        "Delete forward",
        "Insert line break",
        "Undo",
        "Redo",
    ];
    let mut expected_keys = vec![
        vec!["Left"],
        vec!["Right"],
        vec!["Up"],
        vec!["Down"],
        vec!["Home"],
        vec!["End"],
        vec!["Shift+Left"],
        vec!["Shift+Right"],
        vec!["Shift+Up"],
        vec!["Shift+Down"],
        vec!["Shift+Home"],
        vec!["Shift+End"],
        vec!["Ctrl+a"],
        vec!["Backspace"],
        vec!["Delete"],
        vec!["Enter"],
        vec!["Ctrl+z"],
        vec!["Ctrl+y", "Ctrl+Shift+z"],
    ];
    if mode == "line-break-x" {
        expected_keys[15] = vec!["x"];
    } else if mode == "unbind-delete-forward" {
        expected_keys[14].clear();
    }

    assert_eq!(resolved.actions().len(), expected_ids.len(), "case {case}");
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
        let expected_available = enabled
            && match index {
                16 => has_handler(handlers, "undo"),
                17 => has_handler(handlers, "redo"),
                _ => true,
            };
        assert_eq!(
            action.availability() == ActionAvailability::Enabled,
            expected_available,
            "case {case} action {}",
            expected_ids[index]
        );
    }
}

fn text_area_event(value: &str) -> Event {
    let key = |code, modifiers, action| {
        Event::Key(KeyEvent {
            code,
            modifiers,
            action,
            text: None,
            protocol: KeyProtocol::Legacy,
        })
    };
    let character = |value, modifiers| key(KeyCode::Character(value), modifiers, KeyAction::Press);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    let control = Modifiers {
        control: true,
        ..Modifiers::NONE
    };
    let control_shift = Modifiers {
        control: true,
        shift: true,
        ..Modifiers::NONE
    };
    match value {
        "text-X" => Event::Text("X".to_owned()),
        "text-x" => Event::Text("x".to_owned()),
        "paste-X-newline-Y" => Event::Paste("X\nY".to_owned()),
        "paste-x" => Event::Paste("x".to_owned()),
        "enter" => key(KeyCode::Enter, Modifiers::NONE, KeyAction::Press),
        "repeat-enter" => key(KeyCode::Enter, Modifiers::NONE, KeyAction::Repeat),
        "shift-enter" => key(KeyCode::Enter, shift, KeyAction::Press),
        "control-enter" => key(KeyCode::Enter, control, KeyAction::Press),
        "left" => key(KeyCode::Left, Modifiers::NONE, KeyAction::Press),
        "right" => key(KeyCode::Right, Modifiers::NONE, KeyAction::Press),
        "up" => key(KeyCode::Up, Modifiers::NONE, KeyAction::Press),
        "down" => key(KeyCode::Down, Modifiers::NONE, KeyAction::Press),
        "home" => key(KeyCode::Home, Modifiers::NONE, KeyAction::Press),
        "end" => key(KeyCode::End, Modifiers::NONE, KeyAction::Press),
        "shift-left" => key(KeyCode::Left, shift, KeyAction::Press),
        "shift-up" => key(KeyCode::Up, shift, KeyAction::Press),
        "shift-home" => key(KeyCode::Home, shift, KeyAction::Press),
        "shift-end" => key(KeyCode::End, shift, KeyAction::Press),
        "control-a" => character('a', control),
        "backspace" => key(KeyCode::Backspace, Modifiers::NONE, KeyAction::Press),
        "delete" => key(KeyCode::Delete, Modifiers::NONE, KeyAction::Press),
        "release-left" => key(KeyCode::Left, Modifiers::NONE, KeyAction::Release),
        "control-z" => character('z', control),
        "control-y" => character('y', control),
        "control-shift-z" => character('z', control_shift),
        _ => panic!("unknown TextArea action event {value}"),
    }
}

fn fixture_state(value: String, cursor: &str, anchor: &str) -> TextAreaState {
    let state = TextAreaState::new(value, fixture_usize(cursor));
    if anchor == "-" {
        state
    } else {
        state.select(fixture_usize(anchor))
    }
}

fn has_handler(handlers: &str, expected: &str) -> bool {
    handlers == "both" || handlers == expected
}

fn text_area_message_name(message: &TextAreaActionMessage) -> &'static str {
    match message {
        TextAreaActionMessage::Change(_) => "change",
        TextAreaActionMessage::Undo => "undo",
        TextAreaActionMessage::Redo => "redo",
    }
}

fn fixture_message_names(value: &str) -> Vec<&str> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').collect()
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
