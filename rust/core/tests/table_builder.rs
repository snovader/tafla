//! Builders write any table that fits in its offset type, and reject what
//! they can't write with a `BuildError` from the setter, leaving the
//! builder as it was. The examples document the errors.

mod common;

use common::{
    description::{Field, Layout, Nested, ScalarType, Source, Table},
    read::read_table,
    strategy,
};
use proptest::{
    prelude::*,
    sample::{Index, select},
};
use tafla::{
    Alignment, BuildError, ByteBuffer, NestedTable, OffsetType, TableBuilder, TableLayout,
};

proptest! {
    #![proptest_config(common::config())]

    /// After any fields, a field as long as fits is written and reads back,
    /// and a unit longer is rejected with the length that it would make
    /// the table
    #[test]
    fn test_the_longest_field_that_fits_is_written_and_a_unit_more_is_rejected(
        table in tables_of_short_offsets(),
        long in long_field(),
    ) {
        let offset_type = table.layout.offset_type;
        let max = offset_type.max_value() as usize;
        let unit = long.unit(offset_type.size());

        // The fields can leave no room for even the empty field
        let empty = with_field(&table, long.field(0));
        let empty_bytes = empty.to_bytes();
        prop_assume!(empty_bytes.is_ok());
        let empty_bytes = empty_bytes.unwrap();

        let units = (max - empty_bytes.len()) / unit;
        let longest = with_field(&table, long.field(units));
        let bytes = longest.to_bytes().unwrap();
        prop_assert!(max - unit < bytes.len() && bytes.len() <= max, "{} bytes", bytes.len());

        let read = read_table(&bytes, &longest);
        prop_assert_eq!(read.as_ref(), Ok(&longest));

        let too_long = long.field(units + 1);
        let mut builder = empty.builder().unwrap();
        for field in &table.fields {
            field.set(&mut builder).unwrap();
        }

        let length = empty_bytes.len() + (units + 1) * unit;
        let error = BuildError::TooLong {
            index: table.fields.len(),
            length: length as u64,
            offset_type,
        };
        prop_assert_eq!(too_long.set(&mut builder), Err(error));

        empty.fields.last().unwrap().set(&mut builder).unwrap();
        prop_assert_eq!(builder.to_bytes(), Ok(empty_bytes));
    }

    /// A field longer than the offsets address on its own, set after any of
    /// the fields, is rejected, and the builder writes the same table once
    /// the other fields are set
    #[test]
    fn test_a_field_too_long_for_the_offset_type_is_rejected_and_leaves_the_builder_as_it_was(
        table in tables_of_short_offsets(),
        long in long_field(),
        at in any::<Index>(),
    ) {
        prop_assume!(!table.fields.is_empty());

        let offset_type = table.layout.offset_type;
        let max = offset_type.max_value() as usize;
        let too_long = long.field(max / long.unit(offset_type.size()) + 1);
        let at = at.index(table.fields.len());

        let mut builder = table.builder().unwrap();
        for field in &table.fields[..at] {
            field.set(&mut builder).unwrap();
        }

        let error = too_long.set(&mut builder).unwrap_err();
        prop_assert!(
            matches!(
                error,
                BuildError::TooLong { index, length, offset_type: size }
                    if index == at && length > max as u64 && size == offset_type
            ),
            "{:?}",
            error
        );

        for field in &table.fields[at..] {
            field.set(&mut builder).unwrap();
        }

        prop_assert_eq!(builder.to_bytes(), table.to_bytes());
    }

    /// A table nested from its builder, borrowed or owned, writes the same
    /// parent as from its bytes: the builder is written in place when it
    /// has the parent's offset type and no header, and copied otherwise. The
    /// table starts at the alignment it's set with and holds its bytes, and
    /// the vector's items read back as its bytes.
    #[test]
    fn test_a_table_nested_from_its_builder_writes_the_same_parent_as_from_its_bytes(
        table in strategy::table(),
        before in 0..=7_usize,
    ) {
        let bytes = table.to_bytes();
        prop_assume!(bytes.is_ok());
        let bytes = bytes.unwrap();

        // Of the table's offset type when the parent fits in it, so that a
        // builder without a header is written in place whatever its offset
        // type, and otherwise of uint32 offsets
        let mut offset_type = table.layout.offset_type;
        if longest_parent(offset_type, before, bytes.len()) > offset_type.max_value() as usize {
            offset_type = OffsetType::Uint32;
        }
        let layout = TableLayout::DEFAULT.with_offset_type(offset_type);

        let borrowed = table.build()?;
        let from_borrowed = parent_of(|| (&borrowed).into(), before, layout)?;
        let from_owned = parent_of(|| table.build().unwrap().into(), before, layout)?;
        let from_bytes = parent_of(|| bytes.as_slice().into(), before, layout)?;
        prop_assert_eq!(&from_borrowed, &from_bytes);
        prop_assert_eq!(&from_owned, &from_bytes);

        let buffer = ByteBuffer::new(&from_bytes, layout);
        let nested = buffer.root_offset(1)?;
        prop_assert_eq!(nested % 8, 0);
        prop_assert_eq!(&from_bytes[nested..nested + bytes.len()], bytes.as_slice());

        let vector = buffer.get_vector(buffer.root_offset(2)?)?;
        let items = vector.iter().collect::<Result<Vec<_>, _>>()?;
        prop_assert_eq!(items, [bytes.as_slice(); 2]);
    }
}

