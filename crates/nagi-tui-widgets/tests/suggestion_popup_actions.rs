//! Shared SuggestionPopup action and Composer integration tests

mod support;

use nagi_tui::{
    App, Effect, Event, KeyAction, KeyCode, KeyEvent, KeyProtocol, Modifiers, MouseButton,
    MouseEvent, MouseKind, Node, NodeId, Runtime, Size, Style, VirtualClock,
};
use nagi_tui_widgets::{
    Composer, ComposerState, SuggestionId, SuggestionItems, SuggestionPopup, SuggestionPopupStatus,
    SuggestionRowContext,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum SuggestionMessage {
    Change(ComposerState),
    Select(SuggestionId),
    Accept(SuggestionId),
    Dismiss,
    Submit,
}

struct SuggestionApp {
    composer: ComposerState,
    candidates: SuggestionItems,
    selected: Option<SuggestionId>,
    status: SuggestionPopupStatus,
    enabled: bool,
    open: bool,
    messages: Vec<String>,
}

impl SuggestionApp {
    fn normalized_selection(&self) -> Option<&SuggestionId> {
        if self.candidates.is_empty() {
            return None;
        }
        self.selected
            .as_ref()
            .and_then(|selected| {
                self.candidates
                    .as_slice()
                    .iter()
                    .find(|candidate| *candidate == selected)
            })
            .or_else(|| self.candidates.get(0))
    }
}

impl App for SuggestionApp {
    type Message = SuggestionMessage;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match &message {
            SuggestionMessage::Change(state) => {
                self.composer = state.clone();
                self.messages
                    .push(format!("change:{}", state.text_area().value()));
            }
            SuggestionMessage::Select(id) => {
                self.selected = Some(id.clone());
                self.messages.push(format!("select:{}", id.as_str()));
            }
            SuggestionMessage::Accept(id) => {
                self.messages.push(format!("accept:{}", id.as_str()));
            }
            SuggestionMessage::Dismiss => {
                self.open = false;
                self.messages.push("dismiss".to_owned());
            }
            SuggestionMessage::Submit => self.messages.push("submit".to_owned()),
        }
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let composer = Composer::new(
            "composer",
            "composer-viewport",
            "composer-caret",
            self.composer.clone(),
            SuggestionMessage::Change,
            || SuggestionMessage::Submit,
        )
        .rows(1, 1)
        .into_node();
        SuggestionPopup::new(
            "suggestions",
            composer,
            "composer-caret",
            "composer",
            self.candidates.clone(),
            self.selected.clone(),
            |context: SuggestionRowContext| {
                let style = if context.is_selected() {
                    Style {
                        reverse: true,
                        ..Style::default()
                    }
                } else {
                    Style::default()
                };
                Node::styled_text(context.id().as_str(), style)
            },
            SuggestionMessage::Select,
            SuggestionMessage::Accept,
            || SuggestionMessage::Dismiss,
        )
        .status(self.status)
        .enabled(self.enabled)
        .open(self.open)
        .visible_rows(3)
        .into_node()
    }
}

#[test]
fn suggestion_popup_matches_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/suggestion-popup.txt",
        "widget-suggestion-popup",
        &[
            "candidates",
            "selected",
            "status",
            "enabled",
            "event",
            "expected-selected",
            "expected-value",
            "expected-open",
            "expected-messages",
            "consumed",
        ],
    ) else {
        return;
    };

    for record in records {
        let mut runtime = Runtime::with_clock(
            SuggestionApp {
                composer: ComposerState::at_end(""),
                candidates: SuggestionItems::new(fixture_ids(record.field("candidates")))
                    .expect("fixture candidates are unique"),
                selected: fixture_optional_id(record.field("selected")),
                status: fixture_status(record.field("status")),
                enabled: fixture_bool(record.field("enabled")),
                open: true,
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(20, 8)),
            VirtualClock::new(),
        )
        .expect("runtime");
        runtime
            .render_if_dirty()
            .expect("initial render")
            .expect("initial frame");
        assert!(
            runtime
                .request_focus(&NodeId::from("composer"))
                .expect("focus request"),
            "case {}",
            record.id
        );
        runtime.render_if_dirty().expect("focused render");

        let mut consumed = None;
        if record.field("event") != "-" {
            let dispatch = runtime
                .dispatch_event(&fixture_event(record.field("event")))
                .expect("event dispatch");
            consumed = Some(dispatch.consumed());
            runtime.process_pending().expect("pending messages");
            runtime.render_if_dirty().expect("updated render");
        }

        assert_eq!(
            runtime
                .app()
                .normalized_selection()
                .map(SuggestionId::as_str),
            fixture_optional_text(record.field("expected-selected")),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().composer.text_area().value(),
            record.field("expected-value"),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().open,
            fixture_bool(record.field("expected-open")),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().messages,
            fixture_strings(record.field("expected-messages")),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.interaction().focused(),
            Some(&NodeId::from("composer")),
            "case {}",
            record.id
        );
        if record.field("consumed") != "-" {
            assert_eq!(
                consumed,
                Some(fixture_bool(record.field("consumed"))),
                "case {}",
                record.id
            );
        }
    }
}

fn fixture_ids(value: &str) -> Vec<SuggestionId> {
    fixture_strings(value)
        .into_iter()
        .map(SuggestionId::new)
        .collect()
}

fn fixture_optional_id(value: &str) -> Option<SuggestionId> {
    fixture_optional_text(value).map(SuggestionId::new)
}

fn fixture_optional_text(value: &str) -> Option<&str> {
    (value != "-").then_some(value)
}

fn fixture_strings(value: &str) -> Vec<String> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').map(str::to_owned).collect()
    }
}

fn fixture_status(value: &str) -> SuggestionPopupStatus {
    match value {
        "ready" => SuggestionPopupStatus::Ready,
        "loading" => SuggestionPopupStatus::Loading,
        _ => panic!("invalid SuggestionPopup status {value}"),
    }
}

fn fixture_bool(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        _ => panic!("invalid Boolean {value}"),
    }
}

fn fixture_event(value: &str) -> Event {
    let key = |code, action| {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers::NONE,
            action,
            text: None,
            protocol: KeyProtocol::Legacy,
        })
    };
    match value {
        "down" => key(KeyCode::Down, KeyAction::Press),
        "repeat-down" => key(KeyCode::Down, KeyAction::Repeat),
        "up" => key(KeyCode::Up, KeyAction::Press),
        "enter" => key(KeyCode::Enter, KeyAction::Press),
        "repeat-enter" => key(KeyCode::Enter, KeyAction::Repeat),
        "escape" => key(KeyCode::Escape, KeyAction::Press),
        "repeat-escape" => key(KeyCode::Escape, KeyAction::Repeat),
        "text-X" => Event::Text("X".to_owned()),
        "mouse-second" => Event::Mouse(MouseEvent {
            kind: MouseKind::Press,
            button: MouseButton::Left,
            x: 1,
            y: 3,
            modifiers: Modifiers::NONE,
        }),
        _ => panic!("invalid SuggestionPopup event {value}"),
    }
}
