//! The values of a message's or a struct's fields, as the schema types
//! them, between a table's bytes and its JSON.
//!
//! Arrays hold their elements as the runtime reads and writes them, and
//! structs their bytes, so a value takes memory in proportion to its table.

use super::scalar::ScalarValue;
use crate::model::Scalar;

/// A field's value, of the type that its field has in the schema
#[derive(Debug)]
pub enum Value {
    Scalar(ScalarValue),
    Array(Elements),
    String(String),
    /// A struct's bytes: its scalars, little-endian, one after another
    Struct(Vec<u8>),
    /// The bytes of every struct of an array, one struct after another
    StructArray(Vec<u8>),
    /// A nested table's fields, or those of a message, in their order
    Table(Vec<Value>),
    /// The fields of each of a vector's tables
    Vector(Vec<Vec<Value>>),
}

/// The elements of an array of scalars
#[derive(Debug)]
pub enum Elements {
    Int8(Vec<i8>),
    Uint8(Vec<u8>),
    Int16(Vec<i16>),
    Uint16(Vec<u16>),
    Int32(Vec<i32>),
    Uint32(Vec<u32>),
    Int64(Vec<i64>),
    Uint64(Vec<u64>),
    Float32(Vec<f32>),
    Float64(Vec<f64>),
}

/// Runs `$body` with `$elements` bound to the vector of any element type,
/// and `$variant` to the scalar value's variant of the same type
macro_rules! each_type {
    ($value:expr, |$elements:ident, $variant:ident| $body:expr) => {
        match $value {
            Elements::Int8($elements) => {
                let $variant = ScalarValue::Int8;
                $body
            }
            Elements::Uint8($elements) => {
                let $variant = ScalarValue::Uint8;
                $body
            }
            Elements::Int16($elements) => {
                let $variant = ScalarValue::Int16;
                $body
            }
            Elements::Uint16($elements) => {
                let $variant = ScalarValue::Uint16;
                $body
            }
            Elements::Int32($elements) => {
                let $variant = ScalarValue::Int32;
                $body
            }
            Elements::Uint32($elements) => {
                let $variant = ScalarValue::Uint32;
                $body
            }
            Elements::Int64($elements) => {
                let $variant = ScalarValue::Int64;
                $body
            }
            Elements::Uint64($elements) => {
                let $variant = ScalarValue::Uint64;
                $body
            }
            Elements::Float32($elements) => {
                let $variant = ScalarValue::Float32;
                $body
            }
            Elements::Float64($elements) => {
                let $variant = ScalarValue::Float64;
                $body
            }
        }
    };
}

impl Elements {
    /// No elements of a scalar type
    ///
    /// # Panics
    ///
    /// For `bool`, which has no arrays.
    pub fn new(scalar: Scalar) -> Self {
        match scalar {
            Scalar::Bool => unreachable!("bools have no arrays"),
            Scalar::Int8 => Self::Int8(Vec::new()),
            Scalar::Uint8 => Self::Uint8(Vec::new()),
            Scalar::Int16 => Self::Int16(Vec::new()),
            Scalar::Uint16 => Self::Uint16(Vec::new()),
            Scalar::Int32 => Self::Int32(Vec::new()),
            Scalar::Uint32 => Self::Uint32(Vec::new()),
            Scalar::Int64 => Self::Int64(Vec::new()),
            Scalar::Uint64 => Self::Uint64(Vec::new()),
            Scalar::Float32 => Self::Float32(Vec::new()),
            Scalar::Float64 => Self::Float64(Vec::new()),
        }
    }

    /// Appends an element, which is of the elements' type
    ///
    /// # Panics
    ///
    /// If it's of another type.
    pub fn push(&mut self, value: ScalarValue) {
        match (self, value) {
            (Self::Int8(elements), ScalarValue::Int8(value)) => elements.push(value),
            (Self::Uint8(elements), ScalarValue::Uint8(value)) => elements.push(value),
            (Self::Int16(elements), ScalarValue::Int16(value)) => elements.push(value),
            (Self::Uint16(elements), ScalarValue::Uint16(value)) => elements.push(value),
            (Self::Int32(elements), ScalarValue::Int32(value)) => elements.push(value),
            (Self::Uint32(elements), ScalarValue::Uint32(value)) => elements.push(value),
            (Self::Int64(elements), ScalarValue::Int64(value)) => elements.push(value),
            (Self::Uint64(elements), ScalarValue::Uint64(value)) => elements.push(value),
            (Self::Float32(elements), ScalarValue::Float32(value)) => elements.push(value),
            (Self::Float64(elements), ScalarValue::Float64(value)) => elements.push(value),
            _ => unreachable!("an element is of its array's type"),
        }
    }

    /// Calls `visit` with each element in order
    pub fn for_each(&self, mut visit: impl FnMut(ScalarValue)) {
        each_type!(self, |elements, variant| {
            for &element in elements {
                visit(variant(element));
            }
        });
    }
}
