//! Standard widget width profile propagation tests

use nagi_text::WidthProfile;
use nagi_tui::{
    App, Effect, Node, NodeId, Runtime, RuntimeConfig, Size, ViewContext, VirtualClock,
};
use nagi_tui_widgets::{TextArea, TextAreaState};

struct WidthProfileTextAreaApp;

impl App for WidthProfileTextAreaApp {
    type Message = ();

    fn update(&mut self, (): Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, context: ViewContext) -> Node<Self::Message> {
        TextArea::new("input", TextAreaState::new("·X", "·".len()), |_| ())
            .width_profile(context.width_profile)
            .soft_wrap(context.size.width)
            .into_node()
    }
}

#[test]
fn text_area_uses_runtime_cjk_profile_for_wrap_and_cursor() {
    let mut config = RuntimeConfig::new(Size::new(2, 2));
    config.width_profile = WidthProfile::CJK;
    let mut runtime =
        Runtime::with_clock(WidthProfileTextAreaApp, config, VirtualClock::new()).expect("runtime");
    runtime.render_if_dirty().unwrap();
    assert!(runtime.request_focus(&NodeId::from("input")).unwrap());

    let frame = runtime.render_if_dirty().unwrap().expect("focused frame");
    let cursor = frame.surface().cursor().expect("visible cursor");
    assert_eq!((cursor.x, cursor.y), (0, 1));
}
