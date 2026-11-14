//! Prints the allocations, and the bytes that they ask for, of each builder
//! that `builder_comparison` times, per call of each row, counted by a global
//! allocator that wraps the system's, as the `allocation` binary counts the
//! codecs'. A reallocation counts as an allocation of its new size, and
//! freeing isn't counted.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use tafla::prototype::{DirectNestedTable, DirectTableBuilder, InlineBuffer, VecBuffer};
use tafla::{NestedTable, TableBuilder};
use tafla_benchmarks::codec::Error;
use tafla_benchmarks::prototype::{Builder, Visit, for_each_builder};
use tafla_benchmarks::workloads::{self, Document, LIST_ITEMS, List, RECORD, Record};

#[global_allocator]
static ALLOCATOR: Counting = Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

/// Calls of each row that the counts are divided among
const CALLS: usize = 100;

const ROWS: [&str; 5] = [
    "record",
    "document of 100 rows",
    "document of 100,000 rows",
    "vector of 100 nested tables",
    "array of 100 structs",
];

/// The system's allocator, which counts allocations and their bytes
struct Counting;

// SAFETY: each method passes its arguments on to the system's allocator,
// whose contract is the same
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: as the caller of `alloc()` guarantees
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: as the caller of `alloc_zeroed()` guarantees
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        // SAFETY: as the caller of `realloc()` guarantees
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: as the caller of `dealloc()` guarantees
        unsafe { System.dealloc(pointer, layout) }
    }
}

fn count(bytes: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    BYTES.fetch_add(bytes, Ordering::Relaxed);
}

struct Inputs {
    record: Record<'static>,
    small: Document<'static>,
    large: Document<'static>,
    list: List,
}

/// The counts of each builder, per row
struct Counts<'i> {
    inputs: &'i Inputs,
    rows: [Vec<String>; 5],
}

impl Visit for Counts<'_> {
    fn visit<T: Builder>(&mut self, name: &'static str) {
        let inputs = self.inputs;
        let counts = [
            counts(|| T::record(&inputs.record)),
            counts(|| T::document(&inputs.small)),
            counts(|| T::document(&inputs.large)),
            counts(|| T::list(&inputs.list)),
            counts(|| T::structs(&inputs.list)),
        ];

        for (row, (bytes, allocations)) in counts.into_iter().enumerate() {
            let line = format!("  {name:22} {bytes:>12} B {allocations:>6} allocations");
            self.rows[row].push(line);
        }
    }
}

/// The bytes and allocations per call of `call`, after a call that
/// initializes whatever is initialized once
fn counts(call: impl Fn() -> Result<Vec<u8>, Error>) -> (usize, usize) {
    black_box(call().ok());

    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let bytes = BYTES.load(Ordering::Relaxed);
    for _ in 0..CALLS {
        black_box(call().ok());
    }
    let allocations = (ALLOCATIONS.load(Ordering::Relaxed) - allocations) / CALLS;
    let bytes = (BYTES.load(Ordering::Relaxed) - bytes) / CALLS;

    (bytes, allocations)
}

fn main() {
    let inputs = Inputs {
        record: RECORD,
        small: workloads::document(100),
        large: workloads::document(100_000),
        list: workloads::list(LIST_ITEMS),
    };

    let mut counts = Counts {
        inputs: &inputs,
        rows: Default::default(),
    };
    for_each_builder(&mut counts);

    print_sizes();

    println!("\nAllocations and bytes allocated per call of each builder");
    for (name, lines) in ROWS.iter().zip(&counts.rows) {
        println!("\n{name}: serialize");
        for line in lines {
            println!("{line}");
        }
    }
}

/// Prints the sizes of the builders, which a builder that nests another
/// holds within its own
fn print_sizes() {
    println!("Sizes in bytes");
    println!(
        "  TableBuilder              {:>4}",
        size_of::<TableBuilder<'_>>()
    );
    println!(
        "  NestedTable               {:>4}",
        size_of::<NestedTable<'_>>()
    );
    println!(
        "  direct (Vec, 4 segments)  {:>4}",
        size_of::<DirectTableBuilder<'_, VecBuffer, 4>>()
    );
    println!(
        "  direct (Vec, 0 segments)  {:>4}",
        size_of::<DirectTableBuilder<'_, VecBuffer, 0>>()
    );
    println!(
        "  direct (inline, 4)        {:>4}",
        size_of::<DirectTableBuilder<'_, InlineBuffer, 4>>()
    );
    println!(
        "  direct (inline, 0)        {:>4}",
        size_of::<DirectTableBuilder<'_, InlineBuffer, 0>>()
    );
    println!(
        "  DirectNestedTable (Vec, 4) {:>3}",
        size_of::<DirectNestedTable<'_, VecBuffer, 4>>()
    );
}
