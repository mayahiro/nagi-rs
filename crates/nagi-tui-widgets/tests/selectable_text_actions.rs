//! Shared SelectableText semantic-action integration tests

mod support;

use std::ops::Range;

use nagi_text::WidthProfile;
use nagi_tui::{
    ActionAvailability, App, BindingConflictKind, Color, Effect, Event, EventDispatch,
    HorizontalAlignment, Insets, KeyAction, KeyBinding, KeyCode, KeyEvent, KeyMap, KeyProtocol,
    KeyScope, KeyStroke, Modifiers, MouseButton, MouseEvent, MouseKind, Node, NodeId,
    ParagraphOptions, RepeatPolicy, Runtime, RuntimeConfig, RuntimeError, ScrollAxis,
    ScrollViewportOptions, Size, Style, TEXT_COPY_DOCUMENT_ACTION_ID,
    TEXT_COPY_SELECTION_ACTION_ID, TEXT_CURSOR_DOCUMENT_END_ACTION_ID,
    TEXT_CURSOR_DOCUMENT_START_ACTION_ID, TEXT_CURSOR_LEFT_ACTION_ID,
    TEXT_CURSOR_LINE_END_ACTION_ID, TEXT_CURSOR_LINE_START_ACTION_ID, TEXT_CURSOR_RIGHT_ACTION_ID,
    TEXT_CURSOR_WORD_LEFT_ACTION_ID, TEXT_CURSOR_WORD_RIGHT_ACTION_ID, TEXT_SELECT_ALL_ACTION_ID,
    TEXT_SELECTION_EXTEND_DOCUMENT_END_ACTION_ID, TEXT_SELECTION_EXTEND_DOCUMENT_START_ACTION_ID,
    TEXT_SELECTION_EXTEND_LEFT_ACTION_ID, TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID,
    TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID, TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID,
    TEXT_SELECTION_EXTEND_WORD_LEFT_ACTION_ID, TEXT_SELECTION_EXTEND_WORD_RIGHT_ACTION_ID,
    TextSpan, VirtualClock, WrapMode,
};
use nagi_tui_widgets::{
    SelectableText, SelectableTextContent, SelectableTextState, TextCopyKind, TextCopyRequest,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum SelectableTextMessage {
    Change(SelectableTextState),
    Copy(TextCopyRequest),
    Scroll(nagi_tui::ScrollOffset),
}

struct SelectableTextFixtureApp {
    content: SelectableTextContent,
    state: SelectableTextState,
    enabled: bool,
    copy: bool,
    key_map: KeyMap,
    messages: Vec<SelectableTextMessage>,
}

struct SelectableTextPointerApp {
    content: SelectableTextContent,
    state: SelectableTextState,
    enabled: bool,
    options: ParagraphOptions,
    scroll_axis: Option<ScrollAxis>,
    messages: Vec<SelectableTextMessage>,
}

impl App for SelectableTextPointerApp {
    type Message = SelectableTextMessage;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if let SelectableTextMessage::Change(state) = message {
            self.state = state;
            self.messages.push(SelectableTextMessage::Change(state));
        } else {
            self.messages.push(message);
        }
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let text = SelectableText::new(
            "text",
            self.content.clone(),
            self.state,
            SelectableTextMessage::Change,
        )
        .enabled(self.enabled)
        .paragraph_options(self.options)
        .into_node();
        let Some(axis) = self.scroll_axis else {
            return text;
        };
        Node::scroll_viewport_with_options(
            "scroll",
            text,
            ScrollViewportOptions {
                axis,
                on_scroll: Some(Box::new(|state| {
                    SelectableTextMessage::Scroll(state.offset)
                })),
                ..ScrollViewportOptions::default()
            },
        )
    }
}

