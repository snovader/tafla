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
    Alignment, BuildError, InPlaceTable, NestedTable, OffsetType, TableBuilder, TableLayout,
    TableReader,
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

        let reader = TableReader::new(&from_bytes, layout);
        let nested = reader.root_offset(1)?;
        prop_assert_eq!(nested % 8, 0);
        prop_assert_eq!(&from_bytes[nested..nested + bytes.len()], bytes.as_slice());

        let vector = reader.get_vector(reader.root_offset(2)?)?;
        let items = vector.iter().collect::<Result<Vec<_>, _>>()?;
        prop_assert_eq!(items, [bytes.as_slice(); 2]);
    }

    /// A table's bytes are those that `to_bytes()` copies, `finish()` lends
    /// and `into_bytes()` returns, whatever the builder held before: in a
    /// buffer that held other bytes, and in a builder that `reset()` starts
    /// after another table, finished or not, or after a reset that failed
    #[test]
    fn test_a_table_is_written_alike_into_any_buffer_and_after_a_reset(
        table in strategy::table(),
        before in strategy::table(),
        set_before in any::<Index>(),
        garbage in proptest::collection::vec(any::<u8>(), 0..300),
    ) {
        let expected = table.to_bytes();
        let owned = table.build().and_then(TableBuilder::into_bytes);
        prop_assert_eq!(&owned, &expected);
        let from_garbage = table
            .builder_in(garbage)
            .and_then(|mut builder| table.finish_on(&mut builder).map(<[u8]>::to_vec));
        prop_assert_eq!(&from_garbage, &expected);

        let Ok(mut builder) = before.builder() else {
            return Ok(());
        };
        // As many of the other table's fields as `set_before` picks, of
        // which any that fails leaves the builder as it was
        let set = set_before.index(before.fields.len() + 1);
        for field in &before.fields[..set] {
            let _ = field.set(&mut builder);
        }
        let _ = builder.reset(usize::MAX, TableLayout::DEFAULT);

        let reused = table
            .reset(&mut builder)
            .and_then(|()| table.finish_on(&mut builder).map(<[u8]>::to_vec));
        prop_assert_eq!(reused, expected);
    }

    /// Tables nested from builders and written in place, with
    /// `set_table_with()` and `set_vector_with()`, write the bytes that their
    /// builders write, and fail as they fail, for tables of any offset type
    /// and of short ones, whose limits the fields reach. A vector whose
    /// count and items' ends alone make its table too long fails before its
    /// items, where its builders fail for them or count them too. Nesting
    /// every table as one that's written in place writes most of them in
    /// place, and the tables as they're generated mix them with tables from
    /// bytes and builders of other layouts.
    #[test]
    fn test_tables_written_in_place_write_what_their_builders_write(
        table in prop_oneof![strategy::table(), tables_of_short_offsets()],
        nested_in_place in any::<bool>(),
    ) {
        let table = if nested_in_place { table.nested_in_place() } else { table };

        let in_place = table.to_bytes_in_place();
        let from_builders = table.to_bytes();

        if !fails_before_its_items(&in_place, &from_builders) {
            prop_assert_eq!(in_place, from_builders);
        }
    }
}

