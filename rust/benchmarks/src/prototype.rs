//! The tafla codec's serialization with the prototype `DirectTableBuilder`.
//!
//! It replaces `TableBuilder` in the codec's calls, for the
//! `builder_comparison` and `builder_allocation` binaries, which time and
//! count them, built with the `prototype` feature.
//!
//! Each function makes the codec's calls, with a builder of bytes `B` and
//! `N` segments within it, and returns the table from `to_bytes()`, or from
//! `into_bytes()` when `INTO`. Each is a function of its own, as the codec's
//! `serialize()` is compiled, so that every loop that calls it times the
//! same code, and the setters' fills are each called from one place, which
//! the compiler inlines them into, as it inlines the codec's. `tests/prototype.rs` checks that they write
//! the codec's bytes.

use std::marker::PhantomData;

use tafla::prototype::{Buffer, DirectTableBuilder, InlineBuffer, NO_SEGMENTS, VecBuffer};
use tafla::{Alignment, BuildError, TableLayout};

use crate::codec::{Error, Workload};
use crate::codecs::Tafla;
use crate::workloads::{
    Document, DocumentShape, Item, List, ListShape, Record, RecordShape, StructsShape,
};

const DOCUMENT_LAYOUT: TableLayout<true> = TableLayout::DEFAULT.sized().versioned();
const DOCUMENT_VERSION: u32 = 1;

/// An item as a struct: its three float32s, aligned to 4 in an array
const ITEM_SIZE: usize = 12;
const ITEM_ALIGNMENT: Alignment = Alignment::Four;

/// Serializes the record as the tafla codec does
///
/// # Errors
///
/// The builder's error, which the record doesn't cause.
#[inline(never)]
pub fn record<B: Buffer, const N: usize, const INTO: bool>(
    input: &Record<'_>,
) -> Result<Vec<u8>, Error> {
    let mut builder = DirectTableBuilder::<B, N>::with_buffer(4, TableLayout::DEFAULT)?;
    builder.set_f64(input.x)?;
    builder.set_f64(input.y)?;
    builder.set_i64(input.id)?;
    builder.set_str(input.name.as_ref())?;

    Ok(finish::<B, N, INTO>(builder)?)
}

/// Serializes a document as the tafla codec does
///
/// # Errors
///
/// The builder's error, which the documents don't cause.
#[inline(never)]
pub fn document<B: Buffer, const N: usize, const INTO: bool>(
    input: &Document<'_>,
) -> Result<Vec<u8>, Error> {
    let mut document =
        DirectTableBuilder::<B, N>::versioned_with_buffer(4, DOCUMENT_LAYOUT, DOCUMENT_VERSION)?;
    document.set_f64(input.x)?;
    document.set_f64(input.y)?;
    document.set_str(input.name.as_ref())?;
    document.set_table_with(4, |columns| {
        columns.set_i64(input.base)?;
        columns.set_u32_array(input.keys.as_ref())?;
        columns.set_f32_array(input.a.as_ref())?;
        columns.set_f32_array(input.b.as_ref())
    })?;

    Ok(finish::<B, N, INTO>(document)?)
}

/// Serializes the vector of nested tables as the tafla codec does
///
/// # Errors
///
/// The builder's error, which the list doesn't cause.
#[inline(never)]
pub fn list<B: Buffer, const N: usize, const INTO: bool>(input: &List) -> Result<Vec<u8>, Error> {
    let items = &input.items;

    let mut builder = DirectTableBuilder::<B, N>::with_buffer(1, TableLayout::DEFAULT)?;
    builder.set_vector_with(items.len(), 2, |index, table| {
        let item = &items[index];
        table.set_f32(item.value)?;
        table.set_table_with(2, |pair| {
            pair.set_f32(item.first)?;
            pair.set_f32(item.second)
        })
    })?;

    Ok(finish::<B, N, INTO>(builder)?)
}

/// Serializes the array of structs as the tafla codec does, from a `Vec` of
/// their bytes that the builder owns
///
/// # Errors
///
/// The builder's error, which the list doesn't cause.
#[inline(never)]
pub fn structs<B: Buffer, const N: usize, const INTO: bool>(
    input: &List,
) -> Result<Vec<u8>, Error> {
    let mut structs = Vec::with_capacity(input.items.len() * ITEM_SIZE);
    for item in &input.items {
        structs.extend_from_slice(&item_struct(item));
    }

    let mut builder = DirectTableBuilder::<B, N>::with_buffer(1, TableLayout::DEFAULT)?;
    builder.set_struct_array(structs, ITEM_SIZE, ITEM_ALIGNMENT)?;

    Ok(finish::<B, N, INTO>(builder)?)
}

/// Takes the table from the builder, inlined into the caller, so that the
/// builder isn't moved into a function of its own
#[allow(clippy::inline_always)]
#[inline(always)]
fn finish<B: Buffer, const N: usize, const INTO: bool>(
    builder: DirectTableBuilder<'_, B, N>,
) -> Result<Vec<u8>, BuildError> {
    if INTO {
        builder.into_bytes()
    } else {
        builder.to_bytes()
    }
}