#[test]
fn selectable_text_pointer_selection_matches_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/selectable-text-pointer.txt",
        "widget-selectable-text-pointer",
        &[
            "content",
            "width",
            "height",
            "wrap",
            "alignment",
            "profile",
            "scroll",
            "enabled",
            "cursor",
            "anchor",
            "events",
            "expected-cursor",
            "expected-anchor",
            "messages",
            "consumed",
            "capture",
            "focus",
            "offset",
        ],
    ) else {
        return;
    };
    for record in records {
        let content = SelectableTextContent::plain(record.text("content"));
        let state = content.normalize_state(fixture_state(
            record.field("cursor"),
            record.field("anchor"),
        ));
        let mut config = RuntimeConfig::new(Size::new(
            fixture_u32(record.field("width")),
            fixture_u32(record.field("height")),
        ));
        config.width_profile = match record.field("profile") {
            "modern" => WidthProfile::MODERN,
            "cjk" => WidthProfile::CJK,
            value => panic!("invalid pointer WidthProfile {value}"),
        };
        let options = ParagraphOptions {
            wrap: match record.field("wrap") {
                "word" => WrapMode::Word,
                "hard" => WrapMode::Hard,
                "none" => WrapMode::None,
                value => panic!("invalid pointer WrapMode {value}"),
            },
            alignment: match record.field("alignment") {
                "start" => HorizontalAlignment::Start,
                "center" => HorizontalAlignment::Center,
                "end" => HorizontalAlignment::End,
                value => panic!("invalid pointer alignment {value}"),
            },
        };
        let mut runtime = Runtime::with_clock(
            SelectableTextPointerApp {
                content,
                state,
                enabled: fixture_boolean(record.field("enabled")),
                options,
                scroll_axis: match record.field("scroll") {
                    "none" => None,
                    "vertical" => Some(ScrollAxis::Vertical),
                    "horizontal" => Some(ScrollAxis::Horizontal),
                    value => panic!("invalid pointer scroll axis {value}"),
                },
                messages: Vec::new(),
            },
            config,
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        let mut consumed = Vec::new();
        for event in record.field("events").split(',') {
            let dispatch = runtime
                .dispatch_event(&pointer_fixture_event(event))
                .unwrap();
            consumed.push(dispatch.consumed());
            runtime.process_pending().unwrap();
            runtime.render_if_dirty().unwrap();
        }
        assert_eq!(
            runtime.app().state,
            fixture_state(
                record.field("expected-cursor"),
                record.field("expected-anchor")
            ),
            "case {} state",
            record.id
        );
        assert_eq!(
            pointer_fixture_messages(&runtime.app().messages),
            record.field("messages"),
            "case {} messages",
            record.id
        );
        let expected_consumed = record
            .field("consumed")
            .split(',')
            .map(fixture_boolean)
            .collect::<Vec<_>>();
        assert_eq!(consumed, expected_consumed, "case {} consumed", record.id);
        assert_eq!(
            runtime
                .interaction()
                .pointer_capture()
                .map_or("none", NodeId::as_str),
            record.field("capture"),
            "case {} capture",
            record.id
        );
        assert_eq!(
            runtime
                .interaction()
                .focused()
                .map_or("none", NodeId::as_str),
            record.field("focus"),
            "case {} focus",
            record.id
        );
        let expected_offset = fixture_scroll_offset(record.field("offset"));
        assert_eq!(
            runtime.interaction().scroll_offset(&NodeId::from("scroll")),
            expected_offset,
            "case {} scroll offset",
            record.id
        );
    }
}

impl App for SelectableTextFixtureApp {
    type Message = SelectableTextMessage;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if let SelectableTextMessage::Change(state) = message {
            self.state = state;
            self.messages.push(SelectableTextMessage::Change(state));
        } else {
            self.messages.push(message);
        }
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let mut text = SelectableText::new(
            "text",
            self.content.clone(),
            self.state,
            SelectableTextMessage::Change,
        )
        .enabled(self.enabled);
        if self.copy {
            text = text.on_copy(SelectableTextMessage::Copy);
        }
        Node::padding(text.into_node(), Insets::all(0))
            .with_key_scope(KeyScope::new("scope", self.key_map.clone()))
    }
}

