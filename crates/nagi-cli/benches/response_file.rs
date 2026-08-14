//! Standalone Response File expansion benchmark

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::OsString;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nagi_cli::{Diagnostic, ResponseFileOptions, ResponseFileReadRequest, expand_response_files};

const ITERATIONS: usize = 12;
const LARGE_ARGUMENT_COUNT: usize = 1_000;

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
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
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

fn source(tokens: usize) -> Vec<u8> {
    let mut output = Vec::new();
    for index in 0..tokens {
        if index != 0 {
            output.push(b' ');
        }
        output.extend_from_slice(format!("value-{index}").as_bytes());
    }
    output
}

fn sample_source(source: &[u8], expected_arguments: usize) -> Vec<Sample> {
    let mut samples = Vec::with_capacity(ITERATIONS);
    let template = vec![OsString::from("@args.txt")];
    for _ in 0..ITERATIONS {
        let mut reader = |request: &ResponseFileReadRequest<'_>| {
            Ok::<_, Diagnostic>(source[..source.len().min(request.read_limit())].to_vec())
        };
        let mut standard_input = std::io::empty();
        let baseline = reset_metrics();
        let started = Instant::now();
        let arguments = template.clone();
        let expanded = expand_response_files(
            arguments,
            "/work",
            &ResponseFileOptions::default(),
            &mut reader,
            &mut standard_input,
        )
        .unwrap();
        assert_eq!(expanded.len(), expected_arguments);
        black_box(&expanded);
        drop(expanded);
        samples.push(read_metrics(started.elapsed(), baseline));
    }
    samples
}

fn sample_literals(arguments: &[OsString]) -> Vec<Sample> {
    let mut samples = Vec::with_capacity(ITERATIONS);
    let mut input = arguments.to_vec();
    for _ in 0..ITERATIONS {
        let mut reader = |_: &ResponseFileReadRequest<'_>| -> Result<Vec<u8>, Diagnostic> {
            panic!("literal benchmark invoked the reader")
        };
        let mut standard_input = std::io::empty();
        let baseline = reset_metrics();
        let started = Instant::now();
        let expanded = expand_response_files(
            input,
            "/work",
            &ResponseFileOptions::default(),
            &mut reader,
            &mut standard_input,
        )
        .unwrap();
        assert_eq!(expanded.len(), arguments.len());
        black_box(&expanded);
        samples.push(read_metrics(started.elapsed(), baseline));
        input = expanded;
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
        "{label} iterations={ITERATIONS} min_ns={} median_ns={} median_allocs={} median_allocated_bytes={} median_peak_live_bytes={} median_retained_bytes={}",
        elapsed[0].as_nanos(),
        elapsed[middle].as_nanos(),
        allocations[middle],
        allocated_bytes[middle],
        peak_live_bytes[middle],
        retained_bytes[middle],
    );
}

fn main() {
    report("cli-response-file-single", sample_source(&source(1), 1));
    report(
        "cli-response-file-1000-tokens",
        sample_source(&source(LARGE_ARGUMENT_COUNT), LARGE_ARGUMENT_COUNT),
    );
    let literals = (0..LARGE_ARGUMENT_COUNT)
        .map(|index| OsString::from(format!("value-{index}")))
        .collect::<Vec<_>>();
    report(
        "cli-response-file-1000-literal-arguments",
        sample_literals(&literals),
    );
}
