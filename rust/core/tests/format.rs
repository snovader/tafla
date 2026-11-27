//! Tables that `TableBuilder` writes follow the format's rules, as `SPEC.md`
//! at the root of the repository states them: the header holds the length
//! and version, offsets lie within the table, values follow each other in
//! field order with only the padding that aligns arrays and nested tables,
//! multi-byte values are little-endian, nested tables and vector items hold
//! the bytes of their tables, and vectors of strings hold the end of each
//! string and then the strings' UTF-8, one after another.

mod common;

use common::{
    description::{Field, Nested, Source, Table, struct_alignment, struct_bytes},
    strategy,
};
use proptest::prelude::*;
use tafla::Alignment;

proptest! {
    #![proptest_config(common::config())]

    #[test]
    fn test_sized_tables_hold_their_length_in_the_header(table in strategy::table()) {
        let bytes = table.to_bytes().unwrap();

        for found in tables_in(&bytes, &table) {
            let layout = found.table.layout;
            if layout.sized {
                let width = layout.offset_type.size();
                prop_assert_eq!(&found.bytes[..width], slot_bytes(found.bytes.len(), width));
            }
        }
    }

    #[test]
    fn test_versioned_tables_hold_their_version_in_the_header(table in strategy::table()) {
        let bytes = table.to_bytes().unwrap();

        for found in tables_in(&bytes, &table) {
            let layout = found.table.layout;
            if let Some(version) = found.table.version {
                let width = layout.offset_type.size();
                let position = if layout.sized { width } else { 0 };
                let version = usize::try_from(version).unwrap();

                let slot = &found.bytes[position..position + width];
                prop_assert_eq!(slot, slot_bytes(version, width));
            }
        }
    }

    /// Every offset of every table lies after its header and offsets, and
    /// its field ends within the table. A table nested with no fields is
    /// empty, and can end its parent.
    #[test]
    fn test_offsets_lie_within_their_table(table in strategy::table()) {
        let bytes = table.to_bytes().unwrap();

        for found in tables_in(&bytes, &table) {
            let offsets_end = found.offsets_end();

            for (index, field) in found.fields.iter().enumerate() {
                let end = field.position + field.length;
                prop_assert!(
                    offsets_end <= field.position && end <= found.bytes.len(),
                    "Field {} at {} of {} bytes, in a table of {} bytes with offsets to {}",
                    index, field.position, field.length, found.bytes.len(), offsets_end
                );
            }
        }
    }

    /// From the start of the table that holds them, which is aligned in
    /// turn when it's nested: an array of scalars at a multiple of their
    /// size, and an array of structs of their largest scalar's
    #[test]
    fn test_arrays_start_at_a_multiple_of_their_alignment(table in strategy::table()) {
        let bytes = table.to_bytes().unwrap();

        for found in tables_in(&bytes, &table) {
            let width = found.table.layout.offset_type.size();

            for field in &found.fields {
                if let Some(Value::Array { alignment, .. }) = Value::of(field.field) {
                    let start = field.position + width;
                    prop_assert_eq!(start % alignment, 0, "{:?} at {}", field.field, start);
                }
            }
        }
    }

    /// Scalars hold their values' `to_le_bytes()`, structs their scalars',
    /// arrays and strings their counts' and their elements', and vectors
    /// their counts' and their items' ends', which count from the vector's
    /// position, after the ends. Vectors of strings hold their counts' and
    /// their strings' ends', and then the strings' UTF-8 without padding.
    #[test]
    fn test_values_are_little_endian(table in strategy::table()) {
        let bytes = table.to_bytes().unwrap();

        for found in tables_in(&bytes, &table) {
            let width = found.table.layout.offset_type.size();

            for field in &found.fields {
                let value = &found.bytes[field.position..field.position + field.length];

                match (field.field, Value::of(field.field)) {
                    (_, Some(Value::Scalar(scalar))) => prop_assert_eq!(value, scalar),
                    (_, Some(Value::Array { count, elements, .. })) => {
                        prop_assert_eq!(&value[..width], slot_bytes(count, width));
                        prop_assert_eq!(&value[width..], elements);
                    }
                    (Field::Vector(items), None) => {
                        prop_assert_eq!(&value[..width], slot_bytes(items.len(), width));

                        let mut end = (items.len() + 1) * width;
                        for (index, item) in items.iter().enumerate() {
                            end += item.table.to_bytes().unwrap().len();

                            let position = (index + 1) * width;
                            let slot = &value[position..position + width];
                            prop_assert_eq!(slot, slot_bytes(end, width));
                        }
                    }
                    (Field::StringVector(strings), None) => {
                        prop_assert_eq!(&value[..width], slot_bytes(strings.len(), width));

                        let start = (strings.len() + 1) * width;
                        let mut end = start;
                        for (index, string) in strings.iter().enumerate() {
                            end += string.len();

                            let position = (index + 1) * width;
                            let slot = &value[position..position + width];
                            prop_assert_eq!(slot, slot_bytes(end, width));
                        }
                        let utf8 = strings.concat();
                        prop_assert_eq!(&value[start..], utf8.as_bytes());
                    }
                    _ => {}
                }
            }
        }
    }

    /// Values follow the header and offsets in field order, each after the
    /// previous one, with only the zeroed padding that aligns an array's
    /// elements and a nested table to their alignment between them, and the
    /// last one ends the table
    #[test]
    fn test_values_follow_each_other_with_only_zeroed_padding_to_align_them(
        table in strategy::table(),
    ) {
        let bytes = table.to_bytes().unwrap();

        for found in tables_in(&bytes, &table) {
            let width = found.table.layout.offset_type.size();
            let mut end = found.offsets_end();

            for field in &found.fields {
                let position = end + padding(field.field, end, width);
                prop_assert_eq!(field.position, position, "{:?}", field.field);
                prop_assert!(found.bytes[end..position].iter().all(|&byte| byte == 0));

                end = position + field.length;
            }

            prop_assert_eq!(end, found.bytes.len());
        }
    }

    /// The bytes that a builder writes are the same nested in another table
    /// as on their own
    #[test]
    fn test_nested_tables_and_vector_items_hold_the_bytes_of_their_tables(
        table in strategy::table(),
    ) {
        let bytes = table.to_bytes().unwrap();

        for found in tables_in(&bytes, &table) {
            if let Some(own_bytes) = &found.own_bytes {
                prop_assert_eq!(found.bytes, own_bytes);
            }
        }
    }
}

