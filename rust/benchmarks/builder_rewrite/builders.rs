//! The tafla codec's serialization with main's `TableBuilder`, which
//! `setup.py` copies into the runtime as `tafla::main_builder`, with the
//! rewrite, and with variants of the rewrite, for the `builder_comparison`
//! and `builder_allocation` binaries, and the rows that they compare.
//!
//! Each function makes the codec's calls and is a function of its own, as
//! the codec's `serialize()` is compiled, so that every loop that calls it
//! times the same code, with the fills inlined into it. Beside the
//! comparison's rows, it writes the vector of 100 nested tables from a
//! builder per table, rather than in place, tables of 32 medium and 32
//! short values, and the shapes of the pull request's second review: tables
//! nested in place one in another, each padded for an array of float64 in
//! the innermost, tables of more values than the four segments that a
//! builder holds within itself, of values just shorter than a segment,
//! and of 100-byte values, and builders nested by reference and owned.
//! `uneven()` and `too_long()` are the vectors whose peak memory
//! `builder_allocation` counts.

use std::marker::PhantomData;

use tafla::{Alignment, BuildError, OffsetType, TableLayout};

use crate::codec::Error;
use crate::workloads::{self, Document, Item, LIST_ITEMS, List, RECORD, Record};

const DOCUMENT_LAYOUT: TableLayout<true> = TableLayout::DEFAULT.sized().versioned();
const DOCUMENT_VERSION: u32 = 1;

const ITEM_SIZE: usize = 12;
const ITEM_ALIGNMENT: Alignment = Alignment::Four;

/// The pairs of values of the tables of medium and short values
const PAIRS: usize = 16;

/// A table's values shorter than a segment of 256 bytes: 16 strings and 16
/// float32 arrays
pub struct Medium {
    pub strings: Vec<String>,
    pub arrays: Vec<Vec<f32>>,
}

/// Medium values: strings of 128 bytes and arrays of 48 elements, 192 bytes
pub fn medium() -> Medium {
    values(128, 48)
}

/// Short values: strings of 64 bytes and arrays of 24 elements, 96 bytes
pub fn short() -> Medium {
    values(64, 24)
}

fn values(characters: usize, elements: u16) -> Medium {
    let strings = (0..PAIRS)
        .map(|index| format!("{index:03}").repeat(characters)[..characters].to_owned())
        .collect();
    let arrays = (0..PAIRS)
        .map(|index| (0..elements).map(|element| f32::from(element) + index as f32).collect())
        .collect();

    Medium { strings, arrays }
}

/// Tables nested in place, `depth` in one another, each after a `uint8`,
/// around a table of an array of four float64 and 500 arrays of 200 bytes,
/// so that each is padded
pub struct Nesting {
    pub name: String,
    pub depth: usize,
    pub column: Vec<f64>,
    pub small: Vec<u8>,
}

/// The arrays of 200 bytes of the innermost of the nested tables
const NESTED_ARRAYS: usize = 500;

pub fn nesting(depth: usize) -> Nesting {
    Nesting {
        name: format!("tables nested in place, padded, depth {depth}"),
        depth,
        column: vec![1.5; 4],
        small: vec![7; 200],
    }
}

/// A table of `count` arrays of the same bytes
pub struct Arrays {
    pub name: String,
    pub count: usize,
    pub value: Vec<u8>,
}

pub fn arrays(count: usize, length: usize) -> Arrays {
    Arrays {
        name: format!("table of {count} arrays of {length} B"),
        count,
        value: vec![7; length],
    }
}

/// A table of `count` columns of 100 float64, as the documents' are
pub struct Columns {
    pub name: String,
    pub count: usize,
    pub column: Vec<f64>,
}

pub fn columns(count: usize) -> Columns {
    Columns {
        name: format!("table of {count} float64 columns of 100 rows"),
        count,
        column: vec![1.5; 100],
    }
}

/// `builders` builders of `fields` `uint32` each, nested in a vector
pub struct Scalars {
    pub name: String,
    pub builders: usize,
    pub fields: usize,
}

pub fn scalars(builders: usize, fields: usize, owned: bool) -> Scalars {
    let how = if owned { "owned" } else { "by reference" };
    Scalars {
        name: format!("vector of {builders} builders of {fields} uint32, {how}"),
        builders,
        fields,
    }
}

