//! Standalone derived Help document benchmark

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nagi_cli::Command;
use nagi_cli_document::{ManRenderer, MarkdownRenderer};

const SAMPLES: usize = 12;

struct TrackingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

// SAFETY: every operation delegates to System with the original pointer and
// layout contract and only records successful allocation operations
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller provides the GlobalAlloc layout contract
        let allocated = unsafe { System.alloc(layout) };
        if !allocated.is_null() {
            record_allocation(layout.size());
        }
        allocated
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller provides the GlobalAlloc layout contract
        let allocated = unsafe { System.alloc_zeroed(layout) };
        if !allocated.is_null() {
            record_allocation(layout.size());
        }
        allocated
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: the pointer and layout came from this allocator
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the pointer and layout came from this allocator
        let allocated = unsafe { System.realloc(pointer, layout, new_size) };
        if !allocated.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(new_size, Ordering::Relaxed);
            if new_size >= layout.size() {
                let increase = new_size - layout.size();
                let live = LIVE_BYTES.fetch_add(increase, Ordering::Relaxed) + increase;
                update_peak(live);
            } else {
                LIVE_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        allocated
    }
}

fn record_allocation(bytes: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    update_peak(live);
}

fn update_peak(live: usize) {
    let mut peak = PEAK_LIVE_BYTES.load(Ordering::Relaxed);
    while live > peak {
        match PEAK_LIVE_BYTES.compare_exchange_weak(
            peak,
            live,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(current) => peak = current,
        }
    }
}

#[derive(Clone, Copy)]
struct Sample {
    elapsed: Duration,
    allocations: usize,
    allocated_bytes: usize,
    peak_live_bytes: usize,
    retained_bytes: i64,
    output_bytes: usize,
}

fn reset_metrics() -> usize {
    let baseline = LIVE_BYTES.load(Ordering::Relaxed);
    ALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    PEAK_LIVE_BYTES.store(baseline, Ordering::Relaxed);
    baseline
}

fn signed_difference(left: usize, right: usize) -> i64 {
    if left >= right {
        i64::try_from(left - right).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(right - left).unwrap_or(i64::MAX)
    }
}

fn main() {
    for command_count in [1, 1_000] {
        let command = graph(command_count);
        report(
            &format!("cli-document-markdown-{command_count}"),
            sample(&command, Format::Markdown),
        );
        report(
            &format!("cli-document-man-{command_count}"),
            sample(&command, Format::Man),
        );
    }
}

#[derive(Clone, Copy)]
enum Format {
    Markdown,
    Man,
}

fn graph(command_count: usize) -> Command {
    let mut root = Command::new("qed").about("Coding workspace");
    for index in 1..command_count {
        root = root.subcommand(
            Command::new(format!("run-{index}"))
                .about("Run one deterministic task")
                .note("Generated benchmark command"),
        );
    }
    root
}

fn sample(command: &Command, format: Format) -> Vec<Sample> {
    let mut warmup_bytes = 0_usize;
    command
        .visit_help_documents(|document| {
            warmup_bytes = warmup_bytes.saturating_add(match format {
                Format::Markdown => MarkdownRenderer.render(document).len(),
                Format::Man => ManRenderer.render(document).len(),
            });
            true
        })
        .expect("benchmark graph must be valid");
    assert!(warmup_bytes > 0);

    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let baseline = reset_metrics();
        let mut output_bytes = 0_usize;
        let started = Instant::now();
        black_box(command)
            .visit_help_documents(|document| {
                output_bytes = output_bytes.saturating_add(match format {
                    Format::Markdown => MarkdownRenderer.render(black_box(document)).len(),
                    Format::Man => ManRenderer.render(black_box(document)).len(),
                });
                true
            })
            .expect("benchmark traversal must succeed");
        let elapsed = started.elapsed();
        let live = LIVE_BYTES.load(Ordering::Relaxed);
        samples.push(Sample {
            elapsed,
            allocations: ALLOCATIONS.load(Ordering::Relaxed),
            allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
            peak_live_bytes: PEAK_LIVE_BYTES
                .load(Ordering::Relaxed)
                .saturating_sub(baseline),
            retained_bytes: signed_difference(live, baseline),
            output_bytes,
        });
    }
    samples
}

fn report(label: &str, samples: Vec<Sample>) {
    let mut elapsed: Vec<_> = samples.iter().map(|sample| sample.elapsed).collect();
    let mut allocations: Vec<_> = samples.iter().map(|sample| sample.allocations).collect();
    let mut allocated_bytes: Vec<_> = samples
        .iter()
        .map(|sample| sample.allocated_bytes)
        .collect();
    let mut peak_live_bytes: Vec<_> = samples
        .iter()
        .map(|sample| sample.peak_live_bytes)
        .collect();
    let mut retained_bytes: Vec<_> = samples.iter().map(|sample| sample.retained_bytes).collect();
    elapsed.sort_unstable();
    allocations.sort_unstable();
    allocated_bytes.sort_unstable();
    peak_live_bytes.sort_unstable();
    retained_bytes.sort_unstable();
    let middle = samples.len() / 2;
    println!(
        "{label} samples={SAMPLES} median_ns={} median_allocs={} median_allocated_bytes={} median_peak_live_bytes={} median_retained_bytes={} output_bytes={}",
        elapsed[middle].as_nanos(),
        allocations[middle],
        allocated_bytes[middle],
        peak_live_bytes[middle],
        retained_bytes[middle],
        samples[middle].output_bytes
    );
}
