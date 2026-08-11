//! Shared TextArea visual-line and caret-viewport integration tests

mod support;

use std::ops::Range;

use nagi_tui::{
    ActionAvailability, App, Effect, Event, KeyAction, KeyCode, KeyEvent, Length, Modifiers, Node,
    NodeId, Runtime, Size, TEXT_CURSOR_DOWN_ACTION_ID, TEXT_CURSOR_UP_ACTION_ID, VirtualClock,
};
use nagi_tui_widgets::{TextArea, TextAreaBoundaryNavigation, TextAreaState};

struct VisualTextAreaApp {
    state: TextAreaState,
    wrap: Option<u32>,
    boundary: TextAreaBoundaryNavigation,
    height: u32,
}

impl App for VisualTextAreaApp {
    type Message = TextAreaState;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.state = message;
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let mut area = TextArea::new("area", self.state.clone(), |state| state)
            .boundary_navigation(self.boundary)
            .viewport("area-viewport", "area-caret", Length::Fixed(self.height));
        if let Some(width) = self.wrap {
            area = area.soft_wrap(width);
        }
        area.into_node()
    }
}

#[test]
fn visual_text_area_matches_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/text-area-visual.txt",
        "widget-text-area-visual",
        &[
            "initial",
            "cursor",
            "wrap",
            "boundary",
            "events",
            "height",
            "expected-cursor",
            "expected-preferred",
            "expected-selection",
            "expected-ranges",
            "expected-cursor-line",
            "expected-offset",
            "expected-up",
            "expected-down",
            "expected-consumed",
        ],
    ) else {
        return;
    };

    for record in records {
        let height = number(record.field("height")) as u32;
        let mut runtime = Runtime::with_clock(
            VisualTextAreaApp {
                state: TextAreaState::new(record.text("initial"), number(record.field("cursor"))),
                wrap: optional_number(record.field("wrap")).map(|value| value as u32),
                boundary: match record.field("boundary") {
                    "consume" => TextAreaBoundaryNavigation::Consume,
                    "bubble" => TextAreaBoundaryNavigation::Bubble,
                    value => panic!("invalid boundary {value}"),
                },
                height,
            },
            nagi_tui::RuntimeConfig::new(Size::new(12, height)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        assert!(
            runtime.request_focus(&NodeId::from("area")).unwrap(),
            "case {}",
            record.id
        );
        runtime.render_if_dirty().unwrap();

        let mut last_consumed = None;
        if record.field("events") != "-" {
            for event in record.field("events").split(',') {
                let dispatch = runtime
                    .dispatch_event(&visual_text_area_event(event))
                    .unwrap();
                last_consumed = Some(dispatch.consumed());
                runtime.process_pending().unwrap();
                runtime.render_if_dirty().unwrap();
            }
        }

        assert_eq!(
            runtime.app().state.cursor(),
            number(record.field("expected-cursor")),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().state.preferred_column(),
            optional_number(record.field("expected-preferred")),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().state.selection(),
            optional_range(record.field("expected-selection")),
            "case {}",
            record.id
        );
        assert_eq!(
            runtime
                .interaction()
                .scroll_offset(&NodeId::from("area-viewport"))
                .y,
            number(record.field("expected-offset")) as u32,
            "case {}",
            record.id
        );
        if record.field("expected-consumed") != "-" {
            assert_eq!(
                last_consumed,
                Some(record.field("expected-consumed") == "true"),
                "case {}",
                record.id
            );
        }

        let groups = runtime.active_action_groups().unwrap();
        let actions = groups
            .iter()
            .find(|group| group.owner().as_str() == "area")
            .unwrap_or_else(|| panic!("case {} has no TextArea actions", record.id));
        assert_availability(
            actions,
            TEXT_CURSOR_UP_ACTION_ID,
            record.field("expected-up"),
            &record.id,
        );
        assert_availability(
            actions,
            TEXT_CURSOR_DOWN_ACTION_ID,
            record.field("expected-down"),
            &record.id,
        );
    }
}

fn visual_text_area_event(value: &str) -> Event {
    let (code, shift) = match value {
        "up" => (KeyCode::Up, false),
        "down" => (KeyCode::Down, false),
        "left" => (KeyCode::Left, false),
        "right" => (KeyCode::Right, false),
        "shift-up" => (KeyCode::Up, true),
        "shift-down" => (KeyCode::Down, true),
        value => panic!("invalid visual TextArea event {value}"),
    };
    Event::Key(KeyEvent {
        code,
        modifiers: Modifiers {
            shift,
            ..Modifiers::NONE
        },
        action: KeyAction::Press,
        text: None,
        protocol: nagi_tui::KeyProtocol::Legacy,
    })
}

fn assert_availability(actions: &nagi_tui::ResolvedActions, id: &str, expected: &str, case: &str) {
    let action = actions
        .actions()
        .iter()
        .find(|action| action.id().as_str() == id)
        .unwrap_or_else(|| panic!("case {case} has no action {id}"));
    let expected = match expected {
        "enabled" => ActionAvailability::Enabled,
        "pass" => ActionAvailability::DisabledPassThrough,
        value => panic!("invalid availability {value}"),
    };
    assert_eq!(action.availability(), expected, "case {case} action {id}");
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

fn number(value: &str) -> usize {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
}