/// Whether a table recorded in place failed for a vector whose count and
/// items' ends alone make its table too long, which fails before its items
/// are recorded: with that length, where its builders failed for the vector
/// with its items too, which is longer, or for an item of it first
fn fails_before_its_items(
    in_place: &Result<Vec<u8>, BuildError>,
    from_builders: &Result<Vec<u8>, BuildError>,
) -> bool {
    let (
        Err(BuildError::TooLong {
            index,
            length,
            offset_type,
        }),
        Err(BuildError::TooLong {
            index: builder_index,
            length: builder_length,
            offset_type: builder_offset_type,
        }),
    ) = (in_place, from_builders)
    else {
        return false;
    };

    let same_field = index == builder_index && offset_type == builder_offset_type;
    !same_field || length < builder_length
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
    /// A vector of one string
    VectorString,
    /// A vector of empty strings, each an end of its own
    EmptyStrings,
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
        LongField::VectorString,
        LongField::EmptyStrings,
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
            Self::VectorString => Field::StringVector(vec!["a".repeat(units)]),
            Self::EmptyStrings => Field::StringVector(vec![String::new(); units]),
        }
    }

    /// The bytes that each unit adds in a table of `width`-byte offsets
    const fn unit(self, width: usize) -> usize {
        match self {
            Self::Floats => 8,
            Self::Structs => 3,
            Self::EmptyItems | Self::EmptyStrings => width,
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
        let reader = TableReader::new(&table, layout);
        assert_eq!(table.len(), max);
        assert_eq!(reader.get_u8(reader.root_offset(1).unwrap()).unwrap(), 7);

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
fn test_write_the_layouts_version() {
    const FIRST: TableLayout<true> = TableLayout::DEFAULT.sized().with_version(1);
    const SECOND: TableLayout<true> = FIRST.with_version(2);

    assert_eq!(SECOND.version(), 2);
    assert!(SECOND.is_sized());
    assert!(SECOND.is_versioned());

    let bytes = TableBuilder::new(0, SECOND).unwrap().to_bytes().unwrap();

    // A reader reads a table of any version, whatever its layout's
    assert_eq!(TableReader::new(&bytes, FIRST).version(), Ok(2));
    assert_eq!(TableReader::new(&bytes, SECOND).version(), Ok(2));
}

#[test]
fn test_reject_version_too_large_for_slot() {
    for (offset_type, version) in [(OffsetType::Uint8, 0x100), (OffsetType::Uint16, 0x1_0000)] {
        let layout = TableLayout::DEFAULT.with_offset_type(offset_type);

        let error = TableBuilder::new(1, layout.with_version(version)).unwrap_err();
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
        assert!(TableBuilder::new(1, layout.with_version(version - 1)).is_ok());
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
    let reader = TableReader::new(&table, TableLayout::DEFAULT);
    let position = reader.root_offset(0).unwrap();
    assert_eq!(position, 8);
    assert_eq!(
        reader
            .get_u8(reader.field_offset(position, 1).unwrap())
            .unwrap(),
        2
    );

    let vector = reader.get_vector(reader.root_offset(1).unwrap()).unwrap();
    assert_eq!(
        vector.iter().collect::<Result<Vec<_>, _>>(),
        Ok(vec![&nested.to_bytes().unwrap()[..]])
    );
}

/// The values of the items of the vector in the examples below
const VALUES: [f32; 3] = [1.0, 2.0, 3.0];

/// An error of the caller's, which a function that writes items in place
/// returns as well as a `BuildError`
#[derive(Debug, PartialEq, Eq)]
enum CallerError {
    Build(BuildError),
    Caller,
}

impl From<BuildError> for CallerError {
    fn from(error: BuildError) -> Self {
        Self::Build(error)
    }
}

/// Writes a vector of the benchmarks' items, each a float32 and a pair of
/// float32s, in place
fn items_in_place(values: &[f32], builder: &mut TableBuilder) -> Result<Vec<u8>, BuildError> {
    builder.set_vector_with(values.len(), 2, |index, item| {
        item.set_f32(values[index])?;
        item.set_table_with(2, |pair| {
            pair.set_f32(values[index] + 1.0)?;
            pair.set_f32(values[index] + 2.0)
        })
    })?;

    builder.to_bytes()
}

/// Writes the vector of [`items_in_place()`] from a builder per item
fn items_from_builders(values: &[f32]) -> Vec<u8> {
    let item = |value: f32| {
        let mut pair = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
        pair.set_f32(value + 1.0).unwrap();
        pair.set_f32(value + 2.0).unwrap();

        let mut item = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
        item.set_f32(value).unwrap();
        item.set_table(pair).unwrap();
        item
    };

    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    builder
        .set_vector(values.iter().copied().map(item))
        .unwrap();
    builder.to_bytes().unwrap()
}

/// An error that the caller returns while items are written leaves the
/// builder as it was, with none of their fields, so the vector can be set
/// again
#[test]
fn test_an_item_that_fails_leaves_the_builder_as_it_was() {
    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    let error = builder.set_vector_with(3, 2, |index, item| {
        item.set_f32(1.0)?;
        if index == 2 {
            return Err(CallerError::Caller);
        }
        Ok(item.set_f32(2.0)?)
    });

    assert_eq!(error, Err(CallerError::Caller));
    assert_eq!(
        items_in_place(&VALUES, &mut builder),
        Ok(items_from_builders(&VALUES))
    );
}

/// An item, or a table nested in one, that's left with fields unset fails
/// once its function returns, and leaves the builder as it was
#[test]
fn test_reject_incomplete_item() {
    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    let error = builder.set_vector_with(3, 2, |index, item| {
        item.set_f32(1.0)?;
        if index == 1 {
            return Ok(());
        }
        item.set_f32(2.0)
    });
    assert_eq!(error, Err(BuildError::Incomplete { set: 1, fields: 2 }));

    let error = builder.set_vector_with(3, 2, |_, item| {
        item.set_f32(1.0)?;
        item.set_table_with(2, |pair| pair.set_f32(2.0))
    });
    assert_eq!(error, Err(BuildError::Incomplete { set: 1, fields: 2 }));

    assert_eq!(
        items_in_place(&VALUES, &mut builder),
        Ok(items_from_builders(&VALUES))
    );
}

/// A panic while items are written leaves the builder as it was too, for a
/// caller that catches it
#[test]
fn test_a_panic_while_items_are_written_leaves_the_builder_as_it_was() {
    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        builder.set_vector_with(3, 2, |index, item| {
            item.set_f32(1.0)?;
            assert!(index < 2, "The caller's panic, which the test expects");
            item.set_f32(2.0)
        })
    }));

    assert!(panicked.is_err());
    assert_eq!(
        items_in_place(&VALUES, &mut builder),
        Ok(items_from_builders(&VALUES))
    );
}

