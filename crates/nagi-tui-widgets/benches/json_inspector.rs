//! JSON document indexing and bounded inspector construction benchmark

use std::hint::black_box;
use std::time::{Duration, Instant};

use nagi_tui_widgets::{
    JsonDocument, JsonInspector, JsonInspectorState, JsonNumber, JsonPointer, JsonValue,
};

const SAMPLE_COUNT: usize = 12;
const VIEW_CALLS_PER_SAMPLE: usize = 10_000;
const VISIBLE_ROWS: usize = 8;
const LARGE_VALUES: usize = 99_999;

fn array_value(count: usize) -> JsonValue {
    JsonValue::array((0..count).map(|index| {
        JsonValue::number(JsonNumber::new(index.to_string()).expect("generated JSON number"))
    }))
}

fn document_samples(root: &JsonValue) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        black_box(JsonDocument::new(root.clone()).expect("benchmark document within limits"));
        samples.push(started.elapsed());
    }
    samples
}

fn inspector_samples(document: &JsonDocument, state: &JsonInspectorState) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        for _ in 0..VIEW_CALLS_PER_SAMPLE {
            let node = JsonInspector::new("inspector", document.clone(), state.clone(), |_| ())
                .viewport(VISIBLE_ROWS)
                .on_copy(|_| ())
                .into_node();
            black_box(node);
        }
        samples.push(started.elapsed());
    }
    samples
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn main() {
    let large_root = array_value(LARGE_VALUES);
    let document_elapsed = median(document_samples(&large_root));
    println!(
        "json-document values={} median_ns_per_build={}",
        LARGE_VALUES + 1,
        document_elapsed.as_nanos(),
    );

    for count in [VISIBLE_ROWS, LARGE_VALUES] {
        let document = JsonDocument::new(array_value(count)).expect("benchmark document");
        let selected = JsonPointer::new(format!("/{}", count / 2)).expect("generated pointer");
        let state = JsonInspectorState::new(selected, [JsonPointer::root()]);
        let elapsed = median(inspector_samples(&document, &state));
        println!(
            "json-inspector values={} visible_rows={} calls_per_sample={} median_ns_per_call={}",
            count + 1,
            VISIBLE_ROWS,
            VIEW_CALLS_PER_SAMPLE,
            elapsed.as_nanos() / VIEW_CALLS_PER_SAMPLE as u128,
        );
    }
}
