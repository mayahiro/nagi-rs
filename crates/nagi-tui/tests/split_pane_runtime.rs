//! Core SplitPane semantic omission and rendering tests

use std::cell::Cell;
use std::rc::Rc;

use nagi_tui::{App, Effect, Node, Runtime, Size, SplitPaneOptions, VirtualClock, VirtualFragment};

struct LazyPaneApp {
    builds: Rc<Cell<usize>>,
}

impl App for LazyPaneApp {
    type Message = ();

    fn update(&mut self, (): ()) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let builds = Rc::clone(&self.builds);
        let secondary = Node::virtual_scroll_viewport("secondary", Size::new(20, 4), move |_| {
            builds.set(builds.get() + 1);
            VirtualFragment::new(Default::default(), Node::text("secondary"))
        });
        Node::split_pane(
            Node::text("primary").focusable("primary"),
            secondary,
            SplitPaneOptions {
                primary_minimum: 5,
                secondary_minimum: 5,
                ..SplitPaneOptions::default()
            },
        )
    }
}

#[test]
fn collapsed_pane_is_omitted_from_semantics_and_lazy_preparation() {
    let builds = Rc::new(Cell::new(0));
    let mut runtime = Runtime::with_clock(
        LazyPaneApp {
            builds: Rc::clone(&builds),
        },
        nagi_tui::RuntimeConfig::new(Size::new(8, 3)),
        VirtualClock::new(),
    )
    .expect("runtime");
    runtime.render_if_dirty().expect("collapsed render");

    assert_eq!(builds.get(), 0);
    assert!(
        !runtime
            .request_focus(&"secondary".into())
            .expect("hidden focus")
    );

    runtime.resize(Size::new(12, 3));
    runtime.render_if_dirty().expect("expanded render");

    assert_eq!(builds.get(), 1);
    assert!(
        runtime
            .request_focus(&"secondary".into())
            .expect("visible focus")
    );
}

#[test]
fn divider_uses_one_cell_between_rendered_panes() {
    struct RenderApp;

    impl App for RenderApp {
        type Message = ();

        fn update(&mut self, (): ()) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
            Node::split_pane(
                Node::text("A"),
                Node::text("B"),
                SplitPaneOptions::default(),
            )
        }
    }

    let mut runtime = Runtime::with_clock(
        RenderApp,
        nagi_tui::RuntimeConfig::new(Size::new(5, 1)),
        VirtualClock::new(),
    )
    .expect("runtime");
    let frame = runtime.render_if_dirty().expect("render").expect("frame");

    assert_eq!(frame.surface().cell(0, 0).expect("primary").content(), "A");
    assert_eq!(frame.surface().cell(2, 0).expect("divider").content(), "│");
    assert_eq!(
        frame.surface().cell(3, 0).expect("secondary").content(),
        "B"
    );
}
