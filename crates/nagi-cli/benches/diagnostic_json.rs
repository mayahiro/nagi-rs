//! Standalone JSON Diagnostic Renderer benchmark

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nagi_cli::{
    Diagnostic, DiagnosticCategory, DiagnosticCode, DiagnosticRenderer, DiagnosticTarget,
    JsonDiagnosticRenderer,
};

const SAMPLES: usize = 12;
const RENDERS_PER_SAMPLE: usize = 1_000;

struct TrackingAllocator;

static TRACKING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

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
        // SAFETY: the pointer and layout came from this allocator
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the pointer and layout came from this allocator
        let allocated = unsafe { System.realloc(pointer, layout, new_size) };
        if !allocated.is_null() {
            record_allocation(new_size);
        }
        allocated
    }
}

fn record_allocation(bytes: usize) {
    if TRACKING.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy)]
struct Sample {
    elapsed: Duration,
    allocations: usize,
    allocated_bytes: usize,
}

fn main() {
    report("cli-diagnostic-json-structured", sample(structured()));
    report(
        "cli-diagnostic-json-64k-message",
        sample(Diagnostic::new(
            DiagnosticCode::HandlerError,
            "x".repeat(65_536),
        )),
    );
}

fn structured() -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::application("profile-blocked"),
        "profile \"prod\" blocked",
    )
    .with_category(DiagnosticCategory::Usage)
    .with_command_path(vec!["nagi".to_owned(), "deploy".to_owned()])
    .with_usage("nagi deploy --profile <PROFILE>")
    .with_target(
        DiagnosticTarget::option("profile")
            .with_command_id_path(vec!["root".to_owned(), "deploy".to_owned()]),
    )
    .with_target(
        DiagnosticTarget::argument("target")
            .with_command_id_path(vec!["root".to_owned(), "deploy".to_owned()]),
    )
    .with_hint("choose staging")
    .with_hint("inspect the provider configuration")
}

fn sample(diagnostic: Diagnostic) -> Vec<Sample> {
    let renderer = JsonDiagnosticRenderer;
    let warmup = renderer.render_diagnostic(&diagnostic);
    assert!(warmup.ends_with("}\n"));
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        ALLOCATIONS.store(0, Ordering::Relaxed);
        ALLOCATED_BYTES.store(0, Ordering::Relaxed);
        TRACKING.store(true, Ordering::Release);
        let started = Instant::now();
        for _ in 0..RENDERS_PER_SAMPLE {
            black_box(renderer.render_diagnostic(black_box(&diagnostic)));
        }
        let elapsed = started.elapsed();
        TRACKING.store(false, Ordering::Release);
        samples.push(Sample {
            elapsed: elapsed / RENDERS_PER_SAMPLE as u32,
            allocations: ALLOCATIONS.load(Ordering::Relaxed) / RENDERS_PER_SAMPLE,
            allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed) / RENDERS_PER_SAMPLE,
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
    elapsed.sort_unstable();
    allocations.sort_unstable();
    allocated_bytes.sort_unstable();
    let middle = samples.len() / 2;
    println!(
        "{label} samples={SAMPLES} renders_per_sample={RENDERS_PER_SAMPLE} median_ns={} median_allocs={} median_allocated_bytes={}",
        elapsed[middle].as_nanos(),
        allocations[middle],
        allocated_bytes[middle]
    );
}
