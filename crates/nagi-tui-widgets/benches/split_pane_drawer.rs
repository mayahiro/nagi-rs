//! SplitPane and Drawer controlled view-construction benchmark

use std::hint::black_box;
use std::time::{Duration, Instant};

use nagi_tui::{Length, Node};
use nagi_tui_widgets::{Drawer, DrawerSide, SplitPane, SplitPaneState};

const SAMPLE_COUNT: usize = 12;
const CALLS_PER_SAMPLE: usize = 10_000;

fn samples(mut construct: impl FnMut() -> Node<()>) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        for _ in 0..CALLS_PER_SAMPLE {
            black_box(construct());
        }
        samples.push(started.elapsed());
    }
    samples
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn report(path: &str, samples: Vec<Duration>) {
    println!(
        "split-pane-drawer path={path} calls_per_sample={CALLS_PER_SAMPLE} median_ns_per_call={}",
        median(samples).as_nanos() / CALLS_PER_SAMPLE as u128,
    );
}

fn main() {
    report(
        "split-pane",
        samples(|| {
            SplitPane::new(
                "split",
                Node::text("primary").with_id("primary"),
                Node::text("secondary").with_id("secondary"),
                SplitPaneState::new(2_500),
            )
            .minimums(18, 30)
            .focus_targets("primary", "secondary")
            .on_resize(|_| ())
            .into_node()
        }),
    );
    report(
        "drawer-closed",
        samples(|| {
            Drawer::new("drawer", Node::text("base"), false)
                .side(DrawerSide::Bottom)
                .size(Length::Fixed(5))
                .body(|| Node::text("details"))
                .on_dismiss(|| ())
                .into_node()
        }),
    );
    report(
        "drawer-open",
        samples(|| {
            Drawer::new("drawer", Node::text("base"), true)
                .side(DrawerSide::Bottom)
                .size(Length::Fixed(5))
                .body(|| Node::text("details"))
                .on_dismiss(|| ())
                .into_node()
        }),
    );
}
