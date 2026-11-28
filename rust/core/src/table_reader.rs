use std::str;

use crate::{ArrayView, DecodeError, Element, OffsetType, StringVector, TableLayout, Vector};

/// Reads a table from the supplied bytes, in the layout it was written with.
///
/// Fields are found by index with [`TableReader::root_offset()`], or
/// [`TableReader::field_offset()`] for a table nested in another, and then
/// read at that position by their type's getter.
///
/// Every read checks that it stays within the bytes and returns a
/// [`DecodeError`] otherwise, so no bytes, however corrupted, make a read
/// panic or return values from outside them. Strings must be valid UTF-8.
///
/// Arrays, strings, structs and vector items are read in place, borrowing
/// the bytes rather than copying them.
#[derive(Clone, Copy, Debug)]
pub struct TableReader<'a, const VERSIONED: bool = false> {
    bytes: &'a [u8],
    // What the reader takes from its layout. It reads tables of any version,
    // so it doesn't keep the layout's: holding the whole layout, 4 bytes
    // longer, made a record's read take 1.2 times as long.
    sized: bool,
    offset_type: OffsetType,
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

impl<'a, const VERSIONED: bool> TableReader<'a, VERSIONED> {
    pub const fn new(bytes: &'a [u8], layout: TableLayout<VERSIONED>) -> Self {
        Self {
            bytes,
            sized: layout.is_sized(),
            offset_type: layout.offset_type(),
        }
    }

    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Whether the layout that the reader was given is sized
    pub const fn is_sized(&self) -> bool {
        self.sized
    }

    /// The offset type of the layout that the reader was given
    pub const fn offset_type(&self) -> OffsetType {
        self.offset_type
    }

    /// The byte length of the header slots before the field offsets
    const fn header_size(&self) -> usize {
        let slots = if self.sized { 1 } else { 0 } + if VERSIONED { 1 } else { 0 };
        slots * self.offset_type.size()
    }

