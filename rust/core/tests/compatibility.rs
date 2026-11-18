//! Tables that the TypeScript runtime wrote, which this crate must write
//! byte for byte from the same fields and read back. The bytes come from the
//! TypeScript `TableBuilder` with the same calls, field for field, as the
//! builders here.

use tafla::{Alignment, OffsetType, TableBuilder, TableLayout, TableReader};

const KINDS: [u8; 290] = [
    96, 0, 0, 0, 97, 0, 0, 0, 98, 0, 0, 0, 100, 0, 0, 0, 102, 0, 0, 0, 106, 0, 0, 0, 110, 0, 0, 0,
    118, 0, 0, 0, 126, 0, 0, 0, 130, 0, 0, 0, 138, 0, 0, 0, 144, 0, 0, 0, 150, 0, 0, 0, 158, 0, 0,
    0, 164, 0, 0, 0, 172, 0, 0, 0, 188, 0, 0, 0, 204, 0, 0, 0, 216, 0, 0, 0, 236, 0, 0, 0, 252, 0,
    0, 0, 0, 1, 0, 0, 16, 1, 0, 0, 20, 1, 0, 0, 128, 255, 0, 128, 255, 255, 0, 0, 0, 128, 255, 255,
    255, 255, 0, 0, 0, 0, 0, 0, 0, 128, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 192, 63, 0,
    0, 0, 0, 0, 0, 2, 192, 2, 0, 0, 0, 255, 2, 1, 0, 0, 0, 3, 0, 2, 0, 0, 0, 252, 255, 5, 0, 1, 0,
    0, 0, 6, 0, 1, 0, 0, 0, 249, 255, 255, 255, 2, 0, 0, 0, 8, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 0, 1,
    0, 0, 0, 246, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 1, 0, 0, 0, 11, 0, 0, 0, 0, 0, 0,
    0, 3, 0, 0, 0, 0, 0, 72, 65, 0, 0, 0, 128, 0, 0, 128, 127, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 43, 192, 0, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 195, 169, 226, 130, 172, 240, 159, 152,
    128, 239, 187, 191, 0, 0, 0, 0, 2, 0, 0, 0, 14, 0, 0, 0, 14, 0, 0, 0, 1, 2,
];

const DOCUMENT: [u8; 146] = [
    146, 0, 0, 0, 3, 0, 0, 0, 28, 0, 0, 0, 36, 0, 0, 0, 56, 0, 0, 0, 108, 0, 0, 0, 109, 0, 0, 0,
    82, 73, 157, 128, 38, 170, 77, 64, 16, 0, 0, 0, 69, 117, 114, 111, 112, 101, 47, 83, 116, 111,
    99, 107, 104, 111, 108, 109, 12, 0, 0, 0, 20, 0, 0, 0, 36, 0, 0, 0, 128, 98, 97, 218, 255, 255,
    255, 255, 3, 0, 0, 0, 0, 0, 0, 0, 128, 81, 1, 0, 0, 163, 2, 0, 3, 0, 0, 0, 0, 0, 174, 65, 0, 0,
    148, 65, 0, 0, 80, 192, 251, 2, 0, 0, 0, 34, 0, 0, 0, 37, 0, 0, 0, 8, 0, 0, 0, 12, 0, 0, 0, 0,
    0, 0, 63, 3, 0, 0, 0, 7, 0, 8, 0, 9, 0, 1, 2, 3,
];

const SMALL: [u8; 15] = [15, 4, 7, 12, 2, 104, 105, 2, 1, 0, 2, 0, 7, 7, 7];

const MEDIUM: [u8; 44] = [
    44, 0, 1, 2, 10, 0, 20, 0, 0, 0, 2, 0, 0, 0, 128, 63, 0, 0, 0, 64, 2, 0, 8, 0, 24, 0, 1, 42, 4,
    0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 244, 63,
];

