// The codec uses a part of the generated code. tafla-gen's tests check its
// code with clippy's pedantic lints, and this crate's nursery lints suggest
// otherwise, such as `map_or()` for a `match` of an `Option`.
#[allow(dead_code, clippy::nursery)]
#[path = "tafla_gen/workloads.rs"]
mod generated;

use std::borrow::Cow;

use tafla::DecodeError;

use self::generated::{
    ColumnsFields, DocumentFields, ItemFields, ItemList, ItemListFields, PairFields, RecordFields,
    Triple, Triples, TriplesFields,
};
use crate::codec::{Codec, DocumentWorkload, Error, Workload};
use crate::item_access::{ItemAccess, no_items, sum_item};
use crate::workloads::{
    Document, DocumentShape, Item, List, ListShape, Record, RecordShape, Sample, SampleShape,
    StructsShape, sum_column,
};

/// The code that tafla-gen generates from `workloads.tafla`.
///
/// It writes each message's table in one call with a `TableSizer` and a
/// `TableWriter`, rather than nesting `TableBuilder`s, and reads it with the
/// generated getters, which borrow strings and arrays as the `Tafla` codec
/// does.
///
/// The generated code writes from structs of each table's fields, which
/// borrow strings and arrays. Its vector and array of structs take the
/// workload's items as they are, as an application writes its own types,
/// and convert each into the item's fields or struct as they write it. The
/// generated getters of the vector and of the array of structs return views,
/// whose `iter()` `deserialize()` reads into the workload's items.
#[derive(Clone, Copy, Debug)]
pub struct TaflaGen;

impl Codec for TaflaGen {
    const NAME: &'static str = "tafla-gen";
}

impl Workload<RecordShape> for TaflaGen {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &Record<'_>) -> Result<Vec<u8>, Error> {
        let fields = RecordFields {
            x: input.x,
            y: input.y,
            id: input.id,
            name: input.name.as_ref(),
        };

        Ok(fields.serialize()?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Record<'_>, Error> {
        let record = generated::Record::new(bytes);

        Ok(Record {
            x: record.x()?,
            y: record.y()?,
            id: record.id()?,
            name: Cow::Borrowed(record.name()?),
        })
    }
}

impl Workload<DocumentShape> for TaflaGen {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &Document<'_>) -> Result<Vec<u8>, Error> {
        let columns = ColumnsFields {
            base: input.base,
            keys: input.keys.as_ref(),
            a: input.a.as_ref(),
            b: input.b.as_ref(),
        };
        let fields = DocumentFields {
            x: input.x,
            y: input.y,
            name: input.name.as_ref(),
            columns,
        };

        Ok(fields.serialize()?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Document<'_>, Error> {
        let document = generated::Document::new(bytes)?;
        let columns = document.columns()?;

        Ok(Document {
            x: document.x()?,
            y: document.y()?,
            name: Cow::Borrowed(document.name()?),
            base: columns.base()?,
            keys: columns.keys()?.to_cow(),
            a: columns.a()?.to_cow(),
            b: columns.b()?.to_cow(),
        })
    }
}

impl DocumentWorkload for TaflaGen {
    fn sum_a(self, bytes: &[u8]) -> Result<f64, Error> {
        let a = generated::Document::new(bytes)?.columns()?.a()?;

        Ok(sum_column(a.iter()))
    }
}

impl Workload<ListShape> for TaflaGen {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &List) -> Result<Vec<u8>, Error> {
        Ok(ItemListFields {
            items: &input.items,
        }
        .serialize()?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<List, Error> {
        let tables = ItemList::new(bytes).items()?;

        let mut items = Vec::with_capacity(tables.len());
        for item in tables.iter() {
            let item = item?;
            let pair = item.pair()?;
            items.push(Item {
                value: item.value()?,
                first: pair.first()?,
                second: pair.second()?,
            });
        }

        Ok(List { items })
    }
}

impl Workload<StructsShape> for TaflaGen {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &List) -> Result<Vec<u8>, Error> {
        Ok(TriplesFields {
            items: &input.items,
        }
        .serialize()?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<List, Error> {
        let items = Triples::new(bytes)
            .items()?
            .iter()
            .map(item_of_triple)
            .collect();

        Ok(List { items })
    }
}

impl Workload<SampleShape> for TaflaGen {
    type Bytes = [u8; generated::Sample::SIZE];

    fn serialize(self, input: &Sample) -> Result<[u8; generated::Sample::SIZE], Error> {
        let sample = generated::Sample {
            key: input.key,
            a: input.a,
            b: input.b,
        };

        Ok(sample.to_bytes())
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Sample, Error> {
        let sample = generated::Sample::read(bytes)?;

        Ok(Sample {
            key: sample.key,
            a: sample.a,
            b: sample.b,
        })
    }
}

/// The views that the generated getters return, which read each item when
/// it's asked for
impl ItemAccess for TaflaGen {
    const NAME: &'static str = "tafla-gen";

    fn last_table(self, bytes: &[u8]) -> Result<f64, Error> {
        let items = ItemList::new(bytes).items()?;
        let last = items.len().checked_sub(1).ok_or_else(no_items)?;

        Ok(sum_table(&items.get(last).ok_or_else(no_items)??)?)
    }

    fn every_table(self, bytes: &[u8]) -> Result<f64, Error> {
        let items = ItemList::new(bytes).items()?;

        let mut sum = 0.0;
        for item in items.iter() {
            sum += sum_table(&item?)?;
        }

        Ok(sum)
    }

    fn last_struct(self, bytes: &[u8]) -> Result<f64, Error> {
        let triples = Triples::new(bytes).items()?;
        let last = triples.len().checked_sub(1).ok_or_else(no_items)?;

        Ok(sum_triple(triples.get(last).ok_or_else(no_items)?))
    }

    fn every_struct(self, bytes: &[u8]) -> Result<f64, Error> {
        let triples = Triples::new(bytes).items()?;

        Ok(triples
            .iter()
            .map(sum_triple)
            .fold(0.0, |sum, item| sum + item))
    }
}

/// Sums an item of the vector, with the decode error that the hand-written
/// `Tafla` returns from its own
fn sum_table(item: &generated::Item<'_>) -> Result<f64, DecodeError> {
    let pair = item.pair()?;

    Ok(sum_item(item.value()?, pair.first()?, pair.second()?))
}

fn sum_triple(triple: Triple) -> f64 {
    sum_item(triple.value, triple.first, triple.second)
}

/// A workload's item, as the vector's generated fields hold it
impl From<&Item> for ItemFields {
    fn from(item: &Item) -> Self {
        Self {
            value: item.value,
            pair: PairFields {
                first: item.first,
                second: item.second,
            },
        }
    }
}

/// A workload's item, as the array's generated struct holds it
impl From<&Item> for Triple {
    fn from(item: &Item) -> Self {
        Self {
            value: item.value,
            first: item.first,
            second: item.second,
        }
    }
}

const fn item_of_triple(triple: Triple) -> Item {
    Item {
        value: triple.value,
        first: triple.first,
        second: triple.second,
    }
}
