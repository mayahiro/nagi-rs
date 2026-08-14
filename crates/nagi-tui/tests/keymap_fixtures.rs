//! Shared scoped-key-map conformance fixtures

mod support;

use nagi_tui::{
    ActionAvailability, ActionDescriptor, ActionId, BindingConflictKind, BindingSupport, Event,
    KeyAction, KeyBinding, KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers,
    NodeId, RepeatPolicy, resolve_actions,
};

#[test]
fn key_strokes_match_shared_fixtures() {
    let Some(records) = support::load(
        "interaction/key-stroke.txt",
        "key-stroke",
        &["event", "stroke", "repeat", "matches", "notation"],
    ) else {
        return;
    };

    for record in records {
        let binding = KeyBinding::new(stroke(record.field("stroke")))
            .with_repeat_policy(repeat_policy(record.field("repeat")));
        assert_eq!(
            binding.matches(&event(record.field("event"))),
            boolean(record.field("matches")),
            "case {}",
            record.id
        );
        assert_eq!(
            binding.stroke().notation(),
            record.field("notation"),
            "case {}",
            record.id
        );
    }
}

#[test]
fn key_scope_resolution_matches_shared_fixtures() {
    let Some(records) = support::load(
        "interaction/key-scope.txt",
        "key-scope",
        &[
            "action",
            "label",
            "defaults",
            "layers",
            "availability",
            "visible",
            "expected-bindings",
            "expected-scopes",
        ],
    ) else {
        return;
    };

    for record in records {
        let action_id = ActionId::from(record.field("action"));
        let descriptor = ActionDescriptor::new(
            action_id.clone(),
            record.field("label"),
            bindings(record.field("defaults")),
        )
        .with_availability(availability(record.field("availability")))
        .with_help_visible(boolean(record.field("visible")));
        let scopes = scopes(record.field("layers"), &action_id);
        let owner = NodeId::from("owner");
        let resolved = resolve_actions(&owner, &[descriptor], &scopes)
            .unwrap_or_else(|error| panic!("case {}: {error}", record.id));
        let action = &resolved.actions()[0];

        assert_eq!(action.id(), &action_id, "case {}", record.id);
        assert_eq!(action.label(), record.field("label"), "case {}", record.id);
        assert_eq!(
            action.bindings(),
            bindings(record.field("expected-bindings")),
            "case {}",
            record.id
        );
        assert_eq!(
            action.availability(),
            availability(record.field("availability")),
            "case {}",
            record.id
        );
        assert_eq!(
            action.is_help_visible(),
            boolean(record.field("visible")),
            "case {}",
            record.id
        );
        assert_eq!(
            resolved.help_actions().count(),
            usize::from(boolean(record.field("visible"))),
            "case {}",
            record.id
        );
        let actual_scopes: Vec<_> = resolved.scope_path().iter().map(NodeId::as_str).collect();
        assert_eq!(
            actual_scopes,
            list(record.field("expected-scopes")),
            "case {}",
            record.id
        );
    }
}

#[test]
fn key_label_resolution_matches_shared_fixtures() {
    let Some(records) = support::load(
        "interaction/keymap-label.txt",
        "keymap-label",
        &["action", "label", "layers", "expected", "expected-scopes"],
    ) else {
        return;
    };

    for record in records {
        let action_id = ActionId::from(record.field("action"));
        let descriptor = ActionDescriptor::new(
            action_id.clone(),
            record.text("label"),
            [KeyBinding::new(KeyStroke::new(
                KeyCode::Enter,
                Modifiers::NONE,
            ))],
        );
        let scopes = label_scopes(record.field("layers"), &action_id);
        let resolved = resolve_actions(&NodeId::from("owner"), &[descriptor], &scopes)
            .unwrap_or_else(|error| panic!("case {}: {error}", record.id));

        assert_eq!(
            resolved.actions()[0].label(),
            record.text("expected"),
            "case {}",
            record.id
        );
        assert_eq!(
            resolved.help_actions().next().unwrap().label(),
            record.text("expected"),
            "case {}",
            record.id
        );
        assert_eq!(
            resolved
                .scope_path()
                .iter()
                .map(NodeId::as_str)
                .collect::<Vec<_>>(),
            list(record.field("expected-scopes")),
            "case {}",
            record.id
        );
    }
}

