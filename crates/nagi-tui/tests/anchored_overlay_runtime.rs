//! AnchoredOverlay rendering and pointer-routing integration tests

use nagi_tui::{
    App, Effect, Event, EventResult, MouseButton, MouseEvent, MouseKind, Node, Runtime, Size,
    VirtualClock,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OverlayMessage {
    Base,
    Overlay,
}

struct OverlayApp {
    anchor_present: bool,
    messages: Vec<OverlayMessage>,
}

impl App for OverlayApp {
    type Message = OverlayMessage;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let anchor = if self.anchor_present {
            Node::text("anchor").with_id("anchor")
        } else {
            Node::text("anchor").with_id("other")
        };
        let base = Node::column([
            anchor,
            Node::text("base").on_event("base", |event| {
                if matches!(event, Event::Mouse(_)) {
                    EventResult::consumed().emit(OverlayMessage::Base)
                } else {
                    EventResult::ignored()
                }
            }),
        ]);
        let overlay = Node::text("popup").on_event("overlay", |event| {
            if matches!(event, Event::Mouse(_)) {
                EventResult::consumed().emit(OverlayMessage::Overlay)
            } else {
                EventResult::ignored()
            }
        });
        Node::anchored_overlay(base, "anchor", overlay)
    }
}

#[test]
fn overlay_renders_and_routes_after_overlapping_base_content() {
    let mut runtime = Runtime::with_clock(
        OverlayApp {
            anchor_present: true,
            messages: Vec::new(),
        },
        nagi_tui::RuntimeConfig::new(Size::new(8, 3)),
        VirtualClock::new(),
    )
    .expect("runtime");
    let frame = runtime
        .render_if_dirty()
        .expect("render")
        .expect("initial frame");
    assert_eq!(
        frame.surface().cell(0, 1).expect("popup cell").content(),
        "p"
    );

    let dispatch = runtime
        .dispatch_event(&pointer_at(0, 1))
        .expect("pointer dispatch");
    assert!(dispatch.consumed());
    runtime.process_pending().expect("pending messages");
    assert_eq!(runtime.app().messages, [OverlayMessage::Overlay]);
}

#[test]
fn missing_anchor_omits_overlay_from_rendering_and_routing() {
    let mut runtime = Runtime::with_clock(
        OverlayApp {
            anchor_present: false,
            messages: Vec::new(),
        },
        nagi_tui::RuntimeConfig::new(Size::new(8, 3)),
        VirtualClock::new(),
    )
    .expect("runtime");
    let frame = runtime
        .render_if_dirty()
        .expect("render")
        .expect("initial frame");
    assert_eq!(
        frame.surface().cell(0, 1).expect("base cell").content(),
        "b"
    );

    let dispatch = runtime
        .dispatch_event(&pointer_at(0, 1))
        .expect("pointer dispatch");
    assert!(dispatch.consumed());
    runtime.process_pending().expect("pending messages");
    assert_eq!(runtime.app().messages, [OverlayMessage::Base]);
}

#[test]
fn zero_width_cursor_anchor_is_a_visible_placement_point() {
    struct CursorApp;

    impl App for CursorApp {
        type Message = ();

        fn update(&mut self, (): ()) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
            let base = Node::column([
                Node::row([
                    Node::text("A"),
                    Node::cursor_anchor("owner").with_id("anchor"),
                    Node::text("B"),
                ]),
                Node::text("base"),
            ]);
            Node::anchored_overlay(base, "anchor", Node::text("popup"))
        }
    }

    let mut runtime = Runtime::with_clock(
        CursorApp,
        nagi_tui::RuntimeConfig::new(Size::new(8, 3)),
        VirtualClock::new(),
    )
    .expect("runtime");
    let frame = runtime
        .render_if_dirty()
        .expect("render")
        .expect("initial frame");
    assert_eq!(
        frame.surface().cell(1, 1).expect("popup cell").content(),
        "p"
    );
}

fn pointer_at(x: u32, y: u32) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseKind::Press,
        button: MouseButton::Left,
        x,
        y,
        modifiers: nagi_tui::Modifiers::NONE,
    })
}