fn item_struct(item: &Item) -> [u8; ITEM_SIZE] {
    let mut bytes = [0; ITEM_SIZE];
    bytes[0..4].copy_from_slice(&item.value.to_le_bytes());
    bytes[4..8].copy_from_slice(&item.first.to_le_bytes());
    bytes[8..12].copy_from_slice(&item.second.to_le_bytes());

    bytes
}

/// A builder of the comparison's tables, as the binaries compare them
pub trait Builder: 'static {
    /// # Errors
    ///
    /// The builder's error, which the record doesn't cause.
    fn record(input: &Record<'_>) -> Result<Vec<u8>, Error>;

    /// # Errors
    ///
    /// The builder's error, which the documents don't cause.
    fn document(input: &Document<'_>) -> Result<Vec<u8>, Error>;

    /// # Errors
    ///
    /// The builder's error, which the list doesn't cause.
    fn list(input: &List) -> Result<Vec<u8>, Error>;

    /// # Errors
    ///
    /// The builder's error, which the list doesn't cause.
    fn structs(input: &List) -> Result<Vec<u8>, Error>;
}

/// `TableBuilder`, through the tafla codec itself
pub struct Reference;

impl Builder for Reference {
    #[inline]
    fn record(input: &Record<'_>) -> Result<Vec<u8>, Error> {
        Workload::<RecordShape>::serialize(Tafla, input)
    }

    #[inline]
    fn document(input: &Document<'_>) -> Result<Vec<u8>, Error> {
        Workload::<DocumentShape>::serialize(Tafla, input)
    }

    #[inline]
    fn list(input: &List) -> Result<Vec<u8>, Error> {
        Workload::<ListShape>::serialize(Tafla, input)
    }

    #[inline]
    fn structs(input: &List) -> Result<Vec<u8>, Error> {
        Workload::<StructsShape>::serialize(Tafla, input)
    }
}

/// `DirectTableBuilder` of bytes `B` and `N` segments within it, taking the
/// table with `into_bytes()` when `INTO`
pub struct Direct<B, const N: usize, const INTO: bool>(PhantomData<B>);

impl<B: Buffer, const N: usize, const INTO: bool> Builder for Direct<B, N, INTO> {
    #[inline]
    fn record(input: &Record<'_>) -> Result<Vec<u8>, Error> {
        record::<B, N, INTO>(input)
    }

    #[inline]
    fn document(input: &Document<'_>) -> Result<Vec<u8>, Error> {
        document::<B, N, INTO>(input)
    }

    #[inline]
    fn list(input: &List) -> Result<Vec<u8>, Error> {
        list::<B, N, INTO>(input)
    }

    #[inline]
    fn structs(input: &List) -> Result<Vec<u8>, Error> {
        structs::<B, N, INTO>(input)
    }
}

/// Another builder's functions again, as functions of their own, for an
/// A/A pair
pub struct Again<T>(PhantomData<T>);

impl<T: Builder> Builder for Again<T> {
    #[inline]
    fn record(input: &Record<'_>) -> Result<Vec<u8>, Error> {
        T::record(input)
    }

    #[inline]
    fn document(input: &Document<'_>) -> Result<Vec<u8>, Error> {
        T::document(input)
    }

    #[inline]
    fn list(input: &List) -> Result<Vec<u8>, Error> {
        T::list(input)
    }

    #[inline]
    fn structs(input: &List) -> Result<Vec<u8>, Error> {
        T::structs(input)
    }
}

/// Something done with each builder that the binaries compare
pub trait Visit {
    fn visit<T: Builder>(&mut self, name: &'static str);
}

/// The proposed design
pub type Proposed = Direct<VecBuffer, 4, false>;

/// Visits `TableBuilder` and each direct builder, `TableBuilder` and the
/// proposed design twice each, as A/A pairs
pub fn for_each_builder(visitor: &mut impl Visit) {
    visitor.visit::<Reference>("TableBuilder");
    visitor.visit::<Again<Reference>>("TableBuilder A/A");
    visitor.visit::<Proposed>("direct");
    visitor.visit::<Again<Proposed>>("direct A/A");
    visitor.visit::<Direct<VecBuffer<256, true>, 4, true>>("direct, into_bytes");
    visitor.visit::<Direct<VecBuffer<0>, 4, false>>("Vec from 0");
    visitor.visit::<Direct<VecBuffer<128>, 4, false>>("Vec from 128");
    visitor.visit::<Direct<VecBuffer<512>, 4, false>>("Vec from 512");
    visitor.visit::<Direct<VecBuffer<256, false>, 4, false>>("Vec 256, new+copy");
    visitor.visit::<Direct<InlineBuffer, 4, false>>("inline 128");
    visitor.visit::<Direct<InlineBuffer, 4, true>>("inline, into_bytes");
    visitor.visit::<Direct<VecBuffer, 0, false>>("segments in a Vec");
    visitor.visit::<Direct<InlineBuffer, 0, false>>("inline, segments in Vec");
    visitor.visit::<Direct<VecBuffer<256, true, NO_SEGMENTS>, 4, false>>("copying");
}