/// The items of the uneven vector, after its first, whose fields are each
/// `UNEVEN_NAME` long, and the others' empty
pub const UNEVEN_ITEMS: usize = 100_000;
pub const UNEVEN_NAME: usize = 250;

/// The items of a vector in place that makes a table of `uint16` offsets
/// too long, each a string of 250 bytes, which every builder rejects once
/// they're all written
pub const TOO_LONG_ITEMS: usize = 30_000;

macro_rules! serializers {
    ($module:ident, $($builder:ident)::+) => {
        pub mod $module {
            use super::*;
            use $($builder)::+::{InPlaceTable, TableBuilder};

            #[inline(never)]
            pub fn record(input: &Record<'_>) -> Result<Vec<u8>, Error> {
                let mut builder = TableBuilder::new(4, TableLayout::DEFAULT)?;
                builder.set_f64(input.x)?;
                builder.set_f64(input.y)?;
                builder.set_i64(input.id)?;
                builder.set_str(input.name.as_ref())?;

                Ok(builder.to_bytes()?)
            }

            #[inline(never)]
            pub fn document(input: &Document<'_>) -> Result<Vec<u8>, Error> {
                let mut document = TableBuilder::versioned(4, DOCUMENT_LAYOUT, DOCUMENT_VERSION)?;
                document.set_f64(input.x)?;
                document.set_f64(input.y)?;
                document.set_str(input.name.as_ref())?;
                document.set_table_with(4, |columns| {
                    columns.set_i64(input.base)?;
                    columns.set_u32_array(input.keys.as_ref())?;
                    columns.set_f32_array(input.a.as_ref())?;
                    columns.set_f32_array(input.b.as_ref())
                })?;

                Ok(document.to_bytes()?)
            }

            #[inline(never)]
            pub fn list(input: &List) -> Result<Vec<u8>, Error> {
                let items = &input.items;

                let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
                builder.set_vector_with(items.len(), 2, |index, table| {
                    let item = &items[index];
                    table.set_f32(item.value)?;
                    table.set_table_with(2, |pair| {
                        pair.set_f32(item.first)?;
                        pair.set_f32(item.second)
                    })
                })?;

                Ok(builder.to_bytes()?)
            }

            #[inline(never)]
            pub fn structs(input: &List) -> Result<Vec<u8>, Error> {
                let mut structs = Vec::with_capacity(input.items.len() * ITEM_SIZE);
                for item in &input.items {
                    structs.extend_from_slice(&item_struct(item));
                }

                let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
                builder.set_struct_array(structs, ITEM_SIZE, ITEM_ALIGNMENT)?;

                Ok(builder.to_bytes()?)
            }

            #[inline(never)]
            pub fn builders(input: &List) -> Result<Vec<u8>, Error> {
                let mut items = Vec::with_capacity(input.items.len());
                for item in &input.items {
                    let mut pair = TableBuilder::new(2, TableLayout::DEFAULT)?;
                    pair.set_f32(item.first)?;
                    pair.set_f32(item.second)?;

                    let mut table = TableBuilder::new(2, TableLayout::DEFAULT)?;
                    table.set_f32(item.value)?;
                    table.set_table(pair)?;
                    items.push(table);
                }

                let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
                builder.set_vector(items)?;

                Ok(builder.to_bytes()?)
            }

            #[inline(never)]
            pub fn medium(input: &Medium) -> Result<Vec<u8>, Error> {
                let mut builder = TableBuilder::new(2 * PAIRS, TableLayout::DEFAULT)?;
                for (string, array) in input.strings.iter().zip(&input.arrays) {
                    builder.set_str(string.as_str())?;
                    builder.set_f32_array(array.as_slice())?;
                }

                Ok(builder.to_bytes()?)
            }

            #[inline(never)]
            pub fn nested(input: &Nesting) -> Result<Vec<u8>, Error> {
                fn level<'a>(
                    table: &mut InPlaceTable<'_, 'a>,
                    depth: usize,
                    input: &'a Nesting,
                ) -> Result<(), BuildError> {
                    table.set_u8(1)?;
                    if depth > 0 {
                        return table.set_table_with(2, |inner| level(inner, depth - 1, input));
                    }

                    table.set_table_with(1 + NESTED_ARRAYS, |inner| {
                        inner.set_f64_array(input.column.as_slice())?;
                        for _ in 0..NESTED_ARRAYS {
                            inner.set_u8_array(input.small.as_slice())?;
                        }
                        Ok(())
                    })
                }

                let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
                builder.set_table_with(2, |table| level(table, input.depth, input))?;

                Ok(builder.to_bytes()?)
            }

            #[inline(never)]
            pub fn arrays(input: &Arrays) -> Result<Vec<u8>, Error> {
                let mut builder = TableBuilder::new(input.count, TableLayout::DEFAULT)?;
                for _ in 0..input.count {
                    builder.set_u8_array(input.value.as_slice())?;
                }

                Ok(builder.to_bytes()?)
            }

            #[inline(never)]
            pub fn columns(input: &Columns) -> Result<Vec<u8>, Error> {
                let mut builder = TableBuilder::new(input.count, TableLayout::DEFAULT)?;
                for _ in 0..input.count {
                    builder.set_f64_array(input.column.as_slice())?;
                }

                Ok(builder.to_bytes()?)
            }

            #[inline(never)]
            pub fn by_reference(input: &Scalars) -> Result<Vec<u8>, Error> {
                let mut builders = Vec::with_capacity(input.builders);
                for index in 0..input.builders {
                    builders.push(scalars(index, input.fields)?);
                }

                let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
                builder.set_vector(builders.iter())?;

                Ok(builder.to_bytes()?)
            }

            #[inline(never)]
            pub fn owned(input: &Scalars) -> Result<Vec<u8>, Error> {
                let mut builders = Vec::with_capacity(input.builders);
                for index in 0..input.builders {
                    builders.push(scalars(index, input.fields)?);
                }

                let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
                builder.set_vector(builders)?;

                Ok(builder.to_bytes()?)
            }

            #[allow(clippy::cast_possible_truncation)]
            #[inline]
            fn scalars(index: usize, fields: usize) -> Result<TableBuilder<'static>, BuildError> {
                let mut builder = TableBuilder::new(fields, TableLayout::DEFAULT)?;
                for _ in 0..fields {
                    builder.set_u32(index as u32)?;
                }

                Ok(builder)
            }

            #[inline(never)]
            pub fn uneven(name: &str) -> Result<Vec<u8>, BuildError> {
                let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
                builder.set_vector_with(UNEVEN_ITEMS, 12, |index, item| {
                    let value = if index == 0 { name } else { "" };
                    for _ in 0..12 {
                        item.set_str(value)?;
                    }
                    Ok::<_, BuildError>(())
                })?;

                builder.to_bytes()
            }

            #[inline(never)]
            pub fn too_long(name: &str) -> Result<Vec<u8>, BuildError> {
                let layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint16);
                let mut builder = TableBuilder::new(1, layout)?;
                builder.set_vector_with(TOO_LONG_ITEMS, 1, |_, item| item.set_str(name))?;

                builder.to_bytes()
            }
        }
    };
}

