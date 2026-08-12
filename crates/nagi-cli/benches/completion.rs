//! Standalone handler-free completion benchmark

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nagi_cli::{CancellationToken, Command, CompletionEngine, CompletionInput, OptionSpec};

const ITERATIONS: usize = 12;
const REQUESTS_PER_ITERATION: usize = 1_000;
const UNRELATED_BRANCHES: usize = 100;
const OPTIONS_PER_BRANCH: usize = 8;

struct TrackingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

// SAFETY: every operation delegates to System with the original layout and
// only updates independent atomic counters after successful allocation
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

fn record_allocation(size: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(size, Ordering::Relaxed) + size;
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
}

fn reset_metrics() -> usize {
    let baseline = LIVE_BYTES.load(Ordering::Relaxed);
    ALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    PEAK_LIVE_BYTES.store(baseline, Ordering::Relaxed);
    baseline
}

fn read_metrics(elapsed: Duration, baseline: usize) -> Sample {
    let live = LIVE_BYTES.load(Ordering::Relaxed);
    Sample {
        elapsed,
        allocations: ALLOCATIONS.load(Ordering::Relaxed) / REQUESTS_PER_ITERATION,
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed) / REQUESTS_PER_ITERATION,
        peak_live_bytes: PEAK_LIVE_BYTES
            .load(Ordering::Relaxed)
            .saturating_sub(baseline),
        retained_bytes: signed_difference(live, baseline),
    }
}

fn signed_difference(left: usize, right: usize) -> i64 {
    if left >= right {
        i64::try_from(left - right).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(right - left).unwrap_or(i64::MAX)
    }
}

fn benchmark_command(unrelated_branches: usize) -> Command {
    let mut root = Command::new("root")
        .option(OptionSpec::count("verbose").long("verbose").inherited())
        .subcommand(Command::new("run"));
    for branch_index in 0..unrelated_branches {
        let mut branch = Command::new(format!("branch-{branch_index}"));
        for option_index in 0..OPTIONS_PER_BRANCH {
            branch = branch.option(
                OptionSpec::flag(format!("option-{option_index}"))
                    .long(format!("option-{option_index}"))
                    .inherited(),
            );
        }
        root = root.subcommand(branch);
    }
    root
}

fn sample(unrelated_branches: usize) -> Vec<Sample> {
    let engine = CompletionEngine::new(&benchmark_command(unrelated_branches)).unwrap();
    let cancellation = CancellationToken::new();
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let baseline = reset_metrics();
        let started = Instant::now();
        for _ in 0..REQUESTS_PER_ITERATION {
            let result = engine
                .complete(&cancellation, CompletionInput::new(["run"], "--v"))
                .unwrap();
            assert_eq!(result.candidates().len(), 1);
            assert_eq!(result.candidates()[0].value(), "--verbose");
            black_box(&result);
            drop(result);
        }
        samples.push(read_metrics(started.elapsed(), baseline));
    }
    samples
}

fn report(label: &str, samples: Vec<Sample>) {
    let mut elapsed: Vec<_> = samples
        .iter()
        .map(|sample| sample.elapsed / REQUESTS_PER_ITERATION as u32)
        .collect();
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
        "{label} iterations={ITERATIONS} requests_per_iteration={REQUESTS_PER_ITERATION} min_ns={} median_ns={} median_allocs={} median_allocated_bytes={} median_peak_live_bytes={} median_retained_bytes={}",
        elapsed[0].as_nanos(),
        elapsed[middle].as_nanos(),
        allocations[middle],
        allocated_bytes[middle],
        peak_live_bytes[middle],
        retained_bytes[middle],
    );
}

fn main() {
    report("cli-completion-selected-path", sample(0));
    report(
        "cli-completion-100-unrelated-branches",
        sample(UNRELATED_BRANCHES),
    );
}