#[test]
fn test_reject_item_field_beyond_its_count() {
    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    let error = builder.set_table_with(1, |table| {
        table.set_u8(1)?;
        table.set_u8(2)
    });
    assert_eq!(error, Err(BuildError::TooManyFields { fields: 1 }));

    builder.set_table_with(1, |table| table.set_u8(1)).unwrap();
    assert_eq!(builder.to_bytes(), Ok(vec![4, 0, 0, 0, 4, 0, 0, 0, 1]));
}

/// The vector's count and its items' ends are checked before any item is
/// written, so the error's length is theirs alone
#[test]
fn test_reject_vector_count_too_long_before_writing_items() {
    let layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint8);
    let mut builder = TableBuilder::new(1, layout).unwrap();
    let error = builder.set_vector_with(300, 1, |_, item| item.set_u8(1));

    assert_eq!(
        error,
        Err(BuildError::TooLong {
            index: 0,
            length: 1 + 301,
            offset_type: OffsetType::Uint8
        })
    );
}

/// Items that make the table too long are rejected once they're written,
/// with the length that they make, and leave the builder as it was
#[test]
fn test_reject_vector_items_that_make_the_table_too_long() {
    let layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint8);
    let pairs = |builder: &mut TableBuilder| {
        builder.set_vector_with(2, 1, |_, item| {
            item.set_table_aligned(&[1, 2][..], Alignment::Two)
        })
    };

    let mut builder = TableBuilder::new(1, layout).unwrap();
    let error = builder.set_vector_with(60, 1, |_, item| item.set_u32(1));
    assert_eq!(
        error,
        Err(BuildError::TooLong {
            index: 0,
            length: 1 + 61 + 60 * 5,
            offset_type: OffsetType::Uint8
        })
    );
    pairs(&mut builder).unwrap();

    let mut expected = TableBuilder::new(1, layout).unwrap();
    pairs(&mut expected).unwrap();
    assert_eq!(builder.to_bytes(), expected.to_bytes());
}