#[test]
fn selectable_text_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/selectable-text.txt",
        "widget-selectable-text",
        &[
            "content",
            "cursor",
            "anchor",
            "enabled",
            "copy",
            "hidden",
            "mode",
            "event",
            "expected-cursor",
            "expected-anchor",
            "message",
            "copy-kind",
            "copy-range",
            "copy-text",
            "consumed",
            "focus",
            "availability",
        ],
    ) else {
        return;
    };

    for record in records {
        let content = fixture_content(
            record.text("content"),
            fixture_boolean(record.field("hidden")),
        );
        let state = content.normalize_state(fixture_state(
            record.field("cursor"),
            record.field("anchor"),
        ));
        let enabled = fixture_boolean(record.field("enabled"));
        let copy = fixture_boolean(record.field("copy"));

        if record.field("availability") != "-" {
            let mut text = SelectableText::new(
                "text",
                content.clone(),
                state,
                SelectableTextMessage::Change,
            )
            .enabled(enabled);
            if copy {
                text = text.on_copy(SelectableTextMessage::Copy);
            }
            let action_id = fixture_action_id(record.field("event"));
            let descriptor = text
                .action_descriptors()
                .into_iter()
                .find(|descriptor| descriptor.id().as_str() == action_id)
                .unwrap_or_else(|| panic!("case {} has no {action_id} action", record.id));
            assert_eq!(
                fixture_availability(descriptor.availability()),
                record.field("availability"),
                "case {} availability",
                record.id
            );
        }

        let mut runtime = Runtime::with_clock(
            SelectableTextFixtureApp {
                content,
                state,
                enabled,
                copy,
                key_map: fixture_key_map(record.field("mode")),
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(24, 6)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();

        if enabled {
            assert!(
                runtime.request_focus(&NodeId::from("text")).unwrap(),
                "case {} focus request",
                record.id
            );
        } else {
            assert!(
                !runtime.request_focus(&NodeId::from("text")).unwrap(),
                "case {} disabled focus request",
                record.id
            );
        }

        let dispatch = if record.field("event") == "none" {
            None
        } else {
            let dispatch = runtime
                .dispatch_event(&fixture_event(record.field("event")))
                .unwrap();
            runtime.process_pending().unwrap();
            Some(dispatch)
        };

        let expected_state = fixture_state(
            record.field("expected-cursor"),
            record.field("expected-anchor"),
        );
        assert_eq!(
            runtime.app().state,
            expected_state,
            "case {} state",
            record.id
        );
        assert_messages(&runtime.app().messages, &record);
        assert_dispatch(dispatch.as_ref(), record.field("consumed"), &record.id);
        let actual_focus = runtime.interaction().focused().map(NodeId::as_str);
        let expected_focus = match record.field("focus") {
            "none" => None,
            "text" => Some("text"),
            value => panic!("invalid SelectableText focus {value}"),
        };
        assert_eq!(actual_focus, expected_focus, "case {} focus", record.id);
    }
}

#[test]
fn selectable_text_declares_the_ordered_action_contract() {
    let descriptors = SelectableText::new(
        "text",
        SelectableTextContent::plain("text"),
        SelectableTextState::with_selection(4, 0),
        SelectableTextMessage::Change,
    )
    .on_copy(SelectableTextMessage::Copy)
    .action_descriptors();
    let expected = [
        (
            TEXT_CURSOR_LEFT_ACTION_ID,
            "Left",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_CURSOR_RIGHT_ACTION_ID,
            "Right",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_CURSOR_WORD_LEFT_ACTION_ID,
            "Ctrl+Left",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_CURSOR_WORD_RIGHT_ACTION_ID,
            "Ctrl+Right",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_CURSOR_LINE_START_ACTION_ID,
            "Home",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_CURSOR_LINE_END_ACTION_ID,
            "End",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_CURSOR_DOCUMENT_START_ACTION_ID,
            "Ctrl+Home",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_CURSOR_DOCUMENT_END_ACTION_ID,
            "Ctrl+End",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_SELECTION_EXTEND_LEFT_ACTION_ID,
            "Shift+Left",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID,
            "Shift+Right",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_SELECTION_EXTEND_WORD_LEFT_ACTION_ID,
            "Ctrl+Shift+Left",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_SELECTION_EXTEND_WORD_RIGHT_ACTION_ID,
            "Ctrl+Shift+Right",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID,
            "Shift+Home",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID,
            "Shift+End",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_SELECTION_EXTEND_DOCUMENT_START_ACTION_ID,
            "Ctrl+Shift+Home",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_SELECTION_EXTEND_DOCUMENT_END_ACTION_ID,
            "Ctrl+Shift+End",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_SELECT_ALL_ACTION_ID,
            "Ctrl+a",
            RepeatPolicy::AllowRepeat,
        ),
        (
            TEXT_COPY_SELECTION_ACTION_ID,
            "Ctrl+c",
            RepeatPolicy::InitialOnly,
        ),
        (
            TEXT_COPY_DOCUMENT_ACTION_ID,
            "Ctrl+Shift+c",
            RepeatPolicy::InitialOnly,
        ),
    ];
    assert_eq!(descriptors.len(), expected.len());
    for (descriptor, (id, binding, repeat)) in descriptors.iter().zip(expected) {
        assert_eq!(descriptor.id().as_str(), id);
        assert_eq!(descriptor.default_bindings().len(), 1, "action {id}");
        assert_eq!(
            descriptor.default_bindings()[0].stroke().notation(),
            binding
        );
        assert_eq!(descriptor.default_bindings()[0].repeat_policy(), repeat);
    }
}

struct SelectableTextVisualApp {
    enabled: bool,
}

impl App for SelectableTextVisualApp {
    type Message = SelectableTextState;

    fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        SelectableText::new(
            "visual",
            SelectableTextContent::styled([
                TextSpan::new(
                    "ab",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                ),
                TextSpan::new(
                    "cd",
                    Style {
                        foreground: Color::Indexed(2),
                        ..Style::default()
                    },
                ),
            ]),
            SelectableTextState::with_selection(3, 1),
            |state| state,
        )
        .enabled(self.enabled)
        .into_node()
    }
}

