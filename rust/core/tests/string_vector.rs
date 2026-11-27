//! Vectors of strings read back the strings they were written with, whole,
//! by iterating and each from its ends alone, and are written alike by a
//! builder and by a sizer and its writer, as a vector of the strings' UTF-8.
//! Reading every string reads what reading each string reads, and fails
//! where any of them fails, whatever the bytes. Readers reject what isn't a
//! vector of strings with a `DecodeError`, and writers what they can't
//! write. The examples document the errors.

mod common;

use std::error::Error;

use common::at_byte_offset;
use proptest::{
    char,
    collection::vec,
    prelude::*,
    sample::{Index, select},
};
use tafla::{
    BuildError, DecodeError, OffsetType, StringVector, TableBuilder, TableLayout, TableReader,
    TableSizer,
};

const OFFSET_TYPES: [OffsetType; 3] = [OffsetType::Uint8, OffsetType::Uint16, OffsetType::Uint32];

proptest! {
    #![proptest_config(common::config())]

    #[test]
    fn test_reads_back_every_string_whole_by_iterating_and_each_from_its_ends(
        strings in strings(),
        layout in layout(),
        byte_offset in 0..8_usize,
    ) {
        let bytes = table_of(&strings, layout);
        prop_assume!(bytes.is_ok());
        let copy = at_byte_offset(&bytes?, byte_offset);

        let reader = TableReader::new(copy.bytes(), layout);
        let position = reader.root_offset(0)?;
        let view = reader.get_str_vector(position)?;
        let count = view.len();

        prop_assert_eq!(count, strings.len());
        prop_assert_eq!(view.is_empty(), strings.is_empty());
        prop_assert_eq!(&view.to_vec()?, &strings);
        prop_assert_eq!(&view.iter().collect::<Result<Vec<_>, _>>()?, &strings);
        for (index, string) in strings.iter().enumerate() {
            prop_assert_eq!(view.get(index), Some(Ok(string.as_str())));
            let item = reader.get_str_vector_item(position, count, index);
            prop_assert_eq!(item, Some(Ok(string.as_str())));
        }
        prop_assert_eq!(view.get(count), None);
        prop_assert_eq!(reader.get_str_vector_item(position, count, count), None);
    }

    /// The bytes of a vector whose items are the strings' UTF-8, which a
    /// vector of tables would hold
    #[test]
    fn test_writes_the_bytes_of_a_vector_of_the_strings_utf8(
        strings in strings(),
        layout in layout(),
    ) {
        let mut builder = TableBuilder::new(1, layout)?;
        let items = strings.iter().map(String::as_bytes);
        let as_items = builder.set_vector(items).and_then(|()| builder.to_bytes());

        prop_assert_eq!(table_of(&strings, layout), as_items);
    }

    /// The table's offset, the count and an end for each string, and the
    /// strings' UTF-8, without padding
    #[test]
    fn test_takes_an_end_and_its_utf8_for_each_string(
        strings in strings(),
        offset_type in select(OFFSET_TYPES.to_vec()),
    ) {
        let layout = TableLayout::DEFAULT.with_offset_type(offset_type);
        let bytes = table_of(&strings, layout);
        prop_assume!(bytes.is_ok());

        let width = offset_type.size();
        let utf8: usize = strings.iter().map(String::len).sum();
        prop_assert_eq!(bytes?.len(), width + width + width * strings.len() + utf8);
    }

    #[test]
    fn test_a_sizer_and_its_writer_write_what_a_builder_writes(
        strings in strings(),
        before in vec(any::<u8>(), 0..5),
        layout in layout(),
    ) {
        let mut builder = TableBuilder::new(2, layout)?;
        builder.set_u8_array(&before)?;
        let built = builder.set_str_vector(as_strs(&strings)).and_then(|()| builder.to_bytes());

        let mut sizer = TableSizer::new(2, layout)?;
        sizer.add_u8_array(before.len());
        sizer.add_str_vector(&strings);
        let written = sizer.writer().and_then(|mut writer| {
            writer.set_u8_array(0, &before);
            writer.set_str_vector(1, &strings);
            writer.into_bytes()
        });

        prop_assert_eq!(written, built);
    }

    /// Bytes overwritten anywhere after the table's offset: the vector's
    /// count, ends and strings, including ends that split characters
    #[test]
    fn test_reading_every_string_reads_what_reading_each_reads_whatever_overwrites_it(
        strings in prop_oneof![strings(), long_strings()],
        overwrites in vec((any::<Index>(), any::<u8>()), 1..=4),
        offset_type in select(OFFSET_TYPES.to_vec()),
    ) {
        let layout = TableLayout::DEFAULT.with_offset_type(offset_type);
        let bytes = table_of(&strings, layout);
        prop_assume!(bytes.is_ok());
        let mut bytes = bytes?;

        let width = offset_type.size();
        for (at, byte) in overwrites {
            let at = width + at.index(bytes.len() - width);
            bytes[at] = byte;
        }

        let reader = TableReader::new(&bytes, layout);
        if let Ok(view) = reader.get_str_vector(width) {
            check_every_string(&reader, width, view)?;
        }
    }

    /// Any bytes, read as a vector of strings at any position, in any
    /// offset type
    #[test]
    fn test_reading_every_string_reads_what_reading_each_reads_from_any_bytes(
        bytes in vec(prop_oneof![4 => 0..8_u8, 4 => any::<u8>(), 2 => 0x80..0xc0_u8], 0..96),
        offset_type in select(OFFSET_TYPES.to_vec()),
        position in 0..16_usize,
    ) {
        let reader = TableReader::new(&bytes, TableLayout::DEFAULT.with_offset_type(offset_type));
        if let Ok(view) = reader.get_str_vector(position) {
            check_every_string(&reader, position, view)?;
        }
    }
}

