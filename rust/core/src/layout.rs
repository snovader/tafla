use std::fmt;

use crate::BuildError;

/// A table's offset type, which limits how long the table can be.
///
/// It's the unsigned integer type of the table's header slots, field offsets,
/// counts, lengths and item ends. Each variant is named after its type, as
/// `SPEC.md` and schemas name it, and displays as that name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OffsetType {
    /// `uint8`, for tables of up to 255 bytes
    Uint8,
    /// `uint16`, for tables of up to 65,535 bytes
    Uint16,
    /// `uint32`, for tables of up to 4 GiB
    Uint32,
}

impl OffsetType {
    /// The type's name, as `SPEC.md` and schemas spell it: `uint8`, `uint16`
    /// or `uint32`
    pub const fn name(self) -> &'static str {
        match self {
            Self::Uint8 => "uint8",
            Self::Uint16 => "uint16",
            Self::Uint32 => "uint32",
        }
    }

    /// The offset size, the type's width in bytes: 1, 2 or 4
    pub const fn size(self) -> usize {
        match self {
            Self::Uint8 => 1,
            Self::Uint16 => 2,
            Self::Uint32 => 4,
        }
    }

    /// The type's largest value, which is also the longest table that its
    /// offsets address
    pub const fn max_value(self) -> u32 {
        match self {
            Self::Uint8 => 0xff,
            Self::Uint16 => 0xffff,
            Self::Uint32 => 0xffff_ffff,
        }
    }
}

impl fmt::Display for OffsetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A multiple of bytes from a table's start that a field starts at, so that
/// its elements can be read in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Alignment {
    One = 1,
    Two = 2,
    Four = 4,
    Eight = 8,
}

impl Alignment {
    /// The alignment in bytes: 1, 2, 4 or 8
    pub const fn bytes(self) -> usize {
        self as usize
    }

    /// The bytes of padding that bring `offset` to a multiple of the
    /// alignment
    pub(crate) const fn padding(self, offset: usize) -> usize {
        offset.wrapping_neg() % self.bytes()
    }
}

/// The header and offset type of a table.
///
/// A format creates its layout once and passes it to both the `TableBuilder`
/// that writes the table and the `TableReader` that reads it, so that the two
/// can't disagree. Nested tables use [`TableLayout::DEFAULT`].
///
/// `VERSIONED` says whether the header holds a version, so that the compiler
/// offers the layout's `version()` and `TableReader::version()` only where
/// there is one. A layout with a version is the layout of that version of
/// the format: its builders and writers write the version, and its readers
/// read tables of any version, so that a program can read a table's version
/// before it chooses how to read the fields. The layouts of a format's
/// versions should differ in the version only.
///
/// ```
/// use tafla::{OffsetType, TableLayout};
///
/// const LAYOUT: TableLayout<true> = TableLayout::DEFAULT.sized().with_version(1);
/// const FRAME: TableLayout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint8);
///
/// assert_eq!(LAYOUT.header_size(), 8);
/// assert_eq!(LAYOUT.version(), 1);
/// assert_eq!(FRAME.header_size(), 0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TableLayout<const VERSIONED: bool = false> {
    sized: bool,
    offset_type: OffsetType,
    /// The version, which is zero when the layout has none
    version: u32,
}

impl TableLayout {
    /// No header slots, and offsets and lengths of the offset type `uint32`
    pub const DEFAULT: Self = Self {
        sized: false,
        offset_type: OffsetType::Uint32,
        version: 0,
    };
}

impl TableLayout<true> {
    /// The version that builders and writers write to the header
    pub const fn version(self) -> u32 {
        self.version
    }
}

impl<const VERSIONED: bool> TableLayout<VERSIONED> {
    /// Puts `version` in a slot of the header, after the length when sized,
    /// which builders and writers write there. Readers can check the version
    /// that a table holds, with `TableReader::version()`, before reading any
    /// field.
    ///
    /// A version must fit in the layout's offset type, which
    /// [`TableLayout::with_offset_type()`] may set later, so the builders
    /// and sizers check it, and return [`BuildError::VersionTooLarge`].
    #[must_use]
    pub const fn with_version(self, version: u32) -> TableLayout<true> {
        TableLayout {
            sized: self.sized,
            offset_type: self.offset_type,
            version,
        }
    }

    /// The version to write to the header, when it has a slot for one
    pub(crate) const fn header_version(self) -> Option<u32> {
        if VERSIONED { Some(self.version) } else { None }
    }