/// A table nested in place that fits its own offsets, but makes its parent
/// too long, is rejected once it's written, as the parent's field, and
/// leaves the parent as it was
#[test]
fn test_reject_table_in_place_that_makes_its_parent_too_long() {
    let layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint8);
    let column = |builder: &mut TableBuilder| builder.set_u8_array(vec![1; 200]);
    let short = |builder: &mut TableBuilder| {
        builder.set_table_with(1, |table| table.set_u8_array(vec![3; 10]))
    };

    let mut builder = TableBuilder::new(2, layout).unwrap();
    column(&mut builder).unwrap();
    let error = builder.set_table_with(1, |table| table.set_u8_array(vec![2; 100]));
    assert_eq!(
        error,
        Err(BuildError::TooLong {
            index: 1,
            length: 2 + 1 + 200 + 1 + 1 + 100,
            offset_type: OffsetType::Uint8,
        })
    );
    short(&mut builder).unwrap();

    let mut expected = TableBuilder::new(2, layout).unwrap();
    column(&mut expected).unwrap();
    short(&mut expected).unwrap();
    assert_eq!(builder.to_bytes(), expected.to_bytes());
}

/// A table nested in place whose fill leaves a field unset fails once the
/// fill returns, and leaves the builder as it was
#[test]
fn test_reject_incomplete_table_in_place() {
    let pair = |builder: &mut TableBuilder| {
        builder.set_table_with(2, |table| {
            table.set_u8(2)?;
            table.set_u8(3)
        })
    };

    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    let error = builder.set_table_with(2, |table| table.set_u8(1));
    assert_eq!(error, Err(BuildError::Incomplete { set: 1, fields: 2 }));
    pair(&mut builder).unwrap();

    let mut expected = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    pair(&mut expected).unwrap();
    assert_eq!(builder.to_bytes(), expected.to_bytes());
}

/// A builder takes the alignment of the arrays in the tables nested in it in
/// place, so that its parent pads it to keep them aligned from the root
#[test]
fn test_a_builder_is_padded_for_the_tables_nested_in_it_in_place() {
    let mut nested = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    nested.set_u8(1).unwrap();
    nested
        .set_table_with(1, |table| table.set_f64_array(&[0.5, 1.5]))
        .unwrap();
    let mut builder = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    builder.set_u8(2).unwrap();
    builder.set_table(nested).unwrap();

    let bytes = builder.to_bytes().unwrap();
    let buffer = TableReader::new(&bytes, TableLayout::DEFAULT);
    let nested = buffer.root_offset(1).unwrap();
    let table = buffer.field_offset(nested, 1).unwrap();
    let array = buffer.field_offset(table, 0).unwrap();

    // The nested table is padded from 9 to 16, and the table in place in it
    // from 9 to 16 in it, so that the array's elements, after its count at
    // 4 in that table, lie at 40, a multiple of 8 from the root
    assert_eq!(nested, 16);
    assert_eq!((table - nested, array - table), (16, 4));
    assert_eq!(buffer.get_f64_array(array).unwrap().to_vec(), [0.5, 1.5]);
}

/// A float64 array of 40 elements: 320 bytes
fn long_column() -> Vec<f64> {
    (0..40).map(|index| f64::from(index) * 0.5).collect()
}

/// Writes a table of a uint8, the table that `set_table` sets and another
/// uint8
fn nested_between_scalars(
    set_table: impl FnOnce(&mut TableBuilder) -> Result<(), BuildError>,
) -> Result<Vec<u8>, BuildError> {
    let mut parent = TableBuilder::new(3, TableLayout::DEFAULT)?;
    parent.set_u8(2)?;
    set_table(&mut parent)?;
    parent.set_u8(3)?;
    parent.to_bytes()
}

/// A builder nested in its parent, borrowed or owned, is copied in as its
/// bytes are, with a sized header's length
#[test]
fn test_a_nested_builder_is_copied_with_its_length() {
    let layout = TableLayout::DEFAULT.sized();
    let nested = || -> Result<TableBuilder, BuildError> {
        let mut nested = TableBuilder::new(3, layout)?;
        nested.set_u8(1)?;
        nested.set_f64_array(long_column())?;
        nested.set_str("x".repeat(300))?;
        Ok(nested)
    };

    let borrowed = nested().unwrap();
    let bytes = borrowed.to_bytes().unwrap();
    let length = u32::try_from(bytes.len()).unwrap();
    assert_eq!(bytes[..4], length.to_le_bytes());

    let expected = nested_between_scalars(|parent| {
        parent.set_table_aligned(bytes.as_slice(), Alignment::Eight)
    });
    assert!(expected.is_ok());
    assert_eq!(
        nested_between_scalars(|parent| parent.set_table(&borrowed)),
        expected
    );
    assert_eq!(
        nested_between_scalars(|parent| parent.set_table(nested()?)),
        expected
    );
}

