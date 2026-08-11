//! Shared Composer state, action, length, and viewport integration tests

mod support;

use std::ops::Range;

use nagi_tui::{
    ActionAvailability, App, BindingSupport, Effect, Event, Insets, KeyAction, KeyBinding, KeyCode,
    KeyEvent, KeyMap, KeyProtocol, KeyScope, KeyStroke, Modifiers, Node, NodeId, RepeatPolicy,
    Runtime, Size, TEXT_CURSOR_DOWN_ACTION_ID, TEXT_CURSOR_UP_ACTION_ID,
    TEXT_INSERT_LINE_BREAK_ACTION_ID, VirtualClock,
};
use nagi_tui_widgets::{
    COMPOSER_SUBMIT_ACTION_ID, Composer, ComposerOverflowPolicy, ComposerState,
    HISTORY_NEXT_ACTION_ID, HISTORY_PREVIOUS_ACTION_ID, TextAreaState,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum ComposerMessage {
    Change(ComposerState),
    Submit,
}

#[derive(Clone, Copy)]
enum FixtureLimit {
    None,
    Bytes(usize),
    Graphemes(usize),
}

struct ComposerApp {
    state: ComposerState,
    history: Vec<String>,
    wrap: Option<u32>,
    min_rows: u32,
    max_rows: u32,
    limit: FixtureLimit,
    overflow: ComposerOverflowPolicy,
    enabled: bool,
    submit_enabled: bool,
    key_map: KeyMap,
    validation: bool,
    messages: Vec<ComposerMessage>,
}

impl ComposerApp {
    fn composer(&self) -> Composer<ComposerMessage> {
        let mut composer = Composer::new(
            "composer",
            "composer-viewport",
            "composer-caret",
            self.state.clone(),
            ComposerMessage::Change,
            || ComposerMessage::Submit,
        )
        .enabled(self.enabled)
        .submit_enabled(self.submit_enabled)
        .rows(self.min_rows, self.max_rows)
        .history(self.history.clone());
        if let Some(width) = self.wrap {
            composer = composer.soft_wrap(width);
        }
        composer = match self.limit {
            FixtureLimit::None => composer,
            FixtureLimit::Bytes(maximum) => composer.maximum_utf8_bytes(maximum, self.overflow),
            FixtureLimit::Graphemes(maximum) => composer.maximum_graphemes(maximum, self.overflow),
        };
        if self.validation {
            composer = composer.validation(Node::text("!"));
        }
        composer
    }
}

impl App for ComposerApp {
    type Message = ComposerMessage;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if let ComposerMessage::Change(state) = &message {
            self.state = state.clone();
        }
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::padding(self.composer().into_node(), Insets::all(0))
            .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn composer_matches_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/composer.txt",
        "widget-composer",
        &[
            "initial",
            "cursor",
            "selection",
            "history",
            "wrap",
            "min-rows",
            "max-rows",
            "limit",
            "overflow",
            "enabled",
            "submit-enabled",
            "scope",
            "validation",
            "events",
            "expected",
            "expected-cursor",
            "expected-selection",
            "expected-history",
            "expected-draft",
            "expected-draft-cursor",
            "expected-messages",
            "expected-rows",
            "expected-offset",
            "expected-maximum",
            "expected-validation-row",
            "expected-submit",
            "expected-previous",
            "expected-next",
            "expected-up",
            "expected-down",
            "expected-consumed",
        ],
    ) else {
        return;
    };

    for record in records {
        let state = fixture_composer_state(
            record.text("initial"),
            number(record.field("cursor")),
            record.field("selection"),
        );
        let mut runtime = Runtime::with_clock(
            ComposerApp {
                state,
                history: fixture_history(record.field("history")),
                wrap: optional_number(record.field("wrap")).map(|value| value as u32),
                min_rows: number(record.field("min-rows")) as u32,
                max_rows: number(record.field("max-rows")) as u32,
                limit: fixture_limit(record.field("limit")),
                overflow: fixture_overflow(record.field("overflow")),
                enabled: boolean(record.field("enabled")),
                submit_enabled: boolean(record.field("submit-enabled")),
                key_map: fixture_key_map(record.field("scope")),
                validation: record.field("validation") == "error",
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(16, 10)),
            VirtualClock::new(),
        )
        .unwrap();
        let mut frame = runtime.render_if_dirty().unwrap().unwrap();
        if runtime.app().enabled {
            assert!(
                runtime.request_focus(&NodeId::from("composer")).unwrap(),
                "case {}",
                record.id
            );
            if let Some(rendered) = runtime.render_if_dirty().unwrap() {
                frame = rendered;
            }
        } else {
            assert!(
                !runtime.request_focus(&NodeId::from("composer")).unwrap(),
                "case {}",
                record.id
            );
        }

        let mut last_consumed = None;
        if record.field("events") != "-" {
            for event in record.field("events").split(',') {
                let dispatch = runtime.dispatch_event(&fixture_event(event)).unwrap();
                last_consumed = Some(dispatch.consumed());
                runtime.process_pending().unwrap();
                if let Some(rendered) = runtime.render_if_dirty().unwrap() {
                    frame = rendered;
                }
            }
        }

        assert_composer_state(&runtime.app().state, &record);
        assert_eq!(
            runtime
                .app()
                .messages
                .iter()
                .map(|message| match message {
                    ComposerMessage::Change(_) => "change",
                    ComposerMessage::Submit => "submit",
                })
                .collect::<Vec<_>>(),
            fixture_names(record.field("expected-messages")),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().composer().visible_rows(),
            number(record.field("expected-rows")) as u32,
            "case {}",
            record.id
        );
        let scroll = runtime
            .interaction()
            .scroll_state(&NodeId::from("composer-viewport"))
            .unwrap_or_else(|| panic!("case {} has no Composer viewport", record.id));
        assert_eq!(
            scroll.offset.y,
            number(record.field("expected-offset")) as u32,
            "case {}",
            record.id
        );
        assert_eq!(
            scroll.maximum.y,
            number(record.field("expected-maximum")) as u32,
            "case {}",
            record.id
        );
        if let Some(row) = optional_number(record.field("expected-validation-row")) {
            assert_eq!(
                frame
                    .surface()
                    .cell(0, i32::try_from(row).unwrap())
                    .unwrap()
                    .content(),
                "!",
                "case {}",
                record.id
            );
        }
        let descriptors = runtime.app().composer().action_descriptors();
        assert_availability(
            &descriptors,
            COMPOSER_SUBMIT_ACTION_ID,
            record.field("expected-submit"),
            &record.id,
        );
        assert_availability(
            &descriptors,
            HISTORY_PREVIOUS_ACTION_ID,
            record.field("expected-previous"),
            &record.id,
        );
        assert_availability(
            &descriptors,
            HISTORY_NEXT_ACTION_ID,
            record.field("expected-next"),
            &record.id,
        );
        assert_availability(
            &descriptors,
            TEXT_CURSOR_UP_ACTION_ID,
            record.field("expected-up"),
            &record.id,
        );
        assert_availability(
            &descriptors,
            TEXT_CURSOR_DOWN_ACTION_ID,
            record.field("expected-down"),
            &record.id,
        );
        if record.field("expected-consumed") != "-" {
            assert_eq!(
                last_consumed,
                Some(boolean(record.field("expected-consumed"))),
                "case {}",
                record.id
            );
        }
    }
}

