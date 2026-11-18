//! The native addon of `@tafla/native`: reads of many values of a table in
//! one call, into a typed array that the caller passes.
//!
//! A call from JavaScript into an addon costs more than a read of a field
//! by the TypeScript runtime, so the addon has no getter of one value. Each
//! of its functions reads through the Rust runtime's `TableReader`, with
//! its bounds checks, and throws its `DecodeError`'s message.
//!
//! The functions are Node-API callbacks written by hand, in `call`, rather
//! than napi-rs's `#[napi]` functions, whose generated code took about 120 ns
//! more in each call, in napi 3.16.2: 144 ns for a function that adds two
//! numbers, against 20 ns by hand.

use std::ops::Range;

mod call;
mod column;
mod workloads;

use napi::bindgen_prelude::{JsObjectValue, Object, Result};
use napi::sys;
use napi_derive::napi;
use tafla::{DecodeError, TableLayout, TableReader, Vector};

use crate::call::{Arguments, Elements, Failure, respond};
use crate::column::{Column, ElementType};

/// `get<Type>Fields(bytes, out)`: reads the first `out.length` fields of the
/// root table of `bytes`, which are all of `out`'s element type, into `out`
unsafe extern "C" fn get_fields<T: Column>(
    env: sys::napi_env,
    info: sys::napi_callback_info,
) -> sys::napi_value {
    // SAFETY: Node-API calls this with its own `env` and `info`
    let arguments = unsafe { Arguments::<2>::new(env, info) };

    respond(env, read_fields(&arguments, &T::TYPE).map(|()| None))
}

fn read_fields(
    arguments: &Arguments<2>,
    element: &ElementType,
) -> std::result::Result<(), Failure> {
    let bytes = arguments.bytes(0)?;
    let mut out = arguments.elements(1, element, bytes)?;
    let reader = TableReader::new(bytes, TableLayout::DEFAULT);

    for index in 0..out.len() {
        let position = reader.root_offset(index)?;
        out.set(index, reader.get_struct(position, element.size)?);
    }

    Ok(())
}

/// The columns that one call of `get<Type>Columns()` can read
const MAX_COLUMNS: usize = 16;

/// The arguments of `get<Type>Columns()`: the bytes, the vector's position,
/// and a path and an array for each column
const COLUMNS_ARGUMENTS: usize = 2 + 2 * MAX_COLUMNS;

/// `get<Type>Columns(bytes, vectorPosition, path, out, ...)`: reads a field
/// of every item of the vector of tables at `vectorPosition` into each
/// `out`, in one pass over the items, and returns the vector's length. Each
/// `path` holds its field's index in the item, or the indexes of the nested
/// tables that lead to it and then its own.
unsafe extern "C" fn get_columns<T: Column>(
    env: sys::napi_env,
    info: sys::napi_callback_info,
) -> sys::napi_value {
    // SAFETY: Node-API calls this with its own `env` and `info`
    let arguments = unsafe { Arguments::<COLUMNS_ARGUMENTS>::new(env, info) };

    respond(env, read_columns(&arguments, &T::TYPE).map(Some))
}

fn read_columns(
    arguments: &Arguments<COLUMNS_ARGUMENTS>,
    element: &ElementType,
) -> std::result::Result<u32, Failure> {
    let bytes = arguments.bytes(0)?;
    let vector_position = arguments.position(1)?;

    let reader = TableReader::new(bytes, TableLayout::DEFAULT);
    let vector = reader.get_vector(vector_position)?;
    let mut columns = ItemColumns::of(arguments, element, bytes, vector.len())?;

    for column in &mut columns.columns {
        let path = &columns.paths[column.path.clone()];
        read_column(&vector, path, element.size, &mut column.out)?;
    }

    // A vector's count is read from an offset, which has at most 32 bits
    Ok(u32::try_from(vector.len()).unwrap_or(u32::MAX))
}

/// Reads the field of `size` bytes that `path` leads to, of every item of
/// the vector, into `out`, which has room for them.
///
/// Each size, and each of a field of the item and a field of a table nested
/// in it, has a loop of its own, in which the compiler inlines the vector's
/// iterator and the reads and knows the size. One loop for every size and
/// path took 43 to 61 µs for a column of 10,000 float32s, as the code around
/// it changed, against 13 µs for a loop written for that field.
fn read_column(
    vector: &Vector<'_>,
    path: &[u32],
    size: usize,
    out: &mut Elements<'_>,
) -> std::result::Result<(), DecodeError> {
    match size {
        1 => read_sized_column::<1>(vector, path, out),
        2 => read_sized_column::<2>(vector, path, out),
        4 => read_sized_column::<4>(vector, path, out),
        _ => read_sized_column::<8>(vector, path, out),
    }
}

