//! Shared Drawer placement, laziness, and dismissal integration tests

mod support;

use std::cell::Cell;
use std::rc::Rc;

use nagi_tui::{
    ActionAvailability, App, Effect, Event, EventDispatch, Insets, KeyAction, KeyCode, KeyEvent,
    KeyProtocol, Length, Modifiers, Node, NodeId, Runtime, Size, VirtualClock,
};
use nagi_tui_widgets::{DISMISS_ACTION_ID, Drawer, DrawerSide};

struct DrawerActionApp {
    open: bool,
    side: DrawerSide,
    modal: bool,
    body: bool,
    dismiss: bool,
    builds: Rc<Cell<usize>>,
    messages: Vec<String>,
}

impl App for DrawerActionApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if message == "false" {
            self.open = false;
        }
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let base = Node::text("bbbbbbbbbb\nbbbbbbbbbb\nbbbbbbbbbb\nbbbbbbbbbb\nbbbbbbbbbb")
            .focusable("base");
        let size = match self.side {
            DrawerSide::Left | DrawerSide::Right => Length::Fixed(4),
            DrawerSide::Top | DrawerSide::Bottom => Length::Fixed(3),
        };
        let mut drawer = Drawer::new("drawer", base, self.open)
            .side(self.side)
            .size(size)
            .modal(self.modal);
        if self.body {
            let builds = Rc::clone(&self.builds);
            drawer = drawer.body(move || {
                builds.set(builds.get() + 1);
                Node::text("D").focusable("drawer-focus")
            });
        }
        if self.dismiss {
            drawer = drawer.on_dismiss(|| "false".to_owned());
        }
        Node::padding(drawer.into_node(), Insets::all(0)).on_event("outer", |_| {
            nagi_tui::EventResult::message("raw".to_owned())
        })
    }
}

#[test]
fn drawer_behavior_matches_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/drawer.txt",
        "widget-drawer",
        &[
            "open",
            "side",
            "modal",
            "body",
            "dismiss",
            "event",
            "start",
            "expected-open",
            "message",
            "consumed",
            "expected-focus",
            "builds",
            "expected-body",
            "availability",
        ],
    ) else {
        return;
    };

    for record in records {
        let open = fixture_bool(record.field("open"));
        let modal = fixture_bool(record.field("modal"));
        let start = record.field("start");
        let enter_from_base = open && modal && start == "base";
        let builds = Rc::new(Cell::new(0));
        let mut runtime = Runtime::with_clock(
            DrawerActionApp {
                open: open && !enter_from_base,
                side: fixture_side(record.field("side")),
                modal,
                body: fixture_bool(record.field("body")),
                dismiss: fixture_bool(record.field("dismiss")),
                builds: Rc::clone(&builds),
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(10, 5)),
            VirtualClock::new(),
        )
        .expect("runtime");
        let mut frame = runtime
            .render_if_dirty()
            .expect("render")
            .expect("initial frame");
        if start == "base" {
            assert!(
                runtime
                    .request_focus(&NodeId::from("base"))
                    .expect("base focus")
            );
        }
        if enter_from_base {
            runtime.app_mut().open = true;
            runtime.request_frame();
            frame = runtime
                .render_if_dirty()
                .expect("open render")
                .expect("open frame");
        } else if start == "drawer-focus" {
            assert!(
                runtime
                    .request_focus(&NodeId::from("drawer-focus"))
                    .expect("drawer focus")
            );
        }

        let descriptor = configured_drawer(&runtime).action_descriptor();
        assert_eq!(
            descriptor.id().as_str(),
            DISMISS_ACTION_ID,
            "case {} ID",
            record.id
        );
        assert_eq!(
            fixture_availability(descriptor.availability()),
            record.field("availability"),
            "case {} availability",
            record.id
        );

        let dispatch = if record.field("event") == "none" {
            None
        } else {
            let dispatch = runtime
                .dispatch_event(&escape_event())
                .expect("escape dispatch");
            runtime.process_pending().expect("pending message");
            if let Some(rendered) = runtime.render_if_dirty().expect("event render") {
                frame = rendered;
            }
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
            runtime.app().open,
            fixture_bool(record.field("expected-open")),
            "case {} open",
            record.id
        );
        assert_eq!(
            builds.get(),
            fixture_usize(record.field("builds")),
            "case {} body builds",
            record.id
        );
        let expected_focus = record.field("expected-focus");
        assert_eq!(
            runtime.interaction().focused().map(NodeId::as_str),
            Some(expected_focus),
            "case {} focus",
            record.id
        );
        assert_body_position(&frame, record.field("expected-body"), &record.id);
    }
}

fn configured_drawer(app_runtime: &Runtime<DrawerActionApp, VirtualClock>) -> Drawer<String> {
    let app = app_runtime.app();
    let mut drawer = Drawer::new("drawer", Node::text("base"), app.open);
    if app.dismiss {
        drawer = drawer.on_dismiss(|| "false".to_owned());
    }
    drawer
}

fn assert_body_position(frame: &nagi_tui::Frame, value: &str, case: &str) {
    let mut positions = Vec::new();
    for y in 0..frame.surface().height() {
        for x in 0..frame.surface().width() {
            if frame
                .surface()
                .cell(
                    i32::try_from(x).expect("surface x"),
                    i32::try_from(y).expect("surface y"),
                )
                .is_some_and(|cell| cell.content() == "D")
            {
                positions.push(format!("{x}:{y}"));
            }
        }
    }
    let expected = if value == "none" {
        Vec::new()
    } else {
        vec![value.to_owned()]
    };
    assert_eq!(positions, expected, "case {case} body position");
}

fn escape_event() -> Event {
    Event::Key(KeyEvent {
        code: KeyCode::Escape,
        modifiers: Modifiers::NONE,
        action: KeyAction::Press,
        text: None,
        protocol: KeyProtocol::Legacy,
    })
}

fn fixture_side(value: &str) -> DrawerSide {
    match value {
        "left" => DrawerSide::Left,
        "right" => DrawerSide::Right,
        "top" => DrawerSide::Top,
        "bottom" => DrawerSide::Bottom,
        _ => panic!("invalid Drawer side {value}"),
    }
}

fn fixture_availability(value: ActionAvailability) -> &'static str {
    match value {
        ActionAvailability::Enabled => "enabled",
        ActionAvailability::DisabledPassThrough => "pass",
        ActionAvailability::DisabledConsume => "consume",
    }
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

fn fixture_usize(value: &str) -> usize {
    value.parse().expect("fixture usize")
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
