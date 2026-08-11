//! Shared FilePicker semantic-action integration tests

mod support;

use nagi_tui::{
    ActionAvailability, App, BindingConflictKind, Effect, Event, Insets, KeyAction, KeyBinding,
    KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers, MouseButton,
    MouseEvent, MouseKind, Node, NodeId, ResolvedActions, Runtime, RuntimeError, Size,
    VirtualClock, resolve_actions,
};
use nagi_tui_widgets::{
    ACTIVATE_ACTION_ID, FilePicker, FilePickerEntry, NAVIGATION_BACK_ACTION_ID,
    SELECTION_FIRST_ACTION_ID, SELECTION_LAST_ACTION_ID, SELECTION_NEXT_ACTION_ID,
    SELECTION_NEXT_PAGE_ACTION_ID, SELECTION_PREVIOUS_ACTION_ID, SELECTION_PREVIOUS_PAGE_ACTION_ID,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FilePickerMessage {
    Select(usize),
    Open(usize),
    Back,
}

struct FilePickerActionApp {
    count: usize,
    hidden: Vec<usize>,
    show_hidden: bool,
    selected: usize,
    viewport: usize,
    handlers: String,
    enabled: bool,
    key_map: KeyMap,
    messages: Vec<FilePickerMessage>,
}

impl App for FilePickerActionApp {
    type Message = FilePickerMessage;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if let FilePickerMessage::Select(selected) = message {
            self.selected = selected;
        }
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let picker = fixture_file_picker(
            self.count,
            &self.hidden,
            self.show_hidden,
            self.selected,
            self.viewport,
            &self.handlers,
            self.enabled,
        )
        .into_node();
        Node::padding(picker, Insets::all(0))
            .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn file_picker_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/file-picker-action.txt",
        "widget-file-picker-action",
        &[
            "count",
            "hidden",
            "show-hidden",
            "selected",
            "viewport",
            "focus",
            "mode",
            "enabled",
            "handlers",
            "event",
            "messages",
            "consumed",
            "focus-after",
            "selected-after",
            "keys",
            "available",
        ],
    ) else {
        return;
    };

    for record in records {
        let count = fixture_usize(record.field("count"));
        let hidden = fixture_usize_list(record.field("hidden"));
        let show_hidden = fixture_boolean(record.field("show-hidden"));
        let selected = fixture_usize(record.field("selected"));
        let viewport = fixture_usize(record.field("viewport"));
        let handlers = record.field("handlers");
        let enabled = fixture_boolean(record.field("enabled"));
        let key_map = file_picker_key_map(record.field("mode"));
        let picker = fixture_file_picker(
            count,
            &hidden,
            show_hidden,
            selected,
            viewport,
            handlers,
            enabled,
        );
        let scopes = [KeyScope::new("scope", key_map.clone())];
        let resolved = resolve_actions(
            &NodeId::from("files"),
            &picker.action_descriptors(),
            &scopes,
        )
        .unwrap();
        assert_file_picker_action_group(
            &record.id,
            &resolved,
            &fixture_key_groups(record.field("keys")),
            &fixture_boolean_list(record.field("available")),
        );

        let mut runtime = Runtime::with_clock(
            FilePickerActionApp {
                count,
                hidden: hidden.clone(),
                show_hidden,
                selected,
                viewport,
                handlers: handlers.to_owned(),
                enabled,
                key_map,
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(40, 20)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        if record.field("focus") == "files" {
            assert!(
                runtime.request_focus(&NodeId::from("files")).unwrap(),
                "case {}",
                record.id
            );
            let groups = runtime.active_action_groups().unwrap();
            assert_eq!(groups.len(), 1, "case {}", record.id);
            assert_eq!(groups[0].owner().as_str(), "files", "case {}", record.id);
        }

        let event = file_picker_event(
            record.field("event"),
            count,
            &hidden,
            show_hidden,
            selected,
            viewport,
        );
        let dispatch = runtime.dispatch_event(&event).unwrap();
        runtime.process_pending().unwrap();
        let expected_messages = fixture_messages(record.field("messages"));
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
        assert_eq!(
            runtime.interaction().focused().map(NodeId::as_str),
            fixture_optional_focus(record.field("focus-after")),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().selected,
            fixture_usize(record.field("selected-after")),
            "case {}",
            record.id
        );
    }
}

#[test]
fn file_picker_root_conflict_is_reported_before_dispatch() {
    let binding = || KeyBinding::new(KeyStroke::character('x', Modifiers::NONE));
    let key_map = KeyMap::new()
        .rebind(SELECTION_PREVIOUS_ACTION_ID, [binding()])
        .unwrap()
        .rebind(SELECTION_PREVIOUS_PAGE_ACTION_ID, [binding()])
        .unwrap();
    let mut runtime = Runtime::with_clock(
        FilePickerActionApp {
            count: 5,
            hidden: Vec::new(),
            show_hidden: false,
            selected: 2,
            viewport: 3,
            handlers: "both".to_owned(),
            enabled: true,
            key_map,
            messages: Vec::new(),
        },
        nagi_tui::RuntimeConfig::new(Size::new(40, 10)),
        VirtualClock::new(),
    )
    .unwrap();

    let error = runtime.render_if_dirty().unwrap_err();
    let RuntimeError::BindingConflict(conflict) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.kind(), BindingConflictKind::AmbiguousBinding);
    assert_eq!(conflict.owner().as_str(), "files");
    assert_eq!(
        conflict
            .actions()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>(),
        [
            SELECTION_PREVIOUS_ACTION_ID,
            SELECTION_PREVIOUS_PAGE_ACTION_ID,
        ]
    );
    assert!(runtime.app().messages.is_empty());
}

#[allow(clippy::too_many_arguments)]
fn fixture_file_picker(
    count: usize,
    hidden: &[usize],
    show_hidden: bool,
    selected: usize,
    viewport: usize,
    handlers: &str,
    enabled: bool,
) -> FilePicker<FilePickerMessage> {
    let mut picker = FilePicker::new(
        "files",
        fixture_entries(count, hidden),
        selected,
        FilePickerMessage::Select,
    )
    .show_hidden(show_hidden)
    .viewport(viewport)
    .enabled(enabled);
    if matches!(handlers, "both" | "open") {
        picker = picker.on_open(FilePickerMessage::Open);
    }
    if matches!(handlers, "both" | "back") {
        picker = picker.on_back(|| FilePickerMessage::Back);
    }
    picker
}

fn fixture_entries(count: usize, hidden: &[usize]) -> Vec<FilePickerEntry> {
    (0..count)
        .map(|index| {
            let entry = if index % 2 == 0 {
                FilePickerEntry::file(
                    format!("entry-{index}"),
                    format!("Entry {index}"),
                    index.to_string(),
                )
            } else {
                FilePickerEntry::directory(
                    format!("entry-{index}"),
                    format!("Entry {index}"),
                    index.to_string(),
                )
            };
            entry.hidden(hidden.contains(&index))
        })
        .collect()
}

fn file_picker_key_map(mode: &str) -> KeyMap {
    let character = |value| KeyBinding::new(KeyStroke::character(value, Modifiers::NONE));
    match mode {
        "default" => KeyMap::new(),
        "previous-k" => KeyMap::new()
            .rebind(SELECTION_PREVIOUS_ACTION_ID, [character('k')])
            .unwrap(),
        "activate-o" => KeyMap::new()
            .rebind(ACTIVATE_ACTION_ID, [character('o')])
            .unwrap(),
        "previous-page-u" => KeyMap::new()
            .rebind(SELECTION_PREVIOUS_PAGE_ACTION_ID, [character('u')])
            .unwrap(),
        "back-b" => KeyMap::new()
            .rebind(NAVIGATION_BACK_ACTION_ID, [character('b')])
            .unwrap(),
        "unbind-next" => KeyMap::new()
            .rebind(SELECTION_NEXT_ACTION_ID, std::iter::empty())
            .unwrap(),
        "unbind-activate" => KeyMap::new()
            .rebind(ACTIVATE_ACTION_ID, std::iter::empty())
            .unwrap(),
        mode => panic!("unknown FilePicker action mode {mode}"),
    }
}

fn file_picker_event(
    value: &str,
    count: usize,
    hidden: &[usize],
    show_hidden: bool,
    selected: usize,
    viewport: usize,
) -> Event {
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
        "right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Press),
        "up" => keyboard(KeyCode::Up, Modifiers::NONE, KeyAction::Press),
        "down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Press),
        "home" => keyboard(KeyCode::Home, Modifiers::NONE, KeyAction::Press),
        "end" => keyboard(KeyCode::End, Modifiers::NONE, KeyAction::Press),
        "page-up" => keyboard(KeyCode::PageUp, Modifiers::NONE, KeyAction::Press),
        "page-down" => keyboard(KeyCode::PageDown, Modifiers::NONE, KeyAction::Press),
        "left" => keyboard(KeyCode::Left, Modifiers::NONE, KeyAction::Press),
        "backspace" => keyboard(KeyCode::Backspace, Modifiers::NONE, KeyAction::Press),
        "repeat-down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Repeat),
        "unknown-down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Unknown),
        "repeat-enter" => keyboard(KeyCode::Enter, Modifiers::NONE, KeyAction::Repeat),
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
        "k" => keyboard(KeyCode::Character('k'), Modifiers::NONE, KeyAction::Press),
        "o" => keyboard(KeyCode::Character('o'), Modifiers::NONE, KeyAction::Press),
        "u" => keyboard(KeyCode::Character('u'), Modifiers::NONE, KeyAction::Press),
        "b" => keyboard(KeyCode::Character('b'), Modifiers::NONE, KeyAction::Press),
        pointer if pointer.starts_with("mouse-") => {
            let (kind, button, candidate) = pointer_event(pointer);
            let y =
                file_picker_pointer_y(count, hidden, show_hidden, selected, viewport, candidate);
            Event::Mouse(MouseEvent {
                kind,
                button,
                x: 0,
                y,
                modifiers: Modifiers::NONE,
            })
        }
        event => panic!("unknown FilePicker action event {event}"),
    }
}

