//! What the tests of the generated Rust share: the values of a table's fields,
//! which the tests read back with the generated getters and build the same
//! table from by hand with `TableBuilder`, arbitrary values, placements
//! and corruptions of tables, and the check that tafla-gen decodes a table
//! as JSON that encodes back into its bytes.

// Each test crate uses a part of these
#![allow(dead_code)]

use std::fmt::{self, Debug};

use proptest::arbitrary::Arbitrary;
use proptest::collection::vec;
use proptest::prelude::*;
use tafla::{Alignment, ArrayView, BuildError, DecodeError, Element, TableBuilder, TableLayout};
use tafla_gen::{DecodeOptions, Root};

/// The cases of each property: enough to draw every kind of edge value
/// several times, while `cargo test` stays fast
pub const CASES: u32 = 128;

/// A float compared by its bits, so that -0 differs from 0 and a NaN equals
/// a NaN of the same sign and payload only. tafla writes a float's IEEE 754
/// bits as they are, and the Rust runtime reads them back as they are.
#[derive(Clone, Copy)]
pub struct Bits<T>(pub T);

impl PartialEq for Bits<f32> {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl PartialEq for Bits<f64> {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Debug for Bits<f32> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} ({:#010x})", self.0, self.0.to_bits())
    }
}

impl Debug for Bits<f64> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} ({:#018x})", self.0, self.0.to_bits())
    }
}

/// The value of a field of a table, of its type in the schema
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A bool, which tables and structs store as the `u8` 1 or 0
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(Bits<f32>),
    F64(Bits<f64>),
    I8Array(Vec<i8>),
    U8Array(Vec<u8>),
    I16Array(Vec<i16>),
    U16Array(Vec<u16>),
    I32Array(Vec<i32>),
    U32Array(Vec<u32>),
    I64Array(Vec<i64>),
    U64Array(Vec<u64>),
    F32Array(Vec<Bits<f32>>),
    F64Array(Vec<Bits<f64>>),
    String(String),
    /// A struct's scalars, those of a struct within it in its place
    Struct(Vec<Value>),
    /// The scalars of each struct of an array, of `size` bytes, aligned to
    /// `alignment`
    StructArray {
        size: usize,
        alignment: Alignment,
        items: Vec<Vec<Value>>,
    },
    /// A nested table's fields, which the generated code writes in place, in
    /// the default layout, at the alignment that `TableBuilder` pads its
    /// builder to
    Table(Vec<Value>),
    /// The fields of a vector's tables, which the generated code writes in
    /// place, in the default layout
    Vector(Vec<Vec<Value>>),
}

impl Value {
    pub const fn f32(value: f32) -> Self {
        Self::F32(Bits(value))
    }

    pub const fn f64(value: f64) -> Self {
        Self::F64(Bits(value))
    }

    pub fn f32_array(values: impl IntoIterator<Item = f32>) -> Self {
        Self::F32Array(values.into_iter().map(Bits).collect())
    }

    pub fn f64_array(values: impl IntoIterator<Item = f64>) -> Self {
        Self::F64Array(values.into_iter().map(Bits).collect())
    }

