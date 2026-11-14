#![doc = include_str!("../README.md")]

mod array_view;
mod byte_buffer;
mod element;
mod errors;
mod layout;
mod table_builder;
mod table_writer;
mod vector;

pub use array_view::{ArrayView, Iter};
pub use byte_buffer::ByteBuffer;
pub use element::Element;
pub use errors::{BuildError, DecodeError};
pub use layout::{Alignment, OffsetType, TableLayout};
pub use table_builder::{InPlaceTable, NestedTable, TableBuilder};
pub use table_writer::{SizerParent, TableSizer, TableWriter, VectorPosition, WriterParent};
pub use vector::{Items, ItemsInPlace, Vector};
