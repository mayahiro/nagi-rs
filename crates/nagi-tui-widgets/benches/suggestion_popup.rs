//! Bounded SuggestionPopup view-construction benchmark

use std::hint::black_box;
use std::time::{Duration, Instant};

use nagi_tui::Node;
use nagi_tui_widgets::{SuggestionId, SuggestionItems, SuggestionPopup, SuggestionRowContext};

const SAMPLE_COUNT: usize = 12;
const CALLS_PER_SAMPLE: usize = 10_000;
const VISIBLE_ROWS: usize = 8;

#[derive(Clone, Copy)]
struct Sample {
    elapsed: Duration,
}

fn sample(items: &SuggestionItems, selected: &SuggestionId) -> Vec<Sample> {
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        for _ in 0..CALLS_PER_SAMPLE {
            let node = SuggestionPopup::new(
                "popup",
                Node::text("base").with_id("anchor"),
                "anchor",
                "anchor",
                items.clone(),
                Some(selected.clone()),
                |context: SuggestionRowContext| Node::text(context.id().as_str()),
                |_: SuggestionId| (),
                |_: SuggestionId| (),
                || (),
            )
            .visible_rows(VISIBLE_ROWS)
            .into_node();
            black_box(node);
        }
        samples.push(Sample {
            elapsed: started.elapsed(),
        });
    }
    samples
}

fn report(count: usize, samples: Vec<Sample>) {
    let mut elapsed: Vec<_> = samples.iter().map(|sample| sample.elapsed).collect();
    elapsed.sort_unstable();
    let middle = samples.len() / 2;
    println!(
        "suggestion-popup candidates={count} visible_rows={VISIBLE_ROWS} calls_per_sample={CALLS_PER_SAMPLE} median_ns_per_call={}",
        elapsed[middle].as_nanos() / CALLS_PER_SAMPLE as u128,
    );
}

fn main() {
    for count in [VISIBLE_ROWS, 100_000] {
        let items = SuggestionItems::new(
            (0..count).map(|index| SuggestionId::new(format!("candidate-{index}"))),
        )
        .expect("generated candidates are unique");
        let selected = items
            .get(count / 2)
            .expect("non-empty benchmark candidates")
            .clone();
        report(count, sample(&items, &selected));
    }
}
