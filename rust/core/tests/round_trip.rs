//! Tables read back the values that they were built from. The examples show
//! builders borrowing and owning values, `ArrayView` reading arrays in place
//! or copying them, structs read from their bytes, and vector items found
//! from their ends.

mod common;

use std::borrow::Cow;

use common::{
    at_byte_offset,
    description::{Field, Layout, Nested, ScalarType, Source, Table},
    read::read_table,
    strategy,
};
use proptest::prelude::*;
use tafla::{Alignment, OffsetType, TableBuilder, TableLayout, TableReader, TableSizer};

proptest! {
    #![proptest_config(common::config())]

    /// Floats compare by their bits, so that -0 and NaN count. The table is
    /// read at any byte offset from an aligned address, so that its arrays
    /// are read in place when they're aligned in memory and copied when
    /// they aren't.
    #[test]
    fn test_tables_read_back_the_values_they_were_built_from(
        table in strategy::table(),
        byte_offset in 0..8_usize,
    ) {
        let bytes = table.to_bytes().unwrap();
        let copy = at_byte_offset(&bytes, byte_offset);

        prop_assert_eq!(read_table(copy.bytes(), &table), Ok(table));
    }
}

#[test]
fn test_round_trip_owned_values() {
    let mut builder = TableBuilder::new(3, TableLayout::DEFAULT).unwrap();
    builder.set_f64_array(vec![1.5, -2.5]).unwrap();
    builder.set_str(String::from("owned")).unwrap();
    builder.set_vector(vec![vec![1_u8, 2], vec![3]]).unwrap();

    let bytes = builder.to_bytes().unwrap();
    let reader = TableReader::new(&bytes, TableLayout::DEFAULT);

    let floats = reader
        .get_f64_array(reader.root_offset(0).unwrap())
        .unwrap();
    assert_eq!(floats.to_vec(), [1.5, -2.5]);
    assert_eq!(
        reader.get_str(reader.root_offset(1).unwrap()).unwrap(),
        "owned"
    );

    let vector = reader.get_vector(reader.root_offset(2).unwrap()).unwrap();
    assert_eq!(
        vector.iter().collect::<Vec<_>>(),
        [Ok(&[1, 2][..]), Ok(&[3])]
    );
}

#[test]
fn test_round_trip_bool_arrays() {
    let mut builder = TableBuilder::new(3, TableLayout::DEFAULT).unwrap();
    builder.set_bool_array(&[true, false, true]).unwrap();
    builder.set_bool_array(Vec::new()).unwrap();
    builder.set_bool_array(vec![false]).unwrap();
    let bytes = builder.to_bytes().unwrap();

    // Each is a uint8 array of 1 and 0, unpadded, after the three offsets:
    // a count of 3 and its bytes at 12, of 0 at 19, and of 1 at 23
    let arrays = [3, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0];
    assert_eq!(bytes[12..], arrays);

    let reader = TableReader::new(&bytes, TableLayout::DEFAULT);
    let at = |index| reader.root_offset(index).unwrap();
    assert_eq!(reader.get_bool_array(at(0)), Ok(vec![true, false, true]));
    assert_eq!(reader.get_bool_array(at(1)), Ok(vec![]));
    assert_eq!(reader.get_bool_array(at(2)), Ok(vec![false]));
    assert_eq!(reader.get_u8_array(at(0)), Ok(&[1, 0, 1][..]));

    let mut sizer = TableSizer::new(3, TableLayout::DEFAULT).unwrap();
    sizer.add_bool_array(3);
    sizer.add_bool_array(0);
    sizer.add_bool_array(1);
    let mut writer = sizer.writer().unwrap();
    writer.set_bool_array(0, &[true, false, true]);
    writer.set_bool_array(1, &[]);
    writer.set_bool_array(2, &[false]);
    assert_eq!(writer.into_bytes(), Ok(bytes));
}

/// Bytes other than 0 and 1, which no writer writes, read as true, as a
/// bool field's do
#[test]
fn test_bool_arrays_read_every_byte_but_0_as_true() {
    let bytes = [5, 0, 0, 0, 2, 0, 0x80, 0xff, 1];
    let reader = TableReader::new(&bytes, TableLayout::DEFAULT);

    assert_eq!(
        reader.get_bool_array(0),
        Ok(vec![true, false, true, true, true])
    );
    assert_eq!(reader.get_u8_array(0), Ok(&bytes[4..]));
}

