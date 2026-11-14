use std::borrow::Cow;

use tafla::{
    Alignment, BuildError, ByteBuffer, DecodeError, InPlaceTable, TableBuilder, TableLayout,
};

use crate::codec::{Codec, DocumentWorkload, Error, Workload};
use crate::item_access::{ItemAccess, no_items, sum_item};
use crate::workloads::{
    Document, DocumentShape, Item, List, ListShape, Record, RecordShape, Sample, SampleShape,
    StructsShape, sum_column,
};

const DOCUMENT_LAYOUT: TableLayout<true> = TableLayout::DEFAULT.sized().versioned();
const DOCUMENT_VERSION: u32 = 1;

const SAMPLE_SIZE: usize = 12;

/// An item as a struct: its three float32s, aligned to 4 in an array
const ITEM_SIZE: usize = 12;
const ITEM_ALIGNMENT: Alignment = Alignment::Four;

/// tafla's runtime, as its README shows.
///
/// Tables are nested from their builders, which borrow the columns, and
/// arrays are read in place as `ArrayView`s, whose `to_cow()` borrows them
/// when they're aligned on a little-endian host.
///
/// The runtime writes tables, and leaves fixed-size records to the standard
/// library, as its README says. The fixed-size record is written with
/// `to_le_bytes()` and read with `ByteBuffer`'s getters, and an array of
/// structs with `to_le_bytes()` and `from_le_bytes()`, as tafla-gen's code
/// writes and reads an array of them.
#[derive(Clone, Copy, Debug)]
pub struct Tafla;

impl Codec for Tafla {
    const NAME: &'static str = "tafla";
}

impl Workload<RecordShape> for Tafla {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &Record<'_>) -> Result<Vec<u8>, Error> {
        let mut builder = TableBuilder::new(4, TableLayout::DEFAULT)?;
        builder.set_f64(input.x)?;
        builder.set_f64(input.y)?;
        builder.set_i64(input.id)?;
        builder.set_str(input.name.as_ref())?;

        Ok(builder.to_bytes()?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Record<'_>, Error> {
        let buffer = ByteBuffer::new(bytes, TableLayout::DEFAULT);

        Ok(Record {
            x: buffer.get_f64(buffer.root_offset(0)?)?,
            y: buffer.get_f64(buffer.root_offset(1)?)?,
            id: buffer.get_i64(buffer.root_offset(2)?)?,
            name: Cow::Borrowed(buffer.get_str(buffer.root_offset(3)?)?),
        })
    }
}

impl Workload<DocumentShape> for Tafla {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &Document<'_>) -> Result<Vec<u8>, Error> {
        let mut document = TableBuilder::versioned(4, DOCUMENT_LAYOUT, DOCUMENT_VERSION)?;
        document.set_f64(input.x)?;
        document.set_f64(input.y)?;
        document.set_str(input.name.as_ref())?;
        document.set_table_with(4, |columns| {
            columns.set_i64(input.base)?;
            columns.set_u32_array(input.keys.as_ref())?;
            columns.set_f32_array(input.a.as_ref())?;
            columns.set_f32_array(input.b.as_ref())
        })?;

        Ok(document.to_bytes()?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Document<'_>, Error> {
        let buffer = open_document(bytes)?;
        let columns = buffer.root_offset(3)?;

        Ok(Document {
            x: buffer.get_f64(buffer.root_offset(0)?)?,
            y: buffer.get_f64(buffer.root_offset(1)?)?,
            name: Cow::Borrowed(buffer.get_str(buffer.root_offset(2)?)?),
            base: buffer.get_i64(buffer.field_offset(columns, 0)?)?,
            keys: buffer
                .get_u32_array(buffer.field_offset(columns, 1)?)?
                .to_cow(),
            a: buffer
                .get_f32_array(buffer.field_offset(columns, 2)?)?
                .to_cow(),
            b: buffer
                .get_f32_array(buffer.field_offset(columns, 3)?)?
                .to_cow(),
        })
    }
}

impl DocumentWorkload for Tafla {
    fn sum_a(self, bytes: &[u8]) -> Result<f64, Error> {
        let buffer = open_document(bytes)?;
        let a = buffer.get_f32_array(buffer.field_offset(buffer.root_offset(3)?, 2)?)?;

        Ok(sum_column(a.iter()))
    }
}

impl Workload<ListShape> for Tafla {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &List) -> Result<Vec<u8>, Error> {
        let items = &input.items;

        let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
        builder.set_vector_with(items.len(), 2, |index, item| set_item(item, &items[index]))?;

        Ok(builder.to_bytes()?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<List, Error> {
        let buffer = ByteBuffer::new(bytes, TableLayout::DEFAULT);
        let vector = buffer.get_vector(buffer.root_offset(0)?)?;

        let mut items = Vec::with_capacity(vector.len());
        for item in vector.iter_in_place() {
            items.push(read_item(item?)?);
        }

        Ok(List { items })
    }
}

impl Workload<StructsShape> for Tafla {
    type Bytes = Vec<u8>;