fn pointer_event(value: &str) -> (MouseKind, MouseButton, usize) {
    for (prefix, kind, button) in [
        ("mouse-left-press-", MouseKind::Press, MouseButton::Left),
        ("mouse-right-press-", MouseKind::Press, MouseButton::Right),
        ("mouse-left-release-", MouseKind::Release, MouseButton::Left),
    ] {
        if let Some(candidate) = value.strip_prefix(prefix) {
            return (kind, button, fixture_usize(candidate));
        }
    }
    panic!("invalid FilePicker pointer event {value}")
}

fn file_picker_pointer_y(
    count: usize,
    hidden: &[usize],
    show_hidden: bool,
    selected: usize,
    viewport: usize,
    candidate: usize,
) -> u32 {
    let visible = fixture_visible_indices(count, hidden, show_hidden);
    let Some(position) = visible.iter().position(|index| *index == candidate) else {
        return 0;
    };
    let selected_position = fixture_normalized_selection(&visible, selected).unwrap_or(0);
    let (start, _) = fixture_viewport_range(visible.len(), selected_position, viewport);
    u32::try_from(position.saturating_sub(start)).unwrap()
}

fn fixture_visible_indices(count: usize, hidden: &[usize], show_hidden: bool) -> Vec<usize> {
    (0..count)
        .filter(|index| show_hidden || !hidden.contains(index))
        .collect()
}

