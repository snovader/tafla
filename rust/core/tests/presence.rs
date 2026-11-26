//! Absent fields, which a format lets its writers leave out with the offset
//! 0 and no bytes, and the fields that a reader of a later revision of a
//! table finds missing from a table of an earlier one, as `SPEC.md` at the
//! root of the repository describes them. The tables are those of the
//! conformance suite's model, with fields left out at every depth;
//! `conformance.rs` reads the suite's cases of presence, and `format.rs`
//! checks such tables against the format's rules.

mod common;

use common::{
    at_byte_offset,
    description::{Field, Table},
    read::{ReadError, read_table},
    strategy,
};
use proptest::{collection::vec, prelude::*, sample::Index};
use tafla::{BuildError, TableBuilder, TableLayout, TableReader, TableSizer};

proptest! {
    #![proptest_config(common::config())]

    /// The table is read at any byte offset from an aligned address, so
    /// that its arrays are read in place when they're aligned in memory and
    /// copied when they aren't
    #[test]
    fn test_reads_back_the_values_of_a_table_with_absent_fields(
        table in strategy::presence_table(),
        byte_offset in 0..8_usize,
    ) {
        let bytes = table.to_bytes()?;
        let copy = at_byte_offset(&bytes, byte_offset);

        prop_assert_eq!(read_table(copy.bytes(), &table), Ok(table));
    }

    /// `TableBuilder` writes absent fields from its own fields, from tables
    /// recorded in place and through `TableSizer` and `TableWriter`, which
    /// the sizer adds nothing for
    #[test]
    fn test_writes_absent_fields_alike_from_a_builder_in_place_and_with_a_writer(
        table in strategy::presence_table(),
    ) {
        let built = table.to_bytes()?;
        prop_assert_eq!(table.to_bytes_in_place(), Ok(built));

        let in_place = table.nested_in_place();
        let built = in_place.to_bytes();
        prop_assume!(built.is_ok());
        prop_assert_eq!(in_place.write(), built);
    }

    #[test]
    fn test_finds_every_field_after_the_tables_own_missing(
        table in strategy::revised_table(),
        missing in 1..=4_usize,
    ) {
        let later = Table {
            missing: Some(missing),
            ..table
        };
        let bytes = later.to_bytes()?;

        prop_assert_eq!(read_table(&bytes, &later), Ok(later));
    }

    /// A reader of an earlier revision reads the fields that it knows from
    /// a table that holds more, and a reader of that revision that knows the
    /// others finds those that are absent missing
    #[test]
    fn test_reads_the_fields_of_an_earlier_revision_and_finds_the_absent_others_missing(
        table in strategy::revised_table(),
        split in any::<Index>(),
    ) {
        let bytes = table.to_bytes()?;
        let known = 1 + split.index(table.fields.len());
        let later = &table.fields[known..];
        let earlier = Table {
            fields: table.fields[..known].to_vec(),
            missing: Some(later.len()),
            ..table.clone()
        };

        let absent = later.iter().filter(|field| matches!(field, Field::Absent));
        let expected = Table {
            missing: Some(absent.count()),
            ..earlier.clone()
        };
        prop_assert_eq!(read_table(&bytes, &earlier), Ok(expected));
    }

    /// Overwritten and cut bytes read as values, absent or present fields,
    /// other counts of missing fields, or fail with a `DecodeError`, alike
    /// on their own and within larger bytes, and finding a vector item from
    /// its ends finds what iterating finds
    #[test]
    fn test_reads_corrupted_tables_with_absent_and_missing_fields_as_values_or_decode_errors(
        table in strategy::revised_table(),
        missing in 0..=4_usize,
        overwrites in vec((any::<Index>(), any::<u8>()), 1..=3),
        cut in any::<Index>(),
        byte_offset in 0..8_usize,
    ) {
        let table = Table {
            missing: Some(missing),
            ..table
        };
        let mut bytes = table.to_bytes()?;
        for (index, byte) in overwrites {
            *index.get_mut(&mut bytes) = byte;
        }
        bytes.truncate(cut.index(bytes.len() + 1));

        let copy = at_byte_offset(&bytes, byte_offset);
        let mut backing = vec![0; bytes.len() + 16];
        backing[8..8 + bytes.len()].copy_from_slice(&bytes);
        let within = &backing[8..8 + bytes.len()];

        let read = read_table(copy.bytes(), &table);
        prop_assert_eq!(&read_table(within, &table), &read);
        if let Err(error) = read {
            prop_assert!(!matches!(error, ReadError::Get { .. }), "{:?}", error);
        }
    }
}

#[test]
fn test_counts_an_absent_field_as_one_of_the_builders() {
    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    builder.set_absent().unwrap();

    assert_eq!(
        builder.set_absent(),
        Err(BuildError::TooManyFields { fields: 1 })
    );
    assert_eq!(builder.to_bytes(), Ok(vec![0; 4]));
}

#[test]
fn test_counts_an_absent_field_as_one_of_a_table_recorded_in_place() {
    let mut builder = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    builder.set_u8(1).unwrap();
    let too_many = builder.set_table_with(1, |table| {
        table.set_absent()?;
        table.set_absent()
    });
    assert_eq!(too_many, Err(BuildError::TooManyFields { fields: 1 }));

    // The failed table left the builder as it was
    builder
        .set_table_with(2, |table| {
            table.set_absent()?;
            table.set_u16(7)
        })
        .unwrap();
    let bytes = builder.to_bytes().unwrap();

    #[rustfmt::skip]
    let expected = [
        // The offsets, the uint8, and the nested table: its offsets, the
        // first of them 0, and the uint16
        8, 0, 0, 0, 9, 0, 0, 0, 1,
        0, 0, 0, 0, 8, 0, 0, 0, 7, 0,
    ];
    assert_eq!(bytes, expected);

    let reader = TableReader::new(&bytes, TableLayout::DEFAULT);
    let table = reader.root_offset(1).unwrap();
    assert_eq!(reader.field_offset(table, 0), Ok(table));
    assert_eq!(
        reader.get_u16(reader.field_offset(table, 1).unwrap()),
        Ok(7)
    );
}

/// An absent field takes no room, so that the array after it is padded from
/// where the value before it ends
#[test]
fn test_pads_an_array_after_an_absent_field_from_the_value_before_it() {
    let mut builder = TableBuilder::new(3, TableLayout::DEFAULT).unwrap();
    builder.set_u8(1).unwrap();
    builder.set_absent().unwrap();
    builder.set_f64_array([0.5].as_slice()).unwrap();

    #[rustfmt::skip]
    let expected = [
        // The offsets, of the uint8, the absent field and the array
        12, 0, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0,
        // The uint8, the padding that puts the float64 after the count at
        // 24, the count and the float64
        1, 0, 0, 0, 0, 0, 0, 0,
        1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xe0, 0x3f,
    ];
    assert_eq!(builder.to_bytes().unwrap(), expected);
}

#[test]
fn test_writes_an_absent_field_with_a_writer_and_rejects_one_past_its_table() {
    let mut sizer = TableSizer::new(2, TableLayout::DEFAULT).unwrap();
    sizer.add_u8();

    let mut writer = sizer.writer().unwrap();
    writer.set_absent(0);
    writer.set_u8(1, 9);
    assert_eq!(writer.into_bytes(), Ok(vec![0, 0, 0, 0, 8, 0, 0, 0, 9]));

    let mut writer = sizer.writer().unwrap();
    writer.set_absent(2);
    assert_eq!(
        writer.into_bytes(),
        Err(BuildError::TooManyFields { fields: 2 })
    );
}
