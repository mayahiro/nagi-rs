//! Geometry-aware pointer dispatch allocation regression tests

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use nagi_tui::{
    App, Effect, Event, EventResult, MouseButton, MouseEvent, MouseKind, Node, ParagraphOptions,
    PointerEventContext, Runtime, Size, Style, TextSpan,
};

const DOCUMENT_BYTES: usize = 100_000;
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

struct PointerApp {
    document: String,
}

impl App for PointerApp {
    type Message = ();

    fn update(&mut self, (): ()) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::paragraph(
            [TextSpan::new(self.document.clone(), Style::default())],
            ParagraphOptions {
                wrap: nagi_tui::WrapMode::None,
                ..ParagraphOptions::default()
            },
        )
        .on_pointer_event("text", |context: &PointerEventContext| {
            if context.event().kind == MouseKind::Press {
                EventResult::consumed().capture_pointer("text")
            } else {
                EventResult::consumed()
            }
        })
    }
}

fn pointer_event(kind: MouseKind) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        button: MouseButton::Left,
        x: u32::try_from(DOCUMENT_BYTES - 1).unwrap(),
        y: 0,
        modifiers: nagi_tui::Modifiers::NONE,
    })
}

#[test]
fn warmed_long_paragraph_pointer_dispatch_does_not_allocate() {
    let mut runtime = Runtime::new(
        PointerApp {
            document: "x".repeat(DOCUMENT_BYTES),
        },
        Size::new(80, 1),
    )
    .unwrap();
    runtime.render_if_dirty().unwrap();
    runtime
        .dispatch_event(&pointer_event(MouseKind::Press))
        .unwrap();
    let movement = pointer_event(MouseKind::Move);

    let allocations = count_allocations(|| {
        for _ in 0..ITERATIONS {
            black_box(runtime.dispatch_event(black_box(&movement)).unwrap());
        }
    });

    assert_eq!(allocations, 0);
}
