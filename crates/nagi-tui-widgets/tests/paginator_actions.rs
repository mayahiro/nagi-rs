//! Shared Paginator semantic-action integration tests

mod action_support;
mod support;

use nagi_tui::{
    ActionAvailability, App, BindingConflictKind, Effect, Event, Insets, KeyAction, KeyBinding,
    KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers, MouseButton,
    MouseEvent, MouseKind, Node, NodeId, ResolvedActions, Runtime, RuntimeError, Size,
    VirtualClock, resolve_actions,
};
use nagi_tui_widgets::{
    Paginator, PaginatorMode, SELECTION_FIRST_ACTION_ID, SELECTION_LAST_ACTION_ID,
    SELECTION_NEXT_ACTION_ID, SELECTION_PREVIOUS_ACTION_ID,
};

struct PaginatorActionApp {
    display: PaginatorMode,
    page: usize,
    total: usize,
    limit: usize,
    enabled: bool,
    key_map: KeyMap,
    messages: Vec<usize>,
}

impl App for PaginatorActionApp {
    type Message = usize;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.page = message;
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let paginator = fixture_paginator(
            self.display,
            self.page,
            self.total,
            self.limit,
            self.enabled,
        )
        .into_node();
        Node::padding(paginator, Insets::all(0))
            .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn paginator_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/paginator-action.txt",
        "widget-paginator-action",
        &[
            "display",
            "total",
            "page",
            "limit",
            "focus",
            "mode",
            "enabled",
            "event",
            "messages",
            "consumed",
            "focus-after",
            "page-after",
            "keys",
            "available",
        ],
    ) else {
        return;
    };

    for record in records {
        let display = fixture_display(record.field("display"));
        let total = fixture_usize(record.field("total"));
        let page = fixture_usize(record.field("page"));
        let limit = fixture_usize(record.field("limit"));
        let enabled = fixture_boolean(record.field("enabled"));
        let key_map = paginator_key_map(record.field("mode"));
        let paginator = fixture_paginator(display, page, total, limit, enabled);
        let scopes = [KeyScope::new("scope", key_map.clone())];
        let resolved = resolve_actions(
            &NodeId::from("pages"),
            &paginator.action_descriptors(),
            &scopes,
        )
        .unwrap();
        assert_paginator_action_group(
            &record.id,
            &resolved,
            &fixture_key_groups(record.field("keys")),
            fixture_boolean(record.field("available")),
        );

        let mut runtime = Runtime::with_clock(
            PaginatorActionApp {
                display,
                page,
                total,
                limit,
                enabled,
                key_map,
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(32, 2)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        if record.field("focus") == "pages" {
            assert!(
                runtime.request_focus(&NodeId::from("pages")).unwrap(),
                "case {}",
                record.id
            );
            let groups =
                action_support::node_declared_groups(runtime.active_action_groups().unwrap());
            assert_eq!(groups.len(), 1, "case {}", record.id);
            assert_eq!(groups[0].owner().as_str(), "pages", "case {}", record.id);
        }

        let event = paginator_event(record.field("event"), display, page, total, limit);
        let dispatch = runtime.dispatch_event(&event).unwrap();
        runtime.process_pending().unwrap();
        let expected_messages = fixture_usize_list(record.field("messages"));
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
            runtime.app().page,
            fixture_usize(record.field("page-after")),
            "case {}",
            record.id
        );
    }
}

#[test]
fn paginator_root_conflict_is_reported_before_dispatch() {
    let binding = || KeyBinding::new(KeyStroke::character('x', Modifiers::NONE));
    let key_map = KeyMap::new()
        .rebind(SELECTION_PREVIOUS_ACTION_ID, [binding()])
        .unwrap()
        .rebind(SELECTION_NEXT_ACTION_ID, [binding()])
        .unwrap();
    let mut runtime = Runtime::with_clock(
        PaginatorActionApp {
            display: PaginatorMode::Dots,
            page: 2,
            total: 5,
            limit: 0,
            enabled: true,
            key_map,
            messages: Vec::new(),
        },
        nagi_tui::RuntimeConfig::new(Size::new(32, 2)),
        VirtualClock::new(),
    )
    .unwrap();

    let error = runtime.render_if_dirty().unwrap_err();
    let RuntimeError::BindingConflict(conflict) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.kind(), BindingConflictKind::AmbiguousBinding);
    assert_eq!(conflict.owner().as_str(), "pages");
    assert_eq!(
        conflict
            .actions()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>(),
        [SELECTION_PREVIOUS_ACTION_ID, SELECTION_NEXT_ACTION_ID]
    );
    assert!(runtime.app().messages.is_empty());
}

