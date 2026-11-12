//! Generic table shapes, with the TypeScript comparison's values.
//!
//! The shapes are sized to bracket how tafla is used: small tables of scalars
//! and a string, documents whose header nests a table of numeric columns,
//! vectors of small nested tables, arrays of structs, and fixed-size records. The values come
//! from a port of the TypeScript comparison's seeded generator, so that the
//! languages' numbers compare.
//!
//! The shapes derive serde's and rkyv's traits, as an application's own types
//! would, so that rmp-serde and rkyv serialize them as they are. flatc's tables
//! and prost's messages are types of their own, which their codecs convert to.
//!
//! Deserialized values borrow strings and columns from their bytes where a
//! codec reads them in place, and own copies otherwise. Equality compares
//! floats by their bits, so that a codec reads back exactly what it wrote.

use std::borrow::Cow;

use rkyv::with::AsOwned;
use serde::{Deserialize, Serialize};

/// Rows per document: a small one and a large one
pub const DOCUMENT_ROWS: [usize; 2] = [100, 100_000];

pub const LIST_ITEMS: usize = 100;

pub const RECORD: Record<'static> = Record {
    x: 59.3293,
    y: 18.0686,
    id: -631_152_000,
    name: Cow::Borrowed("tafla-benchmarks"),
};

pub const SAMPLE: Sample = Sample {
    key: 2_743_826_400,
    a: 21.75,
    b: 3.25,
};

/// A small table: two float64s, an int64 and a 16-byte string
#[derive(Clone, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize)]
pub struct Record<'a> {
    pub x: f64,
    pub y: f64,
    pub id: i64,
    #[serde(borrow)]
    #[rkyv(with = AsOwned)]
    pub name: Cow<'a, str>,
}

/// A document: a sized, versioned table of two float64s and a string, with a
/// nested table of an int64 and three columns of the same length
#[derive(Clone, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize)]
pub struct Document<'a> {
    pub x: f64,
    pub y: f64,
    #[serde(borrow)]
    #[rkyv(with = AsOwned)]
    pub name: Cow<'a, str>,
    pub base: i64,
    #[rkyv(with = AsOwned)]
    pub keys: Cow<'a, [u32]>,
    #[rkyv(with = AsOwned)]
    pub a: Cow<'a, [f32]>,
    #[rkyv(with = AsOwned)]
    pub b: Cow<'a, [f32]>,
}

/// A table of a float32 and a nested table of two float32s, `first` and
/// `second`, or a struct of the three float32s
#[derive(Clone, Copy, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize)]
pub struct Item {
    pub value: f32,
    pub first: f32,
    pub second: f32,
}

/// A table holding a vector of item tables, or an array of item structs
#[derive(Clone, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize)]
pub struct List {
    pub items: Vec<Item>,
}

/// A fixed-size record of a uint32 and two float32s
#[derive(Clone, Copy, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize)]
pub struct Sample {
    pub key: u32,
    pub a: f32,
    pub b: f32,
}

/// One of the workloads' shapes, which names the value that codecs serialize
/// and read back
pub trait Shape {
    /// The workload's name, as errors and reports name it
    const NAME: &'static str;

    /// The value, borrowing from the bytes it was read from
    type Value<'a>;

    /// Whether the values are the same, with floats compared by their bits.
    /// Their lifetimes differ, which generic code can't compare with `==`.
    fn same(left: &Self::Value<'_>, right: &Self::Value<'_>) -> bool;
}

/// The shape of [`Record`]
pub struct RecordShape;

/// The shape of [`Document`]
pub struct DocumentShape;

/// The shape of [`List`], whose items are nested tables
pub struct ListShape;

/// The shape of [`List`], whose items are structs
pub struct StructsShape;

/// The shape of [`Sample`]
pub struct SampleShape;

impl Shape for RecordShape {
    const NAME: &'static str = "record";
    type Value<'a> = Record<'a>;

    fn same(left: &Record<'_>, right: &Record<'_>) -> bool {
        left == right
    }
}

impl Shape for DocumentShape {
    const NAME: &'static str = "document";
    type Value<'a> = Document<'a>;

    fn same(left: &Document<'_>, right: &Document<'_>) -> bool {
        left == right
    }
}