    /// Writes the structs one after another into a `Vec`, which the builder
    /// owns, as tafla-gen's code writes an array of them
    fn serialize(self, input: &List) -> Result<Vec<u8>, Error> {
        let mut structs = Vec::with_capacity(input.items.len() * ITEM_SIZE);
        for item in &input.items {
            structs.extend_from_slice(&item_struct(item));
        }

        let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
        builder.set_struct_array(structs, ITEM_SIZE, ITEM_ALIGNMENT)?;

        Ok(builder.to_bytes()?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<List, Error> {
        let buffer = ByteBuffer::new(bytes, TableLayout::DEFAULT);
        let structs = buffer.get_struct_array(buffer.root_offset(0)?, ITEM_SIZE)?;
        let (structs, _) = structs.as_chunks::<ITEM_SIZE>();

        let items = structs.iter().map(read_item_struct).collect();

        Ok(List { items })
    }
}

impl Workload<SampleShape> for Tafla {
    type Bytes = [u8; SAMPLE_SIZE];

    fn serialize(self, input: &Sample) -> Result<[u8; SAMPLE_SIZE], Error> {
        let mut bytes = [0; SAMPLE_SIZE];
        bytes[0..4].copy_from_slice(&input.key.to_le_bytes());
        bytes[4..8].copy_from_slice(&input.a.to_le_bytes());
        bytes[8..12].copy_from_slice(&input.b.to_le_bytes());

        Ok(bytes)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Sample, Error> {
        let buffer = ByteBuffer::new(bytes, TableLayout::DEFAULT);

        Ok(Sample {
            key: buffer.get_u32(0)?,
            a: buffer.get_f32(4)?,
            b: buffer.get_f32(8)?,
        })
    }
}

/// `ByteBuffer`'s getters by hand, as the codec reads the list
impl ItemAccess for Tafla {
    const NAME: &'static str = "tafla";

    fn last_table(self, bytes: &[u8]) -> Result<f64, Error> {
        let buffer = ByteBuffer::new(bytes, TableLayout::DEFAULT);
        let vector = buffer.get_vector(buffer.root_offset(0)?)?;
        let last = vector.len().checked_sub(1).ok_or_else(no_items)?;
        let item = vector.get_in_place(last).ok_or_else(no_items)??;

        Ok(sum_table(item)?)
    }

    fn every_table(self, bytes: &[u8]) -> Result<f64, Error> {
        let buffer = ByteBuffer::new(bytes, TableLayout::DEFAULT);
        let vector = buffer.get_vector(buffer.root_offset(0)?)?;

        let mut sum = 0.0;
        for item in vector.iter_in_place() {
            sum += sum_table(item?)?;
        }

        Ok(sum)
    }

    fn last_struct(self, bytes: &[u8]) -> Result<f64, Error> {
        let buffer = ByteBuffer::new(bytes, TableLayout::DEFAULT);
        let structs = buffer.get_struct_array(buffer.root_offset(0)?, ITEM_SIZE)?;
        let (structs, _) = structs.as_chunks::<ITEM_SIZE>();

        Ok(sum_struct(structs.last().ok_or_else(no_items)?))
    }

    fn every_struct(self, bytes: &[u8]) -> Result<f64, Error> {
        let buffer = ByteBuffer::new(bytes, TableLayout::DEFAULT);
        let structs = buffer.get_struct_array(buffer.root_offset(0)?, ITEM_SIZE)?;
        let (structs, _) = structs.as_chunks::<ITEM_SIZE>();

        Ok(structs
            .iter()
            .map(sum_struct)
            .fold(0.0, |sum, item| sum + item))
    }
}

/// Reads a document's header, which must carry the version it's written in
fn open_document(bytes: &[u8]) -> Result<ByteBuffer<'_, true>, DecodeError> {
    let buffer = ByteBuffer::new(bytes, DOCUMENT_LAYOUT);

    let version = buffer.version()?;
    if version != DOCUMENT_VERSION {
        return Err(DecodeError::UnsupportedVersion {
            version,
            supported: DOCUMENT_VERSION,
        });
    }

    Ok(buffer)
}

fn set_item(table: &mut InPlaceTable<'_, '_>, item: &Item) -> Result<(), BuildError> {
    table.set_f32(item.value)?;
    table.set_table_with(2, |pair| {
        pair.set_f32(item.first)?;
        pair.set_f32(item.second)
    })
}

/// Reads an item of the vector in place, a table without a header, whose
/// fields `root_offset()` finds from its start
fn read_item(bytes: &[u8]) -> Result<Item, DecodeError> {
    let item = ByteBuffer::new(bytes, TableLayout::DEFAULT);
    let pair = item.root_offset(1)?;

    Ok(Item {
        value: item.get_f32(item.root_offset(0)?)?,
        first: item.get_f32(item.field_offset(pair, 0)?)?,
        second: item.get_f32(item.field_offset(pair, 1)?)?,
    })
}

/// Sums an item of the vector, as [`read_item()`] reads it
fn sum_table(bytes: &[u8]) -> Result<f64, DecodeError> {
    let item = ByteBuffer::new(bytes, TableLayout::DEFAULT);
    let pair = item.root_offset(1)?;

    Ok(sum_item(
        item.get_f32(item.root_offset(0)?)?,
        item.get_f32(item.field_offset(pair, 0)?)?,
        item.get_f32(item.field_offset(pair, 1)?)?,
    ))
}

fn sum_struct(bytes: &[u8; ITEM_SIZE]) -> f64 {
    let item = read_item_struct(bytes);
    sum_item(item.value, item.first, item.second)
}

fn item_struct(item: &Item) -> [u8; ITEM_SIZE] {
    let mut bytes = [0; ITEM_SIZE];
    bytes[0..4].copy_from_slice(&item.value.to_le_bytes());
    bytes[4..8].copy_from_slice(&item.first.to_le_bytes());
    bytes[8..12].copy_from_slice(&item.second.to_le_bytes());

    bytes
}

fn read_item_struct(bytes: &[u8; ITEM_SIZE]) -> Item {
    Item {
        value: f32::from_le_bytes(bytes[0..4].try_into().unwrap_or_default()),
        first: f32::from_le_bytes(bytes[4..8].try_into().unwrap_or_default()),
        second: f32::from_le_bytes(bytes[8..12].try_into().unwrap_or_default()),
    }
}