#[test]
fn composer_descriptor_defaults_are_ordered_and_have_a_legacy_newline_fallback() {
    let descriptors = Composer::new(
        "composer",
        "viewport",
        "caret",
        ComposerState::default(),
        ComposerMessage::Change,
        || ComposerMessage::Submit,
    )
    .action_descriptors();
    assert_eq!(descriptors[0].id().as_str(), COMPOSER_SUBMIT_ACTION_ID);
    assert_eq!(descriptors[1].id().as_str(), HISTORY_PREVIOUS_ACTION_ID);
    assert_eq!(descriptors[2].id().as_str(), HISTORY_NEXT_ACTION_ID);
    assert_eq!(
        descriptors[0].default_bindings()[0].repeat_policy(),
        RepeatPolicy::InitialOnly
    );
    for descriptor in &descriptors[1..=2] {
        assert_eq!(
            descriptor.default_bindings()[0].repeat_policy(),
            RepeatPolicy::AllowRepeat
        );
    }
    let line_break = descriptors
        .iter()
        .find(|descriptor| descriptor.id().as_str() == TEXT_INSERT_LINE_BREAK_ACTION_ID)
        .unwrap();
    assert_eq!(
        line_break
            .default_bindings()
            .iter()
            .map(|binding| binding.stroke().notation())
            .collect::<Vec<_>>(),
        ["Shift+Enter", "Alt+Enter", "Ctrl+o"]
    );
    assert!(
        line_break
            .default_bindings()
            .iter()
            .all(|binding| binding.repeat_policy() == RepeatPolicy::AllowRepeat)
    );
    assert_eq!(
        line_break.default_bindings()[2].support(),
        BindingSupport::Supported
    );
}

fn assert_composer_state(state: &ComposerState, record: &support::Record) {
    assert_eq!(
        state.text_area().value(),
        record.text("expected"),
        "case {}",
        record.id
    );
    assert_eq!(
        state.text_area().cursor(),
        number(record.field("expected-cursor")),
        "case {}",
        record.id
    );
    assert_eq!(
        state.text_area().selection(),
        optional_range(record.field("expected-selection")),
        "case {}",
        record.id
    );
    assert_eq!(
        state.history_index(),
        optional_number(record.field("expected-history")),
        "case {}",
        record.id
    );
    if record.field("expected-draft") == "-" {
        assert!(state.draft().is_none(), "case {}", record.id);
    } else {
        let draft = state
            .draft()
            .unwrap_or_else(|| panic!("case {} has no draft", record.id));
        assert_eq!(
            draft.value(),
            record.text("expected-draft"),
            "case {}",
            record.id
        );
        assert_eq!(
            draft.cursor(),
            number(record.field("expected-draft-cursor")),
            "case {}",
            record.id
        );
    }
}

