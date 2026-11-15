//! The tafla codec's serialization with main's `TableBuilder`, which
//! `setup.py` copies into the runtime as `tafla::main_builder`, with the
//! rewrite, and with variants of the rewrite, for the `builder_comparison`
//! and `builder_allocation` binaries.
//!
//! Each function makes the codec's calls and is a function of its own, as
//! the codec's `serialize()` is compiled, so that every loop that calls it
//! times the same code, with the fills inlined into it. Beside the
//! comparison's rows, it writes the vector of 100 nested tables from a
//! builder per table, rather than in place, and tables of 32 medium and 32
//! short values, shorter than the 256 bytes from which the rewrite keeps a
//! value rather than copying it. `uneven()` and `too_long()` are the vectors whose peak
//! memory `builder_allocation` counts.

use std::marker::PhantomData;

use tafla::{Alignment, BuildError, OffsetType, TableLayout};

use crate::codec::Error;
use crate::workloads::{Document, Item, List, Record};

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
            use $($builder)::+::TableBuilder;

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
serializers!(no_reservation_serializers, tafla::rewrite_no_reservation);
serializers!(boxed_serializers, tafla::rewrite_boxed);
serializers!(segments_128_serializers, tafla::rewrite_segments_128);
serializers!(segments_64_serializers, tafla::rewrite_segments_64);
serializers!(room_64_serializers, tafla::rewrite_room_64);
serializers!(room_128_serializers, tafla::rewrite_room_128);
serializers!(growth_4_serializers, tafla::rewrite_growth_4);

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
builder!(NoReservation, no_reservation_serializers);
builder!(Boxed, boxed_serializers);
builder!(Segments128, segments_128_serializers);
builder!(Segments64, segments_64_serializers);
builder!(Room64, room_64_serializers);
builder!(Room128, room_128_serializers);
builder!(Growth4, growth_4_serializers);

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
/// the rewrite's variants: without the room that a vector in place makes
/// for its items after the first, with an owned builder boxed in a
/// `NestedTable`, keeping runs of 128 or 64 bytes or more as segments,
/// starting with room for 64 or 128 bytes a field, and growing fourfold
pub fn for_each_builder(visitor: &mut impl Visit) {
    visitor.visit::<Main>("main");
    visitor.visit::<Again<Main>>("main A/A");
    visitor.visit::<Rewrite>("rewrite");
    visitor.visit::<Again<Rewrite>>("rewrite A/A");
    visitor.visit::<NoReservation>("no reservation");
    visitor.visit::<Boxed>("boxed");
    visitor.visit::<Segments128>("segments from 128");
    visitor.visit::<Segments64>("segments from 64");
    visitor.visit::<Room64>("room 64 a field");
    visitor.visit::<Room128>("room 128 a field");
    visitor.visit::<Growth4>("growth 4x");
}