    pub fn string(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

/// The elements of an array that a getter read
pub fn to_vec<T: Element>(array: ArrayView<'_, T>) -> Vec<T> {
    array.to_vec()
}

/// A table's fields as its generated getters read them, each to its value or
/// its error
pub type Fields = Vec<Result<Value, DecodeError>>;

/// The fields that a table written from `values` should read as
pub fn expected(values: Vec<Value>) -> Fields {
    values.into_iter().map(Ok).collect()
}

/// The value of a nested table whose fields were read, or the first error of
/// its fields
pub fn table(fields: Fields) -> Result<Value, DecodeError> {
    fields
        .into_iter()
        .collect::<Result<_, _>>()
        .map(Value::Table)
}

/// The value of a vector, or the first error of its items' fields, after
/// reading every item
pub fn vector(items: impl IntoIterator<Item = Fields>) -> Result<Value, DecodeError> {
    let items: Vec<Fields> = items.into_iter().collect();

    items
        .into_iter()
        .map(|fields| fields.into_iter().collect::<Result<Vec<_>, _>>())
        .collect::<Result<_, _>>()
        .map(Value::Vector)
}

/// A message's layout, and its version when it's versioned, as its
/// attributes in the schema set them. The tests repeat them from the schema
/// rather than take them from the generated code, which keeps them private.
#[derive(Clone, Copy, Debug)]
pub struct Header {
    /// The layout without a slot for the version, which `version` adds
    pub layout: TableLayout,
    pub version: Option<u32>,
}

impl Header {
    pub const fn new(layout: TableLayout) -> Self {
        Self {
            layout,
            version: None,
        }
    }

    pub const fn versioned(layout: TableLayout, version: u32) -> Self {
        Self {
            layout,
            version: Some(version),
        }
    }

    /// The header with another version in its slot
    pub const fn with_version(self, version: u32) -> Self {
        Self::versioned(self.layout, version)
    }

    /// `bytes` with the header's version written in its slot, as far as they
    /// hold it, so that a versioned reader reads arbitrary bytes
    pub fn write_version(self, bytes: &[u8]) -> Vec<u8> {
        let mut bytes = bytes.to_vec();
        let Some(version) = self.version else {
            return bytes;
        };

        let width = self.layout.offset_type().size();
        let position = if self.layout.is_sized() { width } else { 0 };
        let slot = &version.to_le_bytes()[..width];
        for (byte, &version_byte) in bytes.iter_mut().skip(position).zip(slot) {
            *byte = version_byte;
        }

        bytes
    }
}

/// Writes a table of `values` by hand with `TableBuilder`, as the generated
/// code writes it: in the header's layout, with nested tables and the tables
/// of vectors from their builders in the default layout, at their default
/// alignment
///
/// # Errors
///
/// Returns the `BuildError` of the first setter that fails, such as a table
/// too long for its offsets.
pub fn build(header: Header, values: &[Value]) -> Result<Vec<u8>, BuildError> {
    let mut builder = match header.version {
        Some(version) => TableBuilder::new(values.len(), header.layout.with_version(version))?,
        None => TableBuilder::new(values.len(), header.layout)?,
    };
    set_all(&mut builder, values)?;

    builder.to_bytes()
}

fn nested(values: &[Value]) -> Result<TableBuilder<'_>, BuildError> {
    let mut builder = TableBuilder::new(values.len(), TableLayout::DEFAULT)?;
    set_all(&mut builder, values)?;

    Ok(builder)
}

fn set_all<'a>(builder: &mut TableBuilder<'a>, values: &'a [Value]) -> Result<(), BuildError> {
    values.iter().try_for_each(|value| set(builder, value))
}

fn set<'a>(builder: &mut TableBuilder<'a>, value: &'a Value) -> Result<(), BuildError> {
    match value {
        Value::Bool(value) => builder.set_u8(u8::from(*value)),
        Value::I8(value) => builder.set_i8(*value),
        Value::U8(value) => builder.set_u8(*value),
        Value::I16(value) => builder.set_i16(*value),
        Value::U16(value) => builder.set_u16(*value),
        Value::I32(value) => builder.set_i32(*value),
        Value::U32(value) => builder.set_u32(*value),
        Value::I64(value) => builder.set_i64(*value),
        Value::U64(value) => builder.set_u64(*value),
        Value::F32(Bits(value)) => builder.set_f32(*value),
        Value::F64(Bits(value)) => builder.set_f64(*value),
        Value::I8Array(values) => builder.set_i8_array(values),
        Value::U8Array(values) => builder.set_u8_array(values),
        Value::I16Array(values) => builder.set_i16_array(values),
        Value::U16Array(values) => builder.set_u16_array(values),
        Value::I32Array(values) => builder.set_i32_array(values),
        Value::U32Array(values) => builder.set_u32_array(values),
        Value::I64Array(values) => builder.set_i64_array(values),
        Value::U64Array(values) => builder.set_u64_array(values),
        Value::F32Array(values) => builder.set_f32_array(floats(values)),
        Value::F64Array(values) => builder.set_f64_array(floats(values)),
        Value::String(string) => builder.set_str(string),
        Value::Struct(scalars) => builder.set_struct(struct_bytes(scalars)),
        Value::StructArray {
            size,
            alignment,
            items,
        } => {
            let bytes = items
                .iter()
                .flat_map(|scalars| struct_bytes(scalars))
                .collect::<Vec<_>>();
            builder.set_struct_array(bytes, *size, *alignment)
        }
        Value::Table(values) => builder.set_table(nested(values)?),
        Value::Vector(items) => {
            let items = items
                .iter()
                .map(|values| nested(values))
                .collect::<Result<Vec<_>, _>>()?;
            builder.set_vector(items)
        }
    }
}