#[test]
fn test_vector_items_found_from_their_ends() {
    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    builder.set_vector([&[1][..], &[], &[2, 3]]).unwrap();

    let bytes = builder.to_bytes().unwrap();
    let reader = TableReader::new(&bytes, TableLayout::DEFAULT);
    let position = reader.root_offset(0).unwrap();

    // The count and the three ends, at 4, then the items, from 20
    assert_eq!(
        bytes[position..20],
        [3, 0, 0, 0, 17, 0, 0, 0, 17, 0, 0, 0, 19, 0, 0, 0]
    );
    assert_eq!(bytes[20..], [1, 2, 3]);

    let vector = reader.get_vector(position).unwrap();
    assert_eq!(vector.len(), 3);
    assert_eq!(vector.get(2), Some(Ok(&[2, 3][..])));
    assert_eq!(vector.get(1), Some(Ok(&[][..])));
    assert_eq!(vector.get(3), None);
}

#[test]
fn test_round_trip_structs() {
    // A struct of a u16 and an f32, of 6 bytes, whose arrays are aligned to 4
    let point = |x: u16, y: f32| [&x.to_le_bytes()[..], &y.to_le_bytes()].concat();
    let points = [point(1, 1.5), point(2, -0.25)].concat();

    let mut builder = TableBuilder::new(3, TableLayout::DEFAULT).unwrap();
    builder.set_u8(7).unwrap();
    builder.set_struct(point(3, 2.5)).unwrap();
    builder
        .set_struct_array(&points, 6, Alignment::Four)
        .unwrap();

    let bytes = builder.to_bytes().unwrap();
    let reader = TableReader::new(&bytes, TableLayout::DEFAULT);

    // The struct follows the u8 at 12 unpadded, at 13
    let position = reader.root_offset(1).unwrap();
    assert_eq!(position, 13);
    assert_eq!(reader.get_struct(position, 6).unwrap(), point(3, 2.5));

    // The array's count is padded from 19 to 20, so that its structs start
    // at 24
    let position = reader.root_offset(2).unwrap();
    assert_eq!(position, 20);
    assert_eq!(reader.get_u32(position), Ok(2));

    let structs = reader.get_struct_array(position, 6).unwrap();
    let read_y = |point: &[u8; 6]| TableReader::new(point, TableLayout::DEFAULT).get_f32(2);
    let ys: Result<Vec<_>, _> = structs.as_chunks().0.iter().map(read_y).collect();
    assert_eq!(structs, points);
    assert_eq!(ys, Ok(vec![1.5, -0.25]));

    // Structs of 0 bytes take none, whatever the count
    assert_eq!(reader.get_struct_array(position, 0), Ok(&[][..]));
}

#[test]
fn test_arrays_in_place_when_aligned() {
    let values = [1.5, -0.0, f64::NAN];
    let mut builder = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    builder.set_u8_array(&[1, 2, 3]).unwrap();
    builder.set_f64_array(&values).unwrap();
    let bytes = builder.to_bytes().unwrap();

    for byte_offset in 0..8 {
        let copy = at_byte_offset(&bytes, byte_offset);
        let reader = TableReader::new(copy.bytes(), TableLayout::DEFAULT);

        let uint8s = reader.get_u8_array(reader.root_offset(0).unwrap()).unwrap();
        assert!(copy.bytes().as_ptr_range().contains(&uint8s.as_ptr()));

        let float64s = reader
            .get_f64_array(reader.root_offset(1).unwrap())
            .unwrap();
        // The array lies a multiple of 8 bytes from the table's start, and
        // is in place where the host aligns f64 to the table's offset: only
        // at 0 on most hosts, and at 0 and 4 on i686, which aligns it to 4
        let in_place =
            cfg!(target_endian = "little") && byte_offset.is_multiple_of(align_of::<f64>());

        assert_eq!(float64s.as_slice().is_some(), in_place);
        assert_eq!(matches!(float64s.to_cow(), Cow::Borrowed(_)), in_place);
        assert_eq!(bits_of(float64s.to_cow().iter().copied()), bits_of(values));
        assert_eq!(bits_of(float64s.iter()), bits_of(values));
        assert_eq!(
            bits_of(float64s.iter().rev()),
            bits_of(values.into_iter().rev())
        );
        assert_eq!(
            float64s.get(1).map(f64::to_bits),
            Some((-0.0_f64).to_bits())
        );
        assert_eq!(float64s.get(3), None);
    }
}