/// A parent of `before` bytes in a `uint8[]`, the table at alignment 8, and a
/// vector of the table twice
fn parent_of<'a>(
    table: impl Fn() -> NestedTable<'a>,
    before: usize,
    layout: TableLayout,
) -> Result<Vec<u8>, BuildError> {
    let mut parent = TableBuilder::new(3, layout)?;
    parent.set_u8_array(vec![0; before])?;
    parent.set_table_aligned(table(), Alignment::Eight)?;
    parent.set_vector([table(), table()])?;

    parent.to_bytes()
}

/// The most bytes that `parent_of()` writes for a table of `length` bytes,
/// with `offset_type` offsets
const fn longest_parent(offset_type: OffsetType, before: usize, length: usize) -> usize {
    let width = offset_type.size();
    let offsets = 3 * width;
    let array = width + before;
    let table = (8 - 1) + length;
    let vector = 3 * width + 2 * length;

    offsets + array + table + vector
}

/// Tables of `uint8` and `uint16` offsets, whose limits a test can reach.
/// Under Miri, only of `uint8` offsets: writing 64 KiB, and reading it back
/// on a big-endian target, takes it minutes, and the examples reach the limit
/// of `uint16` offsets.
fn tables_of_short_offsets() -> impl Strategy<Value = Table> {
    let offset_types = if cfg!(miri) {
        vec![OffsetType::Uint8]
    } else {
        vec![OffsetType::Uint8, OffsetType::Uint16]
    };

    strategy::tables(strategy::layouts(select(offset_types)))
}

fn with_field(table: &Table, field: Field) -> Table {
    let mut table = table.clone();
    table.fields.push(field);

    table
}

/// A field whose byte length grows by a unit with each of its units
#[derive(Clone, Copy, Debug)]
enum LongField {
    Bytes,
    Floats,
    String,
    /// An array of structs of a `u16` and a `u8`, of 3 bytes aligned to 2
    Structs,
    /// A vector of empty tables, each an end of its own
    EmptyItems,
    /// A vector of a table of bytes, from its builder
    Item,
    /// A table of bytes, nested from its bytes
    NestedBytes,
    /// A table of bytes, nested from its builder
    NestedBuilder,
}

fn long_field() -> impl Strategy<Value = LongField> {
    select(vec![
        LongField::Bytes,
        LongField::Floats,
        LongField::String,
        LongField::Structs,
        LongField::EmptyItems,
        LongField::Item,
        LongField::NestedBytes,
        LongField::NestedBuilder,
    ])
}

