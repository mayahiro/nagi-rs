//! Code source, terminal projection, memo, and bounded view benchmark

use std::hint::black_box;
use std::time::{Duration, Instant};

use nagi_tui_widgets::{
    CodeDocument, CodeLayout, CodeLayoutCache, CodeLayoutOptions, CodeLine, CodeView, CodeViewState,
};

const SAMPLE_COUNT: usize = 12;
const VIEW_CALLS_PER_SAMPLE: usize = 10_000;
const CACHE_CALLS_PER_SAMPLE: usize = 100_000;
const VISIBLE_ROWS: usize = 8;
const LARGE_LINES: usize = 100_000;
const LONG_LINE_BYTES: usize = 1024 * 1024;

fn code_lines(count: usize) -> Vec<CodeLine> {
    (0..count)
        .map(|index| {
            CodeLine::plain(format!("{index:06} let value = \"Nagi\";"))
                .expect("generated logical line")
        })
        .collect()
}

fn document_samples(lines: &[CodeLine]) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        black_box(
            CodeDocument::new(lines.iter().cloned(), true)
                .expect("benchmark document within limits"),
        );
        samples.push(started.elapsed());
    }
    samples
}

fn layout_samples(document: &CodeDocument, options: CodeLayoutOptions) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        black_box(
            CodeLayout::new(document.clone(), options).expect("benchmark layout within limits"),
        );
        samples.push(started.elapsed());
    }
    samples
}

fn cache_samples(document: &CodeDocument, options: CodeLayoutOptions) -> Vec<Duration> {
    let cache = CodeLayoutCache::default();
    cache
        .resolve(1, document, options)
        .expect("warm benchmark layout cache");
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        for _ in 0..CACHE_CALLS_PER_SAMPLE {
            black_box(
                cache
                    .resolve(1, document, options)
                    .expect("cached benchmark layout"),
            );
        }
        samples.push(started.elapsed());
    }
    samples
}

fn view_samples(layout: &CodeLayout, state: CodeViewState, visible_rows: usize) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        for _ in 0..VIEW_CALLS_PER_SAMPLE {
            black_box(
                CodeView::new("code", layout.clone(), state, |_| ())
                    .viewport(visible_rows)
                    .on_copy(|_| ())
                    .into_node(),
            );
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
    let large_lines = code_lines(LARGE_LINES);
    let document_elapsed = median(document_samples(&large_lines));
    println!(
        "code-document lines={LARGE_LINES} median_ns_per_build={}",
        document_elapsed.as_nanos()
    );

    let large_document =
        CodeDocument::new(large_lines, true).expect("large benchmark document within limits");
    let options = CodeLayoutOptions::default().with_viewport_width(80);
    let layout_elapsed = median(layout_samples(&large_document, options));
    println!(
        "code-layout lines={LARGE_LINES} median_ns_per_build={}",
        layout_elapsed.as_nanos()
    );

    let cache_elapsed = median(cache_samples(&large_document, options));
    println!(
        "code-layout-cache lines={LARGE_LINES} calls_per_sample={CACHE_CALLS_PER_SAMPLE} median_ns_per_call={}",
        cache_elapsed.as_nanos() / CACHE_CALLS_PER_SAMPLE as u128
    );

    for count in [VISIBLE_ROWS, LARGE_LINES] {
        let document = if count == LARGE_LINES {
            large_document.clone()
        } else {
            CodeDocument::new(code_lines(count), true).expect("small benchmark document")
        };
        let layout = CodeLayout::new(document, options).expect("benchmark view layout");
        let elapsed = median(view_samples(
            &layout,
            CodeViewState::new(count / 2),
            VISIBLE_ROWS,
        ));
        println!(
            "code-view lines={count} visible_rows={VISIBLE_ROWS} calls_per_sample={VIEW_CALLS_PER_SAMPLE} median_ns_per_call={}",
            elapsed.as_nanos() / VIEW_CALLS_PER_SAMPLE as u128
        );
    }

    let long_text = format!("{}END", "a".repeat(LONG_LINE_BYTES));
    let long_document = CodeDocument::new(
        [CodeLine::plain(long_text).expect("generated long line")],
        false,
    )
    .expect("long benchmark document");
    let long_layout = CodeLayout::new(
        long_document,
        CodeLayoutOptions::default()
            .with_viewport_width(80)
            .with_line_numbers(false),
    )
    .expect("long benchmark layout");
    let long_state = CodeViewState::new(0).with_horizontal_offset(LONG_LINE_BYTES as u32);
    let elapsed = median(view_samples(&long_layout, long_state, 1));
    println!(
        "code-view-long-line bytes={} visible_rows=1 calls_per_sample={VIEW_CALLS_PER_SAMPLE} median_ns_per_call={}",
        LONG_LINE_BYTES + 3,
        elapsed.as_nanos() / VIEW_CALLS_PER_SAMPLE as u128
    );
}
