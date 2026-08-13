//! Standalone injected Status Reporter benchmark

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nagi_cli_status::{Reporter, Snapshot, StatusIo};

const SAMPLES: usize = 12;
const UPDATES_PER_SAMPLE: u64 = 10_000;

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
    report("cli-status-terminal-update", sample(true));
    report("cli-status-log-coalesced", sample(false));
}

fn sample(terminal: bool) -> Vec<Sample> {
    let mut output = SinkIo { terminal, bytes: 0 };
    let mut reporter = Reporter::new(&mut output);
    for tick in 0..8 {
        reporter
            .update(Snapshot::spinner(tick, "waiting"))
            .expect("warmup must succeed");
    }

    let mut samples = Vec::with_capacity(SAMPLES);
    let mut tick = 0_u64;
    for _ in 0..SAMPLES {
        ALLOCATIONS.store(0, Ordering::Relaxed);
        ALLOCATED_BYTES.store(0, Ordering::Relaxed);
        TRACKING.store(true, Ordering::Release);
        let started = Instant::now();
        for _ in 0..UPDATES_PER_SAMPLE {
            tick = tick.wrapping_add(1);
            black_box(
                reporter
                    .update(Snapshot::spinner(black_box(tick), "waiting"))
                    .expect("benchmark update must succeed"),
            );
        }
        let elapsed = started.elapsed();
        TRACKING.store(false, Ordering::Release);
        samples.push(Sample {
            elapsed: elapsed / UPDATES_PER_SAMPLE as u32,
            allocations: ALLOCATIONS.load(Ordering::Relaxed) / UPDATES_PER_SAMPLE as usize,
            allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed) / UPDATES_PER_SAMPLE as usize,
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
        "{label} samples={SAMPLES} updates_per_sample={UPDATES_PER_SAMPLE} median_ns={} median_allocs={} median_allocated_bytes={}",
        elapsed[middle].as_nanos(),
        allocations[middle],
        allocated_bytes[middle]
    );
}

struct SinkIo {
    terminal: bool,
    bytes: u64,
}

impl Write for SinkIo {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.bytes = self.bytes.saturating_add(buffer.len() as u64);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl StatusIo for SinkIo {
    fn is_terminal(&self) -> bool {
        self.terminal
    }

    fn terminal_width(&self) -> Option<usize> {
        Some(80)
    }
}