#[test]
fn test_every_kind_of_field() {
    let mut builder = TableBuilder::new(24, TableLayout::DEFAULT).unwrap();
    builder.set_i8(i8::MIN).unwrap();
    builder.set_u8(u8::MAX).unwrap();
    builder.set_i16(i16::MIN).unwrap();
    builder.set_u16(u16::MAX).unwrap();
    builder.set_i32(i32::MIN).unwrap();
    builder.set_u32(u32::MAX).unwrap();
    builder.set_i64(i64::MIN).unwrap();
    builder.set_u64(u64::MAX).unwrap();
    builder.set_f32(1.5).unwrap();
    builder.set_f64(-2.25).unwrap();
    builder.set_i8_array(&[-1, 2]).unwrap();
    builder.set_u8_array(&[3]).unwrap();
    builder.set_i16_array(&[-4, 5]).unwrap();
    builder.set_u16_array(&[6]).unwrap();
    builder.set_i32_array(&[-7]).unwrap();
    builder.set_u32_array(&[8, 9]).unwrap();
    builder.set_i64_array(&[-10]).unwrap();
    builder.set_u64_array(&[11]).unwrap();
    builder.set_f32_array(&[12.5, -0.0, f32::INFINITY]).unwrap();
    builder.set_f64_array(&[-13.5]).unwrap();
    builder.set_f64_array(&[]).unwrap();
    builder.set_str("é€😀\u{feff}").unwrap();
    builder.set_str("").unwrap();
    builder.set_vector([&[1, 2][..], &[]]).unwrap();

    assert_eq!(builder.to_bytes().unwrap(), KINDS);

    let buffer = TableReader::new(&KINDS, TableLayout::DEFAULT);
    let at = |index| buffer.root_offset(index).unwrap();

    assert_eq!(buffer.get_i8(at(0)), Ok(i8::MIN));
    assert_eq!(buffer.get_u8(at(1)), Ok(u8::MAX));
    assert_eq!(buffer.get_i16(at(2)), Ok(i16::MIN));
    assert_eq!(buffer.get_u16(at(3)), Ok(u16::MAX));
    assert_eq!(buffer.get_i32(at(4)), Ok(i32::MIN));
    assert_eq!(buffer.get_u32(at(5)), Ok(u32::MAX));
    assert_eq!(buffer.get_i64(at(6)), Ok(i64::MIN));
    assert_eq!(buffer.get_u64(at(7)), Ok(u64::MAX));
    assert_eq!(buffer.get_f32(at(8)), Ok(1.5));
    assert_eq!(buffer.get_f64(at(9)), Ok(-2.25));
    assert_eq!(buffer.get_i8_array(at(10)).unwrap().to_vec(), [-1, 2]);
    assert_eq!(buffer.get_u8_array(at(11)), Ok(&[3][..]));
    assert_eq!(buffer.get_i16_array(at(12)).unwrap().to_vec(), [-4, 5]);
    assert_eq!(buffer.get_u16_array(at(13)).unwrap().to_vec(), [6]);
    assert_eq!(buffer.get_i32_array(at(14)).unwrap().to_vec(), [-7]);
    assert_eq!(buffer.get_u32_array(at(15)).unwrap().to_vec(), [8, 9]);
    assert_eq!(buffer.get_i64_array(at(16)).unwrap().to_vec(), [-10]);
    assert_eq!(buffer.get_u64_array(at(17)).unwrap().to_vec(), [11]);

    let float32s = buffer.get_f32_array(at(18)).unwrap();
    let bits: Vec<_> = float32s.iter().map(f32::to_bits).collect();
    assert_eq!(bits, [12.5_f32, -0.0, f32::INFINITY].map(f32::to_bits));

    assert_eq!(buffer.get_f64_array(at(19)).unwrap().to_vec(), [-13.5]);
    assert!(buffer.get_f64_array(at(20)).unwrap().is_empty());
    assert_eq!(buffer.get_str(at(21)), Ok("é€😀\u{feff}"));
    assert_eq!(buffer.get_str(at(22)), Ok(""));

    let vector = buffer.get_vector(at(23)).unwrap();
    assert_eq!(
        vector.iter().collect::<Vec<_>>(),
        [Ok(&[1, 2][..]), Ok(&[])]
    );
}