impl LongField {
    fn field(self, units: usize) -> Field {
        let bytes = || vec![Field::U8Array(vec![0; units])];

        match self {
            Self::Bytes => Field::U8Array(vec![0; units]),
            Self::Floats => Field::F64Array(vec![0.0; units]),
            Self::String => Field::String("a".repeat(units)),
            Self::Structs => Field::StructArray {
                types: vec![ScalarType::U16, ScalarType::U8],
                items: vec![vec![Field::U16(1), Field::U8(2)]; units],
            },
            Self::EmptyItems => Field::Vector(vec![nested(Source::Bytes, Vec::new()); units]),
            Self::Item => Field::Vector(vec![nested(Source::Builder, bytes())]),
            Self::NestedBytes => Field::Table {
                nested: nested(Source::Bytes, bytes()),
                alignment: None,
            },
            Self::NestedBuilder => Field::Table {
                nested: nested(Source::Builder, bytes()),
                alignment: None,
            },
        }
    }

    /// The bytes that each unit adds in a table of `width`-byte offsets
    const fn unit(self, width: usize) -> usize {
        match self {
            Self::Floats => 8,
            Self::Structs => 3,
            Self::EmptyItems => width,
            _ => 1,
        }
    }
}

/// A table of `fields` with no header and `uint32` offsets, which fits in any
/// parent
const fn nested(from: Source, fields: Vec<Field>) -> Nested {
    Nested {
        from,
        table: Table {
            layout: Layout::headerless(OffsetType::Uint32),
            version: None,
            fields,
        },
    }
}

#[test]
fn test_reject_field_beyond_count() {
    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    builder.set_u8(1).unwrap();
    let bytes = builder.to_bytes().unwrap();

    let error = builder.set_u8(2).unwrap_err();
    assert_eq!(error, BuildError::TooManyFields { fields: 1 });
    assert_eq!(error.to_string(), "Can't add field 2 to a table of 1");
    assert_eq!(builder.to_bytes().unwrap(), bytes);
}

#[test]
fn test_reject_writing_before_every_field_is_set() {
    for set in 0..5 {
        let mut builder = TableBuilder::new(set + 1, TableLayout::DEFAULT).unwrap();
        for _ in 0..set {
            builder.set_u8(1).unwrap();
        }

        let error = builder.to_bytes().unwrap_err();
        assert_eq!(
            error,
            BuildError::Incomplete {
                set,
                fields: set + 1
            }
        );
        assert_eq!(
            error.to_string(),
            format!("Only {set} of the table's {} fields were set", set + 1)
        );
    }
}

#[test]
fn test_reject_field_that_makes_table_too_long() {
    for (offset_type, max) in [(OffsetType::Uint8, 0xff), (OffsetType::Uint16, 0xffff)] {
        let layout = TableLayout::DEFAULT.with_offset_type(offset_type);
        let width = offset_type.size();

        // Two offsets, the array's length prefix and bytes, and then one byte
        let bytes = vec![0; max - 3 * width - 1];
        let mut fits = TableBuilder::new(2, layout).unwrap();
        fits.set_u8_array(&bytes).unwrap();
        fits.set_u8(7).unwrap();

        let table = fits.to_bytes().unwrap();
        let buffer = ByteBuffer::new(&table, layout);
        assert_eq!(table.len(), max);
        assert_eq!(buffer.get_u8(buffer.root_offset(1).unwrap()).unwrap(), 7);

        let bytes = vec![0; max - 3 * width];
        let mut too_long = TableBuilder::new(2, layout).unwrap();
        too_long.set_u8_array(&bytes).unwrap();

        let error = too_long.set_u8(7).unwrap_err();
        assert_eq!(
            error,
            BuildError::TooLong {
                index: 1,
                length: max as u64 + 1,
                offset_type,
            }
        );
        assert_eq!(
            error.to_string(),
            format!(
                "Field 2 makes the table {} bytes, too long for {offset_type} offsets",
                max + 1
            )
        );
    }
}