/// A table written in place is moved forward by its padding once its fields
/// are set, with its values, and the values before it stay
#[test]
fn test_a_table_in_place_is_moved_by_its_padding() {
    let column = long_column();
    let name = "x".repeat(300);
    let build = |in_place: bool| -> Result<Vec<u8>, BuildError> {
        let mut builder = TableBuilder::new(3, TableLayout::DEFAULT)?;
        builder.set_str(name.as_str())?;
        builder.set_u8(1)?;
        if in_place {
            builder.set_table_with(2, |table| {
                table.set_u8(2)?;
                table.set_f64_array(column.as_slice())
            })?;
        } else {
            let mut table = TableBuilder::new(2, TableLayout::DEFAULT)?;
            table.set_u8(2)?;
            table.set_f64_array(column.as_slice())?;
            builder.set_table(table)?;
        }
        builder.to_bytes()
    };

    let bytes = build(true).unwrap();
    assert_eq!(build(false).as_ref(), Ok(&bytes));

    // The table starts at 317, padded to 320
    let buffer = TableReader::new(&bytes, TableLayout::DEFAULT);
    let table = buffer.root_offset(2).unwrap();
    assert_eq!(table, 320);
    let array = buffer.field_offset(table, 1).unwrap();
    assert_eq!(buffer.get_f64_array(array).unwrap().to_vec(), column);
}

/// Writes `depth` tables nested one in another, in place or each from a
/// builder, each after a uint8 and a string of 300 bytes, around a table of
/// a uint8, a float64 array, which needs each of them padded, and `strings`
/// strings of 30 bytes
fn nested_tables(depth: usize, strings: usize, in_place: bool) -> Result<Vec<u8>, BuildError> {
    fn level(table: &mut InPlaceTable<'_>, depth: usize, strings: usize) -> Result<(), BuildError> {
        table.set_u8(1)?;
        table.set_str(LONG_NAME)?;
        if depth > 0 {
            return table.set_table_with(3, |inner| level(inner, depth - 1, strings));
        }

        table.set_table_with(2 + strings, |inner| {
            inner.set_u8(2)?;
            inner.set_f64_array(COLUMN.as_slice())?;
            (0..strings).try_for_each(|_| inner.set_str(SHORT_NAME))
        })
    }

    fn builder(depth: usize, strings: usize) -> Result<TableBuilder, BuildError> {
        let nested = if depth > 0 {
            builder(depth - 1, strings)?
        } else {
            let mut innermost = TableBuilder::new(2 + strings, TableLayout::DEFAULT)?;
            innermost.set_u8(2)?;
            innermost.set_f64_array(COLUMN.as_slice())?;
            (0..strings).try_for_each(|_| innermost.set_str(SHORT_NAME))?;
            innermost
        };

        let mut table = TableBuilder::new(3, TableLayout::DEFAULT)?;
        table.set_u8(1)?;
        table.set_str(LONG_NAME)?;
        table.set_table(nested)?;
        Ok(table)
    }

    const LONG_NAME: &str = match str::from_utf8(&[b'x'; 300]) {
        Ok(name) => name,
        Err(_) => unreachable!(),
    };
    const SHORT_NAME: &str = LONG_NAME.split_at(30).0;
    const COLUMN: [f64; 3] = [0.5, 1.5, -2.5];

    let mut root = TableBuilder::new(1, TableLayout::DEFAULT)?;
    if in_place {
        root.set_table_with(3, |table| level(table, depth - 1, strings))?;
    } else {
        root.set_table(builder(depth - 1, strings)?)?;
    }
    root.to_bytes()
}