#[test]
fn selectable_text_merges_base_selection_focus_and_disabled_styles() {
    let mut enabled = Runtime::with_clock(
        SelectableTextVisualApp { enabled: true },
        nagi_tui::RuntimeConfig::new(Size::new(4, 1)),
        VirtualClock::new(),
    )
    .unwrap();
    let initial = enabled.render_if_dirty().unwrap().unwrap();
    assert!(initial.surface().cell(0, 0).unwrap().style().bold);
    assert!(!initial.surface().cell(0, 0).unwrap().style().reverse);
    assert!(initial.surface().cell(1, 0).unwrap().style().reverse);
    assert!(initial.surface().cell(2, 0).unwrap().style().reverse);
    assert_eq!(
        initial.surface().cell(3, 0).unwrap().style().foreground,
        Color::Indexed(2)
    );

    assert!(enabled.request_focus(&NodeId::from("visual")).unwrap());
    let focused = enabled.render_if_dirty().unwrap().unwrap();
    for x in 0..4 {
        assert!(focused.surface().cell(x, 0).unwrap().style().underline);
    }

    let mut disabled = Runtime::with_clock(
        SelectableTextVisualApp { enabled: false },
        nagi_tui::RuntimeConfig::new(Size::new(4, 1)),
        VirtualClock::new(),
    )
    .unwrap();
    let frame = disabled.render_if_dirty().unwrap().unwrap();
    assert!(!disabled.request_focus(&NodeId::from("visual")).unwrap());
    for x in 0..4 {
        assert!(frame.surface().cell(x, 0).unwrap().style().dim);
    }
    assert!(frame.surface().cell(1, 0).unwrap().style().reverse);
}

#[test]
fn selectable_text_reports_same_owner_binding_conflicts_before_input() {
    let control = Modifiers {
        control: true,
        ..Modifiers::NONE
    };
    let key_map = KeyMap::new()
        .rebind(
            TEXT_COPY_DOCUMENT_ACTION_ID,
            [KeyBinding::new(KeyStroke::character('c', control))],
        )
        .unwrap();
    let initial = SelectableTextState::with_selection(3, 0);
    let mut runtime = Runtime::with_clock(
        SelectableTextFixtureApp {
            content: SelectableTextContent::plain("text"),
            state: initial,
            enabled: true,
            copy: true,
            key_map,
            messages: Vec::new(),
        },
        nagi_tui::RuntimeConfig::new(Size::new(8, 1)),
        VirtualClock::new(),
    )
    .unwrap();
    let error = runtime.render_if_dirty().unwrap_err();
    let RuntimeError::BindingConflict(conflict) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.kind(), BindingConflictKind::AmbiguousBinding);
    assert_eq!(conflict.owner().as_str(), "text");
    assert_eq!(
        conflict
            .actions()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>(),
        [TEXT_COPY_SELECTION_ACTION_ID, TEXT_COPY_DOCUMENT_ACTION_ID]
    );
    assert_eq!(runtime.app().state, initial);
    assert!(runtime.app().messages.is_empty());
}

fn fixture_content(text: String, hidden: bool) -> SelectableTextContent {
    if hidden {
        SelectableTextContent::styled([TextSpan::new(
            text,
            Style {
                hidden: true,
                ..Style::default()
            },
        )])
    } else {
        SelectableTextContent::plain(text)
    }
}

fn fixture_state(cursor: &str, anchor: &str) -> SelectableTextState {
    let cursor = fixture_usize(cursor);
    if anchor == "-" {
        SelectableTextState::new(cursor)
    } else {
        SelectableTextState::with_selection(cursor, fixture_usize(anchor))
    }
}

