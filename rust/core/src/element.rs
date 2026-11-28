use std::slice;

use crate::Alignment;

/// A number type that tables store in arrays: `i8`, `u8`, `i16`, `u16`,
/// `i32`, `u32`, `i64`, `u64`, `f32` or `f64`. Elements are stored
/// little-endian and aligned to their size from the table's start.
///
/// It's sealed, since an [`ArrayView`](crate::ArrayView) may return its
/// elements in place, which is sound only for types of which every bit
/// pattern is a value. That rules out `bool`, of which any byte but 0 or 1
/// is undefined behavior: [`TableReader::get_bool_array()`](crate::TableReader::get_bool_array)
/// reads an array of bools into a new `Vec`, and
/// [`TableReader::get_u8_array()`](crate::TableReader::get_u8_array) reads
/// its bytes in place.
///
/// ```compile_fail,E0277
/// let bools: tafla::ArrayView<'_, bool>;
/// ```
pub trait Element: sealed::Sealed {}

pub mod sealed {
    use crate::Alignment;

    pub trait Sealed: Copy + 'static {
        /// The element's little-endian bytes
        type Bytes: Copy + AsRef<[u8]> + IntoIterator<Item = u8>;

        /// The element's size, which its arrays are aligned to
        const ALIGNMENT: Alignment;

        fn from_le(bytes: Self::Bytes) -> Self;

        fn to_le(self) -> Self::Bytes;

        /// Splits bytes into the bytes of whole elements, leaving out any
        /// remainder
        fn chunks(bytes: &[u8]) -> &[Self::Bytes];
    }
}

macro_rules! impl_element {
    ($($type:ty => $alignment:ident),*) => {$(
        impl Element for $type {}

        impl sealed::Sealed for $type {
            type Bytes = [u8; size_of::<$type>()];

            const ALIGNMENT: Alignment = Alignment::$alignment;

            #[inline]
            fn from_le(bytes: Self::Bytes) -> Self {
                Self::from_le_bytes(bytes)
            }

            #[inline]
            fn to_le(self) -> Self::Bytes {
                self.to_le_bytes()
            }

            #[inline]
            fn chunks(bytes: &[u8]) -> &[Self::Bytes] {
                bytes.as_chunks().0
            }

        }
    )*};
}

impl_element!(
    i8 => One, u8 => One,
    i16 => Two, u16 => Two,
    i32 => Four, u32 => Four, f32 => Four,
    i64 => Eight, u64 => Eight, f64 => Eight
);

/// Appends `values` to `bytes`, little-endian. Into reserved capacity, the
/// iterator of each element's bytes copies them as fast as a memcpy.
#[inline]
pub fn extend_elements<T: Element>(values: &[T], bytes: &mut Vec<u8>) {
    bytes.extend(values.iter().flat_map(|value| value.to_le()));
}

/// Returns little-endian bytes as elements in place, which is possible on a
/// little-endian host when the bytes are aligned to the element's alignment,
/// as `TableBuilder` aligns them within an aligned table. Single bytes have
/// no order and are always aligned.
#[inline]
pub fn elements_in_place<T: Element>(bytes: &[u8]) -> Option<&[T]> {
    let pointer = bytes.as_ptr().cast::<T>();
    let in_host_order = cfg!(target_endian = "little") || size_of::<T>() == 1;

    if !in_host_order || !pointer.is_aligned() {
        return None;
    }

    // SAFETY: The elements are integers or floats, for which every bit
    // pattern is valid, in the host's byte order. The pointer is aligned for
    // them, and the slice covers whole elements within `bytes`, which it
    // borrows immutably for as long.
    Some(unsafe { slice::from_raw_parts(pointer, bytes.len() / size_of::<T>()) })
}
