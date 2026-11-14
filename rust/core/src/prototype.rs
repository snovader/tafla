//! A prototype of a builder that writes each field's bytes when it's set.
//!
//! It's beside [`crate::TableBuilder`], which records each field and writes
//! them all in `to_bytes()`, and writes as the builders of `FlatBuffers`,
//! protobuf and `MessagePack` do. It's built only with the `prototype`
//! feature, for the evaluation that its draft pull request describes, and
//! isn't part of the runtime's API.
//!
//! [`DirectTableBuilder`] has `TableBuilder`'s setters, bytes, errors and
//! failure atomicity. Its type parameters choose how its bytes grow and
//! which runs it keeps as segments, so that the evaluation can time each
//! choice; [`DirectTableBuilder`]'s defaults are the proposed design.

mod buffer;
mod direct_table_builder;
mod segments;

pub use buffer::{Buffer, InlineBuffer, VecBuffer};
pub use direct_table_builder::{DirectInPlaceTable, DirectNestedTable, DirectTableBuilder};

/// The shortest run kept as a segment.
///
/// Runs of this many bytes or more, of an array's elements, a string, a
/// struct or a table nested as bytes, are kept as segments, which
/// `to_bytes()` copies, rather than copied into the builder's bytes when
/// they're set.
pub const SEGMENT_BYTES: usize = 256;

/// A segment threshold that keeps no run as a segment, so that every value
/// is copied into the builder's bytes when it's set
pub const NO_SEGMENTS: usize = usize::MAX;
