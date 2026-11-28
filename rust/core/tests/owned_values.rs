//! A builder frees each kind of value that it was given to own when it's
//! dropped, whether or not it wrote its table, and the room for its records
//! when it owns nothing. A global allocator counts the bytes that the test's
//! thread holds.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use tafla::{Alignment, BuildError, NestedTable, TableBuilder, TableLayout};

#[global_allocator]
static ALLOCATOR: Counting = Counting;

thread_local! {
    /// The bytes that this thread's allocations hold
    static HELD: Cell<isize> = const { Cell::new(0) };
}

/// The system's allocator, which counts the bytes that each thread holds
struct Counting;

#[allow(clippy::cast_possible_wrap)]
fn hold(bytes: usize, sign: isize) {
    // A thread's allocations may outlive its thread locals while it exits
    let _ = HELD.try_with(|held| held.set(held.get() + sign * bytes as isize));
}

fn held() -> isize {
    HELD.with(Cell::get)
}

// SAFETY: each method passes its arguments on to the system's allocator,
// whose contract is the same
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        hold(layout.size(), 1);
        // SAFETY: as the caller of `alloc()` guarantees
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        hold(layout.size(), 1);
        // SAFETY: as the caller of `alloc_zeroed()` guarantees
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        hold(layout.size(), -1);
        hold(new_size, 1);
        // SAFETY: as the caller of `realloc()` guarantees
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        hold(layout.size(), -1);
        // SAFETY: as the caller of `dealloc()` guarantees
        unsafe { System.dealloc(pointer, layout) }
    }
}

type Set = fn(&mut TableBuilder<'static>) -> Result<(), BuildError>;

/// A field of each kind that a builder can own, alone in its builder, since
/// a builder that owns one value drops each of its records
const OWNED: [(&str, Set); 14] = [
    ("u8 array", |builder| builder.set_u8_array(vec![1, 2, 3])),
    ("f64 array", |builder| builder.set_f64_array(vec![4.0])),
    ("bool array", |builder| {
        builder.set_bool_array(vec![true, false])
    }),
    ("string", |builder| builder.set_str(String::from("owned"))),
    ("struct", |builder| builder.set_struct(vec![5, 6])),
    ("struct array", |builder| {
        builder.set_struct_array(vec![7; 8], 4, Alignment::Four)
    }),
    ("builder that owns nothing", |builder| {
        builder.set_table(TableBuilder::new(0, TableLayout::DEFAULT)?)
    }),
    ("builder that owns a string", |builder| {
        builder.set_table(owns_a_string(TableLayout::DEFAULT)?)
    }),
    ("table's bytes", |builder| builder.set_table(vec![8_u8; 4])),
    ("vector of borrowed bytes", |builder| {
        builder.set_vector([NestedTable::from(&[9_u8])])
    }),
    ("vector of builders that own a string", |builder| {
        builder.set_vector([owns_a_string(TableLayout::DEFAULT.sized())?])
    }),
    ("vector of strings", |builder| {
        builder.set_str_vector(vec!["a", "b"])
    }),
    ("string of a table in place", |builder| {
        builder.set_table_with(1, |table| table.set_str(String::from("in place")))
    }),
    ("string of a vector in place", |builder| {
        builder.set_vector_with(2, 1, |_, item| item.set_str(String::from("item")))
    }),
];

fn owns_a_string(layout: TableLayout) -> Result<TableBuilder<'static>, BuildError> {
    let mut builder = TableBuilder::new(1, layout)?;
    builder.set_str(String::from("nested"))?;

    Ok(builder)
}

/// The bytes that `build` leaves this thread holding once the builder it
/// returns is dropped
fn left_held<'a>(build: impl FnOnce() -> Result<TableBuilder<'a>, BuildError>) -> isize {
    let before = held();
    drop(build().unwrap());
    held() - before
}

#[test]
fn test_a_builder_frees_each_kind_of_value_that_it_owns() {
    for (kind, set) in OWNED {
        let owned = || {
            let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
            set(&mut builder)?;
            Ok(builder)
        };
        let written = || {
            let builder = owned()?;
            builder.to_bytes()?;
            Ok(builder)
        };

        assert_eq!(left_held(owned), 0, "{kind}");
        assert_eq!(left_held(written), 0, "{kind}, written");
    }
}

#[test]
fn test_a_builder_frees_its_records_when_it_owns_nothing() {
    let borrowed = || {
        let mut builder = TableBuilder::new(5, TableLayout::DEFAULT)?;
        builder.set_u8(1)?;
        builder.set_str("borrowed")?;
        builder.set_u8_array(&[2, 3][..])?;
        builder.set_table(&[4_u8; 4])?;
        builder.set_str_vector(&["a", "b"][..])?;
        Ok(builder)
    };

    assert_eq!(left_held(borrowed), 0);
}

#[test]
fn test_a_builder_frees_what_a_failed_table_in_place_owned() {
    let failed_in_place = || {
        let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
        let error = builder.set_table_with(2, |table| {
            table.set_str(String::from("dropped"))?;
            table.set_u8_array(vec![1; 300])?;
            Err(BuildError::Incomplete { set: 2, fields: 3 })
        });
        assert!(error.is_err());
        builder.set_u8(1)?;
        Ok(builder)
    };

    assert_eq!(left_held(failed_in_place), 0);
}
