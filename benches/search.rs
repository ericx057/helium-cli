#[path = "../src/search.rs"]
#[allow(dead_code)]
mod search;

use search::{Index, Tab};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

struct Meter;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static TRACK: AtomicBool = AtomicBool::new(false);

// Track requested heap bytes; allocator metadata and process RSS are separate.
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && TRACK.load(Ordering::Relaxed) {
            let current = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(current, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        if TRACK.load(Ordering::Relaxed) {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        }
    }
}

#[global_allocator]
static ALLOCATOR: Meter = Meter;

fn main() {
    for count in [20, 100, 1000] {
        let tabs: Vec<Tab> = (0..count)
            .map(|id| Tab {
                window_id: (id / 10 + 1) as i64,
                tab_id: (id + 1) as i64,
                title: format!(
                    "Project {id}: Rust browser automation notes and tutorial | Example"
                ),
                url: format!("https://site{}.example.com/project/{id}", id % 7),
            })
            .collect();
        LIVE.store(0, Ordering::Relaxed);
        PEAK.store(0, Ordering::Relaxed);
        TRACK.store(true, Ordering::Relaxed);
        let start = Instant::now();
        let index = Index::new(&tabs);
        black_box(index.title("Project 12:"));
        let build_ns = start.elapsed().as_nanos();
        let active = LIVE.load(Ordering::Relaxed) + std::mem::size_of_val(&index);
        let peak = PEAK.load(Ordering::Relaxed) + std::mem::size_of_val(&index);
        TRACK.store(false, Ordering::Relaxed);
        for query in ["Project 12:", "nonexistent", "rust"] {
            for _ in 0..1000 {
                black_box(index.title(black_box(query)));
            }
            let mut batches = Vec::with_capacity(200);
            for _ in 0..200 {
                let start = Instant::now();
                for _ in 0..100 {
                    black_box(index.title(black_box(query)));
                }
                batches.push(start.elapsed().as_nanos() as f64 / 100.0);
            }
            batches.sort_by(f64::total_cmp);
            let mut individual = Vec::with_capacity(5000);
            for _ in 0..5000 {
                let start = Instant::now();
                black_box(index.title(black_box(query)));
                individual.push(start.elapsed().as_nanos());
            }
            individual.sort_unstable();
            println!(
                "{{\"tabs\":{count},\"query\":{query:?},\"build_ns\":{build_ns},\"index_bytes\":{active},\"peak_index_bytes\":{peak},\"batch_mean_p50_ns\":{:.2},\"batch_mean_p95_ns\":{:.2},\"observed_single_call_p95_ns\":{}}}",
                batches[100], batches[189], individual[4749]
            );
        }
    }
}
