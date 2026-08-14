//! Standalone application Value Resolver benchmark

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nagi_cli::{
    Command, Diagnostic, OptionSpec, ParseResult, ValueResolution, ValueResolutionRequest,
    ValueSource,
};

const ITERATIONS: usize = 12;
const LARGE_GRAPH_SIZE: usize = 1_000;

struct TrackingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static RESOLVER_CALLS: AtomicUsize = AtomicUsize::new(0);

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

fn selected_command(values: usize) -> Command {
    let mut command = Command::new("root");
    for index in 0..values {
        command = command
            .option(OptionSpec::value(format!("value-{index}")).long(format!("value-{index}")));
    }
    command
}

fn unselected_command(branches: usize) -> Command {
    let mut command = Command::new("root")
        .subcommand(Command::new("run").option(OptionSpec::value("selected").long("selected")));
    for index in 0..branches {
        command = command.subcommand(
            Command::new(format!("branch-{index}")).option(
                OptionSpec::value(format!("branch-value-{index}"))
                    .long(format!("branch-value-{index}")),
            ),
        );
    }
    command
}

fn resolve(_request: &ValueResolutionRequest<'_>) -> Result<ValueResolution, Diagnostic> {
    RESOLVER_CALLS.fetch_add(1, Ordering::Relaxed);
    Ok(ValueResolution::replace("benchmark-config", ["value"]))
}

fn sample(
    command: &Command,
    arguments: &[&str],
    expected_calls: usize,
    last_value_id: &str,
) -> Vec<Sample> {
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        RESOLVER_CALLS.store(0, Ordering::Relaxed);
        let baseline = reset_metrics();
        let started = Instant::now();
        let result = command
            .parse_with_value_resolver(
                arguments.iter().copied(),
                std::iter::empty::<(&str, &str)>(),
                &resolve,
            )
            .unwrap();
        let ParseResult::Invocation(invocation) = result else {
            panic!("benchmark parse did not produce an Invocation");
        };
        assert_eq!(RESOLVER_CALLS.load(Ordering::Relaxed), expected_calls);
        let value = &invocation.parsed_values(last_value_id).unwrap()[0];
        assert_eq!(value.source(), ValueSource::External);
        black_box(&invocation);
        drop(invocation);
        samples.push(read_metrics(started.elapsed(), baseline));
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
        "{label} iterations={ITERATIONS} min_us={} median_us={} median_allocs={} median_allocated_bytes={} median_peak_live_bytes={} median_retained_bytes={}",
        elapsed[0].as_micros(),
        elapsed[middle].as_micros(),
        allocations[middle],
        allocated_bytes[middle],
        peak_live_bytes[middle],
        retained_bytes[middle],
    );
}

fn main() {
    let single = selected_command(1);
    report(
        "cli-value-resolver-single",
        sample(&single, &[], 1, "value-0"),
    );

    let selected = selected_command(LARGE_GRAPH_SIZE);
    report(
        "cli-value-resolver-1000-selected",
        sample(&selected, &[], LARGE_GRAPH_SIZE, "value-999"),
    );

    let unselected = unselected_command(LARGE_GRAPH_SIZE);
    report(
        "cli-value-resolver-1000-unselected-branches",
        sample(&unselected, &["run"], 1, "selected"),
    );
}