fn fixture_key_map(mode: &str) -> KeyMap {
    match mode {
        "default" => KeyMap::new(),
        "selection-alt" => KeyMap::new()
            .rebind(
                TEXT_COPY_SELECTION_ACTION_ID,
                [KeyBinding::new(KeyStroke::character(
                    'c',
                    Modifiers {
                        alt: true,
                        ..Modifiers::NONE
                    },
                ))],
            )
            .unwrap(),
        "selection-unbound" => KeyMap::new()
            .rebind(TEXT_COPY_SELECTION_ACTION_ID, std::iter::empty())
            .unwrap(),
        value => panic!("invalid SelectableText key map mode {value}"),
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
    let control = Modifiers {
        control: true,
        ..Modifiers::NONE
    };
    let alt = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };
    let control_shift = Modifiers {
        control: true,
        shift: true,
        ..Modifiers::NONE
    };
    match value {
        "left" => key(KeyCode::Left, Modifiers::NONE, KeyAction::Press),
        "right" => key(KeyCode::Right, Modifiers::NONE, KeyAction::Press),
        "control-left" => key(KeyCode::Left, control, KeyAction::Press),
        "control-right" => key(KeyCode::Right, control, KeyAction::Press),
        "home" => key(KeyCode::Home, Modifiers::NONE, KeyAction::Press),
        "end" => key(KeyCode::End, Modifiers::NONE, KeyAction::Press),
        "control-home" => key(KeyCode::Home, control, KeyAction::Press),
        "control-end" => key(KeyCode::End, control, KeyAction::Press),
        "shift-left" => key(KeyCode::Left, shift, KeyAction::Press),
        "shift-right" => key(KeyCode::Right, shift, KeyAction::Press),
        "control-shift-left" => key(KeyCode::Left, control_shift, KeyAction::Press),
        "control-shift-right" => key(KeyCode::Right, control_shift, KeyAction::Press),
        "shift-home" => key(KeyCode::Home, shift, KeyAction::Press),
        "shift-end" => key(KeyCode::End, shift, KeyAction::Press),
        "control-shift-home" => key(KeyCode::Home, control_shift, KeyAction::Press),
        "control-shift-end" => key(KeyCode::End, control_shift, KeyAction::Press),
        "control-a" => key(KeyCode::Character('a'), control, KeyAction::Press),
        "control-c" => key(KeyCode::Character('c'), control, KeyAction::Press),
        "control-shift-c" => key(KeyCode::Character('c'), control_shift, KeyAction::Press),
        "alt-c" => key(KeyCode::Character('c'), alt, KeyAction::Press),
        "repeat-left" => key(KeyCode::Left, Modifiers::NONE, KeyAction::Repeat),
        "repeat-control-c" => key(KeyCode::Character('c'), control, KeyAction::Repeat),
        value => panic!("invalid SelectableText event {value}"),
    }
}

fn pointer_fixture_event(value: &str) -> Event {
    let (kind, coordinates) = value
        .split_once('@')
        .unwrap_or_else(|| panic!("invalid pointer event {value}"));
    let (x, y) = coordinates
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid pointer coordinates {coordinates}"));
    let (kind, button, modifiers) = match kind {
        "press" => (MouseKind::Press, MouseButton::Left, Modifiers::NONE),
        "shift-press" => (
            MouseKind::Press,
            MouseButton::Left,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
        ),
        "right-press" => (MouseKind::Press, MouseButton::Right, Modifiers::NONE),
        "move" => (MouseKind::Move, MouseButton::Left, Modifiers::NONE),
        "release" => (MouseKind::Release, MouseButton::Left, Modifiers::NONE),
        value => panic!("invalid pointer event kind {value}"),
    };
    Event::Mouse(MouseEvent {
        kind,
        button,
        x: fixture_u32(x),
        y: fixture_u32(y),
        modifiers,
    })
}