fn fixture_composer_state(value: String, cursor: usize, selection: &str) -> ComposerState {
    let mut text_area = TextAreaState::new(value, cursor);
    if let Some(range) = optional_range(selection) {
        let anchor = if cursor == range.start {
            range.end
        } else {
            range.start
        };
        text_area = text_area.select(anchor);
    }
    ComposerState::new(text_area)
}

fn fixture_history(value: &str) -> Vec<String> {
    if value == "-" {
        Vec::new()
    } else {
        value
            .split('/')
            .map(|entry| support::text_value(entry).unwrap())
            .collect()
    }
}

fn fixture_limit(value: &str) -> FixtureLimit {
    if value == "-" {
        return FixtureLimit::None;
    }
    let (unit, maximum) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid Composer limit {value}"));
    match unit {
        "bytes" => FixtureLimit::Bytes(number(maximum)),
        "graphemes" => FixtureLimit::Graphemes(number(maximum)),
        _ => panic!("invalid Composer limit unit {unit}"),
    }
}

fn fixture_overflow(value: &str) -> ComposerOverflowPolicy {
    match value {
        "reject" => ComposerOverflowPolicy::Reject,
        "truncate" => ComposerOverflowPolicy::Truncate,
        _ => panic!("invalid Composer overflow {value}"),
    }
}

fn fixture_key_map(value: &str) -> KeyMap {
    match value {
        "-" => KeyMap::new(),
        "swap" => {
            let control = Modifiers {
                control: true,
                ..Modifiers::NONE
            };
            KeyMap::new()
                .rebind(
                    COMPOSER_SUBMIT_ACTION_ID,
                    [KeyBinding::new(KeyStroke::new(KeyCode::Enter, control))],
                )
                .unwrap()
                .rebind(
                    TEXT_INSERT_LINE_BREAK_ACTION_ID,
                    [KeyBinding::new(KeyStroke::new(
                        KeyCode::Enter,
                        Modifiers::NONE,
                    ))],
                )
                .unwrap()
        }
        _ => panic!("invalid Composer scope {value}"),
    }
}

fn fixture_event(value: &str) -> Event {
    let key = |code, modifiers, action| {
        Event::Key(KeyEvent {
            code,
            modifiers,
            action,
            text: None,
            protocol: KeyProtocol::Legacy,
        })
    };
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    let alt = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };
    let control = Modifiers {
        control: true,
        ..Modifiers::NONE
    };
    match value {
        "enter" => key(KeyCode::Enter, Modifiers::NONE, KeyAction::Press),
        "repeat-enter" => key(KeyCode::Enter, Modifiers::NONE, KeyAction::Repeat),
        "shift-enter" => key(KeyCode::Enter, shift, KeyAction::Press),
        "alt-enter" => key(KeyCode::Enter, alt, KeyAction::Press),
        "control-enter" => key(KeyCode::Enter, control, KeyAction::Press),
        "control-o" => key(KeyCode::Character('o'), control, KeyAction::Press),
        "up" => key(KeyCode::Up, Modifiers::NONE, KeyAction::Press),
        "down" => key(KeyCode::Down, Modifiers::NONE, KeyAction::Press),
        "backspace" => key(KeyCode::Backspace, Modifiers::NONE, KeyAction::Press),
        "text-x" => Event::Text("x".to_owned()),
        "paste-xy" => Event::Paste("xy".to_owned()),
        "paste-lines" => Event::Paste("x\ny".to_owned()),
        "paste-combining" => Event::Paste("e\u{0301}x".to_owned()),
        _ => panic!("invalid Composer event {value}"),
    }
}

fn assert_availability(
    descriptors: &[nagi_tui::ActionDescriptor],
    id: &str,
    expected: &str,
    case: &str,
) {
    let descriptor = descriptors
        .iter()
        .find(|descriptor| descriptor.id().as_str() == id)
        .unwrap_or_else(|| panic!("case {case} has no action {id}"));
    let expected = match expected {
        "enabled" => ActionAvailability::Enabled,
        "pass" => ActionAvailability::DisabledPassThrough,
        "consume" => ActionAvailability::DisabledConsume,
        _ => panic!("invalid availability {expected}"),
    };
    assert_eq!(
        descriptor.availability(),
        expected,
        "case {case} action {id}"
    );
}

fn optional_range(value: &str) -> Option<Range<usize>> {
    (value != "-").then(|| {
        let (start, end) = value
            .split_once(':')
            .unwrap_or_else(|| panic!("invalid range {value}"));
        number(start)..number(end)
    })
}

fn optional_number(value: &str) -> Option<usize> {
    (value != "-").then(|| number(value))
}

fn fixture_names(value: &str) -> Vec<&str> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').collect()
    }
}

fn boolean(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        _ => panic!("invalid boolean {value}"),
    }
}

fn number(value: &str) -> usize {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
}
