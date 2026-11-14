use std::str;

use crate::{ArrayView, DecodeError, Element, OffsetType, TableLayout, Vector};

/// Reads a table from the supplied bytes, in the layout it was written with.
///
/// Fields are found by index with [`ByteBuffer::root_offset()`], or
/// [`ByteBuffer::field_offset()`] for a table nested in another, and then
/// read at that position by their type's getter.
///
/// Every read checks that it stays within the bytes and returns a
/// [`DecodeError`] otherwise, so no bytes, however corrupted, make a read
/// panic or return values from outside them. Strings must be valid UTF-8.
///
/// Arrays, strings, structs and vector items are read in place, borrowing
/// the bytes rather than copying them.
#[derive(Clone, Copy, Debug)]
pub struct ByteBuffer<'a, const VERSIONED: bool = false> {
    bytes: &'a [u8],
    layout: TableLayout<VERSIONED>,
}

macro_rules! scalar_getters {
    ($($name:ident -> $type:ty),*) => {$(
        #[doc = concat!("Reads the `", stringify!($type), "` at `position`")]
        ///
        /// # Errors
        ///
        /// [`DecodeError::OutOfRange`] if the value doesn't lie within the bytes.
        #[inline]
        pub fn $name(&self, position: usize) -> Result<$type, DecodeError> {
            self.get(position)
        }
    )*};
}

macro_rules! array_getters {
    ($($name:ident -> $type:ty),*) => {$(
        #[doc = concat!("Reads the array of `", stringify!($type), "` at `position`")]
        ///
        /// # Errors
        ///
        /// [`DecodeError::OutOfRange`] if the array doesn't lie within the bytes.
        #[inline]
        pub fn $name(&self, position: usize) -> Result<ArrayView<'a, $type>, DecodeError> {
            self.get_array(position)
        }
    )*};
}

impl<'a, const VERSIONED: bool> ByteBuffer<'a, VERSIONED> {
    pub const fn new(bytes: &'a [u8], layout: TableLayout<VERSIONED>) -> Self {
        Self { bytes, layout }
    }

    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub const fn layout(&self) -> TableLayout<VERSIONED> {
        self.layout
    }

    /// Returns the table's byte length from its header when the layout is
    /// sized, and otherwise the length of the bytes
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the header doesn't lie within the
    /// bytes.
    pub fn size(&self) -> Result<usize, DecodeError> {
        if self.layout.is_sized() {
            self.get_size(0)
        } else {
            Ok(self.bytes.len())
        }
    }

    /// Returns the position of field `index` of the root table
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the field's offset doesn't lie within
    /// the bytes.
    pub fn root_offset(&self, index: usize) -> Result<usize, DecodeError> {
        let slot = index
            .saturating_mul(self.offset_type().size())
            .saturating_add(self.layout.header_size());

        self.get_size(slot)
    }

    /// Returns the position of field `index` of a nested table, which has no
    /// header and stores its field offsets from its own start.
    ///
    /// `table_position` is the nested table's position, which is the
    /// position of the field that holds it.
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the field's offset doesn't lie within
    /// the bytes.
    pub fn field_offset(&self, table_position: usize, index: usize) -> Result<usize, DecodeError> {
        let slot = index
            .saturating_mul(self.offset_type().size())
            .saturating_add(table_position);
        let offset = self.get_size(slot)?;

        // Only a 32-bit host can overflow, at a position past any bytes
        table_position.checked_add(offset).ok_or_else(|| {
            DecodeError::out_of_range(table_position, offset as u64, self.bytes.len())
        })
    }

    scalar_getters!(
        get_i8 -> i8, get_u8 -> u8,
        get_i16 -> i16, get_u16 -> u16,
        get_i32 -> i32, get_u32 -> u32,
        get_i64 -> i64, get_u64 -> u64,
        get_f32 -> f32, get_f64 -> f64
    );