/// A struct's scalars little-endian, one after another, written with the
/// standard library rather than the generated code
pub fn struct_bytes(scalars: &[Value]) -> Vec<u8> {
    scalars
        .iter()
        .flat_map(|scalar| match scalar {
            Value::Bool(value) => vec![u8::from(*value)],
            Value::I8(value) => value.to_le_bytes().to_vec(),
            Value::U8(value) => value.to_le_bytes().to_vec(),
            Value::I16(value) => value.to_le_bytes().to_vec(),
            Value::U16(value) => value.to_le_bytes().to_vec(),
            Value::I32(value) => value.to_le_bytes().to_vec(),
            Value::U32(value) => value.to_le_bytes().to_vec(),
            Value::I64(value) => value.to_le_bytes().to_vec(),
            Value::U64(value) => value.to_le_bytes().to_vec(),
            Value::F32(Bits(value)) => value.to_le_bytes().to_vec(),
            Value::F64(Bits(value)) => value.to_le_bytes().to_vec(),
            _ => unreachable!("structs hold scalars"),
        })
        .collect()
}

fn floats<T: Copy>(values: &[Bits<T>]) -> Vec<T> {
    values.iter().map(|value| value.0).collect()
}

/// Checks the fields read from a truncated table, or the error of the
/// reader's constructor: each field reads as the value that it was written
/// with, since the bytes it reads haven't changed, or fails with
/// `DecodeError::OutOfRange`
pub fn check_truncated(
    read: Result<Fields, DecodeError>,
    written: &[Value],
) -> Result<(), TestCaseError> {
    let fields = match read {
        Ok(fields) => fields,
        Err(error) => return check_out_of_range(&error),
    };

    prop_assert_eq!(fields.len(), written.len());
    for (index, (field, value)) in fields.iter().zip(written).enumerate() {
        match field {
            Ok(field) => prop_assert_eq!(field, value, "field {}", index),
            Err(error) => check_out_of_range(error)?,
        }
    }

    Ok(())
}

fn check_out_of_range(error: &DecodeError) -> Result<(), TestCaseError> {
    prop_assert!(
        matches!(error, DecodeError::OutOfRange { .. }),
        "{:?}",
        error
    );

    Ok(())
}

/// Every truncation of `bytes`, from none of them to all but the last
pub fn truncations(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    (0..bytes.len()).map(|length| &bytes[..length])
}

// Arbitrary values

/// The most elements of an array, tables of a vector and characters of a
/// string, in small tables that keep the tests fast
pub const MAX_LENGTH: usize = 8;

/// Integers over the type's full range, and half of the time its limits and
/// zero
pub fn integers<T>(min: T, max: T) -> impl Strategy<Value = T>
where
    T: Arbitrary + Copy + Default + 'static,
{
    prop_oneof![
        any::<T>(),
        prop::sample::select(vec![min, max, T::default()])
    ]
}

/// Float32s of any bits, NaNs of either sign with any payload, quiet or
/// signaling, and -0, 0, the infinities and the extremes, each a third of the
/// time
pub fn float32s() -> impl Strategy<Value = f32> {
    let nans = (any::<bool>(), 1..=0x007f_ffff_u32).prop_map(|(negative, payload)| {
        f32::from_bits((u32::from(negative) << 31) | 0x7f80_0000 | payload)
    });
    let edges = vec![
        -0.0,
        0.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        f32::MIN,
        // The smallest positive float32, a subnormal
        f32::from_bits(1),
    ];

    prop_oneof![
        any::<u32>().prop_map(f32::from_bits),
        nans,
        prop::sample::select(edges),
    ]
}

/// Float64s as [`float32s()`] draws float32s
pub fn float64s() -> impl Strategy<Value = f64> {
    let nans = (any::<bool>(), 1..=0x000f_ffff_ffff_ffff_u64).prop_map(|(negative, payload)| {
        f64::from_bits((u64::from(negative) << 63) | 0x7ff0_0000_0000_0000 | payload)
    });
    let edges = vec![
        -0.0,
        0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        f64::MIN,
        f64::from_bits(1),
    ];

    prop_oneof![
        any::<u64>().prop_map(f64::from_bits),
        nans,
        prop::sample::select(edges),
    ]
}

/// Strings of up to [`MAX_LENGTH`] characters, as [`strings_up_to()`]
/// draws them
pub fn strings() -> impl Strategy<Value = String> {
    strings_up_to(MAX_LENGTH)
}

