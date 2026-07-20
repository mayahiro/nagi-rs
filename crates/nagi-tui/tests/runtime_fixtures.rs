//! Shared runtime vertical-slice conformance fixtures

mod support;

use nagi_tui::{
    App, Capabilities, Effect, Event, Node, Runtime, Size, Style, Subscription, VirtualClock,
    encode,
};

#[derive(Default)]
struct Echo {
    text: String,
}

impl App for Echo {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.text.push_str(&message);
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::border(Node::text(&self.text), Style::default())
    }
}

#[test]
fn input_update_surface_and_vt_output_match_shared_fixtures() {
    let Some(records) = support::load(
        "runtime/roundtrip.txt",
        "runtime-roundtrip",
        &["width", "height", "input", "expected"],
    ) else {
        return;
    };

    for record in records {
        let width = number(record.field("width"));
        let height = number(record.field("height"));
        let input = record.decoded("input");
        let expected = record.text("expected");
        let mut runtime = Runtime::with_clock(
            Echo::default(),
            nagi_tui::RuntimeConfig::new(Size::new(width, height)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap().unwrap();
        let mut decoder = nagi_tui::TimedInputDecoder::new(
            VirtualClock::new(),
            std::time::Duration::from_millis(25),
        );

        for event in decoder.feed(&input) {
            if let Event::Text(text) = event {
                runtime.enqueue(text).unwrap();
            }
        }
        let frame = runtime.step().unwrap().unwrap();

        assert_eq!(frame.surface().snapshot(), expected, "case {}", record.id);
        let output = encode(frame.operations(), Capabilities::BASELINE);
        assert!(
            output.windows(input.len()).any(|window| window == input),
            "case {} did not reach VT output",
            record.id
        );
    }
}

#[derive(Default)]
struct TextInputApp {
    value: String,
}

impl App for TextInputApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.value = message;
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::text_input("input", &self.value, |value| value)
    }
}

#[test]
fn text_input_cursor_snapshot_matches_shared_fixture() {
    let Some(records) = support::load(
        "interaction/text-input-runtime.txt",
        "text-input-runtime",
        &["width", "height", "input", "expected"],
    ) else {
        return;
    };
    for record in records {
        let mut runtime = Runtime::with_clock(
            TextInputApp::default(),
            nagi_tui::RuntimeConfig::new(Size::new(
                number(record.field("width")),
                number(record.field("height")),
            )),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        runtime
            .request_focus(&nagi_tui::NodeId::from("input"))
            .unwrap();
        let mut decoder = nagi_tui::TimedInputDecoder::new(
            VirtualClock::new(),
            std::time::Duration::from_millis(25),
        );
        for event in decoder.feed(&record.decoded("input")) {
            runtime.dispatch_event(&event).unwrap();
        }

        let frame = runtime.step().unwrap().unwrap();

        assert_eq!(
            frame.surface().snapshot(),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

fn number(value: &str) -> u32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
}
