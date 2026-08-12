//! Runtime width profile propagation tests

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nagi_text::{WidthProfile, text_width};
use nagi_tui::{App, Effect, Node, Runtime, RuntimeConfig, Size, Style, ViewContext, VirtualClock};

struct WidthProfileApp {
    observed: Arc<AtomicUsize>,
}

impl App for WidthProfileApp {
    type Message = ();

    fn update(&mut self, (): Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, context: ViewContext) -> Node<Self::Message> {
        self.observed
            .store(text_width("·", context.width_profile), Ordering::Relaxed);
        Node::border(Node::text("·X"), Style::default())
    }
}

#[test]
fn runtime_profile_controls_context_layout_and_fallback_glyphs() {
    let observed = Arc::new(AtomicUsize::new(0));
    let mut config = RuntimeConfig::new(Size::new(4, 3));
    config.width_profile = WidthProfile::CJK;
    let mut runtime = Runtime::with_clock(
        WidthProfileApp {
            observed: Arc::clone(&observed),
        },
        config,
        VirtualClock::new(),
    )
    .unwrap();

    let frame = runtime.render_if_dirty().unwrap().expect("initial frame");
    assert_eq!(observed.load(Ordering::Relaxed), 2);
    assert_eq!(frame.surface().cell(0, 0).unwrap().content(), "+");
    assert_eq!(frame.surface().cell(1, 1).unwrap().content(), "·");
    assert!(frame.surface().cell(2, 1).unwrap().is_continuation());
}
