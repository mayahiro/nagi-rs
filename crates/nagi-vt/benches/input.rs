//! Standalone enhanced-keyboard input benchmark

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nagi_vt::{Decoder, Event};

const SAMPLE_COUNT: usize = 12;
const CALLS_PER_SAMPLE: usize = 100_000;
const INPUT: &[u8] = b"\x1B[97;2:1;65u";

struct TrackingAllocator;

static TRACKING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

// SAFETY: every operation delegates to System with the original allocation
// contract and updates independent counters only after successful allocation
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract
        let pointer = unsafe { System.alloc(layout) };
        record(pointer, layout.size());
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract
        let pointer = unsafe { System.alloc_zeroed(layout) };
        record(pointer, layout.size());
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout came from this allocator
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the pointer and layout came from this allocator
        let result = unsafe { System.realloc(pointer, layout, new_size) };
        record(result, new_size);
        result
    }
}

fn record(pointer: *mut u8, bytes: usize) {
    if !pointer.is_null() && TRACKING.load(Ordering::Relaxed) {
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

fn decode(decoder: &mut Decoder) {
    let events = decoder.feed(black_box(INPUT));
    assert!(matches!(events.as_slice(), [Event::Key(_)]));
    black_box(events);
}

fn main() {
    let mut decoder = Decoder::new();
    decode(&mut decoder);
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        ALLOCATIONS.store(0, Ordering::Relaxed);
        ALLOCATED_BYTES.store(0, Ordering::Relaxed);
        let started = Instant::now();
        TRACKING.store(true, Ordering::Relaxed);
        for _ in 0..CALLS_PER_SAMPLE {
            decode(&mut decoder);
        }
        TRACKING.store(false, Ordering::Relaxed);
        samples.push(Sample {
            elapsed: started.elapsed(),
            allocations: ALLOCATIONS.load(Ordering::Relaxed),
            allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        });
    }

    let mut elapsed: Vec<_> = samples.iter().map(|sample| sample.elapsed).collect();
    let mut allocations: Vec<_> = samples.iter().map(|sample| sample.allocations).collect();
    let mut allocated_bytes: Vec<_> = samples
        .iter()
        .map(|sample| sample.allocated_bytes)
        .collect();
    elapsed.sort_unstable();
    allocations.sort_unstable();
    allocated_bytes.sort_unstable();
    let middle = SAMPLE_COUNT / 2;
    println!(
        "vt-kitty-key calls_per_sample={CALLS_PER_SAMPLE} median_ns_per_call={} median_allocs_per_call={} median_allocated_bytes_per_call={}",
        elapsed[middle].as_nanos() / CALLS_PER_SAMPLE as u128,
        allocations[middle] / CALLS_PER_SAMPLE,
        allocated_bytes[middle] / CALLS_PER_SAMPLE,
    );
}
