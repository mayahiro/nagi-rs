//! Shared Calendar semantic-action integration tests

mod support;

use nagi_tui::{
    ActionAvailability, App, BindingConflictKind, Effect, Event, Insets, KeyAction, KeyBinding,
    KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers, MouseButton,
    MouseEvent, MouseKind, Node, NodeId, ResolvedActions, Runtime, RuntimeError, Size,
    VirtualClock, resolve_actions,
};
use nagi_tui_widgets::{
    ACTIVATE_ACTION_ID, Calendar, CalendarDate, SELECTION_FIRST_DAY_OF_MONTH_ACTION_ID,
    SELECTION_LAST_DAY_OF_MONTH_ACTION_ID, SELECTION_NEXT_DAY_ACTION_ID,
    SELECTION_NEXT_MONTH_ACTION_ID, SELECTION_NEXT_WEEK_ACTION_ID,
    SELECTION_PREVIOUS_DAY_ACTION_ID, SELECTION_PREVIOUS_MONTH_ACTION_ID,
    SELECTION_PREVIOUS_WEEK_ACTION_ID,
};

struct CalendarActionApp {
    year: i32,
    month: i32,
    selected: CalendarDate,
    show_adjacent: bool,
    enabled: bool,
    key_map: KeyMap,
    messages: Vec<CalendarDate>,
}

impl App for CalendarActionApp {
    type Message = CalendarDate;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.selected = message;
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let calendar = fixture_calendar(
            self.year,
            self.month,
            self.selected,
            self.show_adjacent,
            self.enabled,
        )
        .into_node();
        Node::padding(calendar, Insets::all(0))
            .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn calendar_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/calendar-action.txt",
        "widget-calendar-action",
        &[
            "displayed",
            "selected",
            "show-adjacent",
            "focus",
            "mode",
            "enabled",
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
        let (year, month) = fixture_displayed(record.field("displayed"));
        let selected = fixture_date(record.field("selected"));
        let show_adjacent = fixture_boolean(record.field("show-adjacent"));
        let enabled = fixture_boolean(record.field("enabled"));
        let key_map = calendar_key_map(record.field("mode"));
        let calendar = fixture_calendar(year, month, selected, show_adjacent, enabled);
        let scopes = [KeyScope::new("scope", key_map.clone())];
        let resolved = resolve_actions(
            &NodeId::from("calendar"),
            &calendar.action_descriptors(),
            &scopes,
        )
        .unwrap();
        assert_calendar_action_group(
            &record.id,
            &resolved,
            &fixture_key_groups(record.field("keys")),
            &fixture_boolean_list(record.field("available")),
        );

        let mut runtime = Runtime::with_clock(
            CalendarActionApp {
                year,
                month,
                selected,
                show_adjacent,
                enabled,
                key_map,
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(24, 8)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        if record.field("focus") == "calendar" {
            assert!(
                runtime.request_focus(&NodeId::from("calendar")).unwrap(),
                "case {}",
                record.id
            );
            let groups = runtime.active_action_groups().unwrap();
            assert_eq!(groups.len(), 1, "case {}", record.id);
            assert_eq!(groups[0].owner().as_str(), "calendar", "case {}", record.id);
        }

        let event = calendar_event(record.field("event"));
        let dispatch = runtime.dispatch_event(&event).unwrap();
        runtime.process_pending().unwrap();
        let expected_messages = fixture_dates(record.field("messages"));
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
            fixture_date(record.field("selected-after")),
            "case {}",
            record.id
        );
    }
}

#[test]
fn calendar_root_conflict_is_reported_before_dispatch() {
    let binding = || KeyBinding::new(KeyStroke::character('x', Modifiers::NONE));
    let key_map = KeyMap::new()
        .rebind(SELECTION_PREVIOUS_DAY_ACTION_ID, [binding()])
        .unwrap()
        .rebind(SELECTION_PREVIOUS_WEEK_ACTION_ID, [binding()])
        .unwrap();
    let mut runtime = Runtime::with_clock(
        CalendarActionApp {
            year: 2024,
            month: 2,
            selected: CalendarDate::new(2024, 2, 15),
            show_adjacent: false,
            enabled: true,
            key_map,
            messages: Vec::new(),
        },
        nagi_tui::RuntimeConfig::new(Size::new(24, 8)),
        VirtualClock::new(),
    )
    .unwrap();

    let error = runtime.render_if_dirty().unwrap_err();
    let RuntimeError::BindingConflict(conflict) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.kind(), BindingConflictKind::AmbiguousBinding);
    assert_eq!(conflict.owner().as_str(), "calendar");
    assert_eq!(
        conflict
            .actions()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>(),
        [
            SELECTION_PREVIOUS_DAY_ACTION_ID,
            SELECTION_PREVIOUS_WEEK_ACTION_ID
        ]
    );
    assert!(runtime.app().messages.is_empty());
}

fn fixture_calendar(
    year: i32,
    month: i32,
    selected: CalendarDate,
    show_adjacent: bool,
    enabled: bool,
) -> Calendar<CalendarDate> {
    Calendar::new("calendar", year, month, selected, |date| date)
        .show_adjacent(show_adjacent)
        .enabled(enabled)
}

