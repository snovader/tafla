//! The tafla codec's serialization with three builders: main's
//! `TableBuilder`, which `setup.py` copies into the runtime as
//! `tafla::main_builder`, #217's rewrite, which writes each field when it's
//! set and keeps long values as segments, copied in as
//! `tafla::rewrite_builder`, and the runtime's own, which writes into one
//! buffer and hands it out. For the `builder_comparison` and
//! `builder_allocation` binaries, and the rows that they compare.
//!
//! Each function makes the codec's calls and is a function of its own, as
//! the codec's `serialize()` is compiled, so that every loop that calls it
//! times the same code, with the fills inlined into it. It gives the table
//! that it wrote to an [`Out`], which copies it when the harness checks the
//! bytes, and otherwise only keeps it from being optimized away. Main's
//! builder and #217's give it from `to_bytes()`. The runtime's gives it
//! three ways: from `into_bytes()`, the buffer as an owned `Vec`; from
//! `to_bytes()`, a copy; and from `finish()`, a view of the buffer of one
//! builder that the loop reuses for every table, with `reset()`.
//!
//! Beside the comparison's rows, it writes the vector of 100 nested tables
//! from a builder per table, rather than in place, tables of 32 medium and
//! 32 short values, the shapes of #217's second review, the strings
//! comparison's tables of 16 strings, the documents with their columns
//! padded, and the document of 100,000 rows into a buffer with room for it. `uneven()` and `too_long()` are the vectors
//! whose peak memory `builder_allocation` counts.

use std::hint::black_box;
use std::marker::PhantomData;

use tafla::{Alignment, BuildError, OffsetType, TableLayout};

use crate::codec::Error;
use crate::string_workloads::{COUNT, StringWorkload, Strings, string_workloads};
use crate::workloads::{self, Document, Item, LIST_ITEMS, List, RECORD, Record};

const DOCUMENT_VERSION: u32 = 1;
const DOCUMENT_LAYOUT: TableLayout<true> =
    TableLayout::DEFAULT.sized().with_version(DOCUMENT_VERSION);

const ITEM_SIZE: usize = 12;
const ITEM_ALIGNMENT: Alignment = Alignment::Four;

/// The pairs of values of the tables of medium and short values
const PAIRS: usize = 16;

/// Where a builder's function puts each table that it writes, and the
/// runtime's builder that the reusing functions write every table with
pub struct Out {
    pub sink: Sink,
    reused: Option<tafla::TableBuilder>,
}

impl Out {
    /// An output that only keeps the tables from being optimized away
    pub const fn timing() -> Self {
        Self {
            sink: Sink { kept: None },
            reused: None,
        }
    }

    /// An output that keeps a copy of the tables, one after another
    pub const fn checking() -> Self {
        Self {
            sink: Sink {
                kept: Some(Vec::new()),
            },
            reused: None,
        }
    }