/// Strings of up to `max_length` characters, empty too: of any characters,
/// of characters outside the Basic Multilingual Plane, which take 4 bytes of
/// UTF-8, and either after a U+FEFF, which is part of the string rather than
/// a byte order mark
pub fn strings_up_to(max_length: usize) -> impl Strategy<Value = String> {
    let astral = (0x1_0000..=u32::from(char::MAX))
        .prop_map(|code_point| char::from_u32(code_point).expect("not a surrogate"));
    let characters = prop_oneof![any::<char>(), astral];
    let strings = vec(characters, 0..=max_length).prop_map(String::from_iter);

    prop_oneof![
        strings.clone(),
        strings.prop_map(|string| format!("\u{feff}{string}")),
    ]
}

/// Arrays of up to [`MAX_LENGTH`] elements, empty too
pub fn arrays<S: Strategy>(elements: S) -> impl Strategy<Value = Vec<S::Value>> {
    vec(elements, 0..=MAX_LENGTH)
}

/// Where a reader finds a table's bytes: `byte_offset` bytes past a multiple
/// of 8 in a larger buffer of `fill`, so that its arrays may be unaligned in
/// memory
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    byte_offset: usize,
    fill: u8,
}

/// Placements that align a table's bytes to each element size, or to none
pub fn placements() -> impl Strategy<Value = Placement> {
    (0..16_usize, any::<u8>()).prop_map(|(byte_offset, fill)| Placement { byte_offset, fill })
}

impl Placement {
    /// Copies `bytes` to the placement
    pub fn place(self, bytes: &[u8]) -> Placed {
        let mut backing = vec![self.fill; 8 + self.byte_offset + bytes.len() + 8];
        let start = backing.as_ptr().align_offset(8) + self.byte_offset;
        backing[start..start + bytes.len()].copy_from_slice(bytes);

        Placed {
            backing,
            start,
            len: bytes.len(),
        }
    }
}

/// A copy of a table's bytes at a placement
pub struct Placed {
    backing: Vec<u8>,
    start: usize,
    len: usize,
}

impl Placed {
    pub fn bytes(&self) -> &[u8] {
        &self.backing[self.start..self.start + self.len]
    }
}

/// Bytes written over a table's, from `at` modulo the table's length, as
/// many as fit
#[derive(Clone, Debug)]
pub struct Overwrite {
    at: usize,
    bytes: Vec<u8>,
}

/// Overwrites of 1 to 4 bytes, half of them in the first 16 bytes of a
/// table, where its header and offsets are
pub fn overwrites() -> impl Strategy<Value = Overwrite> {
    let positions = prop_oneof![0..16_usize, any::<usize>()];

    (positions, vec(any::<u8>(), 1..=4)).prop_map(|(at, bytes)| Overwrite { at, bytes })
}

impl Overwrite {
    /// A copy of `bytes` with the overwrite applied
    pub fn apply(&self, bytes: &[u8]) -> Vec<u8> {
        let mut overwritten = bytes.to_vec();
        if let Some(at) = self.at.checked_rem(bytes.len()) {
            for (byte, &with) in overwritten[at..].iter_mut().zip(&self.bytes) {
                *byte = with;
            }
        }

        overwritten
    }
}

/// Bytes of any value, of up to 64, mostly small values, so that offsets and
/// lengths often lie within them
pub fn arbitrary_bytes() -> impl Strategy<Value = Vec<u8>> {
    vec(prop_oneof![3 => 0..16_u8, 1 => any::<u8>()], 0..=64)
}

/// Checks that tafla-gen decodes the table of the message or struct `name`
/// in `bytes`, which a conforming writer wrote, as JSON that encodes back
/// into the same bytes, and that decoding those bytes again reads the same
/// JSON, with strings in UTF-8 and in ASCII
pub fn check_json_round_trip(schema: &str, name: &str, bytes: &[u8]) -> Result<(), TestCaseError> {
    let schema = tafla_gen::parse(schema).expect("an example schema is valid");
    let root = Root::find(&schema, name).expect("the name is a message or a struct");

    for ascii in [false, true] {
        let options = DecodeOptions {
            ascii,
            ..DecodeOptions::default()
        };
        let json = root
            .decode(bytes, &options)
            .map_err(|error| TestCaseError::fail(format!("{name}: {error}")))?;
        prop_assert!(!ascii || json.is_ascii(), "{}", json);

        let encoded = root.encode(&json);
        prop_assert_eq!(encoded.as_deref(), Ok(bytes), "{}: {}", name, json);
        prop_assert_eq!(root.decode(bytes, &options), Ok(json));
    }

    Ok(())
}
