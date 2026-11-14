//! Counts the allocations of the module's Rust heap, which `tracemalloc`
//! doesn't trace, for the prototype's allocation report: the `Vec` of the
//! prototype `DirectTableBuilder`. Only a build with the feature
//! `count-allocations` has it, and none that's timed, since each allocation
//! updates the counters.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use pyo3::prelude::*;

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static BASELINE: AtomicUsize = AtomicUsize::new(0);

fn added(size: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    let current = CURRENT.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(current, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(pointer, layout, size) };
        if !moved.is_null() {
            CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
            added(size);
        }
        moved
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Starts counting allocations from zero, and the peak from the bytes held
/// now
#[pyfunction]
pub fn _reset_rust_allocations() {
    let current = CURRENT.load(Ordering::Relaxed);
    ALLOCATIONS.store(0, Ordering::Relaxed);
    BASELINE.store(current, Ordering::Relaxed);
    PEAK.store(current, Ordering::Relaxed);
}

/// Returns the number of allocations and reallocations since the reset, and
/// the most bytes held at once above those held at the reset
#[pyfunction]
pub fn _rust_allocations() -> (usize, usize) {
    (
        ALLOCATIONS.load(Ordering::Relaxed),
        PEAK.load(Ordering::Relaxed) - BASELINE.load(Ordering::Relaxed),
    )
}