fn read_sized_column<const SIZE: usize>(
    vector: &Vector<'_>,
    path: &[u32],
    out: &mut Elements<'_>,
) -> std::result::Result<(), DecodeError> {
    match *path {
        [field] => read_items::<SIZE>(vector, out, |item| item.root_offset(field as usize)),
        [table, field] => read_items::<SIZE>(vector, out, |item| {
            item.field_offset(item.root_offset(table as usize)?, field as usize)
        }),
        _ => read_items::<SIZE>(vector, out, |item| field_position(item, path)),
    }
}

/// Reads the `SIZE` bytes at the position that `position_in` returns for
/// each item of the vector into `out`
#[inline]
fn read_items<const SIZE: usize>(
    vector: &Vector<'_>,
    out: &mut Elements<'_>,
    position_in: impl Fn(&TableReader<'_>) -> std::result::Result<usize, DecodeError>,
) -> std::result::Result<(), DecodeError> {
    for (index, item) in vector.iter_in_place().enumerate() {
        let item = TableReader::new(item?, TableLayout::DEFAULT);
        let position = position_in(&item)?;

        out.set_sized::<SIZE>(index, item.get_struct(position, SIZE)?);
    }

    Ok(())
}

/// The fields of a vector's items that a call reads, and the arrays that it
/// reads them into
struct ItemColumns<'call> {
    /// Every column's path, one after another, copied from the caller's
    /// arrays, so that the loop over the items reads them from a slice
    paths: Vec<u32>,
    columns: Vec<ItemColumn<'call>>,
}

struct ItemColumn<'call> {
    /// Where the column's path lies in the paths
    path: Range<usize>,
    out: Elements<'call>,
}

impl<'call> ItemColumns<'call> {
    /// The path and the array of each column that a call was passed, after
    /// the bytes and the vector's position, for a vector of `items`
    fn of(
        arguments: &'call Arguments<COLUMNS_ARGUMENTS>,
        element: &ElementType,
        bytes: &[u8],
        items: usize,
    ) -> std::result::Result<Self, Failure> {
        let passed = arguments.count().saturating_sub(2);
        if passed == 0 || !passed.is_multiple_of(2) || passed > 2 * MAX_COLUMNS {
            return Err(Failure::Type(
                "Expected a path and an array for each of 1 to 16 columns",
            ));
        }

        let mut paths = Vec::new();
        let mut columns = Vec::with_capacity(passed / 2);
        for argument in (2..2 + passed).step_by(2) {
            let path = arguments.path(argument)?;
            let out = arguments.elements(argument + 1, element, bytes)?;

            if path.len() == 0 {
                return Err(Failure::Type("Expected path to hold a field's index"));
            }
            if items > out.len() {
                return Err(Failure::Range(format!(
                    "The vector has {items} items, and out has room for {}",
                    out.len()
                )));
            }

            let start = paths.len();
            paths.extend(path.indexes());
            columns.push(ItemColumn {
                path: start..paths.len(),
                out,
            });
        }

        Ok(Self { paths, columns })
    }
}

/// The position of the field that `path` leads to, from the root table of
/// `item` through its nested tables. `path` holds an index at least.
#[inline]
fn field_position(item: &TableReader<'_>, path: &[u32]) -> std::result::Result<usize, DecodeError> {
    let mut position = item.root_offset(path[0] as usize)?;
    for &field in &path[1..] {
        position = item.field_offset(position, field as usize)?;
    }

    Ok(position)
}

macro_rules! register {
    ($exports:ident, $($type:ty => $fields:literal, $column:literal;)*) => {$(
        $exports.create_named_method($fields, get_fields::<$type>)?;
        $exports.create_named_method($column, get_columns::<$type>)?;
    )*};
}

/// Adds the addon's functions to its module
///
/// # Errors
///
/// Node-API's error, if it can't create a function.
#[napi(module_exports)]
pub fn exports(mut exports: Object) -> Result<()> {
    register!(
        exports,
        i8 => "getInt8Fields", "getInt8Columns";
        u8 => "getUint8Fields", "getUint8Columns";
        i16 => "getInt16Fields", "getInt16Columns";
        u16 => "getUint16Fields", "getUint16Columns";
        i32 => "getInt32Fields", "getInt32Columns";
        u32 => "getUint32Fields", "getUint32Columns";
        i64 => "getInt64Fields", "getInt64Columns";
        u64 => "getUint64Fields", "getUint64Columns";
        f32 => "getFloat32Fields", "getFloat32Columns";
        f64 => "getFloat64Fields", "getFloat64Columns";
    );
    workloads::register(&mut exports)?;

    Ok(())
}
