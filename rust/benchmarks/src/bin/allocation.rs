//! Prints what each benchmark's call allocates: the allocations and the bytes
//! that they ask for, counted by a global allocator that wraps the system's.
//!
//! The counts cost an atomic addition per allocation, so they run apart from
//! the timings. A reallocation counts as an allocation of its new size, as it
//! may copy into one, and freeing isn't counted. `--strings` counts the
//! strings comparison's calls rather than the workloads'.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use tafla_benchmarks::check;
use tafla_benchmarks::codec::{Codec, DocumentWorkload, Error, Workload};
use tafla_benchmarks::codecs::LeBytes;
use tafla_benchmarks::item_access::{self, ItemAccess, ItemBytes};
use tafla_benchmarks::sized_items;
use tafla_benchmarks::string_workloads::{self, StringsShape};
use tafla_benchmarks::workloads::{
    self, DOCUMENT_ROWS, Document, DocumentShape, LIST_ITEMS, ListShape, RECORD, RecordShape,
    SAMPLE, SampleShape, Shape, StructsShape,
};
use tafla_benchmarks::{for_each_codec, for_each_item_access};

#[global_allocator]
static ALLOCATOR: Counting = Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

/// Calls of each operation that the counts are divided among
const CALLS: usize = 100;

const NAME_WIDTH: usize = 18;

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

fn main() -> Result<(), Error> {
    println!("Allocations and bytes allocated per call");

    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--strings")
    {
        count_strings()
    } else {
        count_workloads()
    }
}

fn count_workloads() -> Result<(), Error> {
    group("record: serialize", || {
        for_each_codec!(|codec| serialize::<RecordShape, _>(codec, &RECORD)?);
        Ok(())
    })?;
    group("record: deserialize", || {
        for_each_codec!(|codec| deserialize::<RecordShape, _>(codec, &RECORD)?);
        Ok(())
    })?;

    for rows in DOCUMENT_ROWS {
        let document = workloads::document(rows);
        let name = format!("document of {rows} rows");

        group(&format!("{name}: serialize"), || {
            for_each_codec!(|codec| serialize::<DocumentShape, _>(codec, &document)?);
            Ok(())
        })?;
        group(&format!("{name}: deserialize"), || {
            for_each_codec!(|codec| deserialize::<DocumentShape, _>(codec, &document)?);
            Ok(())
        })?;
        group(&format!("{name}: sum a column per element"), || {
            for_each_codec!(|codec| sum_a(codec, &document)?);
            Ok(())
        })?;
    }

    let list = workloads::list(LIST_ITEMS);
    let name = format!("vector of {LIST_ITEMS} nested tables");
    group(&format!("{name}: serialize"), || {
        for_each_codec!(|codec| serialize::<ListShape, _>(codec, &list)?);
        Ok(())
    })?;
    group(&format!("{name}: deserialize"), || {
        for_each_codec!(|codec| deserialize::<ListShape, _>(codec, &list)?);
        Ok(())
    })?;

    let name = format!("array of {LIST_ITEMS} structs");
    group(&format!("{name}: serialize"), || {
        for_each_codec!(|codec| serialize::<StructsShape, _>(codec, &list)?);
        Ok(())
    })?;
    group(&format!("{name}: deserialize"), || {
        for_each_codec!(|codec| deserialize::<StructsShape, _>(codec, &list)?);
        Ok(())
    })?;

    let bytes = ItemBytes::write(&list)?;
    for_each_item_access!(|access, _name| item_access::check(access, &list, &bytes)?);
    let (tables, structs) = (bytes.tables.as_slice(), bytes.structs.as_slice());
    let vector = format!("vector of {LIST_ITEMS} nested tables");
    group(&format!("{vector}: read the last item"), || {
        for_each_item_access!(|access, name| read(name, || access.last_table(tables)));
        Ok(())
    })?;
    group(&format!("{vector}: sum every item"), || {
        for_each_item_access!(|access, name| read(name, || access.every_table(tables)));
        Ok(())
    })?;
    group(&format!("{name}: read the last item"), || {
        for_each_item_access!(|access, name| read(name, || access.last_struct(structs)));
        Ok(())
    })?;
    group(&format!("{name}: sum every item"), || {
        for_each_item_access!(|access, name| read(name, || access.every_struct(structs)));
        Ok(())
    })?;

    let sized = sized_items::write(&list)?;
    sized_items::check(&list, &sized)?;
    group(
        &format!("vector of {LIST_ITEMS} sized tables: read the last item"),
        || {
            read("tafla", || sized_items::last_item(&sized));
            Ok(())
        },
    )?;

    group("fixed-size record: serialize", || {
        for_each_codec!(|codec| serialize::<SampleShape, _>(codec, &SAMPLE)?);
        serialize::<SampleShape, _>(LeBytes, &SAMPLE)
    })?;
    group("fixed-size record: deserialize", || {
        for_each_codec!(|codec| deserialize::<SampleShape, _>(codec, &SAMPLE)?);
        deserialize::<SampleShape, _>(LeBytes, &SAMPLE)
    })
}

