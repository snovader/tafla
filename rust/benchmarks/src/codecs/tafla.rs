use std::borrow::Cow;

use tafla::{
    Alignment, BuildError, DecodeError, InPlaceTable, TableBuilder, TableLayout, TableReader,
};

use crate::codec::{Codec, DocumentWorkload, Error, Workload};
use crate::item_access::{ItemAccess, no_items, sum_item};
use crate::string_workloads::{COUNT, Strings, StringsShape};
use crate::workloads::{
    Document, DocumentShape, Item, List, ListShape, Record, RecordShape, Sample, SampleShape,
    StructsShape, sum_column,
};

const DOCUMENT_VERSION: u32 = 1;
const DOCUMENT_LAYOUT: TableLayout<true> =
    TableLayout::DEFAULT.sized().with_version(DOCUMENT_VERSION);

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
/// `to_le_bytes()` and read with `TableReader`'s getters, and an array of
/// structs with `to_le_bytes()` and `from_le_bytes()`, as tafla-gen's code
/// writes and reads an array of them.
///
/// tafla has no vector of strings, so the strings comparison's table is a
/// table of 16 string fields, whose `get_str()` checks each string's UTF-8.
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
        let reader = TableReader::new(bytes, TableLayout::DEFAULT);

        Ok(Record {
            x: reader.get_f64(reader.root_offset(0)?)?,
            y: reader.get_f64(reader.root_offset(1)?)?,
            id: reader.get_i64(reader.root_offset(2)?)?,
            name: Cow::Borrowed(reader.get_str(reader.root_offset(3)?)?),
        })
    }
}

impl Workload<DocumentShape> for Tafla {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &Document<'_>) -> Result<Vec<u8>, Error> {
        let mut document = TableBuilder::new(4, DOCUMENT_LAYOUT)?;
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
        let reader = open_document(bytes)?;
        let columns = reader.root_offset(3)?;

        Ok(Document {
            x: reader.get_f64(reader.root_offset(0)?)?,
            y: reader.get_f64(reader.root_offset(1)?)?,
            name: Cow::Borrowed(reader.get_str(reader.root_offset(2)?)?),
            base: reader.get_i64(reader.field_offset(columns, 0)?)?,
            keys: reader
                .get_u32_array(reader.field_offset(columns, 1)?)?
                .to_cow(),
            a: reader
                .get_f32_array(reader.field_offset(columns, 2)?)?
                .to_cow(),
            b: reader
                .get_f32_array(reader.field_offset(columns, 3)?)?
                .to_cow(),
        })
    }
}

impl DocumentWorkload for Tafla {
    fn sum_a(self, bytes: &[u8]) -> Result<f64, Error> {
        let reader = open_document(bytes)?;
        let a = reader.get_f32_array(reader.field_offset(reader.root_offset(3)?, 2)?)?;

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
        let reader = TableReader::new(bytes, TableLayout::DEFAULT);
        let vector = reader.get_vector(reader.root_offset(0)?)?;

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
    /// owns. tafla-gen's code writes them straight into the table, with
    /// `TableWriter::set_structs()`.
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
        let reader = TableReader::new(bytes, TableLayout::DEFAULT);
        let structs = reader.get_struct_array(reader.root_offset(0)?, ITEM_SIZE)?;
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
        let reader = TableReader::new(bytes, TableLayout::DEFAULT);

        Ok(Sample {
            key: reader.get_u32(0)?,
            a: reader.get_f32(4)?,
            b: reader.get_f32(8)?,
        })
    }
}

impl Workload<StringsShape> for Tafla {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &Strings<'_>) -> Result<Vec<u8>, Error> {
        let mut builder = TableBuilder::new(COUNT, TableLayout::DEFAULT)?;
        for string in &input.items {
            builder.set_str(string.as_ref())?;
        }

        Ok(builder.to_bytes()?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Strings<'_>, Error> {
        let reader = TableReader::new(bytes, TableLayout::DEFAULT);

        let mut items = [const { Cow::Borrowed("") }; COUNT];
        for (index, item) in items.iter_mut().enumerate() {
            *item = Cow::Borrowed(reader.get_str(reader.root_offset(index)?)?);
        }

        Ok(Strings { items })
    }
}

/// `TableReader`'s getters by hand, as the codec reads the list
impl ItemAccess for Tafla {
    const NAME: &'static str = "tafla";

    fn last_table(self, bytes: &[u8]) -> Result<f64, Error> {
        let reader = TableReader::new(bytes, TableLayout::DEFAULT);
        let vector = reader.get_vector(reader.root_offset(0)?)?;
        let last = vector.len().checked_sub(1).ok_or_else(no_items)?;
        let item = vector.get_in_place(last).ok_or_else(no_items)??;

        Ok(sum_table(item)?)
    }

    fn every_table(self, bytes: &[u8]) -> Result<f64, Error> {
        let reader = TableReader::new(bytes, TableLayout::DEFAULT);
        let vector = reader.get_vector(reader.root_offset(0)?)?;

        let mut sum = 0.0;
        for item in vector.iter_in_place() {
            sum += sum_table(item?)?;
        }

        Ok(sum)
    }

    fn last_struct(self, bytes: &[u8]) -> Result<f64, Error> {
        let reader = TableReader::new(bytes, TableLayout::DEFAULT);
        let structs = reader.get_struct_array(reader.root_offset(0)?, ITEM_SIZE)?;
        let (structs, _) = structs.as_chunks::<ITEM_SIZE>();

        Ok(sum_struct(structs.last().ok_or_else(no_items)?))
    }

    fn every_struct(self, bytes: &[u8]) -> Result<f64, Error> {
        let reader = TableReader::new(bytes, TableLayout::DEFAULT);
        let structs = reader.get_struct_array(reader.root_offset(0)?, ITEM_SIZE)?;
        let (structs, _) = structs.as_chunks::<ITEM_SIZE>();

        Ok(structs
            .iter()
            .map(sum_struct)
            .fold(0.0, |sum, item| sum + item))
    }
}

/// Reads a document's header, which must carry the version it's written in
fn open_document(bytes: &[u8]) -> Result<TableReader<'_, true>, DecodeError> {
    let reader = TableReader::new(bytes, DOCUMENT_LAYOUT);

    let version = reader.version()?;
    if version != DOCUMENT_VERSION {
        return Err(DecodeError::UnsupportedVersion {
            version,
            supported: DOCUMENT_VERSION,
        });
    }

    Ok(reader)
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
    let item = TableReader::new(bytes, TableLayout::DEFAULT);
    let pair = item.root_offset(1)?;

    Ok(Item {
        value: item.get_f32(item.root_offset(0)?)?,
        first: item.get_f32(item.field_offset(pair, 0)?)?,
        second: item.get_f32(item.field_offset(pair, 1)?)?,
    })
}

/// Sums an item of the vector, as [`read_item()`] reads it
fn sum_table(bytes: &[u8]) -> Result<f64, DecodeError> {
    let item = TableReader::new(bytes, TableLayout::DEFAULT);
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