fn pointer_fixture_messages(messages: &[SelectableTextMessage]) -> String {
    if messages.is_empty() {
        return "-".to_owned();
    }
    messages
        .iter()
        .map(|message| match message {
            SelectableTextMessage::Change(state) => format!(
                "change:{}:{}",
                state.cursor(),
                state
                    .selection_anchor()
                    .map_or_else(|| "-".to_owned(), |anchor| anchor.to_string())
            ),
            SelectableTextMessage::Scroll(offset) => format!("scroll:{}:{}", offset.x, offset.y),
            SelectableTextMessage::Copy(_) => panic!("unexpected copy message in pointer fixture"),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn fixture_action_id(event: &str) -> &'static str {
    match event {
        "left" | "repeat-left" => TEXT_CURSOR_LEFT_ACTION_ID,
        "right" => TEXT_CURSOR_RIGHT_ACTION_ID,
        "control-left" => TEXT_CURSOR_WORD_LEFT_ACTION_ID,
        "control-right" => TEXT_CURSOR_WORD_RIGHT_ACTION_ID,
        "home" => TEXT_CURSOR_LINE_START_ACTION_ID,
        "end" => TEXT_CURSOR_LINE_END_ACTION_ID,
        "control-home" => TEXT_CURSOR_DOCUMENT_START_ACTION_ID,
        "control-end" => TEXT_CURSOR_DOCUMENT_END_ACTION_ID,
        "shift-left" => TEXT_SELECTION_EXTEND_LEFT_ACTION_ID,
        "shift-right" => TEXT_SELECTION_EXTEND_RIGHT_ACTION_ID,
        "control-shift-left" => TEXT_SELECTION_EXTEND_WORD_LEFT_ACTION_ID,
        "control-shift-right" => TEXT_SELECTION_EXTEND_WORD_RIGHT_ACTION_ID,
        "shift-home" => TEXT_SELECTION_EXTEND_LINE_START_ACTION_ID,
        "shift-end" => TEXT_SELECTION_EXTEND_LINE_END_ACTION_ID,
        "control-shift-home" => TEXT_SELECTION_EXTEND_DOCUMENT_START_ACTION_ID,
        "control-shift-end" => TEXT_SELECTION_EXTEND_DOCUMENT_END_ACTION_ID,
        "control-a" => TEXT_SELECT_ALL_ACTION_ID,
        "control-c" | "repeat-control-c" | "alt-c" => TEXT_COPY_SELECTION_ACTION_ID,
        "control-shift-c" => TEXT_COPY_DOCUMENT_ACTION_ID,
        value => panic!("invalid SelectableText availability event {value}"),
    }
}

fn assert_messages(messages: &[SelectableTextMessage], record: &support::Record) {
    match record.field("message") {
        "-" => assert!(
            messages.is_empty(),
            "case {} messages {messages:?}",
            record.id
        ),
        "change" => assert!(
            matches!(messages, [SelectableTextMessage::Change(_)]),
            "case {} messages {messages:?}",
            record.id
        ),
        "copy" => {
            let [SelectableTextMessage::Copy(request)] = messages else {
                panic!("case {} messages {messages:?}", record.id);
            };
            let expected_kind = match record.field("copy-kind") {
                "selection" => TextCopyKind::Selection,
                "document" => TextCopyKind::Document,
                value => panic!("invalid SelectableText copy kind {value}"),
            };
            assert_eq!(request.source().as_str(), "text", "case {}", record.id);
            assert_eq!(request.kind(), expected_kind, "case {}", record.id);
            assert_eq!(
                request.range(),
                fixture_range(record.field("copy-range")),
                "case {}",
                record.id
            );
            assert_eq!(
                request.text(),
                record.text("copy-text"),
                "case {}",
                record.id
            );
        }
        value => panic!("invalid SelectableText message {value}"),
    }
}

fn assert_dispatch(dispatch: Option<&EventDispatch>, consumed: &str, case: &str) {
    let Some(dispatch) = dispatch else {
        assert_eq!(consumed, "-", "case {case} missing dispatch");
        return;
    };
    assert_ne!(consumed, "-", "case {case} unexpected dispatch");
    assert_eq!(
        dispatch.consumed(),
        fixture_boolean(consumed),
        "case {case}"
    );
}

fn fixture_availability(value: ActionAvailability) -> &'static str {
    match value {
        ActionAvailability::Enabled => "enabled",
        ActionAvailability::DisabledPassThrough => "pass",
        ActionAvailability::DisabledConsume => "consume",
    }
}

fn fixture_range(value: &str) -> Range<usize> {
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid SelectableText range {value}"));
    fixture_usize(start)..fixture_usize(end)
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

fn fixture_u32(value: &str) -> u32 {
    value.parse().unwrap()
}

fn fixture_scroll_offset(value: &str) -> nagi_tui::ScrollOffset {
    let (x, y) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid scroll offset {value}"));
    nagi_tui::ScrollOffset::new(fixture_u32(x), fixture_u32(y))
}
