//! Shared SplitPane semantic-action integration tests

mod support;

use nagi_tui::{
    ActionAvailability, App, Effect, Event, EventDispatch, Insets, KeyAction, KeyCode, KeyEvent,
    KeyProtocol, Modifiers, Node, NodeId, Runtime, Size, SplitPaneAxis, VirtualClock,
};
use nagi_tui_widgets::{
    PANE_FOCUS_NEXT_ACTION_ID, PANE_FOCUS_PREVIOUS_ACTION_ID, PANE_RESIZE_NEXT_ACTION_ID,
    PANE_RESIZE_PREVIOUS_ACTION_ID, SplitPane, SplitPaneState,
};

struct SplitPaneActionApp {
    state: SplitPaneState,
    axis: SplitPaneAxis,
    step: u16,
    focus: bool,
    resize: bool,
    messages: Vec<String>,
}

impl App for SplitPaneActionApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if let Some(value) = message.strip_prefix("ratio:") {
            self.state = SplitPaneState::new(value.parse().expect("ratio message"));
        }
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let mut split = SplitPane::new(
            "split",
            Node::text("primary").focusable("primary"),
            Node::text("secondary").focusable("secondary"),
            self.state,
        )
        .axis(self.axis)
        .resize_step(self.step);
        if self.focus {
            split = split.focus_targets("primary", "secondary");
        }
        if self.resize {
            split = split.on_resize(|state| format!("ratio:{}", state.ratio()));
        }
        Node::padding(split.into_node(), Insets::all(0)).on_event("outer", |_| {
            nagi_tui::EventResult::message("raw".to_owned())
        })
    }
}

#[test]
fn split_pane_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/split-pane.txt",
        "widget-split-pane",
        &[
            "ratio",
            "axis",
            "step",
            "focus",
            "resize",
            "event",
            "start",
            "expected-ratio",
            "message",
            "consumed",
            "expected-focus",
            "availability",
        ],
    ) else {
        return;
    };

    for record in records {
        let state = SplitPaneState::new(fixture_u16(record.field("ratio")));
        let axis = fixture_axis(record.field("axis"));
        let focus = fixture_bool(record.field("focus"));
        let resize = fixture_bool(record.field("resize"));
        let mut descriptors = SplitPane::new(
            "split",
            Node::<String>::text("primary"),
            Node::text("secondary"),
            state,
        )
        .axis(axis);
        if focus {
            descriptors = descriptors.focus_targets("primary", "secondary");
        }
        if resize {
            descriptors = descriptors.on_resize(|next| format!("ratio:{}", next.ratio()));
        }
        let descriptors = descriptors.action_descriptors();
        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.id().as_str())
                .collect::<Vec<_>>(),
            [
                PANE_FOCUS_PREVIOUS_ACTION_ID,
                PANE_FOCUS_NEXT_ACTION_ID,
                PANE_RESIZE_PREVIOUS_ACTION_ID,
                PANE_RESIZE_NEXT_ACTION_ID,
            ],
            "case {} action IDs",
            record.id
        );
        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| fixture_availability(descriptor.availability()))
                .collect::<Vec<_>>(),
            fixture_list(record.field("availability")),
            "case {} availability",
            record.id
        );

        let mut runtime = Runtime::with_clock(
            SplitPaneActionApp {
                state,
                axis,
                step: fixture_u16(record.field("step")),
                focus,
                resize,
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(30, 6)),
            VirtualClock::new(),
        )
        .expect("runtime");
        runtime.render_if_dirty().expect("render");
        let start = NodeId::from(record.field("start"));
        assert!(runtime.request_focus(&start).expect("initial focus"));

        let dispatch = if record.field("event") == "none" {
            None
        } else {
            let dispatch = runtime
                .dispatch_event(&fixture_event(record.field("event")))
                .expect("dispatch");
            runtime.process_pending().expect("pending messages");
            Some(dispatch)
        };

        let expected_messages = fixture_list(record.field("message"));
        assert_eq!(
            runtime.app().messages,
            expected_messages,
            "case {} messages",
            record.id
        );
        assert_dispatch(
            dispatch.as_ref(),
            expected_messages.len(),
            fixture_bool(record.field("consumed")),
            &record.id,
        );
        assert_eq!(
            runtime.app().state.ratio(),
            fixture_u16(record.field("expected-ratio")),
            "case {} ratio",
            record.id
        );
        assert_eq!(
            runtime.interaction().focused().map(NodeId::as_str),
            Some(record.field("expected-focus")),
            "case {} focus",
            record.id
        );
    }
}

fn fixture_event(value: &str) -> Event {
    let (code, modifiers, action) = match value {
        "f6" => (KeyCode::Function(6), Modifiers::NONE, KeyAction::Press),
        "shift-f6" => (
            KeyCode::Function(6),
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
            KeyAction::Press,
        ),
        "alt-left" => (KeyCode::Left, alt(), KeyAction::Press),
        "alt-right" => (KeyCode::Right, alt(), KeyAction::Press),
        "repeat-alt-right" => (KeyCode::Right, alt(), KeyAction::Repeat),
        "alt-up" => (KeyCode::Up, alt(), KeyAction::Press),
        "alt-down" => (KeyCode::Down, alt(), KeyAction::Press),
        _ => panic!("unknown SplitPane event {value}"),
    };
    Event::Key(KeyEvent {
        code,
        modifiers,
        action,
        text: None,
        protocol: KeyProtocol::Legacy,
    })
}

const fn alt() -> Modifiers {
    Modifiers {
        alt: true,
        ..Modifiers::NONE
    }
}

fn fixture_axis(value: &str) -> SplitPaneAxis {
    match value {
        "horizontal" => SplitPaneAxis::Horizontal,
        "vertical" => SplitPaneAxis::Vertical,
        _ => panic!("invalid SplitPane axis {value}"),
    }
}

fn fixture_availability(value: ActionAvailability) -> String {
    match value {
        ActionAvailability::Enabled => "enabled",
        ActionAvailability::DisabledPassThrough => "pass",
        ActionAvailability::DisabledConsume => "consume",
    }
    .to_owned()
}

fn fixture_bool(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        _ => panic!("invalid fixture Boolean {value}"),
    }
}

fn fixture_list(value: &str) -> Vec<String> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').map(str::to_owned).collect()
    }
}

fn fixture_u16(value: &str) -> u16 {
    value.parse().expect("fixture u16")
}

fn assert_dispatch(dispatch: Option<&EventDispatch>, messages: usize, consumed: bool, case: &str) {
    match dispatch {
        Some(dispatch) => {
            assert_eq!(
                dispatch.messages(),
                messages,
                "case {case} dispatch messages"
            );
            assert_eq!(dispatch.consumed(), consumed, "case {case} consumed");
        }
        None => {
            assert_eq!(messages, 0, "case {case} missing dispatch messages");
            assert!(!consumed, "case {case} missing dispatch consumed");
        }
    }
}
