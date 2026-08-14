//! Warmed geometry-aware pointer dispatch benchmark over a long paragraph

use std::hint::black_box;
use std::time::Instant;

use nagi_tui::{
    App, Effect, Event, EventResult, MouseButton, MouseEvent, MouseKind, Node, ParagraphOptions,
    PointerEventContext, Runtime, Size, Style, TextSpan, WrapMode,
};

const DOCUMENT_BYTES: usize = 100_000;
const CALLS_PER_SAMPLE: usize = 10_000;
const SAMPLE_COUNT: usize = 12;

struct PointerApp {
    document: String,
}

impl App for PointerApp {
    type Message = ();

    fn update(&mut self, (): ()) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::paragraph(
            [TextSpan::new(self.document.clone(), Style::default())],
            ParagraphOptions {
                wrap: WrapMode::None,
                ..ParagraphOptions::default()
            },
        )
        .on_pointer_event("text", |context: &PointerEventContext| {
            if context.event().kind == MouseKind::Press {
                EventResult::consumed().capture_pointer("text")
            } else {
                EventResult::consumed()
            }
        })
    }
}

fn pointer_event(kind: MouseKind) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        button: MouseButton::Left,
        x: u32::try_from(DOCUMENT_BYTES - 1).unwrap(),
        y: 0,
        modifiers: nagi_tui::Modifiers::NONE,
    })
}

fn main() {
    let mut runtime = Runtime::new(
        PointerApp {
            document: "x".repeat(DOCUMENT_BYTES),
        },
        Size::new(80, 1),
    )
    .expect("runtime");
    runtime.render_if_dirty().expect("warm paragraph layout");
    runtime
        .dispatch_event(&pointer_event(MouseKind::Press))
        .expect("capture pointer");
    let movement = pointer_event(MouseKind::Move);
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        for _ in 0..CALLS_PER_SAMPLE {
            black_box(
                runtime
                    .dispatch_event(black_box(&movement))
                    .expect("pointer dispatch"),
            );
        }
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    println!(
        "pointer-text-hit document_bytes={DOCUMENT_BYTES} calls_per_sample={CALLS_PER_SAMPLE} median_ns_per_call={}",
        median.as_nanos() / CALLS_PER_SAMPLE as u128
    );
}