fn calendar_key_map(mode: &str) -> KeyMap {
    let character = |value| KeyBinding::new(KeyStroke::character(value, Modifiers::NONE));
    match mode {
        "default" => KeyMap::new(),
        "previous-day-h" => KeyMap::new()
            .rebind(SELECTION_PREVIOUS_DAY_ACTION_ID, [character('h')])
            .unwrap(),
        "next-week-j" => KeyMap::new()
            .rebind(SELECTION_NEXT_WEEK_ACTION_ID, [character('j')])
            .unwrap(),
        "previous-month-u" => KeyMap::new()
            .rebind(SELECTION_PREVIOUS_MONTH_ACTION_ID, [character('u')])
            .unwrap(),
        "activate-o" => KeyMap::new()
            .rebind(ACTIVATE_ACTION_ID, [character('o')])
            .unwrap(),
        "unbind-next-day" => KeyMap::new()
            .rebind(SELECTION_NEXT_DAY_ACTION_ID, std::iter::empty())
            .unwrap(),
        "unbind-activate" => KeyMap::new()
            .rebind(ACTIVATE_ACTION_ID, std::iter::empty())
            .unwrap(),
        mode => panic!("unknown Calendar action mode {mode}"),
    }
}

fn calendar_event(value: &str) -> Event {
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
        "right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Press),
        "up" => keyboard(KeyCode::Up, Modifiers::NONE, KeyAction::Press),
        "down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Press),
        "page-up" => keyboard(KeyCode::PageUp, Modifiers::NONE, KeyAction::Press),
        "page-down" => keyboard(KeyCode::PageDown, Modifiers::NONE, KeyAction::Press),
        "home" => keyboard(KeyCode::Home, Modifiers::NONE, KeyAction::Press),
        "end" => keyboard(KeyCode::End, Modifiers::NONE, KeyAction::Press),
        "repeat-right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Repeat),
        "unknown-right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Unknown),
        "repeat-enter" => keyboard(KeyCode::Enter, Modifiers::NONE, KeyAction::Repeat),
        "release-right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Release),
        "shift-right" => keyboard(
            KeyCode::Right,
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
        "h" => keyboard(KeyCode::Character('h'), Modifiers::NONE, KeyAction::Press),
        "j" => keyboard(KeyCode::Character('j'), Modifiers::NONE, KeyAction::Press),
        "u" => keyboard(KeyCode::Character('u'), Modifiers::NONE, KeyAction::Press),
        "o" => keyboard(KeyCode::Character('o'), Modifiers::NONE, KeyAction::Press),
        pointer if pointer.starts_with("mouse-") => calendar_pointer_event(pointer),
        event => panic!("unknown Calendar action event {event}"),
    }
}

fn calendar_pointer_event(value: &str) -> Event {
    for (prefix, kind, button) in [
        ("mouse-left-press:", MouseKind::Press, MouseButton::Left),
        ("mouse-right-press:", MouseKind::Press, MouseButton::Right),
        ("mouse-left-release:", MouseKind::Release, MouseButton::Left),
    ] {
        if let Some(coordinates) = value.strip_prefix(prefix) {
            let (x, y) = coordinates.split_once(',').unwrap();
            return Event::Mouse(MouseEvent {
                kind,
                button,
                x: fixture_u32(x),
                y: fixture_u32(y),
                modifiers: Modifiers::NONE,
            });
        }
    }
    panic!("invalid Calendar pointer event {value}")
}

fn assert_calendar_action_group(
    case: &str,
    resolved: &ResolvedActions,
    expected_keys: &[Vec<String>],
    expected_available: &[bool],
) {
    let expected_ids = [
        ACTIVATE_ACTION_ID,
        SELECTION_PREVIOUS_DAY_ACTION_ID,
        SELECTION_NEXT_DAY_ACTION_ID,
        SELECTION_PREVIOUS_WEEK_ACTION_ID,
        SELECTION_NEXT_WEEK_ACTION_ID,
        SELECTION_PREVIOUS_MONTH_ACTION_ID,
        SELECTION_NEXT_MONTH_ACTION_ID,
        SELECTION_FIRST_DAY_OF_MONTH_ACTION_ID,
        SELECTION_LAST_DAY_OF_MONTH_ACTION_ID,
    ];
    let expected_labels = [
        "Activate",
        "Previous day",
        "Next day",
        "Previous week",
        "Next week",
        "Previous month",
        "Next month",
        "First day of month",
        "Last day of month",
    ];
    assert_eq!(resolved.owner().as_str(), "calendar", "case {case}");
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
        let availability = if expected_available[index] {
            ActionAvailability::Enabled
        } else {
            ActionAvailability::DisabledPassThrough
        };
        assert_eq!(
            action.availability(),
            availability,
            "case {case} action {}",
            expected_ids[index]
        );
    }
}

fn fixture_displayed(value: &str) -> (i32, i32) {
    let (year, month) = value.split_once('-').unwrap();
    (fixture_i32(year), fixture_i32(month))
}

fn fixture_date(value: &str) -> CalendarDate {
    let mut values = value.split('-').map(fixture_i32);
    let date = CalendarDate::new(
        values.next().unwrap(),
        values.next().unwrap(),
        values.next().unwrap(),
    );
    assert!(values.next().is_none(), "invalid Calendar date {value}");
    date
}

fn fixture_dates(value: &str) -> Vec<CalendarDate> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').map(fixture_date).collect()
    }
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

fn fixture_i32(value: &str) -> i32 {
    value.parse().unwrap()
}

fn fixture_u32(value: &str) -> u32 {
    value.parse().unwrap()
}