    /// Returns the table's byte length from its header when the layout is
    /// sized, and otherwise the length of the bytes
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the header doesn't lie within the
    /// bytes.
    pub fn size(&self) -> Result<usize, DecodeError> {
        if self.sized {
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
    // Inlined into its callers, as are `field_offset()` and the reads of
    // lengths and offsets that it and the getters share, so that a constant
    // layout's offset type and header fold into each read. With several
    // callers, LLVM called them out of line, which `#[inline]` didn't
    // change, and each read the layout from memory and branched on its
    // offset size: a versioned document of 100 rows read by hand took
    // 27.1 ns rather than 12.7 ns. Inlining only this, `field_offset()` and
    // `array_bytes()` called `get_slot()` out of line instead, and took as
    // long.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub fn root_offset(&self, index: usize) -> Result<usize, DecodeError> {
        let slot = index
            .saturating_mul(self.offset_type().size())
            .saturating_add(self.header_size());

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
    // Inlined, as `root_offset()` is
    #[allow(clippy::inline_always)]
    #[inline(always)]
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

    /// Reads the array of bools at `position` into a new `Vec`: a `u8` array
    /// whose bytes are true where they aren't 0. They're copied, since a
    /// `bool` of any byte but 0 or 1 is undefined behavior;
    /// [`TableReader::get_u8_array()`] reads the same bytes in place.
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the array doesn't lie within the bytes.
    pub fn get_bool_array(&self, position: usize) -> Result<Vec<bool>, DecodeError> {
        let bytes = self.array_bytes(position, 1)?;

        // LLVM compares them many at a time: 1,000 bools took 36.0 ns to
        // read, where copying their bytes took 33.3 ns, in Rust 1.100
        Ok(bytes.iter().map(|&byte| byte != 0).collect())
    }

    /// Reads the string at `position`, which must be valid UTF-8. A leading
    /// U+FEFF is part of the string rather than a byte order mark.
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the string doesn't lie within the
    /// bytes, or [`DecodeError::InvalidUtf8`] if it isn't valid UTF-8.
    // Inlined into its callers, such as a schema's generated getters: with
    // several of them, LLVM called it out of line, as it did with
    // `#[inline]`, and tafla-gen's record read in 6.7 ns rather than 4.9 ns,
    // and a table of 16 strings of 8 ASCII characters in 76 ns rather than
    // 51 ns
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub fn get_str(&self, position: usize) -> Result<&'a str, DecodeError> {
        let bytes = self.array_bytes(position, 1)?;

        decode_str(bytes, position)
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

    /// Reads the vector of strings at `position`, checking that its count
    /// and the ends of its strings lie within the bytes, as
    /// [`TableReader::get_vector()`] checks a vector's. Each string is
    /// checked when it's read, as UTF-8 on its own.
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the count or the ends don't lie within
    /// the bytes.
    pub fn get_str_vector(&self, position: usize) -> Result<StringVector<'a>, DecodeError> {
        self.get_vector(position).map(StringVector::new)
    }

    /// Reads string `index` of the vector of `count` strings at `position`,
    /// from its end and the end of the string before it, without reading
    /// the count or any other string, or returns `None` if `index` isn't
    /// below `count`. It reads what [`StringVector::get()`] reads of the
    /// view that [`TableReader::get_str_vector()`] returns, for a caller
    /// that keeps the count rather than the view, such as a binding to
    /// another language, which reads the count once with
    /// `get_str_vector()` and passes it for each string.
    ///
    /// The count isn't read, but it's checked to leave its ends within the
    /// bytes, as `get_str_vector()` checks the count that it reads, so that
    /// any count reads within them. String 0 starts after the ends of
    /// `count` strings, so another count than the vector's reads other
    /// bytes as its strings.
    ///
    /// # Errors
    ///
    /// `Some(Err(_))` with [`DecodeError::OutOfRange`] if `count` strings'
    /// ends don't lie within the bytes, or the error that
    /// [`StringVector::get()`] returns for the string.
    pub fn get_str_vector_item(
        &self,
        position: usize,
        count: usize,
        index: usize,
    ) -> Option<Result<&'a str, DecodeError>> {
        if index >= count {
            return None;
        }

        // The count, and then the end of each string. Saturating, past any
        // bytes, since only a count or position past them overflows.
        let width = self.offset_type().size();
        let slots = (count as u64).saturating_add(1);
        if let Err(error) = self.range(position, slots.saturating_mul(width as u64)) {
            return Some(Err(error));
        }

        // The count and the ends lie within the bytes
        let vector = Vector::new(&self.bytes[position..], position, count, self.offset_type());
        StringVector::new(vector).get(index)
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
    // Inlined, as `root_offset()` is
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn get_size(&self, position: usize) -> Result<usize, DecodeError> {
        self.get_slot(position).map(|size| size as usize)
    }

    /// Reads a length, offset or header slot at `position`
    // Inlined, as `root_offset()` is
    #[allow(clippy::inline_always)]
    #[inline(always)]
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
    // Inlined, as `root_offset()` is
    #[allow(clippy::inline_always)]
    #[inline(always)]
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

impl TableReader<'_, true> {
    /// Returns the version that the table's header carries after its length.
    /// It's as wide as the header's other slots.
    ///
    /// # Errors
    ///
    /// [`DecodeError::OutOfRange`] if the header doesn't lie within the
    /// bytes.
    pub fn version(&self) -> Result<u32, DecodeError> {
        let position = if self.sized {
            self.offset_type().size()
        } else {
            0
        };

        self.get_slot(position)
    }
}

/// Reads the bytes of the string at `position` as UTF-8: a string field's
/// after its length, or a string of a vector of strings, at its first byte
///
/// # Errors
///
/// [`DecodeError::InvalidUtf8`] at `position` if they aren't valid UTF-8.
// Inlined into its callers, as `TableReader::get_str()` is
#[allow(clippy::inline_always)]
#[inline(always)]
pub fn decode_str(bytes: &[u8], position: usize) -> Result<&str, DecodeError> {
    // ASCII first, which `is_ascii()` checks a word at a time at any address,
    // where `str::from_utf8()` checks a string's bytes one at a time but for
    // whole pairs of aligned words in it: the benchmarks' record, whose
    // 16-byte name isn't aligned, read in 6.3 ns rather than 11.8 ns
    if is_ascii(bytes) {
        // SAFETY: ASCII is valid UTF-8
        return Ok(unsafe { str::from_utf8_unchecked(bytes) });
    }

    str::from_utf8(bytes).map_err(|source| DecodeError::InvalidUtf8 { position, source })
}

/// The length from which [`is_ascii()`] calls the standard library's
/// `is_ascii()`, which checks such bytes with SIMD on aarch64 and x86-64
const SHORT_ASCII: usize = 64;

/// Whether `bytes` are ASCII, checking those shorter than [`SHORT_ASCII`] a
/// word at a time, as the standard library's `is_ascii()` does
// Inlined into `decode_str()`'s callers. The standard library's
// `is_ascii()` is a function of its own, which LLVM inlined into a getter
// that it called out of line, but called out of line from larger callers,
// such as a read of a record's fields, by hand or through tafla-gen's
// getters: inlined, the benchmarks' record read in 5.0 ns rather than
// 5.6 ns. A table of 16 strings of 64 bytes or more, through tafla-gen's
// getters, took 1.05 to 1.10 times as long, its read grown by 16 inlined
// checks, and calling `is_ascii()` through a function that's never inlined
// made it slower still, and strings read by hand too.
#[allow(clippy::inline_always)]
#[inline(always)]
fn is_ascii(bytes: &[u8]) -> bool {
    const NON_ASCII: u64 = u64::from_ne_bytes([0x80; 8]);

    if bytes.len() >= SHORT_ASCII {
        return bytes.is_ascii();
    }

    let (words, rest) = bytes.as_chunks::<8>();
    for word in words {
        if u64::from_ne_bytes(*word) & NON_ASCII != 0 {
            return false;
        }
    }

    rest.iter().all(u8::is_ascii)
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
