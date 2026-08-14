//! SplitPane resize, pointer, and collapse-focus integration tests

use nagi_tui::{
    App, Effect, Event, Modifiers, MouseButton, MouseEvent, MouseKind, Node, NodeId, Runtime, Size,
    VirtualClock,
};
use nagi_tui_widgets::{SplitPane, SplitPaneState};

struct FocusSplitApp;

impl App for FocusSplitApp {
    type Message = ();

    fn update(&mut self, (): ()) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        SplitPane::new(
            "split",
            Node::text("primary").focusable("primary"),
            Node::text("secondary").focusable("secondary"),
            SplitPaneState::default(),
        )
        .minimums(5, 5)
        .focus_targets("primary", "secondary")
        .into_node()
    }
}

#[test]
fn collapse_moves_focus_to_the_remaining_pane_without_restoring_on_expand() {
    let mut runtime = Runtime::with_clock(
        FocusSplitApp,
        nagi_tui::RuntimeConfig::new(Size::new(12, 2)),
        VirtualClock::new(),
    )
    .expect("runtime");
    runtime.render_if_dirty().expect("initial render");
    assert!(
        runtime
            .request_focus(&NodeId::from("secondary"))
            .expect("focus")
    );

    runtime.resize(Size::new(8, 2));
    runtime.render_if_dirty().expect("collapsed render");
    assert_eq!(
        runtime.interaction().focused(),
        Some(&NodeId::from("primary"))
    );
    assert!(
        !runtime
            .request_focus(&NodeId::from("secondary"))
            .expect("hidden focus")
    );

    runtime.resize(Size::new(12, 2));
    runtime.render_if_dirty().expect("expanded render");
    assert_eq!(
        runtime.interaction().focused(),
        Some(&NodeId::from("primary"))
    );
}

struct PointerSplitApp {
    state: SplitPaneState,
}

impl App for PointerSplitApp {
    type Message = SplitPaneState;

    fn update(&mut self, state: Self::Message) -> Effect<Self::Message> {
        self.state = state;
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        SplitPane::new(
            "split",
            Node::text("primary"),
            Node::text("secondary"),
            self.state,
        )
        .on_resize(|state| state)
        .into_node()
    }
}

#[test]
fn pointer_drag_captures_updates_controlled_ratio_and_releases() {
    let mut runtime = Runtime::with_clock(
        PointerSplitApp {
            state: SplitPaneState::default(),
        },
        nagi_tui::RuntimeConfig::new(Size::new(11, 3)),
        VirtualClock::new(),
    )
    .expect("runtime");
    runtime.render_if_dirty().expect("initial render");

    let press = runtime
        .dispatch_event(&pointer(MouseKind::Press, 5, 1))
        .expect("press");
    assert!(press.consumed());
    assert_eq!(
        runtime.interaction().pointer_capture(),
        Some(&NodeId::from("split"))
    );

    let movement = runtime
        .dispatch_event(&pointer(MouseKind::Move, 8, 1))
        .expect("move");
    assert!(movement.consumed());
    assert_eq!(movement.messages(), 1);
    runtime.process_pending().expect("move update");
    assert_eq!(runtime.app().state.ratio(), 8_000);
    assert_eq!(
        runtime.interaction().pointer_capture(),
        Some(&NodeId::from("split"))
    );

    let release = runtime
        .dispatch_event(&pointer(MouseKind::Release, 7, 1))
        .expect("release");
    assert!(release.consumed());
    assert_eq!(release.messages(), 1);
    runtime.process_pending().expect("release update");
    assert_eq!(runtime.app().state.ratio(), 7_000);
    assert_eq!(runtime.interaction().pointer_capture(), None);
}

fn pointer(kind: MouseKind, x: u32, y: u32) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        button: MouseButton::Left,
        x,
        y,
        modifiers: Modifiers::NONE,
    })
}