serializers!(main_serializers, tafla::main_builder);
serializers!(rewrite_serializers, tafla);
serializers!(no_segment_room_serializers, tafla::rewrite_no_segment_room);
serializers!(segments_128_serializers, tafla::rewrite_segments_128);
serializers!(segments_64_serializers, tafla::rewrite_segments_64);
serializers!(segments_64_twofold_serializers, tafla::rewrite_segments_64_twofold);
serializers!(segments_64_room_16_serializers, tafla::rewrite_segments_64_room_16);
serializers!(segments_64_room_16_twofold_serializers, tafla::rewrite_segments_64_room_16_twofold);

fn item_struct(item: &Item) -> [u8; ITEM_SIZE] {
    let mut bytes = [0; ITEM_SIZE];
    bytes[0..4].copy_from_slice(&item.value.to_le_bytes());
    bytes[4..8].copy_from_slice(&item.first.to_le_bytes());
    bytes[8..12].copy_from_slice(&item.second.to_le_bytes());

    bytes
}

/// A builder of the comparison's tables, as the binaries compare them
pub trait Builder: 'static {
    fn record(input: &Record<'_>) -> Result<Vec<u8>, Error>;
    fn document(input: &Document<'_>) -> Result<Vec<u8>, Error>;
    fn list(input: &List) -> Result<Vec<u8>, Error>;
    fn structs(input: &List) -> Result<Vec<u8>, Error>;
    fn builders(input: &List) -> Result<Vec<u8>, Error>;
    fn medium(input: &Medium) -> Result<Vec<u8>, Error>;
    fn nested(input: &Nesting) -> Result<Vec<u8>, Error>;
    fn arrays(input: &Arrays) -> Result<Vec<u8>, Error>;
    fn columns(input: &Columns) -> Result<Vec<u8>, Error>;
    fn by_reference(input: &Scalars) -> Result<Vec<u8>, Error>;
    fn owned(input: &Scalars) -> Result<Vec<u8>, Error>;
    fn uneven(name: &str) -> Result<Vec<u8>, BuildError>;
    fn too_long(name: &str) -> Result<Vec<u8>, BuildError>;
}

macro_rules! builder {
    ($name:ident, $module:ident) => {
        pub struct $name;

        impl Builder for $name {
            #[inline]
            fn record(input: &Record<'_>) -> Result<Vec<u8>, Error> {
                $module::record(input)
            }

            #[inline]
            fn document(input: &Document<'_>) -> Result<Vec<u8>, Error> {
                $module::document(input)
            }

            #[inline]
            fn list(input: &List) -> Result<Vec<u8>, Error> {
                $module::list(input)
            }

            #[inline]
            fn structs(input: &List) -> Result<Vec<u8>, Error> {
                $module::structs(input)
            }

            #[inline]
            fn builders(input: &List) -> Result<Vec<u8>, Error> {
                $module::builders(input)
            }

            #[inline]
            fn medium(input: &Medium) -> Result<Vec<u8>, Error> {
                $module::medium(input)
            }

            #[inline]
            fn nested(input: &Nesting) -> Result<Vec<u8>, Error> {
                $module::nested(input)
            }

            #[inline]
            fn arrays(input: &Arrays) -> Result<Vec<u8>, Error> {
                $module::arrays(input)
            }

            #[inline]
            fn columns(input: &Columns) -> Result<Vec<u8>, Error> {
                $module::columns(input)
            }

            #[inline]
            fn by_reference(input: &Scalars) -> Result<Vec<u8>, Error> {
                $module::by_reference(input)
            }

            #[inline]
            fn owned(input: &Scalars) -> Result<Vec<u8>, Error> {
                $module::owned(input)
            }

            #[inline]
            fn uneven(name: &str) -> Result<Vec<u8>, BuildError> {
                $module::uneven(name)
            }

            #[inline]
            fn too_long(name: &str) -> Result<Vec<u8>, BuildError> {
                $module::too_long(name)
            }
        }
    };
}

builder!(Main, main_serializers);
builder!(Rewrite, rewrite_serializers);
builder!(NoSegmentRoom, no_segment_room_serializers);
builder!(Segments128, segments_128_serializers);
builder!(Segments64, segments_64_serializers);
builder!(Segments64Twofold, segments_64_twofold_serializers);
builder!(Segments64Room16, segments_64_room_16_serializers);
builder!(Segments64Room16Twofold, segments_64_room_16_twofold_serializers);

/// Another builder's functions again, as functions of their own, for an
/// A/A pair
pub struct Again<T>(PhantomData<T>);

impl<T: Builder> Builder for Again<T> {
    #[inline]
    fn record(input: &Record<'_>) -> Result<Vec<u8>, Error> {
        T::record(input)
    }

    #[inline]
    fn document(input: &Document<'_>) -> Result<Vec<u8>, Error> {
        T::document(input)
    }

    #[inline]
    fn list(input: &List) -> Result<Vec<u8>, Error> {
        T::list(input)
    }

    #[inline]
    fn structs(input: &List) -> Result<Vec<u8>, Error> {
        T::structs(input)
    }

    #[inline]
    fn builders(input: &List) -> Result<Vec<u8>, Error> {
        T::builders(input)
    }

    #[inline]
    fn medium(input: &Medium) -> Result<Vec<u8>, Error> {
        T::medium(input)
    }

    #[inline]
    fn nested(input: &Nesting) -> Result<Vec<u8>, Error> {
        T::nested(input)
    }

    #[inline]
    fn arrays(input: &Arrays) -> Result<Vec<u8>, Error> {
        T::arrays(input)
    }

    #[inline]
    fn columns(input: &Columns) -> Result<Vec<u8>, Error> {
        T::columns(input)
    }

    #[inline]
    fn by_reference(input: &Scalars) -> Result<Vec<u8>, Error> {
        T::by_reference(input)
    }

    #[inline]
    fn owned(input: &Scalars) -> Result<Vec<u8>, Error> {
        T::owned(input)
    }

    #[inline]
    fn uneven(name: &str) -> Result<Vec<u8>, BuildError> {
        T::uneven(name)
    }

    #[inline]
    fn too_long(name: &str) -> Result<Vec<u8>, BuildError> {
        T::too_long(name)
    }
}

/// Something done with each builder that the binaries compare
pub trait Visit {
    fn visit<T: Builder>(&mut self, name: &'static str);
}

/// Visits main's builder and the rewrite, twice each, as A/A pairs, and
/// the rewrite's variants: without the room for the segments past the first
/// four, keeping runs of 128 or 64 bytes or more as segments, and with 64,
/// growing twofold, starting with room for 16 bytes a field rather than 64,
/// or both
pub fn for_each_builder(visitor: &mut impl Visit) {
    visitor.visit::<Main>("main");
    visitor.visit::<Again<Main>>("main A/A");
    visitor.visit::<Rewrite>("rewrite");
    visitor.visit::<Again<Rewrite>>("rewrite A/A");
    visitor.visit::<NoSegmentRoom>("no segment room");
    visitor.visit::<Segments128>("segments from 128");
    visitor.visit::<Segments64>("segments from 64");
    visitor.visit::<Segments64Twofold>("64, twofold");
    visitor.visit::<Segments64Room16>("64, room 16");
    visitor.visit::<Segments64Room16Twofold>("64, room 16, twofold");
}

/// The inputs of every row
pub struct Inputs {
    pub record: Record<'static>,
    pub small: Document<'static>,
    pub large: Document<'static>,
    pub list: List,
    pub medium: Medium,
    pub short: Medium,
    pub nestings: Vec<Nesting>,
    pub arrays: Vec<Arrays>,
    pub columns: Vec<Columns>,
    pub by_reference: Vec<Scalars>,
    pub owned: Vec<Scalars>,
}

impl Inputs {
    pub fn new() -> Self {
        Self {
            record: RECORD,
            small: workloads::document(100),
            large: workloads::document(100_000),
            list: workloads::list(LIST_ITEMS),
            medium: medium(),
            short: short(),
            nestings: [1, 10, 100, 1000].map(nesting).into(),
            arrays: [
                (9, 256),
                (16, 256),
                (32, 256),
                (4, 255),
                (8, 255),
                (100_000, 255),
                (4, 63),
                (8, 63),
                (100_000, 63),
                // About 16 KB and 1 MB
                (157, 100),
                (9_615, 100),
            ]
            .map(|(count, length)| arrays(count, length))
            .into(),
            columns: [5, 8, 16, 33].map(columns).into(),
            by_reference: [1, 4, 32].map(|fields| scalars(100_000, fields, false)).into(),
            owned: vec![scalars(100_000, 4, true)],
        }
    }
}

impl Default for Inputs {
    fn default() -> Self {
        Self::new()
    }
}

/// Something done with each row of a builder: its name, its input, and the
/// builder's function that writes it
pub trait Rows {
    fn row<I, F>(&mut self, name: &'static str, input: &'static I, write: F)
    where
        I: 'static,
        F: Fn(&I) -> Result<Vec<u8>, Error> + Copy + 'static;
}

/// Visits each row of builder `T`, in the same order for every builder
pub fn for_each_row<T: Builder>(inputs: &'static Inputs, rows: &mut impl Rows) {
    rows.row("record", &inputs.record, T::record);
    rows.row("document of 100 rows", &inputs.small, T::document);
    rows.row("document of 100,000 rows", &inputs.large, T::document);
    rows.row("vector of 100 nested tables", &inputs.list, T::list);
    rows.row("array of 100 structs", &inputs.list, T::structs);
    rows.row("vector of 100 nested tables from builders", &inputs.list, T::builders);
    rows.row("table of 32 medium values", &inputs.medium, T::medium);
    rows.row("table of 32 short values", &inputs.short, T::medium);
    for input in &inputs.nestings {
        rows.row(&input.name, input, T::nested);
    }
    for input in &inputs.arrays {
        rows.row(&input.name, input, T::arrays);
    }
    for input in &inputs.columns {
        rows.row(&input.name, input, T::columns);
    }
    for input in &inputs.by_reference {
        rows.row(&input.name, input, T::by_reference);
    }
    for input in &inputs.owned {
        rows.row(&input.name, input, T::owned);
    }
}
