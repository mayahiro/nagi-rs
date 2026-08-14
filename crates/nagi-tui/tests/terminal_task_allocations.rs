//! Terminal-suspending Effect allocation regression tests

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use nagi_tui::{App, Effect, Node, Runtime, Size, ViewContext};

const ITERATIONS: usize = 1_000;

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

#[derive(Clone, Copy)]
enum Message {
    Start,
    Returned,
}

struct TerminalTaskApp;

impl App for TerminalTaskApp {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Start => Effect::suspend_terminal(|_| Message::Returned),
            Message::Returned => Effect::none(),
        }
    }

    fn view(&self, _context: ViewContext) -> Node<Self::Message> {
        Node::text("")
    }
}

fn round_trip(runtime: &mut Runtime<TerminalTaskApp>) {
    runtime.enqueue(Message::Start).unwrap();
    runtime.process_pending().unwrap();
    assert!(runtime.run_terminal_task());
    runtime.process_pending().unwrap();
}

#[test]
fn warmed_terminal_task_round_trip_allocates_only_cancel_state() {
    let mut runtime = Runtime::new(TerminalTaskApp, Size::new(1, 1)).unwrap();
    round_trip(&mut runtime);

    let allocations = count_allocations(|| {
        for _ in 0..ITERATIONS {
            round_trip(black_box(&mut runtime));
        }
    });

    assert_eq!(allocations, ITERATIONS);
}
