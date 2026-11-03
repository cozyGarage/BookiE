use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::native_connection;
use bookie_core::MAX_QUERY_RESULT_BYTES;

static WATCHING: AtomicBool = AtomicBool::new(false);
static LARGEST_REQUEST: AtomicUsize = AtomicUsize::new(0);

struct Watcher;

unsafe impl GlobalAlloc for Watcher {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(new_size);
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

fn record(size: usize) {
    if WATCHING.load(Ordering::Relaxed) {
        LARGEST_REQUEST.fetch_max(size, Ordering::Relaxed);
    }
}

#[global_allocator]
static ALLOCATOR: Watcher = Watcher;

#[tokio::test]
async fn a_cell_over_the_result_budget_is_refused_before_it_is_copied() {
    let connection = native_connection().await;
    let oversized = MAX_QUERY_RESULT_BYTES + MAX_QUERY_RESULT_BYTES / 2;
    LARGEST_REQUEST.store(0, Ordering::Relaxed);
    WATCHING.store(true, Ordering::Relaxed);

    let result = connection
        .query(&format!("SELECT 1 AS id, repeat('x', {oversized}) AS payload"))
        .await;

    WATCHING.store(false, Ordering::Relaxed);
    let result = result.unwrap();
    assert!(result.rows.is_empty());
    assert!(result.truncated);
    assert!(
        LARGEST_REQUEST.load(Ordering::Relaxed) < MAX_QUERY_RESULT_BYTES / 2,
        "the oversized cell was copied before the budget check"
    );
}
