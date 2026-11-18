//! Reads arbitrary bytes with every getter of `TableReader`, at every position
//! and past the end, in every offset type and header, and reads every array,
//! struct array and vector that it finds there.
//!
//! A panic is a finding, and so is a read that disagrees with the bounds that
//! `SPEC.md` gives it: each read returns the bytes or value that lie where
//! the format puts them, borrowed from the input, exactly when they lie
//! within it, and the decode error otherwise.

#![no_main]

use std::{
    fmt::Debug,
    io::{self, Write},
};

use libfuzzer_sys::fuzz_target;
use tafla::{ArrayView, DecodeError, Element, OffsetType, TableLayout, TableReader, Vector};

/// The elements of an array, and items of a vector, at its start and end
/// that are read and compared with the bytes
const CHECKED: usize = 8;

const OFFSET_TYPES: [OffsetType; 3] = [OffsetType::Uint8, OffsetType::Uint16, OffsetType::Uint32];

/// Positions past any bytes, where a position plus a length overflows
const FAR_POSITIONS: [usize; 5] = [
    usize::MAX,
    usize::MAX - 1,
    usize::MAX - 3,
    usize::MAX - 7,
    isize::MAX as usize,
];

fuzz_target!(|bytes: &[u8]| {
    for offset_type in OFFSET_TYPES {
        let layout = TableLayout::DEFAULT.with_offset_type(offset_type);

        check_header(&TableReader::new(bytes, layout));
        check_header(&TableReader::new(bytes, layout.sized()));
        check_version(&TableReader::new(bytes, layout.versioned()));
        check_version(&TableReader::new(bytes, layout.sized().versioned()));

        let buffer = TableReader::new(bytes, layout);
        for position in positions(bytes.len()) {
            check_position(&buffer, position);
        }
    }
});

/// Every position within the bytes, a few past them, and those far past
fn positions(len: usize) -> impl Iterator<Item = usize> {
    (0..len + 9).chain(FAR_POSITIONS)
}

/// Reads the table's size and its root fields' offsets
fn check_header<const V: bool>(buffer: &TableReader<'_, V>) {
    let bytes = buffer.bytes();
    let layout = buffer.layout();
    let width = layout.offset_size();

    let size = buffer.size();
    if layout.is_sized() {
        check_result(bytes, size, slot(bytes, 0, layout.offset_type()));
    } else {
        assert_eq!(size, Ok(bytes.len()));
    }

    let indices = (0..bytes.len() / width + 2).chain(FAR_POSITIONS);
    for index in indices {
        let position = index as u128 * width as u128 + layout.header_size() as u128;
        let expected = slot(bytes, position, layout.offset_type());

        check_result(bytes, buffer.root_offset(index), expected);
    }
}

fn check_version(buffer: &TableReader<'_, true>) {
    let bytes = buffer.bytes();
    let layout = buffer.layout();
    let position = if layout.is_sized() {
        layout.offset_size()
    } else {
        0
    };
    let expected = slot(bytes, position as u128, layout.offset_type())
        .map(|version| u32::try_from(version).expect("A slot holds at most 4 bytes"));

    check_result(bytes, buffer.version(), expected);
    check_header(buffer);
}

/// Reads every type at `position`
fn check_position(buffer: &TableReader<'_>, position: usize) {
    let bytes = buffer.bytes();
    let offset_type = buffer.layout().offset_type();

    check_scalars(buffer, position);
    check_arrays(buffer, position);

    let string_bytes = array(bytes, offset_type, position, 1);
    check_result(bytes, buffer.get_u8_array(position), string_bytes);
    let string = string_bytes.and_then(|bytes| std::str::from_utf8(bytes).ok());
    check_result(bytes, buffer.get_str(position), string);

    let remaining = bytes.len().saturating_sub(position);
    for size in [0, 1, 2, 3, 8, 13, remaining, remaining + 1, usize::MAX] {
        let expected = bytes.get(position..).and_then(|bytes| bytes.get(..size));
        check_result(bytes, buffer.get_struct(position, size), expected);

        let expected = array(bytes, offset_type, position, size);
        check_result(bytes, buffer.get_struct_array(position, size), expected);
    }

    for index in [0, 1, 2, usize::MAX] {
        let slot_position = position as u128 + index as u128 * offset_type.size() as u128;
        let expected = slot(bytes, slot_position, offset_type).map(|offset| position + offset);

        check_result(bytes, buffer.field_offset(position, index), expected);
    }

    check_vector(buffer, position);
}