/// A table in the bytes of a table: the table itself, a table nested in it
/// or one of its vector items, or a table in one of those
struct TableIn<'a> {
    bytes: &'a [u8],
    table: &'a Table,
    fields: Vec<FieldIn<'a>>,
    /// The bytes that the builder of a table in another writes on its own
    own_bytes: Option<Vec<u8>>,
}

/// A field and where it lies in its table's bytes
struct FieldIn<'a> {
    field: &'a Field,
    position: usize,
    length: usize,
}

impl TableIn<'_> {
    /// The length of the header and offsets, where the values start
    const fn offsets_end(&self) -> usize {
        let layout = self.table.layout;
        layout.header_size() + self.fields.len() * layout.offset_type.size()
    }
}

/// Finds every table in `bytes`, which hold `table`, and the fields of each.
/// Fields lie at the positions that their offsets hold, and are as long as
/// the values that the description gives, and as the tables that their
/// builders write on their own.
fn tables_in<'a>(bytes: &'a [u8], table: &'a Table) -> Vec<TableIn<'a>> {
    let mut tables = Vec::new();
    find_tables(bytes, table, None, &mut tables);

    tables
}

fn find_tables<'a>(
    bytes: &'a [u8],
    table: &'a Table,
    own_bytes: Option<Vec<u8>>,
    tables: &mut Vec<TableIn<'a>>,
) {
    let width = table.layout.offset_type.size();
    let offsets = table.layout.header_size();
    let mut fields = Vec::with_capacity(table.fields.len());

    for (index, field) in table.fields.iter().enumerate() {
        let position = slot(bytes, offsets + index * width, width);
        let length = match field {
            Field::Table { nested, .. } => find_nested(bytes, position, nested, tables),
            Field::Vector(items) => {
                // The items follow the count and the ends
                let mut end = position + (items.len() + 1) * width;
                for item in items {
                    end += find_nested(bytes, end, item, tables);
                }

                end - position
            }
            // The count, the ends and the strings
            Field::StringVector(strings) => {
                (strings.len() + 1) * width + strings.iter().map(String::len).sum::<usize>()
            }
            value => Value::of(value).unwrap().byte_length(width),
        };

        fields.push(FieldIn {
            field,
            position,
            length,
        });
    }

    tables.push(TableIn {
        bytes,
        table,
        fields,
        own_bytes,
    });
}

/// Finds a nested table or vector item at `position`, and the tables in it,
/// and returns its length
fn find_nested<'a>(
    bytes: &'a [u8],
    position: usize,
    nested: &'a Nested,
    tables: &mut Vec<TableIn<'a>>,
) -> usize {
    let own_bytes = nested.table.to_bytes().unwrap();
    let length = own_bytes.len();

    let nested_bytes = bytes.get(position..position + length).unwrap_or_else(|| {
        panic!(
            "A table of {length} bytes at {position} leaves its parent's {}",
            bytes.len()
        )
    });
    find_tables(nested_bytes, &nested.table, Some(own_bytes), tables);

    length
}

/// The value of a scalar, array, string or struct field
enum Value {
    /// The little-endian bytes of a scalar, or of a struct's scalars
    Scalar(Vec<u8>),
    /// The count of an array's elements, of a string's bytes or of an
    /// array's structs, and their little-endian bytes, which start at a
    /// multiple of `alignment` from the table's start
    Array {
        alignment: usize,
        count: usize,
        elements: Vec<u8>,
    },
}

