//! Shared Clipboard Effect conformance fixtures

mod support;

use nagi_tui::{App, Effect, Node, Runtime, RuntimeConfig, Size, ViewContext, VirtualClock};

enum ClipboardMessage {
    Single(String),
    Batch(Vec<String>),
    Sequence(Vec<String>),
}

struct ClipboardApp;

impl App for ClipboardApp {
    type Message = ClipboardMessage;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        let effect = match message {
            ClipboardMessage::Single(value) => Effect::set_clipboard(value),
            ClipboardMessage::Batch(values) => {
                Effect::batch(values.into_iter().map(Effect::set_clipboard))
            }
            ClipboardMessage::Sequence(values) => {
                Effect::sequence(values.into_iter().map(Effect::set_clipboard))
            }
        };
        effect.without_redraw()
    }

    fn view(&self, _context: ViewContext) -> Node<Self::Message> {
        Node::text("clipboard")
    }
}

#[test]
fn clipboard_effect_matches_shared_fixtures() {
    let Some(records) = support::load(
        "effects/clipboard.txt",
        "effect-clipboard",
        &["composition", "values", "expected"],
    ) else {
        return;
    };

    for record in records {
        let values: Vec<String> = record
            .text("values")
            .split(',')
            .map(str::to_owned)
            .collect();
        let mut runtime = Runtime::with_clock(
            ClipboardApp,
            RuntimeConfig::new(Size::new(12, 1)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap().unwrap();

        match record.field("composition") {
            "single" => runtime
                .enqueue(ClipboardMessage::Single(values[0].clone()))
                .unwrap(),
            "updates" => {
                for value in values {
                    runtime.enqueue(ClipboardMessage::Single(value)).unwrap();
                }
            }
            "batch" => runtime.enqueue(ClipboardMessage::Batch(values)).unwrap(),
            "sequence" => runtime.enqueue(ClipboardMessage::Sequence(values)).unwrap(),
            value => panic!("case {} has invalid composition {value}", record.id),
        }
        runtime.process_pending().unwrap();

        assert!(
            runtime.render_if_dirty().unwrap().is_none(),
            "case {} dirtied the view",
            record.id
        );
        assert_eq!(
            runtime
                .pending_clipboard_request()
                .map(nagi_tui::ClipboardRequest::text),
            Some(record.text("expected").as_str()),
            "case {} pending request",
            record.id
        );
        assert_eq!(
            runtime
                .take_clipboard_request()
                .map(|request| request.into_text()),
            Some(record.text("expected")),
            "case {} take",
            record.id
        );
        assert!(
            runtime.take_clipboard_request().is_none(),
            "case {} second take",
            record.id
        );
    }
}