macro_rules! check_scalar {
    ($buffer:expr, $position:expr, $($getter:ident -> $type:ty),*) => {$({
        let bytes = $buffer.bytes();
        let expected = bytes
            .get($position..)
            .and_then(<[u8]>::first_chunk)
            .copied();

        // Compared by their bytes, which are the bits of floats
        let read = $buffer.$getter($position).map(<$type>::to_le_bytes);
        check_result(bytes, read, expected);
    })*};
}

fn check_scalars(buffer: &TableReader<'_>, position: usize) {
    check_scalar!(
        buffer, position,
        get_i8 -> i8, get_u8 -> u8,
        get_i16 -> i16, get_u16 -> u16,
        get_i32 -> i32, get_u32 -> u32,
        get_i64 -> i64, get_u64 -> u64,
        get_f32 -> f32, get_f64 -> f64
    );
}

macro_rules! check_array {
    ($buffer:expr, $position:expr, $($getter:ident -> $type:ty),*) => {$({
        let bytes = $buffer.bytes();
        let offset_type = $buffer.layout().offset_type();
        let expected = array(bytes, offset_type, $position, size_of::<$type>());

        match $buffer.$getter($position) {
            Ok(view) => {
                check_in(view.as_bytes(), expected);
                check_view(&view, <$type>::to_le_bytes);
            }
            Err(error) => check_error(bytes, &error, expected.is_none()),
        }
    })*};
}

fn check_arrays(buffer: &TableReader<'_>, position: usize) {
    check_array!(
        buffer, position,
        get_i8_array -> i8,
        get_i16_array -> i16, get_u16_array -> u16,
        get_i32_array -> i32, get_u32_array -> u32,
        get_i64_array -> i64, get_u64_array -> u64,
        get_f32_array -> f32, get_f64_array -> f64
    );
}

/// Checks that every way of reading an array's elements reads the elements
/// of its bytes, comparing them by their little-endian bytes. Only the first
/// and last `CHECKED` elements are compared, since every position may hold
/// an array as long as the bytes, and reading each in full would take time
/// quadratic in their length.
fn check_view<T: Element + Debug, const N: usize>(
    view: &ArrayView<'_, T>,
    to_le_bytes: fn(T) -> [u8; N],
) {
    let bytes = view.as_bytes();
    let (chunks, rest) = bytes.as_chunks::<N>();
    let len = chunks.len();

    assert_eq!(rest, []);
    assert_eq!(view.len(), len);
    assert_eq!(view.is_empty(), len == 0);
    assert_eq!(view.iter().len(), len);

    let first = &chunks[..len.min(CHECKED)];
    let last = &chunks[len.saturating_sub(CHECKED)..];
    assert!(
        view.iter()
            .take(CHECKED)
            .map(to_le_bytes)
            .eq(first.iter().copied())
    );
    assert!(
        view.iter()
            .rev()
            .take(CHECKED)
            .map(to_le_bytes)
            .eq(last.iter().rev().copied())
    );

    for index in [0, len / 2, len.saturating_sub(1), len, usize::MAX] {
        assert_eq!(view.get(index).map(to_le_bytes), chunks.get(index).copied());
    }

    if len <= CHECKED {
        assert!(
            view.to_vec()
                .into_iter()
                .map(to_le_bytes)
                .eq(chunks.iter().copied())
        );
        assert!(
            view.to_cow()
                .iter()
                .copied()
                .map(to_le_bytes)
                .eq(chunks.iter().copied())
        );
    }

    if let Some(slice) = view.as_slice() {
        assert!(std::ptr::eq(slice.as_ptr().cast::<u8>(), bytes.as_ptr()));
        assert_eq!(slice.len(), len);
    }
}

/// Reads the vector at `position` and each of its items, by iterating, by
/// index and in place
fn check_vector(buffer: &TableReader<'_>, position: usize) {
    let bytes = buffer.bytes();
    let offset_type = buffer.layout().offset_type();
    let width = offset_type.size() as u128;

    // A count, and an end for each item after it, within the bytes
    let count = slot(bytes, position as u128, offset_type);
    let ends = count
        .filter(|&count| position as u128 + width * (count as u128 + 1) <= bytes.len() as u128);

    let vector = match buffer.get_vector(position) {
        Ok(vector) => vector,
        Err(error) => return check_error(bytes, &error, ends.is_none()),
    };
    assert_eq!(Some(vector.len()), ends);
    assert_eq!(vector.is_empty(), ends == Some(0));

    check_items(bytes, position, offset_type, &vector);
}