/// Tables nested in place one in another, each moved by its padding once
/// its fields are set, and again with each table that holds it, are written
/// as builders nested one in another are, and their arrays are aligned from
/// the root
#[test]
fn test_tables_nested_in_place_one_in_another_are_each_padded() {
    const DEPTH: usize = 40;
    for strings in [0, 12] {
        let bytes = nested_tables(DEPTH, strings, true).unwrap();
        assert_eq!(nested_tables(DEPTH, strings, false).as_ref(), Ok(&bytes));

        let buffer = TableReader::new(&bytes, TableLayout::DEFAULT);
        let mut table = buffer.root_offset(0).unwrap();
        for _ in 0..DEPTH {
            table = buffer.field_offset(table, 2).unwrap();
        }
        let array = buffer.field_offset(table, 1).unwrap();
        // The elements follow the array's count of 4 bytes
        assert_eq!((array + 4) % 8, 0);
        let column = buffer.get_f64_array(array).unwrap();
        assert_eq!(column.to_vec(), [0.5, 1.5, -2.5]);
    }
}

/// A vector written in place in each item of a vector in place that makes
/// its table too long is written in each item, and the outer vector fails
/// as it does from builders, after every item ran, those whose inner vector
/// failed and were set again too
#[test]
fn test_vectors_in_place_in_the_items_of_one_that_makes_its_table_too_long() {
    const ITEMS: usize = 40;
    fn names(item: &mut InPlaceTable<'_>, name: &str, count: usize) -> Result<(), BuildError> {
        item.set_vector_with(count, 1, |_, entry| entry.set_str(name))
    }
    let layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint16);
    let name = "x".repeat(250);
    let from_builders = || -> Result<(), BuildError> {
        let mut items = Vec::new();
        for _ in 0..ITEMS {
            let mut entries = Vec::new();
            for _ in 0..10 {
                let mut entry = TableBuilder::new(1, layout)?;
                entry.set_str(name.as_str())?;
                entries.push(entry);
            }
            let mut item = TableBuilder::new(1, layout)?;
            item.set_vector(entries)?;
            items.push(item);
        }
        TableBuilder::new(1, layout)?.set_vector(items)
    };
    let expected = from_builders();
    assert!(matches!(
        expected,
        Err(BuildError::TooLong { index: 0, .. })
    ));

    let mut builder = TableBuilder::new(1, layout).unwrap();
    let mut calls = 0;
    let error = builder.set_vector_with(ITEMS, 1, |_, item| {
        calls += 1;
        names(item, &name, 10)
    });
    assert_eq!((error, calls), (expected, ITEMS));

    // 300 names make an item too long, and the fill sets 10 instead
    let error = builder.set_vector_with(ITEMS, 1, |_, item| {
        if names(item, &name, 300).is_err() {
            names(item, &name, 10)?;
        }
        Ok::<_, BuildError>(())
    });
    assert_eq!(error, expected);

    builder
        .set_vector_with(2, 1, |_, item| names(item, &name, 1))
        .unwrap();
    let mut written = TableBuilder::new(1, layout).unwrap();
    written
        .set_vector_with(2, 1, |_, item| names(item, &name, 1))
        .unwrap();
    assert_eq!(builder.to_bytes(), written.to_bytes());
}

/// A vector written in place whose first item is far longer than the
/// others, so that room for 3,000 more like it would be 2.4 GB, is written:
/// the room that the builder makes for the items after the first is
/// bounded, as a 32-bit target can't allocate 2.4 GB
#[test]
fn test_an_uneven_vector_in_place_is_written() {
    const ITEMS: usize = 3_001;
    const NAMES: usize = 3_000;
    let name = "x".repeat(250);

    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    builder
        .set_vector_with(ITEMS, 1, |index, item| {
            let names = if index == 0 { NAMES } else { 0 };
            item.set_vector_with(names, 1, |_, entry| entry.set_str(name.as_str()))
        })
        .unwrap();
    let bytes = builder.to_bytes().unwrap();

    let buffer = TableReader::new(&bytes, TableLayout::DEFAULT);
    let vector = buffer.get_vector(buffer.root_offset(0).unwrap()).unwrap();
    assert_eq!(vector.len(), ITEMS);
    let names_of = |index: usize| {
        let item = vector.get_in_place(index).unwrap().unwrap();
        let item = TableReader::new(item, TableLayout::DEFAULT);
        item.get_vector(item.root_offset(0).unwrap()).unwrap()
    };

    let first = names_of(0);
    assert_eq!(first.len(), NAMES);
    let entry = first.get_in_place(NAMES - 1).unwrap().unwrap();
    let entry = TableReader::new(entry, TableLayout::DEFAULT);
    assert_eq!(
        entry.get_str(entry.root_offset(0).unwrap()),
        Ok(name.as_str())
    );
    assert!(names_of(ITEMS - 1).is_empty());
}