    /// The copies of the tables kept so far, which it then forgets
    pub fn take_kept(&mut self) -> Vec<u8> {
        self.sink.kept.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// The runtime's builder of the reusing functions, started for a table
    /// of `fields` fields in `layout`, and the sink
    #[inline]
    pub fn reuse<const VERSIONED: bool>(
        &mut self,
        fields: usize,
        layout: TableLayout<VERSIONED>,
    ) -> Result<(&mut tafla::TableBuilder, &mut Sink), BuildError> {
        if let Some(builder) = &mut self.reused {
            builder.reset(fields, layout)?;
        } else {
            self.reused = Some(tafla::TableBuilder::new(fields, layout)?);
        }

        let builder = self.reused.as_mut().expect("started above");
        Ok((builder, &mut self.sink))
    }
}

/// Takes each table that a builder wrote
pub struct Sink {
    kept: Option<Vec<u8>>,
}

impl Sink {
    #[inline]
    pub fn take(&mut self, table: &[u8]) {
        match &mut self.kept {
            Some(kept) => kept.extend_from_slice(table),
            None => {
                black_box(table);
            }
        }
    }
}

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

/// A table of 16 strings of one of the strings comparison's shapes
pub struct StringsRow {
    pub name: String,
    pub strings: Strings<'static>,
}

/// The strings comparison's shapes that #376 timed
const STRING_SHAPES: [&str; 7] = [
    "empty",
    "8 chars of ASCII",
    "16 chars of ASCII",
    "64 chars of ASCII",
    "256 chars of ASCII",
    "1,024 chars of ASCII",
    "8 chars of 3 bytes",
];

fn strings_rows() -> Vec<StringsRow> {
    let workloads: &'static [StringWorkload] = string_workloads().leak();
    STRING_SHAPES
        .iter()
        .map(|&shape| {
            let workload = workloads
                .iter()
                .find(|workload| workload.name == shape)
                .expect("a shape of the strings comparison");
            StringsRow {
                name: format!("{COUNT} × {shape}"),
                strings: workload.input(),
            }
        })
        .collect()
}

/// The document of 100,000 rows, and the length of its table, which a
/// builder that takes a buffer is given room for
pub struct RoomyDocument {
    pub document: Document<'static>,
    pub length: usize,
}

/// The document with a name of 17 bytes rather than 16, so that its table
/// of columns, which follows the name, is padded by 3 bytes
fn padded(document: Document<'static>) -> Document<'static> {
    Document {
        name: "tafla-benchmarks!".into(),
        ..document
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

/// Starts the root table of a function: a builder of its own, or the
/// output's reused builder, with the sink that takes the table
macro_rules! root {
    (fresh, $out:ident, $builder:ident, $sink:ident, $fields:expr, $layout:expr) => {
        let mut $builder = TableBuilder::new($fields, $layout)?;
        let $sink = &mut $out.sink;
    };
    (reused, $out:ident, $builder:ident, $sink:ident, $fields:expr, $layout:expr) => {
        let ($builder, $sink) = $out.reuse($fields, $layout)?;
    };
    (roomy, $out:ident, $builder:ident, $sink:ident, $fields:expr, $layout:expr, $room:expr) => {
        let mut $builder = TableBuilder::with_buffer(Vec::with_capacity($room), $fields, $layout)?;
        let $sink = &mut $out.sink;
    };
}

/// Gives the root table to the sink: copied, as the builder's buffer, or
/// as a view of it
macro_rules! finish {
    (copy, $builder:ident, $sink:ident) => {
        $sink.take(&$builder.to_bytes()?)
    };
    (owned, $builder:ident, $sink:ident) => {
        $sink.take(&$builder.into_bytes()?)
    };
    (view, $builder:ident, $sink:ident) => {
        $sink.take($builder.finish()?)
    };
}

/// Writes a table nested in place in `$target`, `plain`ly, with
/// `set_table_with()`, or `declared` with the alignment that its fields
/// need, with the variant's `set_table_with_alignment()`
macro_rules! nest {
    (plain, $target:ident, $fields:expr, $alignment:expr, $fill:expr) => {
        $target.set_table_with($fields, $fill)
    };
    (declared, $target:ident, $fields:expr, $alignment:expr, $fill:expr) => {
        $target.set_table_with_alignment($fields, $alignment, $fill)
    };
}

/// Sets the document's fields on `$document`, a builder or a reference to
/// one
macro_rules! document_fields {
    ($nest:ident, $document:ident, $input:expr) => {
        $document.set_f64($input.x)?;
        $document.set_f64($input.y)?;
        $document.set_str($input.name.as_ref())?;
        nest!($nest, $document, 4, Alignment::Four, |columns| {
            columns.set_i64($input.base)?;
            columns.set_u32_array($input.keys.as_ref())?;
            columns.set_f32_array($input.a.as_ref())?;
            columns.set_f32_array($input.b.as_ref())
        })?;
    };
}

/// The functions of a builder that borrows its values, main's or #217's,
/// whose tables are copied out with `to_bytes()`
macro_rules! borrowing_serializers {
    ($module:ident, $($builder:ident)::+) => {
        pub mod $module {
            use super::*;
            use $($builder)::+::{InPlaceTable, TableBuilder};

            #[inline(never)]
            pub fn record(input: &Record<'_>, out: &mut Out) -> Result<(), Error> {
                root!(fresh, out, builder, sink, 4, TableLayout::DEFAULT);
                builder.set_f64(input.x)?;
                builder.set_f64(input.y)?;
                builder.set_i64(input.id)?;
                builder.set_str(input.name.as_ref())?;
                finish!(copy, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn document(input: &Document<'_>, out: &mut Out) -> Result<(), Error> {
                root!(fresh, out, document, sink, 4, DOCUMENT_LAYOUT);
                document.set_f64(input.x)?;
                document.set_f64(input.y)?;
                document.set_str(input.name.as_ref())?;
                document.set_table_with(4, |columns| {
                    columns.set_i64(input.base)?;
                    columns.set_u32_array(input.keys.as_ref())?;
                    columns.set_f32_array(input.a.as_ref())?;
                    columns.set_f32_array(input.b.as_ref())
                })?;
                finish!(copy, document, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn list(input: &List, out: &mut Out) -> Result<(), Error> {
                let items = &input.items;

                root!(fresh, out, builder, sink, 1, TableLayout::DEFAULT);
                builder.set_vector_with(items.len(), 2, |index, table| {
                    let item = &items[index];
                    table.set_f32(item.value)?;
                    table.set_table_with(2, |pair| {
                        pair.set_f32(item.first)?;
                        pair.set_f32(item.second)
                    })
                })?;
                finish!(copy, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn structs(input: &List, out: &mut Out) -> Result<(), Error> {
                let mut structs = Vec::with_capacity(input.items.len() * ITEM_SIZE);
                for item in &input.items {
                    structs.extend_from_slice(&item_struct(item));
                }

                root!(fresh, out, builder, sink, 1, TableLayout::DEFAULT);
                builder.set_struct_array(structs, ITEM_SIZE, ITEM_ALIGNMENT)?;
                finish!(copy, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn builders(input: &List, out: &mut Out) -> Result<(), Error> {
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

                root!(fresh, out, builder, sink, 1, TableLayout::DEFAULT);
                builder.set_vector(items)?;
                finish!(copy, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn medium(input: &Medium, out: &mut Out) -> Result<(), Error> {
                root!(fresh, out, builder, sink, 2 * PAIRS, TableLayout::DEFAULT);
                for (string, array) in input.strings.iter().zip(&input.arrays) {
                    builder.set_str(string.as_str())?;
                    builder.set_f32_array(array.as_slice())?;
                }
                finish!(copy, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn strings(input: &StringsRow, out: &mut Out) -> Result<(), Error> {
                root!(fresh, out, builder, sink, COUNT, TableLayout::DEFAULT);
                for string in &input.strings.items {
                    builder.set_str(string.as_ref())?;
                }
                finish!(copy, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn nested(input: &Nesting, out: &mut Out) -> Result<(), Error> {
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

                root!(fresh, out, builder, sink, 1, TableLayout::DEFAULT);
                builder.set_table_with(2, |table| level(table, input.depth, input))?;
                finish!(copy, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn arrays(input: &Arrays, out: &mut Out) -> Result<(), Error> {
                root!(fresh, out, builder, sink, input.count, TableLayout::DEFAULT);
                for _ in 0..input.count {
                    builder.set_u8_array(input.value.as_slice())?;
                }
                finish!(copy, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn columns(input: &Columns, out: &mut Out) -> Result<(), Error> {
                root!(fresh, out, builder, sink, input.count, TableLayout::DEFAULT);
                for _ in 0..input.count {
                    builder.set_f64_array(input.column.as_slice())?;
                }
                finish!(copy, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn by_reference(input: &Scalars, out: &mut Out) -> Result<(), Error> {
                let mut builders = Vec::with_capacity(input.builders);
                for index in 0..input.builders {
                    builders.push(scalars(index, input.fields)?);
                }

                root!(fresh, out, builder, sink, 1, TableLayout::DEFAULT);
                builder.set_vector(builders.iter())?;
                finish!(copy, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn owned(input: &Scalars, out: &mut Out) -> Result<(), Error> {
                let mut builders = Vec::with_capacity(input.builders);
                for index in 0..input.builders {
                    builders.push(scalars(index, input.fields)?);
                }

                root!(fresh, out, builder, sink, 1, TableLayout::DEFAULT);
                builder.set_vector(builders)?;
                finish!(copy, builder, sink);

                Ok(())
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

/// The functions of the runtime's builder, which copies its values, or of
/// a variant of it at `$path`, whose root tables are started `$start`
/// (`fresh` or `reused`) and given to the sink as `$finish` says (`copy`,
/// `owned` or `view`), and whose tables in place are nested as `$nest` says.
/// Their bytes are the borrowing builders'.
macro_rules! copying_serializers {
    ($module:ident, $($path:ident)::+, $start:ident, $finish:ident, $nest:ident) => {
        pub mod $module {
            use super::*;
            use $($path)::+::{InPlaceTable, TableBuilder};

            #[inline(never)]
            pub fn record(input: &Record<'_>, out: &mut Out) -> Result<(), Error> {
                root!($start, out, builder, sink, 4, TableLayout::DEFAULT);
                builder.set_f64(input.x)?;
                builder.set_f64(input.y)?;
                builder.set_i64(input.id)?;
                builder.set_str(input.name.as_ref())?;
                finish!($finish, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn document(input: &Document<'_>, out: &mut Out) -> Result<(), Error> {
                root!($start, out, document, sink, 4, DOCUMENT_LAYOUT);
                document_fields!($nest, document, input);
                finish!($finish, document, sink);

                Ok(())
            }

            /// The document into a buffer with room for it, where the
            /// builder starts one of its own
            #[inline(never)]
            pub fn roomy_document(input: &RoomyDocument, out: &mut Out) -> Result<(), Error> {
                copying_serializers!(@roomy $start, out, document, sink, input.length);
                document_fields!($nest, document, input.document);
                finish!($finish, document, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn list(input: &List, out: &mut Out) -> Result<(), Error> {
                let items = &input.items;

                root!($start, out, builder, sink, 1, TableLayout::DEFAULT);
                builder.set_vector_with(items.len(), 2, |index, table| {
                    let item = &items[index];
                    table.set_f32(item.value)?;
                    nest!($nest, table, 2, Alignment::One, |pair| {
                        pair.set_f32(item.first)?;
                        pair.set_f32(item.second)
                    })
                })?;
                finish!($finish, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn structs(input: &List, out: &mut Out) -> Result<(), Error> {
                let mut structs = Vec::with_capacity(input.items.len() * ITEM_SIZE);
                for item in &input.items {
                    structs.extend_from_slice(&item_struct(item));
                }

                root!($start, out, builder, sink, 1, TableLayout::DEFAULT);
                builder.set_struct_array(structs, ITEM_SIZE, ITEM_ALIGNMENT)?;
                finish!($finish, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn builders(input: &List, out: &mut Out) -> Result<(), Error> {
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

                root!($start, out, builder, sink, 1, TableLayout::DEFAULT);
                builder.set_vector(items)?;
                finish!($finish, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn medium(input: &Medium, out: &mut Out) -> Result<(), Error> {
                root!($start, out, builder, sink, 2 * PAIRS, TableLayout::DEFAULT);
                for (string, array) in input.strings.iter().zip(&input.arrays) {
                    builder.set_str(string.as_str())?;
                    builder.set_f32_array(array.as_slice())?;
                }
                finish!($finish, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn strings(input: &StringsRow, out: &mut Out) -> Result<(), Error> {
                root!($start, out, builder, sink, COUNT, TableLayout::DEFAULT);
                for string in &input.strings.items {
                    builder.set_str(string.as_ref())?;
                }
                finish!($finish, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn nested(input: &Nesting, out: &mut Out) -> Result<(), Error> {
                fn level(
                    table: &mut InPlaceTable<'_>,
                    depth: usize,
                    input: &Nesting,
                ) -> Result<(), BuildError> {
                    table.set_u8(1)?;
                    if depth > 0 {
                        return nest!($nest, table, 2, Alignment::Eight, |inner| {
                            level(inner, depth - 1, input)
                        });
                    }

                    nest!($nest, table, 1 + NESTED_ARRAYS, Alignment::Eight, |inner| {
                        inner.set_f64_array(input.column.as_slice())?;
                        for _ in 0..NESTED_ARRAYS {
                            inner.set_u8_array(input.small.as_slice())?;
                        }
                        Ok(())
                    })
                }

                root!($start, out, builder, sink, 1, TableLayout::DEFAULT);
                nest!($nest, builder, 2, Alignment::Eight, |table| {
                    level(table, input.depth, input)
                })?;
                finish!($finish, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn arrays(input: &Arrays, out: &mut Out) -> Result<(), Error> {
                root!($start, out, builder, sink, input.count, TableLayout::DEFAULT);
                for _ in 0..input.count {
                    builder.set_u8_array(input.value.as_slice())?;
                }
                finish!($finish, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn columns(input: &Columns, out: &mut Out) -> Result<(), Error> {
                root!($start, out, builder, sink, input.count, TableLayout::DEFAULT);
                for _ in 0..input.count {
                    builder.set_f64_array(input.column.as_slice())?;
                }
                finish!($finish, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn by_reference(input: &Scalars, out: &mut Out) -> Result<(), Error> {
                let mut builders = Vec::with_capacity(input.builders);
                for index in 0..input.builders {
                    builders.push(scalars(index, input.fields)?);
                }

                root!($start, out, builder, sink, 1, TableLayout::DEFAULT);
                builder.set_vector(builders.iter())?;
                finish!($finish, builder, sink);

                Ok(())
            }

            #[inline(never)]
            pub fn owned(input: &Scalars, out: &mut Out) -> Result<(), Error> {
                let mut builders = Vec::with_capacity(input.builders);
                for index in 0..input.builders {
                    builders.push(scalars(index, input.fields)?);
                }

                root!($start, out, builder, sink, 1, TableLayout::DEFAULT);
                builder.set_vector(builders)?;
                finish!($finish, builder, sink);

                Ok(())
            }

            #[allow(clippy::cast_possible_truncation)]
            #[inline]
            fn scalars(index: usize, fields: usize) -> Result<TableBuilder, BuildError> {
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

                builder.into_bytes()
            }

            #[inline(never)]
            pub fn too_long(name: &str) -> Result<Vec<u8>, BuildError> {
                let layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint16);
                let mut builder = TableBuilder::new(1, layout)?;
                builder.set_vector_with(TOO_LONG_ITEMS, 1, |_, item| item.set_str(name))?;

                builder.into_bytes()
            }
        }
    };
    // A fresh builder of the document takes a buffer with room for it, and
    // the reused builder has the room of the tables before
    (@roomy fresh, $out:ident, $builder:ident, $sink:ident, $room:expr) => {
        root!(roomy, $out, $builder, $sink, 4, DOCUMENT_LAYOUT, $room);
    };
    (@roomy reused, $out:ident, $builder:ident, $sink:ident, $room:expr) => {
        root!(reused, $out, $builder, $sink, 4, DOCUMENT_LAYOUT);
    };
}

borrowing_serializers!(main_serializers, tafla::main_builder);
borrowing_serializers!(rewrite_serializers, tafla::rewrite_builder);
copying_serializers!(owned_serializers, tafla, fresh, owned, plain);
copying_serializers!(copied_serializers, tafla, fresh, copy, plain);
copying_serializers!(reused_serializers, tafla, reused, view, plain);
copying_serializers!(early_serializers, tafla::early_builder, fresh, owned, plain);
copying_serializers!(declared_serializers, tafla::declared_builder, fresh, owned, declared);
copying_serializers!(first_1k_serializers, tafla::first_1k, fresh, owned, plain);
copying_serializers!(first_4k_serializers, tafla::first_4k, fresh, owned, plain);

fn item_struct(item: &Item) -> [u8; ITEM_SIZE] {
    let mut bytes = [0; ITEM_SIZE];
    bytes[0..4].copy_from_slice(&item.value.to_le_bytes());
    bytes[4..8].copy_from_slice(&item.first.to_le_bytes());
    bytes[8..12].copy_from_slice(&item.second.to_le_bytes());

    bytes
}

/// A builder of the comparison's tables, as the binaries compare them
pub trait Builder: 'static {
    fn record(input: &Record<'_>, out: &mut Out) -> Result<(), Error>;
    fn document(input: &Document<'_>, out: &mut Out) -> Result<(), Error>;
    fn roomy_document(input: &RoomyDocument, out: &mut Out) -> Result<(), Error>;
    fn list(input: &List, out: &mut Out) -> Result<(), Error>;
    fn structs(input: &List, out: &mut Out) -> Result<(), Error>;
    fn builders(input: &List, out: &mut Out) -> Result<(), Error>;
    fn medium(input: &Medium, out: &mut Out) -> Result<(), Error>;
    fn strings(input: &StringsRow, out: &mut Out) -> Result<(), Error>;
    fn nested(input: &Nesting, out: &mut Out) -> Result<(), Error>;
    fn arrays(input: &Arrays, out: &mut Out) -> Result<(), Error>;
    fn columns(input: &Columns, out: &mut Out) -> Result<(), Error>;
    fn by_reference(input: &Scalars, out: &mut Out) -> Result<(), Error>;
    fn owned(input: &Scalars, out: &mut Out) -> Result<(), Error>;
    fn uneven(name: &str) -> Result<Vec<u8>, BuildError>;
    fn too_long(name: &str) -> Result<Vec<u8>, BuildError>;
}

macro_rules! builder {
    ($name:ident, $module:ident, roomy: $roomy:path) => {
        pub struct $name;

        impl Builder for $name {
            #[inline]
            fn record(input: &Record<'_>, out: &mut Out) -> Result<(), Error> {
                $module::record(input, out)
            }

            #[inline]
            fn document(input: &Document<'_>, out: &mut Out) -> Result<(), Error> {
                $module::document(input, out)
            }

            #[inline]
            fn roomy_document(input: &RoomyDocument, out: &mut Out) -> Result<(), Error> {
                $roomy(input, out)
            }

            #[inline]
            fn list(input: &List, out: &mut Out) -> Result<(), Error> {
                $module::list(input, out)
            }

            #[inline]
            fn structs(input: &List, out: &mut Out) -> Result<(), Error> {
                $module::structs(input, out)
            }

            #[inline]
            fn builders(input: &List, out: &mut Out) -> Result<(), Error> {
                $module::builders(input, out)
            }

            #[inline]
            fn medium(input: &Medium, out: &mut Out) -> Result<(), Error> {
                $module::medium(input, out)
            }

            #[inline]
            fn strings(input: &StringsRow, out: &mut Out) -> Result<(), Error> {
                $module::strings(input, out)
            }

            #[inline]
            fn nested(input: &Nesting, out: &mut Out) -> Result<(), Error> {
                $module::nested(input, out)
            }

            #[inline]
            fn arrays(input: &Arrays, out: &mut Out) -> Result<(), Error> {
                $module::arrays(input, out)
            }

            #[inline]
            fn columns(input: &Columns, out: &mut Out) -> Result<(), Error> {
                $module::columns(input, out)
            }

            #[inline]
            fn by_reference(input: &Scalars, out: &mut Out) -> Result<(), Error> {
                $module::by_reference(input, out)
            }

            #[inline]
            fn owned(input: &Scalars, out: &mut Out) -> Result<(), Error> {
                $module::owned(input, out)
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

/// The document of 100,000 rows of a builder that takes no buffer, which
/// it writes as the document of any length
fn roomless<B: Builder>(input: &RoomyDocument, out: &mut Out) -> Result<(), Error> {
    B::document(&input.document, out)
}

builder!(Main, main_serializers, roomy: roomless::<Main>);
builder!(Rewrite, rewrite_serializers, roomy: roomless::<Rewrite>);
builder!(Owned, owned_serializers, roomy: owned_serializers::roomy_document);
builder!(Copied, copied_serializers, roomy: copied_serializers::roomy_document);
builder!(Reused, reused_serializers, roomy: reused_serializers::roomy_document);
builder!(Early, early_serializers, roomy: early_serializers::roomy_document);
builder!(Declared, declared_serializers, roomy: declared_serializers::roomy_document);
builder!(First1K, first_1k_serializers, roomy: first_1k_serializers::roomy_document);
builder!(First4K, first_4k_serializers, roomy: first_4k_serializers::roomy_document);

/// Another builder's functions again, as functions of their own, for an
/// A/A pair
pub struct Again<T>(PhantomData<T>);

impl<T: Builder> Builder for Again<T> {
    #[inline]
    fn record(input: &Record<'_>, out: &mut Out) -> Result<(), Error> {
        T::record(input, out)
    }

    #[inline]
    fn document(input: &Document<'_>, out: &mut Out) -> Result<(), Error> {
        T::document(input, out)
    }

    #[inline]
    fn roomy_document(input: &RoomyDocument, out: &mut Out) -> Result<(), Error> {
        T::roomy_document(input, out)
    }

    #[inline]
    fn list(input: &List, out: &mut Out) -> Result<(), Error> {
        T::list(input, out)
    }

    #[inline]
    fn structs(input: &List, out: &mut Out) -> Result<(), Error> {
        T::structs(input, out)
    }

    #[inline]
    fn builders(input: &List, out: &mut Out) -> Result<(), Error> {
        T::builders(input, out)
    }

    #[inline]
    fn medium(input: &Medium, out: &mut Out) -> Result<(), Error> {
        T::medium(input, out)
    }

    #[inline]
    fn strings(input: &StringsRow, out: &mut Out) -> Result<(), Error> {
        T::strings(input, out)
    }

    #[inline]
    fn nested(input: &Nesting, out: &mut Out) -> Result<(), Error> {
        T::nested(input, out)
    }

    #[inline]
    fn arrays(input: &Arrays, out: &mut Out) -> Result<(), Error> {
        T::arrays(input, out)
    }

    #[inline]
    fn columns(input: &Columns, out: &mut Out) -> Result<(), Error> {
        T::columns(input, out)
    }

    #[inline]
    fn by_reference(input: &Scalars, out: &mut Out) -> Result<(), Error> {
        T::by_reference(input, out)
    }

    #[inline]
    fn owned(input: &Scalars, out: &mut Out) -> Result<(), Error> {
        T::owned(input, out)
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

/// Visits main's builder, #217's, and the runtime's three ways: its
/// buffer taken as an owned `Vec`, a copy from `to_bytes()`, and a view of
/// the buffer of one builder reused for every table. Main's and the
/// runtime's owned buffer run twice, as A/A pairs. Then two variants of the
/// runtime's, each with its buffer taken as an owned `Vec`, which pad a
/// table nested in place without moving what it holds: one that moves what
/// it has written so far as soon as a field raises its alignment, and the
/// tables in place that hold it, and one that's given the alignment that
/// its fields need when it's opened. Last, the runtime's with its buffer
/// starting with room for at least 1 or 4 KiB rather than 256 bytes.
pub fn for_each_builder(visitor: &mut impl Visit) {
    visitor.visit::<Main>("main");
    visitor.visit::<Again<Main>>("main A/A");
    visitor.visit::<Rewrite>("#217");
    visitor.visit::<Owned>("buffer, into_bytes()");
    visitor.visit::<Again<Owned>>("buffer A/A");
    visitor.visit::<Copied>("buffer, to_bytes()");
    visitor.visit::<Reused>("buffer, reused, view");
    visitor.visit::<Early>("early moves");
    visitor.visit::<Declared>("declared alignment");
    visitor.visit::<First1K>("first 1 KiB");
    visitor.visit::<First4K>("first 4 KiB");
}

/// The inputs of every row
pub struct Inputs {
    pub record: Record<'static>,
    pub small: Document<'static>,
    pub large: Document<'static>,
    pub padded_small: Document<'static>,
    pub padded_large: Document<'static>,
    pub roomy: RoomyDocument,
    pub list: List,
    pub medium: Medium,
    pub short: Medium,
    pub strings: Vec<StringsRow>,
    pub nestings: Vec<Nesting>,
    pub arrays: Vec<Arrays>,
    pub columns: Vec<Columns>,
    pub by_reference: Vec<Scalars>,
    pub owned: Vec<Scalars>,
}

impl Inputs {
    pub fn new() -> Self {
        let large = workloads::document(100_000);
        let mut out = Out::checking();
        main_serializers::document(&large, &mut out).expect("main's builder writes the document");
        let roomy = RoomyDocument {
            document: workloads::document(100_000),
            length: out.take_kept().len(),
        };

        Self {
            record: RECORD,
            small: workloads::document(100),
            large,
            padded_small: padded(workloads::document(100)),
            padded_large: padded(workloads::document(100_000)),
            roomy,
            list: workloads::list(LIST_ITEMS),
            medium: medium(),
            short: short(),
            strings: strings_rows(),
            nestings: [1, 10, 100, 1000].map(nesting).into(),
            arrays: [
                (9, 256),
                (16, 256),
                (32, 256),
                (4, 255),
                (8, 255),
                (100_000, 255),
                (4, 127),
                (8, 127),
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
/// builder's function that writes it to an output, which is a type of its
/// own, so that a loop that calls it is compiled for it alone
pub trait Rows {
    fn row<I, F>(&mut self, name: &'static str, input: &'static I, write: F)
    where
        I: 'static,
        F: Fn(&I, &mut Out) -> Result<(), Error> + Copy + 'static;
}

/// Visits each row of builder `T`, in the same order for every builder
pub fn for_each_row<T: Builder>(inputs: &'static Inputs, rows: &mut impl Rows) {
    rows.row("record", &inputs.record, T::record);
    rows.row("document of 100 rows", &inputs.small, T::document);
    rows.row("document of 100,000 rows", &inputs.large, T::document);
    rows.row("document of 100 rows, its columns padded", &inputs.padded_small, T::document);
    rows.row(
        "document of 100,000 rows, its columns padded",
        &inputs.padded_large,
        T::document,
    );
    rows.row(
        "document of 100,000 rows, into a buffer with room for it",
        &inputs.roomy,
        T::roomy_document,
    );
    rows.row("vector of 100 nested tables", &inputs.list, T::list);
    rows.row("array of 100 structs", &inputs.list, T::structs);
    rows.row("vector of 100 nested tables from builders", &inputs.list, T::builders);
    rows.row("table of 32 medium values", &inputs.medium, T::medium);
    rows.row("table of 32 short values", &inputs.short, T::medium);
    for input in &inputs.strings {
        rows.row(&input.name, input, T::strings);
    }
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
