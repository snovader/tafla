use std::{error::Error, fmt, str::Utf8Error};

use crate::OffsetType;

/// Returned when bytes can't be read as the table, array, string, struct,
/// vector or value that a read expects.
///
/// Every read checks its range against the supplied bytes before it returns,
/// so a truncated or corrupted buffer returns this rather than values from
/// outside them. Strings must also be valid UTF-8.
///
/// Checks can't tell a value that was corrupted within range from a real one,
/// such as an offset that points at another field. Formats that need that
/// guarantee can add a checksum.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The read of `byte_length` bytes at `position` would leave the
    /// `available` bytes
    OutOfRange {
        position: usize,
        byte_length: u64,
        available: usize,
    },
    /// The string at `position` isn't valid UTF-8
    InvalidUtf8 { position: usize, source: Utf8Error },
    /// The vector item at `start` ends before it starts, at `end`. An item
    /// starts where the item before it ends, and the first after the ends.
    ItemEndsBeforeStart { start: usize, end: usize },
    /// The header carries format `version`, which the reader can't read,
    /// since it reads version `supported`. `TableReader` doesn't compare
    /// versions: a reader returns this when [`TableReader::version()`] isn't
    /// its own.
    ///
    /// [`TableReader::version()`]: crate::TableReader::version
    UnsupportedVersion { version: u32, supported: u32 },
}

impl DecodeError {
    pub(crate) const fn out_of_range(position: usize, byte_length: u64, available: usize) -> Self {
        Self::OutOfRange {
            position,
            byte_length,
            available,
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange {
                position,
                byte_length,
                available,
            } => write!(
                f,
                "Read exceeds the supplied buffer: {byte_length} bytes at {position} of {available}"
            ),
            Self::InvalidUtf8 { position, .. } => {
                write!(f, "The string at {position} isn't valid UTF-8")
            }
            Self::ItemEndsBeforeStart { start, end } => {
                write!(
                    f,
                    "The vector item at {start} ends before it starts, at {end}"
                )
            }
            Self::UnsupportedVersion { version, supported } => write!(
                f,
                "Unsupported format version {version}; the reader reads version {supported}"
            ),
        }
    }
}

impl Error for DecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::OutOfRange { .. }
            | Self::ItemEndsBeforeStart { .. }
            | Self::UnsupportedVersion { .. } => None,
            Self::InvalidUtf8 { source, .. } => Some(source),
        }
    }
}

/// Returned when a `TableBuilder`, or a `TableSizer` and its `TableWriter`,
/// can't write a table, whatever the values' bytes. A failed setter leaves the
/// builder as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuildError {
    /// The version doesn't fit in the layout's header slots
    VersionTooLarge {
        version: u32,
        offset_type: OffsetType,
    },
    /// The header of `fields` offsets alone is longer than the layout's
    /// offsets address
    HeaderTooLong {
        fields: usize,
        offset_type: OffsetType,
    },
    /// A field was set on a table that already holds its `fields` fields, or
    /// written to a `TableWriter` at an index past them
    TooManyFields { fields: usize },
    /// The field at `index` would make the table `length` bytes, longer than
    /// the layout's offsets address
    TooLong {
        index: usize,
        length: u64,
        offset_type: OffsetType,
    },
    /// The table was written, or nested, with `set` of its `fields` fields
    /// set
    Incomplete { set: usize, fields: usize },
    /// The `length` bytes of an array of structs aren't a whole number of
    /// `size`-byte structs, or `size` is 0
    NotWholeStructs { length: usize, size: usize },
    /// The fields that a `TableWriter` wrote differ from those that its
    /// `TableSizer` added: they wrote `written` of the table's `length`
    /// bytes, in all or before the first write that the table doesn't hold
    NotAsSized { written: usize, length: usize },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::VersionTooLarge {
                version,
                offset_type,
            } => write!(f, "Version {version} doesn't fit in a {offset_type} slot"),
            Self::HeaderTooLong {
                fields,
                offset_type,
            } => write!(
                f,
                "A header of {fields} fields doesn't fit in {offset_type} offsets"
            ),
            Self::TooManyFields { fields } => {
                write!(f, "Can't add field {} to a table of {fields}", fields + 1)
            }
            Self::TooLong {
                index,
                length,
                offset_type,
            } => write!(
                f,
                "Field {} makes the table {length} bytes, too long for {offset_type} offsets",
                index + 1
            ),
            Self::Incomplete { set, fields } => {
                write!(f, "Only {set} of the table's {fields} fields were set")
            }
            Self::NotWholeStructs { size: 0, .. } => write!(f, "A struct can't be 0 bytes"),
            Self::NotWholeStructs { length, size } => {
                write!(
                    f,
                    "{length} bytes aren't a whole number of {size}-byte structs"
                )
            }
            Self::NotAsSized { written, length } => {
                write!(
                    f,
                    "The fields wrote {written} of the table's {length} bytes"
                )
            }
        }
    }
}

impl Error for BuildError {}