#[test]
fn key_conflicts_match_shared_fixtures() {
    let Some(records) = support::load(
        "interaction/key-conflict.txt",
        "key-conflict",
        &[
            "actions",
            "scopes",
            "expected-kind",
            "expected-actions",
            "expected-stroke",
        ],
    ) else {
        return;
    };

    for record in records {
        let actions = action_descriptors(record.field("actions"));
        let scopes = empty_scopes(record.field("scopes"));
        let owner = NodeId::from("owner");
        let result = resolve_actions(&owner, &actions, &scopes);
        if record.field("expected-kind") == "none" {
            let resolved = result.unwrap_or_else(|error| panic!("case {}: {error}", record.id));
            assert_eq!(resolved.owner(), &owner, "case {}", record.id);
            assert_eq!(
                resolved.scope_path(),
                scopes
                    .iter()
                    .map(|scope| scope.id().clone())
                    .collect::<Vec<_>>(),
                "case {}",
                record.id
            );
            continue;
        }

        let error = result.unwrap_err();
        assert_eq!(error.owner(), &owner, "case {}", record.id);
        assert_eq!(
            error.scope_path(),
            scopes
                .iter()
                .map(|scope| scope.id().clone())
                .collect::<Vec<_>>(),
            "case {}",
            record.id
        );
        assert_eq!(
            error.kind(),
            conflict_kind(record.field("expected-kind")),
            "case {}",
            record.id
        );
        let actual_actions: Vec<_> = error.actions().iter().map(ActionId::as_str).collect();
        assert_eq!(
            actual_actions,
            list(record.field("expected-actions")),
            "case {}",
            record.id
        );
        let expected_stroke = (record.field("expected-stroke") != "-")
            .then(|| stroke(record.field("expected-stroke")));
        assert_eq!(error.stroke(), expected_stroke, "case {}", record.id);
    }
}

fn event(value: &str) -> Event {
    if let Some(scalars) = value.strip_prefix("text/") {
        return Event::Text(scalar_text(scalars));
    }
    if let Some(scalars) = value.strip_prefix("paste/") {
        return Event::Paste(scalar_text(scalars));
    }
    if value == "focus-in" {
        return Event::FocusIn;
    }
    let parts: Vec<_> = value.split('/').collect();
    assert_eq!(parts.len(), 4, "invalid key event {value}");
    assert_eq!(parts[0], "key", "invalid event {value}");
    Event::Key(KeyEvent {
        code: key_code(parts[1]),
        modifiers: modifiers(parts[2]),
        action: key_action(parts[3]),
        text: None,
        protocol: KeyProtocol::Legacy,
    })
}

fn stroke(value: &str) -> KeyStroke {
    let (key, modifier_text) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid stroke {value}"));
    KeyStroke::new(key_code(key), modifiers(modifier_text))
}

fn key_code(value: &str) -> KeyCode {
    if let Some(scalar) = value.strip_prefix("char-U+") {
        return KeyCode::Character(scalar_value(scalar));
    }
    if let Some(number) = value.strip_prefix("f-") {
        return KeyCode::Function(number.parse().expect("valid function number"));
    }
    match value {
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "escape" => KeyCode::Escape,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "right" => KeyCode::Right,
        "left" => KeyCode::Left,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "insert" => KeyCode::Insert,
        "delete" => KeyCode::Delete,
        "page-up" => KeyCode::PageUp,
        "page-down" => KeyCode::PageDown,
        "unknown" => KeyCode::Unknown,
        _ => panic!("invalid key {value}"),
    }
}

fn modifiers(value: &str) -> Modifiers {
    let mut result = Modifiers::NONE;
    if value == "-" {
        return result;
    }
    for modifier in value.split('+') {
        match modifier {
            "shift" => result.shift = true,
            "alt" => result.alt = true,
            "control" => result.control = true,
            "meta" => result.meta = true,
            _ => panic!("invalid modifier {modifier}"),
        }
    }
    result
}

fn key_action(value: &str) -> KeyAction {
    match value {
        "unknown" => KeyAction::Unknown,
        "press" => KeyAction::Press,
        "repeat" => KeyAction::Repeat,
        "release" => KeyAction::Release,
        _ => panic!("invalid key action {value}"),
    }
}