#[test]
fn test_versioned_document_with_nested_tables() {
    const LAYOUT: TableLayout<true> = TableLayout::DEFAULT.sized().versioned();

    let mut columns = TableBuilder::new(3, TableLayout::DEFAULT).unwrap();
    columns.set_i64(-631_152_000).unwrap();
    columns.set_u32_array(&[0, 86_400, 172_800]).unwrap();
    columns.set_f32_array(&[21.75, 18.5, -3.25]).unwrap();

    let mut item = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    item.set_f32(0.5).unwrap();
    item.set_u16_array(&[7, 8, 9]).unwrap();

    let mut document = TableBuilder::versioned(5, LAYOUT, 3).unwrap();
    document.set_f64(59.3293).unwrap();
    document.set_str("Europe/Stockholm").unwrap();
    document.set_table(columns).unwrap();
    document.set_i8(-5).unwrap();
    document
        .set_vector([item.into(), tafla::NestedTable::from(&[1, 2, 3])])
        .unwrap();

    assert_eq!(document.to_bytes().unwrap(), DOCUMENT);

    let buffer = TableReader::new(&DOCUMENT, LAYOUT);
    assert_eq!(buffer.size(), Ok(DOCUMENT.len()));
    assert_eq!(buffer.version(), Ok(3));
    assert_eq!(buffer.get_f64(buffer.root_offset(0).unwrap()), Ok(59.3293));
    assert_eq!(
        buffer.get_str(buffer.root_offset(1).unwrap()),
        Ok("Europe/Stockholm")
    );

    let columns = buffer.root_offset(2).unwrap();
    let field = |index| buffer.field_offset(columns, index).unwrap();
    assert_eq!(buffer.get_i64(field(0)), Ok(-631_152_000));
    assert_eq!(
        buffer.get_u32_array(field(1)).unwrap().to_vec(),
        [0, 86_400, 172_800]
    );
    assert_eq!(
        buffer.get_f32_array(field(2)).unwrap().to_vec(),
        [21.75, 18.5, -3.25]
    );
    assert_eq!(buffer.get_i8(buffer.root_offset(3).unwrap()), Ok(-5));

    let vector = buffer.get_vector(buffer.root_offset(4).unwrap()).unwrap();
    let [item, bytes] = vector.iter().collect::<Result<Vec<_>, _>>().unwrap()[..] else {
        panic!("Expected two items");
    };
    let item = TableReader::new(item, TableLayout::DEFAULT);
    assert_eq!(item.get_f32(item.root_offset(0).unwrap()), Ok(0.5));
    assert_eq!(
        item.get_u16_array(item.root_offset(1).unwrap())
            .unwrap()
            .to_vec(),
        [7, 8, 9]
    );
    assert_eq!(bytes, [1, 2, 3]);
}

#[test]
fn test_uint8_offsets_with_table_nested_as_bytes() {
    let layout = TableLayout::DEFAULT
        .sized()
        .with_offset_type(OffsetType::Uint8);

    let mut small = TableBuilder::new(3, layout).unwrap();
    small.set_str("hi").unwrap();
    small.set_u16_array(&[1, 2]).unwrap();
    small.set_table(&[7, 7, 7]).unwrap();

    assert_eq!(small.to_bytes().unwrap(), SMALL);

    let buffer = TableReader::new(&SMALL, layout);
    let position = buffer.root_offset(2).unwrap();
    assert_eq!(position % Alignment::Four.bytes(), 0);
    assert_eq!(buffer.get_str(buffer.root_offset(0).unwrap()), Ok("hi"));
    assert_eq!(
        buffer
            .get_u16_array(buffer.root_offset(1).unwrap())
            .unwrap()
            .to_vec(),
        [1, 2]
    );
    assert_eq!(SMALL[position..], [7, 7, 7]);
}

#[test]
fn test_uint16_offsets_with_vector_items_of_other_offset_types() {
    let layout = TableLayout::DEFAULT
        .sized()
        .with_offset_type(OffsetType::Uint16)
        .versioned();
    let narrow_layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint8);

    let mut narrow = TableBuilder::new(1, narrow_layout).unwrap();
    narrow.set_u8(42).unwrap();
    let mut wide = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    wide.set_f64_array(&[1.25]).unwrap();

    let mut medium = TableBuilder::versioned(2, layout, 513).unwrap();
    medium.set_f32_array(&[1.0, 2.0]).unwrap();
    medium.set_vector([narrow, wide]).unwrap();

    assert_eq!(medium.to_bytes().unwrap(), MEDIUM);

    let buffer = TableReader::new(&MEDIUM, layout);
    assert_eq!(buffer.version(), Ok(513));
    assert_eq!(
        buffer
            .get_f32_array(buffer.root_offset(0).unwrap())
            .unwrap()
            .to_vec(),
        [1.0, 2.0]
    );

    let vector = buffer.get_vector(buffer.root_offset(1).unwrap()).unwrap();
    let [narrow, wide] = vector.iter().collect::<Result<Vec<_>, _>>().unwrap()[..] else {
        panic!("Expected two items");
    };
    let narrow = TableReader::new(narrow, narrow_layout);
    let wide = TableReader::new(wide, TableLayout::DEFAULT);
    assert_eq!(narrow.get_u8(narrow.root_offset(0).unwrap()), Ok(42));
    assert_eq!(
        wide.get_f64_array(wide.root_offset(0).unwrap())
            .unwrap()
            .to_vec(),
        [1.25]
    );
}