    /// Reads the array of bytes at `position`
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the array doesn't lie within the bytes.
    pub fn get_u8_array(&self, position: usize) -> Result<&'a [u8], DecodeError> {
        self.array_bytes(position, 1)
    }

    array_getters!(
        get_i8_array -> i8,
        get_i16_array -> i16, get_u16_array -> u16,
        get_i32_array -> i32, get_u32_array -> u32,
        get_i64_array -> i64, get_u64_array -> u64,
        get_f32_array -> f32, get_f64_array -> f64
    );

    /// Reads the string at `position`, which must be valid UTF-8. A leading
    /// U+FEFF is part of the string rather than a byte order mark.
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the string doesn't lie within the
    /// bytes, or [`DecodeError::InvalidUtf8`] if it isn't valid UTF-8.
    pub fn get_str(&self, position: usize) -> Result<&'a str, DecodeError> {
        let bytes = self.array_bytes(position, 1)?;

        // ASCII first, which `is_ascii()` checks a word at a time at any
        // address, where `str::from_utf8()` checks a string's bytes one at a
        // time but for whole pairs of aligned words in it: the benchmarks'
        // record, whose 16-byte name isn't aligned, read in 6.3 ns rather
        // than 11.8 ns
        if bytes.is_ascii() {
            // SAFETY: ASCII is valid UTF-8
            return Ok(unsafe { str::from_utf8_unchecked(bytes) });
        }

        str::from_utf8(bytes).map_err(|source| DecodeError::InvalidUtf8 { position, source })
    }

    /// Reads the `size` bytes of the struct at `position`, whose scalars lie
    /// at fixed offsets from its start
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the struct doesn't lie within the
    /// bytes.
    pub fn get_struct(&self, position: usize, size: usize) -> Result<&'a [u8], DecodeError> {
        self.range(position, size as u64)
    }

    /// Reads the bytes of the structs of the array at `position`, each of
    /// `size` bytes, one after another after the count. Their number is the
    /// bytes' length over `size`. Structs of 0 bytes take none, so a `size`
    /// of 0 returns no bytes, whatever the count.
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the array doesn't lie within the bytes.
    pub fn get_struct_array(&self, position: usize, size: usize) -> Result<&'a [u8], DecodeError> {
        self.array_bytes(position, size)
    }

    /// Reads the vector at `position`, checking that its count and the ends
    /// of its items lie within the bytes. Each item is checked when it's
    /// read.
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the count or the ends don't lie within
    /// the bytes.
    pub fn get_vector(&self, position: usize) -> Result<Vector<'a>, DecodeError> {
        let width = self.offset_type().size();
        let count = self.get_size(position)?;

        // Every item has an end, so this rejects an impossible count before
        // any item is read
        self.range(position + width, count as u64 * width as u64)?;

        Ok(Vector::new(
            &self.bytes[position..],
            position,
            count,
            self.offset_type(),
        ))
    }

    const fn offset_type(&self) -> OffsetType {
        self.layout.offset_type()
    }

    #[inline]
    fn get<T: Element>(&self, position: usize) -> Result<T, DecodeError> {
        self.bytes
            .get(position..)
            .and_then(|bytes| T::chunks(bytes).first())
            .map(|&bytes| T::from_le(bytes))
            .ok_or_else(|| {
                DecodeError::out_of_range(position, size_of::<T>() as u64, self.bytes.len())
            })
    }

    #[inline]
    fn get_array<T: Element>(&self, position: usize) -> Result<ArrayView<'a, T>, DecodeError> {
        self.array_bytes(position, size_of::<T>())
            .map(ArrayView::new)
    }

    /// Reads a length or offset at `position`
    #[inline]
    fn get_size(&self, position: usize) -> Result<usize, DecodeError> {
        self.get_slot(position).map(|size| size as usize)
    }

    /// Reads a length, offset or header slot at `position`
    #[inline]
    fn get_slot(&self, position: usize) -> Result<u32, DecodeError> {
        let offset_type = self.offset_type();

        self.bytes
            .get(position..)
            .and_then(|bytes| read_slot(bytes, offset_type))
            .ok_or_else(|| {
                DecodeError::out_of_range(position, offset_type.size() as u64, self.bytes.len())
            })
    }

    /// Returns the bytes of the elements after the length prefix at
    /// `position`
    #[inline]
    fn array_bytes(&self, position: usize, element_size: usize) -> Result<&'a [u8], DecodeError> {
        let length = self.get_size(position)?;

        // The length prefix lies within the bytes, so this doesn't overflow.
        // A struct can be as large as a usize, and a byte length past u64
        // lies past any bytes, so the product saturates.
        let start = position + self.offset_type().size();
        self.range(start, (length as u64).saturating_mul(element_size as u64))
    }

    #[inline]
    fn range(&self, position: usize, byte_length: u64) -> Result<&'a [u8], DecodeError> {
        usize::try_from(byte_length)
            .ok()
            .and_then(|byte_length| self.bytes.get(position..)?.get(..byte_length))
            .ok_or_else(|| DecodeError::out_of_range(position, byte_length, self.bytes.len()))
    }
}

impl ByteBuffer<'_, true> {
    /// Returns the version that the table's header carries after its length.
    /// It's as wide as the header's other slots.
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the header doesn't lie within the
    /// bytes.
    pub fn version(&self) -> Result<u32, DecodeError> {
        let position = if self.layout.is_sized() {
            self.offset_type().size()
        } else {
            0
        };

        self.get_slot(position)
    }
}

/// Reads a length, offset or header slot as wide as `offset_type` from the
/// start of `bytes`
#[inline]
pub fn read_slot(bytes: &[u8], offset_type: OffsetType) -> Option<u32> {
    match offset_type {
        OffsetType::Uint8 => bytes.first().map(|&byte| u32::from(byte)),
        OffsetType::Uint16 => bytes
            .first_chunk()
            .map(|&bytes| u32::from(u16::from_le_bytes(bytes))),
        OffsetType::Uint32 => bytes.first_chunk().map(|&bytes| u32::from_le_bytes(bytes)),
    }
}
