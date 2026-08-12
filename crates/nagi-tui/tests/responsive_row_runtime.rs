//! Core ResponsiveRow semantic omission and Overlay measurement tests

use std::cell::Cell;
use std::rc::Rc;

use nagi_tui::{
    App, Effect, Length, Node, ResponsiveRowItem, ResponsiveRowOptions, Runtime, Size, ViewContext,
    VirtualClock, VirtualFragment,
};

struct ResponsiveApp {
    builds: Rc<Cell<usize>>,
}

impl App for ResponsiveApp {
    type Message = ();

    fn update(&mut self, (): ()) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: ViewContext) -> Node<Self::Message> {
        let builds = Rc::clone(&self.builds);
        let low = Node::virtual_scroll_viewport("low", Size::new(10, 1), move |_| {
            builds.set(builds.get() + 1);
            VirtualFragment::new(Default::default(), Node::text("low"))
        });
        Node::responsive_row(
            [
                ResponsiveRowItem::new(Node::text("high!").focusable("high")).priority(10),
                ResponsiveRowItem::new(low),
            ],
            ResponsiveRowOptions { gap: 1, height: 0 },
        )
    }
}

#[test]
fn hidden_item_is_omitted_from_semantics_and_lazy_preparation() {
    let builds = Rc::new(Cell::new(0));
    let mut runtime = Runtime::with_clock(
        ResponsiveApp {
            builds: Rc::clone(&builds),
        },
        nagi_tui::RuntimeConfig::new(Size::new(5, 1)),
        VirtualClock::new(),
    )
    .expect("runtime");
    runtime.render_if_dirty().expect("narrow render");

    assert_eq!(builds.get(), 0);
    assert!(!runtime.request_focus(&"low".into()).expect("hidden focus"));

    runtime.resize(Size::new(16, 1));
    runtime.render_if_dirty().expect("wide render");

    assert_eq!(builds.get(), 1);
    assert!(runtime.request_focus(&"low".into()).expect("visible focus"));
}

struct OverlayApp;

impl App for OverlayApp {
    type Message = ();

    fn update(&mut self, (): ()) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: ViewContext) -> Node<Self::Message> {
        Node::column([
            Node::overlay(Node::text("A"), Node::text("layer\nlayer\nlayer")),
            Node::text("B").with_length(Length::Auto),
        ])
    }
}

#[test]
fn overlay_uses_only_base_intrinsic_measurement() {
    let mut runtime = Runtime::with_clock(
        OverlayApp,
        nagi_tui::RuntimeConfig::new(Size::new(8, 4)),
        VirtualClock::new(),
    )
    .expect("runtime");
    let frame = runtime.render_if_dirty().expect("render").expect("frame");

    assert_eq!(frame.surface().cell(0, 0).expect("layer").content(), "l");
    assert_eq!(
        frame.surface().cell(0, 1).expect("base sibling").content(),
        "B"
    );
}