/// Reads the first `CHECKED` items, by iterating, by index and in place, and
/// the last by index
fn check_items(bytes: &[u8], position: usize, offset_type: OffsetType, vector: &Vector<'_>) {
    let len = vector.len();

    assert_eq!(vector.iter().len(), len);
    assert_eq!(vector.iter_in_place().len(), len);

    let iterated = vector.iter().zip(vector.iter_in_place()).take(CHECKED);
    for (index, (item, item_in_place)) in iterated.enumerate() {
        check_item(bytes, position, offset_type, index, &item, &item_in_place);

        assert_eq!(vector.get(index), Some(item));
        assert_eq!(vector.get_in_place(index), Some(item_in_place));
    }

    if let Some(index) = len.checked_sub(1) {
        let item = vector.get(index).expect("The item is within the vector");
        let item_in_place = vector
            .get_in_place(index)
            .expect("The item is within the vector");

        check_item(bytes, position, offset_type, index, &item, &item_in_place);
    }

    assert_eq!(vector.get(len), None);
    assert_eq!(vector.get_in_place(usize::MAX), None);
}

/// Checks item `index` of the vector at `position`, which starts where the
/// one before it ends, and the first after the count and the ends
fn check_item(
    bytes: &[u8],
    position: usize,
    offset_type: OffsetType,
    index: usize,
    item: &Result<&[u8], DecodeError>,
    item_in_place: &Result<&[u8], DecodeError>,
) {
    let width = offset_type.size();
    let count =
        slot(bytes, position as u128, offset_type).expect("The count lies within the bytes");
    let end_of = |index: usize| {
        slot(bytes, (position + (index + 1) * width) as u128, offset_type)
            .expect("The ends lie within the bytes")
    };

    let start = index.checked_sub(1).map_or((count + 1) * width, end_of);
    let end = end_of(index);
    let expected = (start <= end)
        .then(|| bytes[position..].get(start..end))
        .flatten();

    match item {
        Ok(item) => check_in(item, expected),
        Err(DecodeError::ItemEndsBeforeStart { .. }) => assert!(start > end),
        Err(error) => check_error(bytes, error, expected.is_none()),
    }

    match item_in_place {
        Ok(item_in_place) => {
            let rest = &bytes[position + start..];
            assert!(expected.is_some());
            assert!(std::ptr::eq(*item_in_place, rest));
        }
        Err(error) => assert_eq!(Err(error), item.as_ref().map(|_| ())),
    }
}

/// The value of the slot as wide as `offset_type` at `position`, or `None`
/// when it doesn't lie within the bytes
fn slot(bytes: &[u8], position: u128, offset_type: OffsetType) -> Option<usize> {
    let slot = range(bytes, position, offset_type.size() as u128)?;

    Some(
        slot.iter()
            .rev()
            .fold(0, |value, &byte| value << 8 | usize::from(byte)),
    )
}

/// The bytes of the elements of `element_size` bytes after the length prefix
/// at `position`, or `None` when the prefix or the elements don't lie within
/// the bytes
fn array(
    bytes: &[u8],
    offset_type: OffsetType,
    position: usize,
    element_size: usize,
) -> Option<&[u8]> {
    let length = slot(bytes, position as u128, offset_type)?;
    let start = position as u128 + offset_type.size() as u128;

    range(bytes, start, length as u128 * element_size as u128)
}

fn range(bytes: &[u8], position: u128, byte_length: u128) -> Option<&[u8]> {
    if position + byte_length > bytes.len() as u128 {
        return None;
    }

    // Within the bytes, so both fit in a usize
    Some(&bytes[position as usize..][..byte_length as usize])
}

/// Checks that a read returns the expected value, or fails with the decode
/// error when there's none
fn check_result<T: PartialEq + Debug>(
    bytes: &[u8],
    read: Result<T, DecodeError>,
    expected: Option<T>,
) {
    match (read, expected) {
        (Ok(value), expected) => assert_eq!(Some(value), expected),
        (Err(error), expected) => check_error(bytes, &error, expected.is_none()),
    }
}

/// Checks that `read` is the expected bytes, borrowed from the input rather
/// than copied
fn check_in(read: &[u8], expected: Option<&[u8]>) {
    let expected = expected.expect("A read succeeds only within the bytes");

    assert!(std::ptr::eq(read, expected));
}

/// Checks that a read failed where it should have, with an error that
/// describes the bytes
fn check_error(bytes: &[u8], error: &DecodeError, expected: bool) {
    assert!(expected, "{error}");
    // Formatted into nothing, since most reads fail, and allocating each
    // message slowed the fuzzer down
    write!(io::sink(), "{error}").expect("An error formats");

    match *error {
        DecodeError::OutOfRange { available, .. } => assert_eq!(available, bytes.len()),
        DecodeError::InvalidUtf8 { position, .. } => assert!(position < bytes.len()),
        DecodeError::ItemEndsBeforeStart { start, end } => assert!(end < start),
        _ => panic!("A read failed with {error:?}"),
    }
}