/// A vector written in place that makes its table too long runs every
/// item's fill, as one that doesn't would, and fails with the length that
/// all its items make, or with the first error that a fill returns after
/// the table was too long
#[test]
fn test_a_vector_in_place_that_makes_its_table_too_long_runs_every_item() {
    const ITEMS: usize = 330;
    let layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint16);
    let values = [7_u8; 200];
    let column = |item: &mut InPlaceTable<'_>| item.set_u8_array(values.to_vec());

    let mut builder = TableBuilder::new(1, layout).unwrap();
    let mut calls = 0;
    let error = builder.set_vector_with(ITEMS, 1, |_, item| {
        calls += 1;
        column(item)
    });
    // The count and the ends, after a header of 2 bytes, then each item's
    // header, count and values
    let length = 2 + 2 * (ITEMS + 1) + ITEMS * (2 + 2 + values.len());
    assert_eq!(calls, ITEMS);
    assert_eq!(
        error,
        Err(BuildError::TooLong {
            index: 0,
            length: length as u64,
            offset_type: OffsetType::Uint16
        })
    );

    let error = builder.set_vector_with(ITEMS, 1, |index, item| {
        if index == ITEMS - 1 {
            return Err(CallerError::Caller);
        }
        Ok(column(item)?)
    });
    assert_eq!(error, Err(CallerError::Caller));

    builder
        .set_vector_with(2, 1, |_, item| column(item))
        .unwrap();
    let mut expected = TableBuilder::new(1, layout).unwrap();
    expected
        .set_vector_with(2, 1, |_, item| column(item))
        .unwrap();
    assert_eq!(builder.to_bytes(), expected.to_bytes());
}

/// Tables longer than their `uint32` offsets address are rejected before
/// their values are copied, on every target: a vector of strings that holds
/// one string of 1 GiB five times, among the builder's fields and in a table
/// nested in place, which leaves the builder as it was. Its length is
/// counted past 4 GiB, which a 32-bit `usize` doesn't hold.
#[test]
#[cfg_attr(miri, ignore = "allocates 1 GiB")]
fn test_reject_vectors_of_strings_past_4_gib_before_copying_them() {
    const GIB: u64 = 1 << 30;
    let string = "x".repeat(1 << 30);
    let strings = [string.as_str(); 5];
    let too_long = |index, length| {
        Err(BuildError::TooLong {
            index,
            length,
            offset_type: OffsetType::Uint32,
        })
    };

    let mut builder = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    builder.set_u8(1).unwrap();
    // The header's 2 offsets and the uint8, then the vector's count, and the
    // end and bytes of each string
    let length = 8 + 1 + 4 + 5 * (4 + GIB);
    assert_eq!(
        builder.set_str_vector(strings.as_slice()),
        too_long(1, length)
    );
    // The table's offset, and the vector after it
    let error = builder.set_table_with(1, |table| table.set_str_vector(strings.as_slice()));
    assert_eq!(error, too_long(0, 4 + 4 + 5 * (4 + GIB)));

    builder.set_str_vector(&["x"; 5][..]).unwrap();
    let mut expected = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    expected.set_u8(1).unwrap();
    expected.set_str_vector(&["x"; 5][..]).unwrap();
    assert_eq!(builder.finish(), expected.finish());
}

