//! Tables read back the values that they were built from. The examples show
//! builders borrowing and owning values, `ArrayView` reading arrays in place
//! or copying them, structs read from their bytes, and vector items found
//! from their ends.

mod common;

use std::borrow::Cow;

use common::{at_byte_offset, read::read_table, strategy};
use proptest::prelude::*;
use tafla::{Alignment, TableBuilder, TableLayout, TableReader};

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
    let buffer = TableReader::new(&bytes, TableLayout::DEFAULT);

    let floats = buffer
        .get_f64_array(buffer.root_offset(0).unwrap())
        .unwrap();
    assert_eq!(floats.to_vec(), [1.5, -2.5]);
    assert_eq!(
        buffer.get_str(buffer.root_offset(1).unwrap()).unwrap(),
        "owned"
    );

    let vector = buffer.get_vector(buffer.root_offset(2).unwrap()).unwrap();
    assert_eq!(
        vector.iter().collect::<Vec<_>>(),
        [Ok(&[1, 2][..]), Ok(&[3])]
    );
}

#[test]
fn test_vector_items_found_from_their_ends() {
    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    builder.set_vector([&[1][..], &[], &[2, 3]]).unwrap();

    let bytes = builder.to_bytes().unwrap();
    let buffer = TableReader::new(&bytes, TableLayout::DEFAULT);
    let position = buffer.root_offset(0).unwrap();

    // The count and the three ends, at 4, then the items, from 20
    assert_eq!(
        bytes[position..20],
        [3, 0, 0, 0, 17, 0, 0, 0, 17, 0, 0, 0, 19, 0, 0, 0]
    );
    assert_eq!(bytes[20..], [1, 2, 3]);

    let vector = buffer.get_vector(position).unwrap();
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
    let buffer = TableReader::new(&bytes, TableLayout::DEFAULT);

    // The struct follows the u8 at 12 unpadded, at 13
    let position = buffer.root_offset(1).unwrap();
    assert_eq!(position, 13);
    assert_eq!(buffer.get_struct(position, 6).unwrap(), point(3, 2.5));

    // The array's count is padded from 19 to 20, so that its structs start
    // at 24
    let position = buffer.root_offset(2).unwrap();
    assert_eq!(position, 20);
    assert_eq!(buffer.get_u32(position), Ok(2));

    let structs = buffer.get_struct_array(position, 6).unwrap();
    let read_y = |point: &[u8; 6]| TableReader::new(point, TableLayout::DEFAULT).get_f32(2);
    let ys: Result<Vec<_>, _> = structs.as_chunks().0.iter().map(read_y).collect();
    assert_eq!(structs, points);
    assert_eq!(ys, Ok(vec![1.5, -0.25]));

    // Structs of 0 bytes take none, whatever the count
    assert_eq!(buffer.get_struct_array(position, 0), Ok(&[][..]));
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
        let buffer = TableReader::new(copy.bytes(), TableLayout::DEFAULT);

        let uint8s = buffer.get_u8_array(buffer.root_offset(0).unwrap()).unwrap();
        assert!(copy.bytes().as_ptr_range().contains(&uint8s.as_ptr()));

        let float64s = buffer
            .get_f64_array(buffer.root_offset(1).unwrap())
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

    let buffer = TableReader::new(&bytes, TableLayout::DEFAULT);
    let vector = buffer.get_vector(buffer.root_offset(1).unwrap()).unwrap();
    assert_eq!(vector.iter().collect::<Vec<_>>(), [Ok(&own[..]), Ok(&own)]);
}

fn bits_of(values: impl IntoIterator<Item = f64>) -> Vec<u64> {
    values.into_iter().map(f64::to_bits).collect()
}
