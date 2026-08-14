//! Status Reporter allocation regression tests

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::io::{self, Write};

use nagi_cli_status::{Reporter, Snapshot, StatusIo};

const ITERATIONS: u64 = 1_000;

struct TrackingAllocator;

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

// SAFETY: every operation delegates to System with the original pointer and
// layout contract and only records successful allocation operations
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller provides the GlobalAlloc layout contract
        let allocated = unsafe { System.alloc(layout) };
        if !allocated.is_null() {
            record_allocation();
        }
        allocated
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller provides the GlobalAlloc layout contract
        let allocated = unsafe { System.alloc_zeroed(layout) };
        if !allocated.is_null() {
            record_allocation();
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
            record_allocation();
        }
        allocated
    }
}

fn record_allocation() {
    TRACKING.with(|tracking| {
        if tracking.get() {
            ALLOCATIONS.with(|allocations| {
                allocations.set(allocations.get().saturating_add(1));
            });
        }
    });
}

fn count_allocations(run: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|allocations| allocations.set(0));
    TRACKING.with(|tracking| {
        assert!(
            !tracking.replace(true),
            "allocation tracking is already active"
        );
    });
    run();
    TRACKING.with(|tracking| tracking.set(false));
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn warmed_terminal_updates_do_not_allocate() {
    let mut output = SinkIo { terminal: true };
    let mut reporter = Reporter::new(&mut output);
    for tick in 0..8 {
        reporter
            .update(Snapshot::spinner(tick, "waiting"))
            .expect("warmup must succeed");
    }

    let allocations = count_allocations(|| {
        for tick in 0..ITERATIONS {
            black_box(
                reporter
                    .update(Snapshot::spinner(black_box(tick), "waiting"))
                    .expect("update must succeed"),
            );
        }
    });

    assert_eq!(allocations, 0);
}

#[test]
fn warmed_coalesced_fallback_updates_do_not_allocate() {
    let mut output = SinkIo { terminal: false };
    let mut reporter = Reporter::new(&mut output);
    for tick in 0..2 {
        reporter
            .update(Snapshot::spinner(tick, "waiting"))
            .expect("warmup must succeed");
    }

    let allocations = count_allocations(|| {
        for tick in 0..ITERATIONS {
            black_box(
                reporter
                    .update(Snapshot::spinner(black_box(tick), "waiting"))
                    .expect("update must succeed"),
            );
        }
    });

    assert_eq!(allocations, 0);
}

struct SinkIo {
    terminal: bool,
}

impl Write for SinkIo {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
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