impl Shape for ListShape {
    const NAME: &'static str = "vector of nested tables";
    type Value<'a> = List;

    fn same(left: &List, right: &List) -> bool {
        left == right
    }
}

impl Shape for StructsShape {
    const NAME: &'static str = "array of structs";
    type Value<'a> = List;

    fn same(left: &List, right: &List) -> bool {
        left == right
    }
}

impl Shape for SampleShape {
    const NAME: &'static str = "fixed-size record";
    type Value<'a> = Sample;

    fn same(left: &Sample, right: &Sample) -> bool {
        left == right
    }
}

pub fn document(rows: usize) -> Document<'static> {
    let mut random = Xorshift::new(rows);

    // Keys below 2^24 take 4 bytes in protobuf's varints, like the floats
    let mut keys = Vec::with_capacity(rows);
    let mut a = Vec::with_capacity(rows);
    let mut b = Vec::with_capacity(rows);
    for _ in 0..rows {
        keys.push(floor_u32(random.next() * f64::from(1 << 24)));
        a.push(fround((random.next() - 0.5) * 100.0));
        b.push(fround((random.next() - 0.5) * 100.0));
    }

    Document {
        x: RECORD.x,
        y: RECORD.y,
        name: RECORD.name,
        base: RECORD.id,
        keys: Cow::Owned(keys),
        a: Cow::Owned(a),
        b: Cow::Owned(b),
    }
}

pub fn list(items: usize) -> List {
    let mut random = Xorshift::new(items);

    // Float32 values, so that every format reads them back exactly. Fields
    // are evaluated in the order they're written, as in JavaScript.
    let items = (0..items)
        .map(|_| Item {
            value: fround(random.next() * 30.0),
            first: fround(random.next() * 4.0 - 2.0),
            second: fround(random.next() * 50.0),
        })
        .collect();

    List { items }
}

/// The sum of `values` in their order, in an `f64`, as the TypeScript
/// comparison adds them to a `number`. Every codec sums its column with this,
/// so their sums have the same bits.
pub fn sum_column(values: impl IntoIterator<Item = f32>) -> f64 {
    values
        .into_iter()
        .fold(0.0, |sum, value| sum + f64::from(value))
}

/// The TypeScript generator's xorshift, in [0, 1), so every run sees the same
/// values. Its state is a `u32`, whose bits JavaScript's 32-bit shifts keep
/// the same.
struct Xorshift {
    state: u32,
}

impl Xorshift {
    // A seed wraps to 32 bits, as JavaScript's `^` wraps its operands
    #[allow(clippy::cast_possible_truncation)]
    const fn new(seed: usize) -> Self {
        Self {
            state: 0x2545_f491 ^ seed as u32,
        }
    }

    fn next(&mut self) -> f64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;

        f64::from(self.state) / 4_294_967_296.0
    }
}

/// `Math.floor()` of a value in [0, 2^32), as a `Uint32Array` stores it
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn floor_u32(value: f64) -> u32 {
    value as u32
}

/// Rounds to the nearest `f32`, as `Math.fround()` and a `Float32Array` do
#[allow(clippy::cast_possible_truncation)]
const fn fround(value: f64) -> f32 {
    value as f32
}

impl PartialEq for Record<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.x.to_bits() == other.x.to_bits()
            && self.y.to_bits() == other.y.to_bits()
            && self.id == other.id
            && self.name == other.name
    }
}

impl PartialEq for Document<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.x.to_bits() == other.x.to_bits()
            && self.y.to_bits() == other.y.to_bits()
            && self.name == other.name
            && self.base == other.base
            && self.keys == other.keys
            && same_floats(&self.a, &other.a)
            && same_floats(&self.b, &other.b)
    }
}

impl PartialEq for Item {
    fn eq(&self, other: &Self) -> bool {
        self.value.to_bits() == other.value.to_bits()
            && self.first.to_bits() == other.first.to_bits()
            && self.second.to_bits() == other.second.to_bits()
    }
}

impl PartialEq for List {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl PartialEq for Sample {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.a.to_bits() == other.a.to_bits()
            && self.b.to_bits() == other.b.to_bits()
    }
}

fn same_floats(left: &[f32], right: &[f32]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.to_bits() == right.to_bits())
}