    /// Starts the header with the table's total byte length
    #[must_use]
    pub const fn sized(self) -> Self {
        Self {
            sized: true,
            ..self
        }
    }

    /// Sets the offset type, the unsigned integer type of the header slots,
    /// field offsets, counts, lengths and item ends, which limits how long
    /// the table can be. The default is [`OffsetType::Uint32`].
    #[must_use]
    pub const fn with_offset_type(self, offset_type: OffsetType) -> Self {
        Self {
            offset_type,
            ..self
        }
    }

    pub const fn is_sized(self) -> bool {
        self.sized
    }

    pub const fn is_versioned(self) -> bool {
        VERSIONED
    }

    pub const fn offset_type(self) -> OffsetType {
        self.offset_type
    }

    /// The offset size, the width in bytes of the offset type: 1, 2 or 4
    pub const fn offset_size(self) -> usize {
        self.offset_type.size()
    }

    /// The byte length of the header slots before the field offsets
    pub const fn header_size(self) -> usize {
        let slots = if self.sized { 1 } else { 0 } + if VERSIONED { 1 } else { 0 };
        slots * self.offset_type.size()
    }

    /// The byte length of the header of a table of `fields` fields: its
    /// slots and an offset per field, which `TableBuilder` and `TableSizer`
    /// start a table with
    ///
    /// # Errors
    ///
    /// [`BuildError::HeaderTooLong`] if the header is longer than the
    /// layout's offsets address.
    pub(crate) fn header_length(self, fields: usize) -> Result<u32, BuildError> {
        // In u64, since a header longer than 4 GiB would saturate to the
        // limit itself in a 32-bit usize
        let slots = (fields as u64)
            .saturating_add(u64::from(self.sized))
            .saturating_add(u64::from(VERSIONED));
        let header = slots.saturating_mul(self.offset_type.size() as u64);

        u32::try_from(header)
            .ok()
            .filter(|&length| length <= self.offset_type.max_value())
            .ok_or(BuildError::HeaderTooLong {
                fields,
                offset_type: self.offset_type,
            })
    }
}

impl<const VERSIONED: bool> TableLayout<VERSIONED> {
    /// Checks that the layout's version fits in the header's slot
    ///
    /// # Errors
    ///
    /// [`BuildError::VersionTooLarge`] if it doesn't fit in the layout's
    /// offset type.
    pub(crate) const fn check_version(self) -> Result<(), BuildError> {
        if self.version > self.offset_type.max_value() {
            return Err(BuildError::VersionTooLarge {
                version: self.version,
                offset_type: self.offset_type,
            });
        }

        Ok(())
    }
}

impl Default for TableLayout {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_layout() {
        for layout in [TableLayout::DEFAULT, TableLayout::default()] {
            assert!(!layout.is_sized());
            assert!(!layout.is_versioned());
            assert_eq!(layout.offset_type(), OffsetType::Uint32);
            assert_eq!(layout.header_size(), 0);
        }
    }

    #[test]
    fn test_offset_types() {
        let cases = [
            (OffsetType::Uint8, "uint8", 1, 0xff),
            (OffsetType::Uint16, "uint16", 2, 0xffff),
            (OffsetType::Uint32, "uint32", 4, 0xffff_ffff),
        ];

        for (offset_type, name, size, max_value) in cases {
            let layout = TableLayout::DEFAULT.with_offset_type(offset_type);

            assert_eq!(offset_type.name(), name);
            assert_eq!(offset_type.to_string(), name);
            assert_eq!(offset_type.size(), size);
            assert_eq!(offset_type.max_value(), max_value);
            assert_eq!(layout.offset_type(), offset_type);
            assert_eq!(layout.offset_size(), size);
        }
    }

    // The same cases as typescript/core/test/layout.test.ts
    #[test]
    fn test_header_size() {
        let layout = TableLayout::DEFAULT;

        assert_eq!(layout.with_offset_type(OffsetType::Uint8).header_size(), 0);
        assert_eq!(
            layout
                .sized()
                .with_offset_type(OffsetType::Uint8)
                .header_size(),
            1
        );
        assert_eq!(
            layout
                .with_version(0)
                .with_offset_type(OffsetType::Uint16)
                .header_size(),
            2
        );
        assert_eq!(
            layout
                .sized()
                .with_version(0)
                .with_offset_type(OffsetType::Uint16)
                .header_size(),
            4
        );
        assert_eq!(layout.sized().with_version(0).header_size(), 8);
    }
}