impl Value {
    /// Returns the value of a field that holds one rather than tables
    fn of(field: &Field) -> Option<Self> {
        Some(match field {
            Field::I8(value) => Self::scalar(value.to_le_bytes()),
            Field::U8(value) => Self::scalar(value.to_le_bytes()),
            Field::I16(value) => Self::scalar(value.to_le_bytes()),
            Field::U16(value) => Self::scalar(value.to_le_bytes()),
            Field::I32(value) => Self::scalar(value.to_le_bytes()),
            Field::U32(value) => Self::scalar(value.to_le_bytes()),
            Field::I64(value) => Self::scalar(value.to_le_bytes()),
            Field::U64(value) => Self::scalar(value.to_le_bytes()),
            Field::F32(value) => Self::scalar(value.to_le_bytes()),
            Field::F64(value) => Self::scalar(value.to_le_bytes()),
            Field::Bool(value) => Self::scalar([u8::from(*value)]),
            Field::I8Array(values) => Self::array(values, i8::to_le_bytes),
            Field::U8Array(values) => Self::array(values, u8::to_le_bytes),
            Field::I16Array(values) => Self::array(values, i16::to_le_bytes),
            Field::U16Array(values) => Self::array(values, u16::to_le_bytes),
            Field::I32Array(values) => Self::array(values, i32::to_le_bytes),
            Field::U32Array(values) => Self::array(values, u32::to_le_bytes),
            Field::I64Array(values) => Self::array(values, i64::to_le_bytes),
            Field::U64Array(values) => Self::array(values, u64::to_le_bytes),
            Field::F32Array(values) => Self::array(values, f32::to_le_bytes),
            Field::F64Array(values) => Self::array(values, f64::to_le_bytes),
            Field::String(string) => Self::array(string.as_bytes(), u8::to_le_bytes),
            Field::Struct(scalars) => Self::Scalar(struct_bytes(scalars)),
            Field::StructArray { types, items } => Self::Array {
                alignment: struct_alignment(types.iter().copied()).bytes(),
                count: items.len(),
                elements: items
                    .iter()
                    .flat_map(|scalars| struct_bytes(scalars))
                    .collect(),
            },
            Field::Table { .. } | Field::Vector(_) | Field::StringVector(_) => return None,
        })
    }

    fn scalar<const N: usize>(bytes: [u8; N]) -> Self {
        Self::Scalar(bytes.to_vec())
    }

    fn array<T: Copy, const N: usize>(values: &[T], to_le_bytes: fn(T) -> [u8; N]) -> Self {
        Self::Array {
            alignment: N,
            count: values.len(),
            elements: values
                .iter()
                .flat_map(|&value| to_le_bytes(value))
                .collect(),
        }
    }

    /// The value's length in a table of `width`-byte offsets and counts
    const fn byte_length(&self, width: usize) -> usize {
        match self {
            Self::Scalar(bytes) => bytes.len(),
            Self::Array { elements, .. } => width + elements.len(),
        }
    }
}

/// The padding before `field` when it would start at `end`, which aligns
/// an array's elements, after its count, and a nested table's start
fn padding(field: &Field, end: usize, width: usize) -> usize {
    let start = match Value::of(field) {
        Some(Value::Array { .. }) => end + width,
        _ => end,
    };

    start.next_multiple_of(alignment_of(field)) - start
}

/// The multiple of bytes from the start of its table that a field starts
/// at, after its count when it's an array: an array's alignment, a nested
/// table's alignment, and 1 for any other
fn alignment_of(field: &Field) -> usize {
    match (field, Value::of(field)) {
        (_, Some(Value::Array { alignment, .. })) => alignment,
        (Field::Table { nested, alignment }, _) => {
            alignment.map_or_else(|| default_alignment(nested), Alignment::bytes)
        }
        _ => 1,
    }
}

/// The alignment of a table nested without one: a builder's is the largest
/// alignment of its fields, which is that of its arrays, of scalars and of
/// structs, and nested tables, and bytes' is 4
fn default_alignment(nested: &Nested) -> usize {
    match nested.from {
        Source::Builder => nested
            .table
            .fields
            .iter()
            .map(alignment_of)
            .max()
            .unwrap_or(1),
        Source::Bytes => 4,
    }
}

/// Reads a little-endian slot of `width` bytes at `position`
fn slot(bytes: &[u8], position: usize, width: usize) -> usize {
    let slot = bytes
        .get(position..position + width)
        .unwrap_or_else(|| panic!("A slot at {position} leaves the {} bytes", bytes.len()));

    slot.iter()
        .rev()
        .fold(0, |value, &byte| value << 8 | usize::from(byte))
}

/// The little-endian bytes of `value` in a slot of `width` bytes
fn slot_bytes(value: usize, width: usize) -> Vec<u8> {
    let bytes = (value as u64).to_le_bytes();
    assert!(
        bytes[width..].iter().all(|&byte| byte == 0),
        "{value} doesn't fit in {width} bytes"
    );

    bytes[..width].to_vec()
}
