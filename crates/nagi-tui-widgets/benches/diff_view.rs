//! Diff source, terminal projection, memo, copy, and bounded view benchmark

use std::hint::black_box;
use std::time::{Duration, Instant};

use nagi_tui_widgets::{
    CodeLine, DiffDocument, DiffLayout, DiffLayoutCache, DiffLayoutOptions, DiffLine, DiffView,
    DiffViewState,
};

const SAMPLE_COUNT: usize = 12;
const VIEW_CALLS_PER_SAMPLE: usize = 10_000;
const CACHE_CALLS_PER_SAMPLE: usize = 100_000;
const VISIBLE_ROWS: usize = 8;
const LARGE_LINES: usize = 100_000;

fn diff_lines(count: usize) -> Vec<DiffLine> {
    (0..count)
        .map(|index| {
            let number = u64::try_from(index).unwrap_or(u64::MAX) + 1;
            let content = CodeLine::plain(format!("{index:06} let value = \"Nagi\";"))
                .expect("generated logical line");
            match index % 3 {
                0 => DiffLine::context(number, number, content),
                1 => DiffLine::deletion(number, content),
                _ => DiffLine::addition(number, content),
            }
            .expect("generated line number")
        })
        .collect()
}

fn document_samples(lines: &[DiffLine]) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        black_box(
            DiffDocument::new(lines.iter().cloned(), true)
                .expect("benchmark document within limits"),
        );
        samples.push(started.elapsed());
    }
    samples
}

fn layout_samples(document: &DiffDocument, options: DiffLayoutOptions) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        black_box(
            DiffLayout::new(document.clone(), options).expect("benchmark layout within limits"),
        );
        samples.push(started.elapsed());
    }
    samples
}

fn cache_samples(document: &DiffDocument, options: DiffLayoutOptions) -> Vec<Duration> {
    let cache = DiffLayoutCache::default();
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

fn copy_samples(document: &DiffDocument) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        black_box(
            document
                .copy_text_for_lines(0..document.line_count())
                .expect("benchmark copy"),
        );
        samples.push(started.elapsed());
    }
    samples
}

fn view_samples(layout: &DiffLayout, state: DiffViewState) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        for _ in 0..VIEW_CALLS_PER_SAMPLE {
            black_box(
                DiffView::new("diff", layout.clone(), state, |_| ())
                    .viewport(VISIBLE_ROWS)
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
    let large_lines = diff_lines(LARGE_LINES);
    let document_elapsed = median(document_samples(&large_lines));
    println!(
        "diff-document lines={LARGE_LINES} median_ns_per_build={}",
        document_elapsed.as_nanos()
    );

    let large_document =
        DiffDocument::new(large_lines, true).expect("large benchmark document within limits");
    let options = DiffLayoutOptions::default().with_viewport_width(80);
    let layout_elapsed = median(layout_samples(&large_document, options));
    println!(
        "diff-layout lines={LARGE_LINES} median_ns_per_build={}",
        layout_elapsed.as_nanos()
    );

    let cache_elapsed = median(cache_samples(&large_document, options));
    println!(
        "diff-layout-cache lines={LARGE_LINES} calls_per_sample={CACHE_CALLS_PER_SAMPLE} median_ns_per_call={}",
        cache_elapsed.as_nanos() / CACHE_CALLS_PER_SAMPLE as u128
    );

    let copy_elapsed = median(copy_samples(&large_document));
    println!(
        "diff-copy lines={LARGE_LINES} bytes={} median_ns_per_copy={}",
        large_document.text_bytes(),
        copy_elapsed.as_nanos()
    );

    for count in [VISIBLE_ROWS, LARGE_LINES] {
        let document = if count == LARGE_LINES {
            large_document.clone()
        } else {
            DiffDocument::new(diff_lines(count), true).expect("small benchmark document")
        };
        let layout = DiffLayout::new(document, options).expect("benchmark view layout");
        let elapsed = median(view_samples(&layout, DiffViewState::new(count / 2)));
        println!(
            "diff-view lines={count} visible_rows={VISIBLE_ROWS} calls_per_sample={VIEW_CALLS_PER_SAMPLE} median_ns_per_call={}",
            elapsed.as_nanos() / VIEW_CALLS_PER_SAMPLE as u128
        );
    }
}
