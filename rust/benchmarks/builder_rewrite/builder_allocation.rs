//! Prints the allocations, and the bytes that they ask for, of each builder
//! that `builder_comparison` times, per call of each row, counted by a global
//! allocator that wraps the system's, as the `allocation` binary counts the
//! codecs'. A reallocation counts as an allocation of its new size, and
//! freeing isn't counted. Then it prints the most bytes that each builder
//! held at once while it wrote an uneven vector in place, whose first item
//! is long and the others empty, and a vector in place that makes a table of
//! `uint16` offsets too long.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use tafla::{NestedTable, TableBuilder, main_builder};
use tafla_benchmarks::builders::{
    self, Builder, Medium, TOO_LONG_ITEMS, UNEVEN_ITEMS, UNEVEN_NAME, Visit, for_each_builder,
};
use tafla_benchmarks::codec::Error;
use tafla_benchmarks::workloads::{self, Document, LIST_ITEMS, List, RECORD, Record};

#[global_allocator]
static ALLOCATOR: Counting = Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
/// The bytes allocated and not freed, and the most of them since `PEAK` was
/// last reset
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

/// Calls of each row that the counts are divided among
const CALLS: usize = 100;

const ROWS: [&str; 7] = [
    "record",
    "document of 100 rows",
    "document of 100,000 rows",
    "vector of 100 nested tables",
    "array of 100 structs",
    "vector of 100 nested tables from builders",
    "table of 32 medium values",
];

/// The system's allocator, which counts allocations and their bytes
struct Counting;

// SAFETY: each method passes its arguments on to the system's allocator,
// whose contract is the same
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        hold(layout.size());
        // SAFETY: as the caller of `alloc()` guarantees
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        hold(layout.size());
        // SAFETY: as the caller of `alloc_zeroed()` guarantees
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        hold(new_size);
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: as the caller of `realloc()` guarantees
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: as the caller of `dealloc()` guarantees
        unsafe { System.dealloc(pointer, layout) }
    }
}

fn count(bytes: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    BYTES.fetch_add(bytes, Ordering::Relaxed);
}

/// Counts `bytes` as held until they're freed
fn hold(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

/// The most bytes held at once while `call` ran, beyond those held before
fn peak<T>(call: impl FnOnce() -> T) -> usize {
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    black_box(call());

    PEAK.load(Ordering::Relaxed) - before
}

struct Inputs {
    record: Record<'static>,
    small: Document<'static>,
    large: Document<'static>,
    list: List,
    medium: Medium,
}

/// The counts of each builder, per row
struct Counts<'i> {
    inputs: &'i Inputs,
    rows: [Vec<String>; 7],
    peaks: Vec<String>,
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
            counts(|| T::builders(&inputs.list)),
            counts(|| T::medium(&inputs.medium)),
        ];

        for (row, (bytes, allocations)) in counts.into_iter().enumerate() {
            let line = format!("  {name:22} {bytes:>12} B {allocations:>6} allocations");
            self.rows[row].push(line);
        }

        let name_of = |length| "x".repeat(length);
        let uneven = peak(|| T::uneven(&name_of(UNEVEN_NAME)));
        let too_long = peak(|| T::too_long(&name_of(250)));
        self.peaks.push(format!("  {name:22} {uneven:>12} B {too_long:>12} B"));
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
        medium: builders::medium(),
    };

    let mut counts = Counts {
        inputs: &inputs,
        rows: Default::default(),
        peaks: Vec::new(),
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

    println!(
        "\nMost bytes held at once: an uneven vector of {UNEVEN_ITEMS} items of 12 strings, \
         the first's {UNEVEN_NAME} bytes each, and a vector of {TOO_LONG_ITEMS} strings of 250 \
         bytes that makes a table of uint16 offsets too long"
    );
    for line in &counts.peaks {
        println!("{line}");
    }
}

/// Prints the sizes of the builders, which a builder that nests another
/// holds within its own
fn print_sizes() {
    use tafla::{rewrite_boxed, rewrite_no_reservation};

    println!("Sizes in bytes");
    println!("  main TableBuilder         {:>4}", size_of::<main_builder::TableBuilder<'_>>());
    println!("  main NestedTable          {:>4}", size_of::<main_builder::NestedTable<'_>>());
    println!("  rewrite TableBuilder      {:>4}", size_of::<TableBuilder<'_>>());
    println!("  rewrite NestedTable       {:>4}", size_of::<NestedTable<'_>>());
    println!(
        "  no reservation TableBuilder {:>2}",
        size_of::<rewrite_no_reservation::TableBuilder<'_>>()
    );
    println!(
        "  boxed NestedTable         {:>4}",
        size_of::<rewrite_boxed::NestedTable<'_>>()
    );
}