/// Counts the strings comparison's calls, of each shape
fn count_strings() -> Result<(), Error> {
    for workload in string_workloads::string_workloads() {
        let input = workload.input();
        let name = format!("16 × {}", workload.name);

        group(&format!("{name}: serialize"), || {
            for_each_codec!(|codec| serialize::<StringsShape, _>(codec, &input)?);
            Ok(())
        })?;
        group(&format!("{name}: deserialize"), || {
            for_each_codec!(|codec| deserialize::<StringsShape, _>(codec, &input)?);
            Ok(())
        })?;
    }

    Ok(())
}

fn group(name: &str, count: impl FnOnce() -> Result<(), Error>) -> Result<(), Error> {
    println!("\n{name}");
    count()
}

/// Counts writing `input`, after checking that the codec reads it back
fn serialize<S: Shape, C: Workload<S>>(codec: C, input: &S::Value<'_>) -> Result<(), Error> {
    check::round_trip::<S, C>(codec, input)?;

    print_counts::<C>(|| {
        black_box(codec.serialize(black_box(input)).ok());
    });
    Ok(())
}

/// Counts reading the bytes that the codec wrote of `input`
fn deserialize<S: Shape, C: Workload<S>>(codec: C, input: &S::Value<'_>) -> Result<(), Error> {
    check::round_trip::<S, C>(codec, input)?;
    let bytes = codec.serialize(input)?;

    print_counts::<C>(|| {
        black_box(codec.deserialize(black_box(bytes.as_ref())).ok());
    });
    Ok(())
}

/// Counts summing column `a` of the bytes that the codec wrote of `input`
fn sum_a<C: DocumentWorkload>(codec: C, input: &Document<'_>) -> Result<(), Error> {
    check::sum_a(codec, input)?;
    let bytes = codec.serialize(input)?;

    print_counts::<C>(|| {
        black_box(codec.sum_a(black_box(bytes.as_ref())).ok());
    });
    Ok(())
}

/// Counts a way of reading tafla's items, which was checked
fn read(name: &str, read: impl Fn() -> Result<f64, Error>) {
    print_named_counts(name, || {
        black_box(read().ok());
    });
}

/// Prints the allocations and bytes per call of `call`, after a call that
/// initializes whatever the codec initializes once
fn print_counts<C: Codec>(call: impl FnMut()) {
    print_named_counts(C::NAME, call);
}

fn print_named_counts(name: &str, mut call: impl FnMut()) {
    call();

    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let bytes = BYTES.load(Ordering::Relaxed);
    for _ in 0..CALLS {
        call();
    }
    let allocations = (ALLOCATIONS.load(Ordering::Relaxed) - allocations) / CALLS;
    let bytes = (BYTES.load(Ordering::Relaxed) - bytes) / CALLS;

    println!("  {name:NAME_WIDTH$} {bytes:>12} B {allocations:>6} allocations");
}