#[test]
fn test_builder_nested_several_times() {
    let mut nested = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    nested.set_f64_array(&[1.0, 2.0]).unwrap();
    nested.set_str("nested").unwrap();
    let own = nested.to_bytes().unwrap();

    let mut borrowed = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    borrowed.set_table(&nested).unwrap();
    borrowed.set_vector([&nested, &nested]).unwrap();

    let mut copied = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    copied.set_table_aligned(&own, Alignment::Eight).unwrap();
    copied.set_vector([&own, &own]).unwrap();

    let bytes = borrowed.to_bytes().unwrap();
    assert_eq!(bytes, copied.to_bytes().unwrap());

    // The builder is unchanged by nesting
    assert_eq!(nested.to_bytes().unwrap(), own);

    let reader = TableReader::new(&bytes, TableLayout::DEFAULT);
    let vector = reader.get_vector(reader.root_offset(1).unwrap()).unwrap();
    assert_eq!(vector.iter().collect::<Vec<_>>(), [Ok(&own[..]), Ok(&own)]);
}

/// Values of 256 bytes or more, arrays of every element type and of bools,
/// strings and the strings of a vector of them, which the builder keeps
/// rather than copying them when they're set, and copies into the table
/// when it's written, read back, in tables nested from builders, in place
/// and as bytes, and on a big-endian host, little-endian
#[test]
fn test_long_values_read_back() {
    let layout = Layout::headerless(OffsetType::Uint32);
    let nested = |from: Source, fields: Vec<Field>| Nested {
        from,
        table: Table {
            layout,
            version: None,
            fields,
        },
    };
    let floats = (0..40).map(|index| f64::from(index) - 0.5);
    let doubled = floats.clone().map(|value| value * 2.0);

    let table = Table {
        layout: Layout {
            sized: true,
            ..layout
        },
        version: None,
        fields: vec![
            Field::U8(1),
            Field::I8Array((i8::MIN..=i8::MAX).chain(0..44).collect()),
            Field::U8Array((0..=u8::MAX).chain(0..44).collect()),
            Field::I16Array((0..150).map(|index| index * -200).collect()),
            Field::U16Array((0..150).map(|index| index * 300).collect()),
            Field::I32Array((0..80).map(|index| index * -70_000).collect()),
            Field::U32Array((0..80).map(|index| index * 70_000).collect()),
            Field::I64Array((0..40).map(|index| index * -(1 << 40)).collect()),
            Field::U64Array((0..40).map(|index| index << 40).collect()),
            Field::F32Array([-0.0, f32::NAN].into_iter().cycle().take(80).collect()),
            Field::F64Array(floats.clone().collect()),
            Field::BoolArray((0..300).map(|index| index % 3 == 0).collect()),
            Field::String("tafla 𝄞 ".repeat(30)),
            Field::StringVector(vec![
                "tafla 𝄞 ".repeat(30),
                String::new(),
                "x".repeat(255),
                "y".repeat(256),
            ]),
            Field::Struct(floats.clone().map(Field::F64).collect()),
            Field::StructArray {
                types: vec![ScalarType::U8, ScalarType::F64],
                items: (0..30)
                    .map(|index| vec![Field::U8(index), Field::F64(f64::from(index))])
                    .collect(),
            },
            Field::Table {
                nested: nested(Source::Builder, vec![Field::F64Array(doubled.collect())]),
                alignment: None,
            },
            Field::Vector(vec![
                nested(Source::Bytes, vec![Field::U8Array(vec![7; 300])]),
                nested(Source::Builder, vec![Field::F64Array(floats.collect())]),
            ]),
        ],
    };

    let from_builders = table.to_bytes().unwrap();
    let in_place = table.to_bytes_in_place().unwrap();
    assert_eq!(in_place, from_builders);
    assert_eq!(read_table(&from_builders, &table), Ok(table));
}

fn bits_of(values: impl IntoIterator<Item = f64>) -> Vec<u64> {
    values.into_iter().map(f64::to_bits).collect()
}