fn fixture_paginator(
    display: PaginatorMode,
    page: usize,
    total: usize,
    limit: usize,
    enabled: bool,
) -> Paginator<usize> {
    Paginator::new("pages", page, total, |page| page)
        .mode(display)
        .indicator_limit(limit)
        .enabled(enabled)
}

fn paginator_key_map(mode: &str) -> KeyMap {
    match mode {
        "default" => KeyMap::new(),
        "previous-k" => KeyMap::new()
            .rebind(
                SELECTION_PREVIOUS_ACTION_ID,
                [KeyBinding::new(KeyStroke::character('k', Modifiers::NONE))],
            )
            .unwrap(),
        "unbind-next" => KeyMap::new()
            .rebind(SELECTION_NEXT_ACTION_ID, std::iter::empty())
            .unwrap(),
        mode => panic!("unknown Paginator action mode {mode}"),
    }
}

fn paginator_event(
    value: &str,
    display: PaginatorMode,
    page: usize,
    total: usize,
    limit: usize,
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
        "left" => keyboard(KeyCode::Left, Modifiers::NONE, KeyAction::Press),
        "up" => keyboard(KeyCode::Up, Modifiers::NONE, KeyAction::Press),
        "page-up" => keyboard(KeyCode::PageUp, Modifiers::NONE, KeyAction::Press),
        "right" => keyboard(KeyCode::Right, Modifiers::NONE, KeyAction::Press),
        "down" => keyboard(KeyCode::Down, Modifiers::NONE, KeyAction::Press),
        "page-down" => keyboard(KeyCode::PageDown, Modifiers::NONE, KeyAction::Press),
        "home" => keyboard(KeyCode::Home, Modifiers::NONE, KeyAction::Press),
        "end" => keyboard(KeyCode::End, Modifiers::NONE, KeyAction::Press),
        "repeat-left" => keyboard(KeyCode::Left, Modifiers::NONE, KeyAction::Repeat),
        "unknown-left" => keyboard(KeyCode::Left, Modifiers::NONE, KeyAction::Unknown),
        "release-left" => keyboard(KeyCode::Left, Modifiers::NONE, KeyAction::Release),
        "shift-left" => keyboard(
            KeyCode::Left,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "control-left" => keyboard(
            KeyCode::Left,
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "k" => keyboard(KeyCode::Character('k'), Modifiers::NONE, KeyAction::Press),
        pointer if pointer.starts_with("mouse-") => {
            let (kind, button, candidate) = pointer_event(pointer);
            let x = if display == PaginatorMode::Dots && total > 0 {
                let (start, _) = paginator_window(total, page, limit);
                u32::try_from(candidate.saturating_sub(start).saturating_mul(2)).unwrap()
            } else {
                0
            };
            Event::Mouse(MouseEvent {
                kind,
                button,
                x,
                y: 0,
                modifiers: Modifiers::NONE,
            })
        }
        event => panic!("unknown Paginator action event {event}"),
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
    panic!("invalid Paginator pointer event {value}")
}

fn paginator_window(total: usize, page: usize, limit: usize) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    if limit == 0 || limit >= total {
        return (0, total);
    }
    let page = page.min(total.saturating_sub(1));
    let start = page
        .saturating_sub(limit / 2)
        .min(total.saturating_sub(limit));
    (start, start.saturating_add(limit))
}

fn assert_paginator_action_group(
    case: &str,
    resolved: &ResolvedActions,
    expected_keys: &[Vec<String>],
    available: bool,
) {
    let expected_ids = [
        SELECTION_PREVIOUS_ACTION_ID,
        SELECTION_NEXT_ACTION_ID,
        SELECTION_FIRST_ACTION_ID,
        SELECTION_LAST_ACTION_ID,
    ];
    let expected_labels = ["Previous", "Next", "First", "Last"];
    assert_eq!(resolved.owner().as_str(), "pages", "case {case}");
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

fn fixture_display(value: &str) -> PaginatorMode {
    match value {
        "dots" => PaginatorMode::Dots,
        "numeric" => PaginatorMode::Numeric,
        value => panic!("invalid Paginator display {value}"),
    }
}

fn fixture_boolean(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        value => panic!("invalid fixture Boolean {value}"),
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

fn fixture_optional_focus(value: &str) -> Option<&str> {
    (value != "none").then_some(value)
}
