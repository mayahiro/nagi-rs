//! Standalone bounded Content-to-Node projection benchmark

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nagi_content::{Content, Element, ElementKind, Role};
use nagi_tui::{
    ContentProjectionErrorKind, ContentProjectionLimits, ContentProjectionOptions,
    DeclarationValue, PresentationDeclaration, PresentationRule, PresentationSelector,
    PresentationSheet, TextStyleDeclaration, project_content,
};

const INPUT_NODES: usize = 100_000;
const FAILURE_LIMIT: u64 = 128;
const ITERATIONS: usize = 12;

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

fn visible_input() -> (Content, PresentationSheet, ContentProjectionOptions) {
    let paragraph_role = Role::new("paragraph").unwrap();
    let children = (0..24).map(|index| {
        let inline = Element::new(ElementKind::Inline, [Content::text("value")]);
        Element::new(
            ElementKind::Paragraph,
            [Content::text(index.to_string()), inline.into_content()],
        )
        .with_roles([paragraph_role.clone()])
        .unwrap()
        .into_content()
    });
    let root = Element::new(ElementKind::Flow, children).into_content();
    let sheet = PresentationSheet::new([
        PresentationRule::new(
            PresentationSelector::Any,
            PresentationDeclaration::default().with_text_style(
                TextStyleDeclaration::default().with_dim(DeclarationValue::Set(true)),
            ),
        ),
        PresentationRule::new(
            PresentationSelector::Role(paragraph_role),
            PresentationDeclaration::default()
                .with_visual_separator(DeclarationValue::Set(" ".to_owned())),
        ),
    ]);
    (root, sheet, ContentProjectionOptions::default())
}

fn bounded_failure_input() -> (Content, ContentProjectionOptions) {
    let children = (0..INPUT_NODES).map(|_| Content::text("value"));
    let root = Element::new(ElementKind::Flow, children).into_content();
    let options = ContentProjectionOptions::default()
        .with_limits(ContentProjectionLimits::default().with_max_content_nodes(FAILURE_LIMIT));
    (root, options)
}

fn sample_visible() -> Vec<Sample> {
    let (root, sheet, options) = visible_input();
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let baseline = reset_metrics();
        let started = Instant::now();
        let node = project_content::<()>(&root, &sheet, options).expect("visible projection");
        black_box(&node);
        drop(node);
        samples.push(read_metrics(started.elapsed(), baseline));
    }
    samples
}

fn sample_bounded_failure() -> Vec<Sample> {
    let (root, options) = bounded_failure_input();
    let sheet = PresentationSheet::default();
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let baseline = reset_metrics();
        let started = Instant::now();
        let error = project_content::<()>(&root, &sheet, options)
            .err()
            .expect("bounded projection must fail");
        assert_eq!(error.kind(), ContentProjectionErrorKind::ContentNodeLimit);
        black_box(error);
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
    report("content-projection-visible", sample_visible());
    report(
        "content-projection-bounded-failure-100k",
        sample_bounded_failure(),
    );
}