/// Checks that reading every string of `view`, at `position`, reads the
/// strings that reading each reads, by iterating, by index and from the
/// count, and fails with the error of the first that fails
fn check_every_string(
    reader: &TableReader<'_>,
    position: usize,
    view: StringVector<'_>,
) -> Result<(), TestCaseError> {
    let count = view.len();
    let each: Vec<_> = view.iter().collect();

    for (index, string) in each.iter().enumerate() {
        prop_assert_eq!(view.get(index), Some(string.clone()));
        let item = reader.get_str_vector_item(position, count, index);
        prop_assert_eq!(item, Some(string.clone()));
    }

    let each: Result<Vec<_>, _> = each.into_iter().collect();
    prop_assert_eq!(view.to_vec(), each);

    Ok(())
}

/// Lists of up to 8 strings, of ASCII, of characters of two, three and four
/// bytes in UTF-8, and of U+FEFF, empty too
fn strings() -> impl Strategy<Value = Vec<String>> {
    let character = prop_oneof![
        3 => char::range('a', 'z'),
        1 => Just('\u{feff}'),
        1 => char::range('\u{80}', '\u{7ff}'),
        1 => char::range('\u{800}', '\u{ffff}'),
        1 => char::range('\u{1_0000}', '\u{10_ffff}'),
    ];
    let string = vec(character, 0..12).prop_map(String::from_iter);

    vec(string, 0..=8)
}

/// Lists of 100 to 600 strings of ASCII, but for a few of any characters,
/// whose run of bytes is longer than a word many times over
fn long_strings() -> impl Strategy<Value = Vec<String>> {
    let others = vec((any::<Index>(), any::<String>()), 0..4);

    (100..600_usize, 1..40_usize, others).prop_map(|(count, length, others)| {
        let mut strings: Vec<String> = (0..count)
            .map(|index| format!("{index:x>length$}"))
            .collect();
        for (at, string) in others {
            *at.get_mut(&mut strings) = string;
        }

        strings
    })
}

fn layout() -> impl Strategy<Value = TableLayout> {
    let layouts =
        OFFSET_TYPES.map(|offset_type| TableLayout::DEFAULT.with_offset_type(offset_type));
    (select(layouts.to_vec()), any::<bool>())
        .prop_map(|(layout, sized)| if sized { layout.sized() } else { layout })
}

/// The bytes of a table whose only field is a vector of `strings`
fn table_of(strings: &[String], layout: TableLayout) -> Result<Vec<u8>, BuildError> {
    let mut builder = TableBuilder::new(1, layout)?;
    builder.set_str_vector(as_strs(strings))?;

    builder.to_bytes()
}

fn as_strs(strings: &[String]) -> Vec<&str> {
    strings.iter().map(String::as_str).collect()
}

/// The vector of strings of a table whose only field is a vector of
/// `strings`, read from `bytes`, which they were written into
fn view_of(bytes: &[u8]) -> StringVector<'_> {
    let reader = TableReader::new(bytes, TableLayout::DEFAULT);

    reader.get_str_vector(4).unwrap()
}

