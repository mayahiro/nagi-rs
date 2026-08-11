//! Presentation rule allocation regression tests

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use nagi_content::{Class, Element, ElementKind, Role};
use nagi_tui::{
    Color, DeclarationValue, PresentationDeclaration, PresentationRule, PresentationSelector,
    PresentationSheet, PresentationState, Style, TextStyleDeclaration,
};

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

#[test]
fn warmed_resolve_does_not_allocate() {
    let role = Role::new("item").unwrap();
    let class = Class::new("source").unwrap();
    let selected = PresentationState::new("selected").unwrap();
    let focused = PresentationState::new("focused").unwrap();
    let active_states = [focused.clone(), selected.clone()];
    let element = Element::new(ElementKind::Paragraph, [])
        .with_roles([role.clone()])
        .unwrap()
        .with_classes([class.clone()])
        .unwrap();
    let sheet = PresentationSheet::new([
        PresentationRule::new(
            PresentationSelector::Any,
            PresentationDeclaration::default().with_text_style(
                TextStyleDeclaration::default().with_bold(DeclarationValue::Set(true)),
            ),
        ),
        PresentationRule::new(
            PresentationSelector::Class(class),
            PresentationDeclaration::default().with_text_style(
                TextStyleDeclaration::default()
                    .with_foreground(DeclarationValue::Set(Color::Indexed(3))),
            ),
        ),
        PresentationRule::new(
            PresentationSelector::Role(role),
            PresentationDeclaration::default()
                .with_visual_separator(DeclarationValue::Set(" | ".to_owned())),
        )
        .with_required_states([selected, focused])
        .unwrap(),
    ]);
    let inherited_style = Style::default();

    black_box(sheet.resolve(&element, inherited_style, &active_states));
    let allocations = count_allocations(|| {
        for _ in 0..ITERATIONS {
            black_box(black_box(&sheet).resolve(
                black_box(&element),
                black_box(inherited_style),
                black_box(&active_states),
            ));
        }
    });

    assert_eq!(allocations, 0);
}