#[test]
fn test_reject_bytes_that_arent_whole_structs() {
    let cases = [
        (5, 2, "5 bytes aren't a whole number of 2-byte structs"),
        (4, 0, "A struct can't be 0 bytes"),
        (0, 0, "A struct can't be 0 bytes"),
    ];

    for (length, size, message) in cases {
        let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
        let error = builder
            .set_struct_array(vec![0; length], size, Alignment::One)
            .unwrap_err();
        assert_eq!(error, BuildError::NotWholeStructs { length, size });
        assert_eq!(error.to_string(), message);

        // The builder was left as it was
        builder
            .set_struct_array(&[1, 2, 3, 4], 2, Alignment::Two)
            .unwrap();
        assert_eq!(
            builder.to_bytes().unwrap(),
            [4, 0, 0, 0, 2, 0, 0, 0, 1, 2, 3, 4]
        );
    }
}

#[test]
fn test_reject_header_too_long() {
    for (offset_type, max) in [(OffsetType::Uint8, 0xff), (OffsetType::Uint16, 0xffff)] {
        let width = offset_type.size();

        for sized in [false, true] {
            let layout = TableLayout::DEFAULT.with_offset_type(offset_type);
            let layout = if sized { layout.sized() } else { layout };
            let fields = max / width - usize::from(sized);

            assert!(TableBuilder::new(fields, layout).is_ok());
            assert_eq!(
                TableBuilder::new(fields + 1, layout).unwrap_err(),
                BuildError::HeaderTooLong {
                    fields: fields + 1,
                    offset_type
                }
            );
        }
    }

    assert!(matches!(
        TableBuilder::new(usize::MAX, TableLayout::DEFAULT),
        Err(BuildError::HeaderTooLong { .. })
    ));
}

// A table of 4 GiB can't be allocated in a test, but a vector that nests a
// builder of 64 MiB 64 times is that long without allocating more
#[test]
#[cfg_attr(miri, ignore = "allocates 64 MiB")]
fn test_reject_table_of_32_bit_offsets_past_4_gib_before_writing() {
    let bytes = vec![0; 1 << 26];
    let mut nested = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    nested.set_u8_array(&bytes).unwrap();

    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    let error = builder.set_vector(vec![&nested; 64]).unwrap_err();
    assert!(matches!(error, BuildError::TooLong { index: 0, length, .. } if length > 1 << 32));

    builder.set_vector(vec![&nested; 63]).unwrap();
}

#[test]
fn test_reject_version_too_large_for_slot() {
    for (offset_type, version) in [(OffsetType::Uint8, 0x100), (OffsetType::Uint16, 0x1_0000)] {
        let layout = TableLayout::DEFAULT
            .with_offset_type(offset_type)
            .versioned();

        let error = TableBuilder::versioned(1, layout, version).unwrap_err();
        assert_eq!(
            error,
            BuildError::VersionTooLarge {
                version,
                offset_type
            }
        );
        assert_eq!(
            error.to_string(),
            format!("Version {version} doesn't fit in a {offset_type} slot")
        );
        assert!(TableBuilder::versioned(1, layout, version - 1).is_ok());
    }
}

#[test]
fn test_reject_incomplete_nested_builder() {
    let mut incomplete = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    incomplete.set_u8(1).unwrap();

    let mut parent = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    let error = BuildError::Incomplete { set: 1, fields: 2 };
    assert_eq!(parent.set_table(&incomplete).unwrap_err(), error);

    let bytes = [0];
    let items = [tafla::NestedTable::from(&bytes), (&incomplete).into()];
    assert_eq!(parent.set_vector(items).unwrap_err(), error);

    // The parent was left as it was
    let mut nested = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    nested.set_u8(1).unwrap();
    nested.set_u8(2).unwrap();
    parent.set_table(&nested).unwrap();
    parent.set_vector([&nested]).unwrap();

    let table = parent.to_bytes().unwrap();
    let buffer = ByteBuffer::new(&table, TableLayout::DEFAULT);
    let position = buffer.root_offset(0).unwrap();
    assert_eq!(position, 8);
    assert_eq!(
        buffer
            .get_u8(buffer.field_offset(position, 1).unwrap())
            .unwrap(),
        2
    );

    let vector = buffer.get_vector(buffer.root_offset(1).unwrap()).unwrap();
    assert_eq!(
        vector.iter().collect::<Result<Vec<_>, _>>(),
        Ok(vec![&nested.to_bytes().unwrap()[..]])
    );
}