#[test]
fn test_borrows_strings_from_the_bytes() {
    let bytes = table_of(
        &["Oslo".to_owned(), "東京".to_owned()],
        TableLayout::DEFAULT,
    )
    .unwrap();
    let view = view_of(&bytes);

    // After the table's offset, the count and two ends
    let oslo = view.get(0).unwrap().unwrap();
    assert!(std::ptr::eq(oslo.as_ptr(), &raw const bytes[16]));
    let all = view.to_vec().unwrap();
    assert!(std::ptr::eq(all[1].as_ptr(), &raw const bytes[20]));
}

#[test]
fn test_reads_strings_long_and_short_of_ascii_and_others() {
    let strings = [
        "a".repeat(33),
        String::new(),
        "é".repeat(40),
        "東".repeat(300),
        "🌍".repeat(5),
        "b".repeat(4096),
    ];
    let bytes = table_of(&strings, TableLayout::DEFAULT).unwrap();
    let view = view_of(&bytes);

    assert_eq!(view.to_vec().unwrap(), strings);
    assert_eq!(view.get(3), Some(Ok(strings[3].as_str())));
}

#[test]
fn test_fails_for_the_strings_on_either_side_of_an_end_that_splits_a_character() {
    let strings = ["a", "東", "京", "b"].map(str::to_owned);
    let mut bytes = table_of(&strings, TableLayout::DEFAULT).unwrap();
    // The end of the second string, after the table's offset, the count and
    // the first end, a byte into the third string's character
    bytes[12] += 1;
    let view = view_of(&bytes);

    assert_eq!(view.get(0), Some(Ok("a")));
    assert!(matches!(
        view.get(1),
        Some(Err(DecodeError::InvalidUtf8 { position: 25, .. }))
    ));
    assert!(matches!(
        view.get(2),
        Some(Err(DecodeError::InvalidUtf8 { position: 29, .. }))
    ));
    assert_eq!(view.get(3), Some(Ok("b")));
    assert_eq!(view.to_vec(), Err(view.get(1).unwrap().unwrap_err()));
}

#[test]
fn test_names_the_first_byte_of_a_string_that_isnt_valid_utf8() {
    for length in [3, 40] {
        let strings = ["a".to_owned(), "b".repeat(length)];
        let mut bytes = table_of(&strings, TableLayout::DEFAULT).unwrap();
        // The second string starts after the offset, the count, two ends and
        // the first string
        bytes[17] = 0xff;
        let view = view_of(&bytes);

        let error = view.get(1).unwrap().unwrap_err();
        assert!(matches!(
            error,
            DecodeError::InvalidUtf8 { position: 17, .. }
        ));
        assert_eq!(error.to_string(), "The string at 17 isn't valid UTF-8");
        assert!(error.source().is_some());
        assert_eq!(view.to_vec(), Err(error));
    }
}

#[test]
fn test_rejects_a_string_that_ends_before_the_string_before_it() {
    // A count of 2 starts the first string after the ends, at 12, and its
    // end is 13, but the second's is 12
    let bytes = [2, 0, 0, 0, 13, 0, 0, 0, 12, 0, 0, 0, b'a'];
    let reader = TableReader::new(&bytes, TableLayout::DEFAULT);
    let view = reader.get_str_vector(0).unwrap();

    let error = DecodeError::ItemEndsBeforeStart { start: 13, end: 12 };
    assert_eq!(view.get(0), Some(Ok("a")));
    assert_eq!(view.get(1), Some(Err(error.clone())));
    assert_eq!(view.iter().nth(1), Some(Err(error.clone())));
    assert_eq!(
        reader.get_str_vector_item(0, 2, 1),
        Some(Err(error.clone()))
    );
    assert_eq!(view.to_vec(), Err(error));
}

#[test]
fn test_rejects_a_string_that_ends_past_the_bytes() {
    let bytes = [1, 0, 0, 0, 10, 0, 0, 0, b'a'];
    let view = TableReader::new(&bytes, TableLayout::DEFAULT)
        .get_str_vector(0)
        .unwrap();

    let error = DecodeError::OutOfRange {
        position: 8,
        byte_length: 2,
        available: 9,
    };
    assert_eq!(view.get(0), Some(Err(error.clone())));
    assert_eq!(view.to_vec(), Err(error));
}

