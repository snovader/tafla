//! The element types of the typed arrays that the addon reads values into

use napi::sys;

/// A number type that a table stores, as the addon reads it: by its size,
/// into the typed array of its values. The reads don't depend on the type
/// otherwise, so one loop reads every type, rather than a copy of the loop
/// for each, in which the compiler stopped inlining the reader's methods.
pub struct ElementType {
    /// Node-API's type of the typed array
    pub array: sys::napi_typedarray_type,
    /// The bytes of a value, in a table and in the typed array
    pub size: usize,
    /// What a call throws when `out` is something else
    pub expected: &'static str,
}

/// A Rust type of the values of an [`ElementType`], which names the type
/// that a callback reads
pub trait Column {
    const TYPE: ElementType;
}

macro_rules! impl_column {
    ($($type:ty => $array:ident, $name:literal;)*) => {$(
        impl Column for $type {
            const TYPE: ElementType = ElementType {
                array: sys::TypedarrayType::$array,
                size: size_of::<$type>(),
                expected: concat!("Expected out to be a ", $name),
            };
        }
    )*};
}

impl_column!(
    i8 => int8_array, "Int8Array";
    u8 => uint8_array, "Uint8Array";
    i16 => int16_array, "Int16Array";
    u16 => uint16_array, "Uint16Array";
    i32 => int32_array, "Int32Array";
    u32 => uint32_array, "Uint32Array";
    i64 => bigint64_array, "BigInt64Array";
    u64 => biguint64_array, "BigUint64Array";
    f32 => float32_array, "Float32Array";
    f64 => float64_array, "Float64Array";
);