/// A vector written in place whose count and items' ends alone make its
/// table too long reports that length, since its items aren't written,
/// where a vector of their builders reports the length with its items
#[test]
fn test_vector_too_long_before_its_items_reports_its_length_without_them() {
    let layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint8);
    let string = "a".repeat(249);
    let mut builder = TableBuilder::new(2, layout).unwrap();
    builder.set_str(string.as_str()).unwrap();
    let mut item = TableBuilder::new(1, layout).unwrap();
    item.set_u8(1).unwrap();

    // 2 offsets, a count and 249 bytes, and a vector of a count and 3 ends,
    // and of 3 items of an offset and a byte
    assert_eq!(
        builder.set_vector_with(3, 1, |_, item| item.set_u8(1)),
        Err(BuildError::TooLong {
            index: 1,
            length: 256,
            offset_type: OffsetType::Uint8
        })
    );
    assert_eq!(
        builder.set_vector([&item, &item, &item]),
        Err(BuildError::TooLong {
            index: 1,
            length: 262,
            offset_type: OffsetType::Uint8
        })
    );
}

/// `finish()` writes a sized header's length, lends the table's bytes, and
/// fails for a table with fields left to set, as `to_bytes()` and
/// `into_bytes()` do
#[test]
fn test_finish_lends_the_table_and_rejects_an_incomplete_one() {
    let layout = TableLayout::DEFAULT.sized();
    let mut builder = TableBuilder::new(2, layout).unwrap();
    builder.set_u16(1).unwrap();
    let incomplete = BuildError::Incomplete { set: 1, fields: 2 };
    assert_eq!(builder.finish(), Err(incomplete));
    assert_eq!(builder.to_bytes(), Err(incomplete));

    builder.set_str("ab").unwrap();
    let copied = builder.to_bytes().unwrap();
    // The length, 2 offsets, the uint16, and the string's count and bytes
    let expected = [
        20, 0, 0, 0, 12, 0, 0, 0, 14, 0, 0, 0, 1, 0, 2, 0, 0, 0, b'a', b'b',
    ];
    assert_eq!(copied, expected[..]);
    assert_eq!(builder.finish(), Ok(&expected[..]));
    assert_eq!(builder.into_bytes(), Ok(expected.to_vec()));

    let incomplete = TableBuilder::new(1, layout).unwrap();
    assert_eq!(
        incomplete.into_bytes(),
        Err(BuildError::Incomplete { set: 0, fields: 1 })
    );
}

/// A builder copies each value when it's set, so a value that changes or
/// is dropped afterwards doesn't change the table
#[test]
fn test_values_are_copied_when_they_are_set() {
    let mut name = String::from("first");
    let mut column = vec![1.5_f64, 2.5];

    let mut builder = TableBuilder::new(3, TableLayout::DEFAULT).unwrap();
    builder.set_str(name.as_str()).unwrap();
    builder.set_f64_array(column.as_slice()).unwrap();
    builder
        .set_table_with(1, |table| table.set_str(name.as_str()))
        .unwrap();
    name.replace_range(.., "other");
    column[0] = -1.0;
    drop(name);
    drop(column);

    let mut expected = TableBuilder::new(3, TableLayout::DEFAULT).unwrap();
    expected.set_str("first").unwrap();
    expected.set_f64_array(vec![1.5, 2.5]).unwrap();
    expected
        .set_table_with(1, |table| table.set_str("first"))
        .unwrap();
    assert_eq!(builder.finish(), expected.finish());
}

/// `reset()` with a layout that it can't start leaves the builder as it was,
/// with its fields and buffer, and `with_buffer()` writes over what the
/// buffer held
#[test]
fn test_a_reset_that_fails_leaves_the_builder_as_it_was() {
    let layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint8);
    let mut builder = TableBuilder::new(2, layout).unwrap();
    builder.set_u8(1).unwrap();

    assert!(matches!(
        builder.reset(300, layout),
        Err(BuildError::HeaderTooLong { fields: 300, .. })
    ));
    assert_eq!(
        builder.reset(1, layout.with_version(256)),
        Err(BuildError::VersionTooLarge {
            version: 256,
            offset_type: OffsetType::Uint8
        })
    );
    builder.set_u8(2).unwrap();
    assert_eq!(builder.finish(), Ok(&[2, 3, 1, 2][..]));

    let mut reused = TableBuilder::with_buffer(vec![9; 64], 2, layout).unwrap();
    reused.set_u8(1).unwrap();
    reused.set_u8(2).unwrap();
    assert_eq!(reused.into_bytes(), Ok(vec![2, 3, 1, 2]));
}
