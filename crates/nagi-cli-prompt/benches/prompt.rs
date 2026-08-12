//! Standalone injected Prompt benchmark

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nagi_cli::CancellationToken;
use nagi_cli_prompt::{Confirm, Input, InputMode, PromptIo, Prompter, ReadResult};

const ITERATIONS: usize = 12;
const REQUESTS_PER_ITERATION: usize = 1_000;
const LARGE_REQUESTS_PER_ITERATION: usize = 100;
const LARGE_INPUT_BYTES: usize = 65_536;

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

fn sample(
    response: Vec<u8>,
    requests_per_iteration: usize,
    run: impl Fn(&mut BenchIo),
) -> Vec<Sample> {
    let mut prompt_io = BenchIo { response };
    run(&mut prompt_io);
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        ALLOCATIONS.store(0, Ordering::Relaxed);
        ALLOCATED_BYTES.store(0, Ordering::Relaxed);
        TRACKING.store(true, Ordering::Release);
        let started = Instant::now();
        for _ in 0..requests_per_iteration {
            run(black_box(&mut prompt_io));
        }
        let elapsed = started.elapsed();
        TRACKING.store(false, Ordering::Release);
        samples.push(Sample {
            elapsed: elapsed / requests_per_iteration as u32,
            allocations: ALLOCATIONS.load(Ordering::Relaxed) / requests_per_iteration,
            allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed) / requests_per_iteration,
        });
    }
    samples
}

fn report(label: &str, requests_per_iteration: usize, samples: Vec<Sample>) {
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
        "{label} iterations={ITERATIONS} requests_per_iteration={requests_per_iteration} min_ns={} median_ns={} median_allocs={} median_allocated_bytes={}",
        elapsed[0].as_nanos(),
        elapsed[middle].as_nanos(),
        allocations[middle],
        allocated_bytes[middle]
    );
}

fn main() {
    let cancellation = CancellationToken::new();
    let confirm = Confirm::new("Proceed?");
    report(
        "cli-prompt-confirm",
        REQUESTS_PER_ITERATION,
        sample(b"yes".to_vec(), REQUESTS_PER_ITERATION, |prompt_io| {
            let value = Prompter::new(prompt_io)
                .confirm(black_box(&cancellation), black_box(&confirm))
                .expect("Confirm must succeed");
            assert!(value);
            black_box(value);
        }),
    );

    let input = Input::new("Value");
    report(
        "cli-prompt-input-64k",
        LARGE_REQUESTS_PER_ITERATION,
        sample(
            vec![b'a'; LARGE_INPUT_BYTES],
            LARGE_REQUESTS_PER_ITERATION,
            |prompt_io| {
                let value = Prompter::new(prompt_io)
                    .input(black_box(&cancellation), black_box(&input))
                    .expect("Input must succeed");
                assert_eq!(value.len(), LARGE_INPUT_BYTES);
                black_box(value);
            },
        ),
    );
}

struct BenchIo {
    response: Vec<u8>,
}

impl Write for BenchIo {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PromptIo for BenchIo {
    fn is_terminal(&self) -> bool {
        true
    }

    fn read_line(
        &mut self,
        _cancellation: &CancellationToken,
        _mode: InputMode,
        _max_bytes: usize,
    ) -> io::Result<ReadResult> {
        Ok(ReadResult::Line(self.response.clone()))
    }
}