fn scalar_text(value: &str) -> String {
    value
        .strip_prefix("U+")
        .unwrap_or_else(|| panic!("invalid scalar sequence {value}"))
        .split("+U+")
        .map(scalar_value)
        .collect()
}

fn scalar_value(value: &str) -> char {
    u32::from_str_radix(value, 16)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or_else(|| panic!("invalid scalar {value}"))
}

fn bindings(value: &str) -> Vec<KeyBinding> {
    if value == "none" {
        return Vec::new();
    }
    value.split('/').map(binding).collect()
}

fn binding(value: &str) -> KeyBinding {
    let parts: Vec<_> = value.split('@').collect();
    assert_eq!(parts.len(), 3, "invalid binding {value}");
    KeyBinding::new(stroke(parts[0]))
        .with_repeat_policy(repeat_policy(parts[1]))
        .with_support(binding_support(parts[2]))
}

fn repeat_policy(value: &str) -> RepeatPolicy {
    match value {
        "initial" => RepeatPolicy::InitialOnly,
        "allow" => RepeatPolicy::AllowRepeat,
        _ => panic!("invalid repeat policy {value}"),
    }
}

fn binding_support(value: &str) -> BindingSupport {
    match value {
        "unknown" => BindingSupport::Unknown,
        "supported" => BindingSupport::Supported,
        "unsupported" => BindingSupport::Unsupported,
        _ => panic!("invalid binding support {value}"),
    }
}

fn availability(value: &str) -> ActionAvailability {
    match value {
        "enabled" => ActionAvailability::Enabled,
        "disabled-pass-through" => ActionAvailability::DisabledPassThrough,
        "disabled-consume" => ActionAvailability::DisabledConsume,
        _ => panic!("invalid availability {value}"),
    }
}

fn scopes(value: &str, action: &ActionId) -> Vec<KeyScope> {
    if value == "-" {
        return Vec::new();
    }
    value
        .split(';')
        .map(|value| {
            let (id, replacement) = value
                .split_once(':')
                .unwrap_or_else(|| panic!("invalid scope {value}"));
            let mut key_map = KeyMap::new();
            if replacement != "inherit" {
                key_map = key_map
                    .rebind(action.clone(), bindings(replacement))
                    .expect("one override per fixture scope");
            }
            KeyScope::new(id, key_map)
        })
        .collect()
}

fn label_scopes(value: &str, action: &ActionId) -> Vec<KeyScope> {
    if value == "-" {
        return Vec::new();
    }
    value
        .split(';')
        .map(|value| {
            let (id, replacement) = value
                .split_once(':')
                .unwrap_or_else(|| panic!("invalid label scope {value}"));
            let key_map = KeyMap::new()
                .relabel(action.clone(), replacement)
                .expect("one label override per fixture scope");
            KeyScope::new(id, key_map)
        })
        .collect()
}

fn empty_scopes(value: &str) -> Vec<KeyScope> {
    list(value)
        .into_iter()
        .map(|id| KeyScope::new(id, KeyMap::new()))
        .collect()
}

fn action_descriptors(value: &str) -> Vec<ActionDescriptor> {
    value
        .split(';')
        .map(|value| {
            let parts: Vec<_> = value.split('@').collect();
            assert_eq!(parts.len(), 3, "invalid action {value}");
            let bindings = if parts[2] == "none" {
                Vec::new()
            } else {
                parts[2]
                    .split('/')
                    .map(|stroke_value| KeyBinding::new(stroke(stroke_value)))
                    .collect()
            };
            ActionDescriptor::new(parts[0], parts[0], bindings)
                .with_availability(availability(parts[1]))
        })
        .collect()
}

fn conflict_kind(value: &str) -> BindingConflictKind {
    match value {
        "duplicate-action" => BindingConflictKind::DuplicateAction,
        "duplicate-binding" => BindingConflictKind::DuplicateBinding,
        "ambiguous-binding" => BindingConflictKind::AmbiguousBinding,
        _ => panic!("invalid conflict kind {value}"),
    }
}

fn boolean(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        _ => panic!("invalid Boolean {value}"),
    }
}

fn list(value: &str) -> Vec<&str> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').collect()
    }
}
