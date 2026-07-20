//! Standalone eager and virtual ScrollViewport frame benchmark

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nagi_tui::{
    App, Effect, Node, NodeId, Runtime, ScrollOffset, Size, Subscription, ViewContext,
    VirtualFragment,
};

const ROWS: usize = 100_000;
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
                let live = LIVE_BYTES.fetch_add(new_size - layout.size(), Ordering::Relaxed)
                    + new_size
                    - layout.size();
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

struct EagerViewport;

struct VirtualViewportApp;

struct IdentifiedVirtualViewportApp {
    row_ids: Arc<[NodeId]>,
}

impl IdentifiedVirtualViewportApp {
    fn new() -> Self {
        let row_ids = (0..ROWS)
            .map(|index| NodeId::from(format!("row-{index}")))
            .collect::<Vec<_>>()
            .into();
        Self { row_ids }
    }
}

impl App for EagerViewport {
    type Message = ();

    fn update(&mut self, (): ()) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: ViewContext) -> Node<Self::Message> {
        let rows = (0..ROWS).map(|_| Node::text("row"));
        Node::scroll_viewport("viewport", Node::column(rows))
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }
}

impl App for VirtualViewportApp {
    type Message = ();

    fn update(&mut self, (): ()) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: ViewContext) -> Node<Self::Message> {
        Node::virtual_scroll_viewport("viewport", Size::new(80, ROWS as u32), |viewport| {
            let start = viewport.offset.y;
            let end = start.saturating_add(viewport.size.height);
            let rows = (start..end).map(|_| Node::text("row"));
            VirtualFragment::new(ScrollOffset::new(0, start), Node::column(rows))
        })
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }
}

impl App for IdentifiedVirtualViewportApp {
    type Message = ();

    fn update(&mut self, (): ()) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: ViewContext) -> Node<Self::Message> {
        let row_ids = Arc::clone(&self.row_ids);
        Node::virtual_scroll_viewport("viewport", Size::new(80, ROWS as u32), move |viewport| {
            let start = viewport.offset.y;
            let end = start.saturating_add(viewport.size.height);
            let rows = (start..end).map(|row| {
                Node::text("row").with_id(row_ids[usize::try_from(row).expect("row index")].clone())
            });
            VirtualFragment::new(ScrollOffset::new(0, start), Node::column(rows))
        })
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }
}

fn warm_runtime<Application>(runtime: &mut Runtime<Application>)
where
    Application: App<Message = ()>,
{
    for _ in 0..2 {
        runtime.request_frame();
        runtime.render_if_dirty().expect("warm-up frame");
    }
}

fn sample_eager() -> Vec<Sample> {
    let mut runtime = Runtime::new(EagerViewport, Size::new(80, 24)).expect("runtime");
    warm_runtime(&mut runtime);
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        runtime.request_frame();
        let baseline = reset_metrics();
        let started = Instant::now();
        black_box(runtime.render_if_dirty().expect("benchmark frame"));
        samples.push(read_metrics(started.elapsed(), baseline));
    }
    samples
}

fn sample_virtual() -> Vec<Sample> {
    let mut runtime = Runtime::new(VirtualViewportApp, Size::new(80, 24)).expect("runtime");
    warm_runtime(&mut runtime);
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        runtime.request_frame();
        let baseline = reset_metrics();
        let started = Instant::now();
        black_box(runtime.render_if_dirty().expect("benchmark frame"));
        samples.push(read_metrics(started.elapsed(), baseline));
    }
    samples
}

fn sample_identified_virtual() -> Vec<Sample> {
    let mut runtime =
        Runtime::new(IdentifiedVirtualViewportApp::new(), Size::new(80, 24)).expect("runtime");
    warm_runtime(&mut runtime);
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        runtime.request_frame();
        let baseline = reset_metrics();
        let started = Instant::now();
        black_box(runtime.render_if_dirty().expect("benchmark frame"));
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
        "{label} rows={ROWS} iterations={ITERATIONS} min_us={} median_us={} median_allocs={} median_allocated_bytes={} median_peak_live_bytes={} median_retained_bytes={}",
        elapsed[0].as_micros(),
        elapsed[middle].as_micros(),
        allocations[middle],
        allocated_bytes[middle],
        peak_live_bytes[middle],
        retained_bytes[middle],
    );
}

fn main() {
    report("eager", sample_eager());
    report("virtual", sample_virtual());
    report("virtual-identified", sample_identified_virtual());
}