#[test]
fn test_rejects_an_impossible_count_before_anything_allocates_for_it() {
    let reader = TableReader::new(&[255, 255, 255, 255], TableLayout::DEFAULT);

    let error = DecodeError::OutOfRange {
        position: 4,
        byte_length: 4 * 0xffff_ffff,
        available: 4,
    };
    assert_eq!(reader.get_str_vector(0).unwrap_err(), error);

    // A count that the caller passes is checked to leave its ends within the
    // bytes too, however large
    for count in [1, 0xffff_ffff, usize::MAX] {
        let item = reader.get_str_vector_item(0, count, 0);
        assert!(
            matches!(item, Some(Err(DecodeError::OutOfRange { .. }))),
            "{count}: {item:?}"
        );
    }
    let item = reader.get_str_vector_item(usize::MAX, usize::MAX, usize::MAX - 1);
    assert!(matches!(item, Some(Err(DecodeError::OutOfRange { .. }))));
}

#[test]
fn test_finds_no_string_past_the_count() {
    let bytes = table_of(&["a".to_owned(), "b".to_owned()], TableLayout::DEFAULT).unwrap();
    let reader = TableReader::new(&bytes, TableLayout::DEFAULT);
    let view = reader.get_str_vector(4).unwrap();

    for index in [2, usize::MAX] {
        assert_eq!(view.get(index), None);
        assert_eq!(reader.get_str_vector_item(4, 2, index), None);
    }
    assert_eq!(reader.get_str_vector_item(4, 0, 0), None);
    assert_eq!(view.iter().len(), 2);
    assert_eq!(view.iter().nth(2), None);
}

#[test]
fn test_fills_a_table_of_short_offsets_and_rejects_a_byte_more() {
    for offset_type in [OffsetType::Uint8, OffsetType::Uint16] {
        let layout = TableLayout::DEFAULT.with_offset_type(offset_type);
        let width = offset_type.size();
        let max = offset_type.max_value() as usize;
        // Two offsets, a uint8, and the vector's count and its one end
        let room = max - 4 * width - 1;
        let longest = "a".repeat(room);
        let too_long = "a".repeat(room + 1);

        let mut builder = TableBuilder::new(2, layout).unwrap();
        builder.set_u8(7).unwrap();
        assert_eq!(
            builder.set_str_vector(vec![too_long.as_str()]),
            Err(BuildError::TooLong {
                index: 1,
                length: max as u64 + 1,
                offset_type
            })
        );
        builder.set_str_vector(vec![longest.as_str()]).unwrap();
        let bytes = builder.to_bytes().unwrap();

        let reader = TableReader::new(&bytes, layout);
        assert_eq!(bytes.len(), max);
        assert_eq!(reader.get_u8(reader.root_offset(0).unwrap()), Ok(7));
        let view = reader.get_str_vector(reader.root_offset(1).unwrap());
        assert_eq!(view.unwrap().to_vec(), Ok(vec![longest.as_str()]));
    }
}

#[test]
fn test_fails_for_strings_longer_or_more_than_those_sized() {
    let mut sizer = TableSizer::new(1, TableLayout::DEFAULT).unwrap();
    sizer.add_str_vector(&["a", "b"]);
    assert_eq!(sizer.byte_length(), Ok(4 + 4 + 2 * 4 + 2));

    for written in [&["a", "bc"][..], &["a", "b", ""]] {
        let mut writer = sizer.writer().unwrap();
        writer.set_str_vector(0, written);

        assert!(
            matches!(
                writer.into_bytes(),
                Err(BuildError::NotAsSized { length: 18, .. })
            ),
            "{written:?}"
        );
    }
}

#[test]
fn test_fails_for_strings_shorter_than_those_sized() {
    let mut sizer = TableSizer::new(1, TableLayout::DEFAULT).unwrap();
    sizer.add_str_vector(&["ab"]);
    let mut writer = sizer.writer().unwrap();
    writer.set_str_vector(0, &["a"]);

    assert_eq!(
        writer.into_bytes(),
        Err(BuildError::NotAsSized {
            written: 13,
            length: 14
        })
    );
}

#[test]
fn test_writes_strings_borrowed_owned_and_of_any_type_alike() {
    let owned = vec!["a".to_owned(), "é".to_owned()];
    let borrowed = as_strs(&owned);
    let expected = table_of(&owned, TableLayout::DEFAULT).unwrap();

    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    builder.set_str_vector(borrowed.as_slice()).unwrap();
    assert_eq!(builder.to_bytes(), Ok(expected.clone()));

    let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
    builder.set_str_vector(&["a", "é"]).unwrap();
    assert_eq!(builder.to_bytes(), Ok(expected.clone()));

    let mut sizer = TableSizer::new(1, TableLayout::DEFAULT).unwrap();
    sizer.add_str_vector(&owned);
    let mut writer = sizer.writer().unwrap();
    writer.set_str_vector(0, &borrowed);
    assert_eq!(writer.into_bytes(), Ok(expected));
}