fn fixture_normalized_selection(visible: &[usize], selected: usize) -> Option<usize> {
    if visible.is_empty() {
        return None;
    }
    Some(
        visible
            .iter()
            .rposition(|index| *index <= selected)
            .unwrap_or(0),
    )
}

fn fixture_viewport_range(count: usize, selected: usize, height: usize) -> (usize, usize) {
    if count == 0 || height == 0 {
        return (0, count);
    }
    let height = height.min(count);
    let start = selected
        .min(count.saturating_sub(1))
        .saturating_sub(height / 2)
        .min(count.saturating_sub(height));
    (start, start.saturating_add(height))
}

fn assert_file_picker_action_group(
    case: &str,
    resolved: &ResolvedActions,
    expected_keys: &[Vec<String>],
    expected_available: &[bool],
) {
    let expected_ids = [
        ACTIVATE_ACTION_ID,
        SELECTION_PREVIOUS_ACTION_ID,
        SELECTION_NEXT_ACTION_ID,
        SELECTION_FIRST_ACTION_ID,
        SELECTION_LAST_ACTION_ID,
        SELECTION_PREVIOUS_PAGE_ACTION_ID,
        SELECTION_NEXT_PAGE_ACTION_ID,
        NAVIGATION_BACK_ACTION_ID,
    ];
    let expected_labels = [
        "Activate",
        "Previous",
        "Next",
        "First",
        "Last",
        "Previous page",
        "Next page",
        "Back",
    ];
    assert_eq!(resolved.owner().as_str(), "files", "case {case}");
    assert_eq!(resolved.actions().len(), expected_ids.len(), "case {case}");
    assert_eq!(expected_keys.len(), expected_ids.len(), "case {case}");
    assert_eq!(expected_available.len(), expected_ids.len(), "case {case}");
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
        let expected_availability = if expected_available[index] {
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

fn fixture_messages(value: &str) -> Vec<FilePickerMessage> {
    if value == "-" {
        return Vec::new();
    }
    value
        .split(',')
        .map(|message| {
            if message == "back" {
                return FilePickerMessage::Back;
            }
            let (kind, index) = message.split_once(':').unwrap();
            match kind {
                "select" => FilePickerMessage::Select(fixture_usize(index)),
                "open" => FilePickerMessage::Open(fixture_usize(index)),
                kind => panic!("invalid FilePicker fixture message {kind}"),
            }
        })
        .collect()
}

fn fixture_boolean(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        value => panic!("invalid fixture Boolean {value}"),
    }
}

fn fixture_boolean_list(value: &str) -> Vec<bool> {
    value.split(',').map(fixture_boolean).collect()
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

fn fixture_optional_focus(value: &str) -> Option<&str> {
    (value != "none").then_some(value)
}
