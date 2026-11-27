// The generated code keeps flatc's style, which flatc rather than this crate
// would change: it names a table's buffer `_tab` and the builder `_fbb`,
// writes types' names in full rather than `Self` or elided lifetimes, and
// derives `PartialEq` without `Eq` for tables. A struct's code declares
// lifetimes that it doesn't use, implements `Default` by hand, and copies its
// scalars through raw pointers made with `as` casts.
#[allow(
    clippy::borrow_as_ptr,
    clippy::derivable_impls,
    clippy::derive_partial_eq_without_eq,
    clippy::elidable_lifetime_names,
    clippy::extra_unused_lifetimes,
    clippy::ptr_as_ptr,
    clippy::ref_as_ptr,
    clippy::use_self,
    clippy::used_underscore_binding
)]
mod generated;

use std::borrow::Cow;

use flatbuffers::{FlatBufferBuilder, Vector};

use crate::codec::{Codec, DocumentWorkload, Error, Workload};
use crate::string_workloads::{COUNT, Strings, StringsShape};
use crate::workloads::{
    Document, DocumentShape, Item, List, ListShape, Record, RecordShape, Sample, SampleShape,
    StructsShape, sum_column,
};

/// flatc's generated code, used as its documentation shows.
///
/// Messages are written with a builder of the default size and `create()`
/// with each table's arguments, and read with `flatbuffers::root()`, which
/// verifies them, as `root_as_*()` does. Vectors are written
/// from slices with `create_vector()`, which writes them element by element,
/// and a vector of structs from a `Vec` of flatc's structs, which took 16%
/// less time than `create_vector_from_iter()` from the items.
///
/// A message is the builder's buffer from where the builder finished it,
/// without a copy, as the TypeScript comparison takes `asUint8Array()`'s
/// view. Columns are copied into `Vec`s through their `Vector`'s iterator,
/// since the crate has no slices of them.
///
/// Fields that a table leaves out read as `None`, which is an error here,
/// since every workload has them.
///
/// The strings comparison's table is a vector of strings, written with
/// `create_string()`. `flatbuffers::root()` verifies that each string lies
/// within the bytes and ends in its terminator, and that it's UTF-8.
#[derive(Clone, Copy, Debug)]
pub struct FlatBuffers;

/// A finished message in its builder's buffer, which the builder fills from
/// its end
pub struct Finished {
    buffer: Vec<u8>,
    head: usize,
}

impl AsRef<[u8]> for Finished {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[self.head..]
    }
}

impl Codec for FlatBuffers {
    const NAME: &'static str = "flatbuffers";
}

impl Workload<RecordShape> for FlatBuffers {
    type Bytes = Finished;

    fn serialize(self, input: &Record<'_>) -> Result<Finished, Error> {
        let mut builder = FlatBufferBuilder::new();
        let name = builder.create_string(&input.name);

        let args = generated::RecordArgs {
            x: input.x,
            y: input.y,
            id: input.id,
            name: Some(name),
        };
        let record = generated::Record::create(&mut builder, &args);
        builder.finish(record, None);

        Ok(collapse(builder))
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Record<'_>, Error> {
        let record = flatbuffers::root::<generated::Record>(bytes)?;

        Ok(Record {
            x: record.x(),
            y: record.y(),
            id: record.id(),
            name: Cow::Borrowed(record.name().ok_or("The record has no name")?),
        })
    }
}

impl Workload<DocumentShape> for FlatBuffers {
    type Bytes = Finished;

    fn serialize(self, input: &Document<'_>) -> Result<Finished, Error> {
        let mut builder = FlatBufferBuilder::new();
        let keys = builder.create_vector(&input.keys);
        let a = builder.create_vector(&input.a);
        let b = builder.create_vector(&input.b);

        let columns_args = generated::ColumnsArgs {
            base: input.base,
            keys: Some(keys),
            a: Some(a),
            b: Some(b),
        };
        let columns = generated::Columns::create(&mut builder, &columns_args);
        let name = builder.create_string(&input.name);

        let args = generated::DocumentArgs {
            x: input.x,
            y: input.y,
            name: Some(name),
            columns: Some(columns),
        };
        let document = generated::Document::create(&mut builder, &args);
        builder.finish(document, None);

        Ok(collapse(builder))
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Document<'_>, Error> {
        let document = flatbuffers::root::<generated::Document>(bytes)?;
        let columns = document.columns().ok_or("The document has no columns")?;

        Ok(Document {
            x: document.x(),
            y: document.y(),
            name: Cow::Borrowed(document.name().ok_or("The document has no name")?),
            base: columns.base(),
            keys: copy_column(columns.keys().ok_or("The document has no keys")?),
            a: copy_column(columns.a().ok_or("The document has no column a")?),
            b: copy_column(columns.b().ok_or("The document has no column b")?),
        })
    }
}

impl DocumentWorkload for FlatBuffers {
    fn sum_a(self, bytes: &[u8]) -> Result<f64, Error> {
        let document = flatbuffers::root::<generated::Document>(bytes)?;
        let columns = document.columns().ok_or("The document has no columns")?;
        let a = columns.a().ok_or("The document has no column a")?;

        Ok(sum_column(a.iter()))
    }
}

impl Workload<ListShape> for FlatBuffers {
    type Bytes = Finished;

    fn serialize(self, input: &List) -> Result<Finished, Error> {
        let mut builder = FlatBufferBuilder::new();

        let offsets: Vec<_> = input
            .items
            .iter()
            .map(|item| create_item(&mut builder, item))
            .collect();
        let items = builder.create_vector(&offsets);

        let args = generated::ListArgs { items: Some(items) };
        let list = generated::List::create(&mut builder, &args);
        builder.finish(list, None);

        Ok(collapse(builder))
    }

    fn deserialize(self, bytes: &[u8]) -> Result<List, Error> {
        let list = flatbuffers::root::<generated::List>(bytes)?;
        let vector = list.items().ok_or("The list has no items")?;

        let mut items = Vec::with_capacity(vector.len());
        for item in vector {
            let pair = item.pair().ok_or("An item has no pair")?;

            items.push(Item {
                value: item.value(),
                first: pair.first(),
                second: pair.second(),
            });
        }

        Ok(List { items })
    }
}

impl Workload<StructsShape> for FlatBuffers {
    type Bytes = Finished;

    fn serialize(self, input: &List) -> Result<Finished, Error> {
        let mut builder = FlatBufferBuilder::new();
        let structs: Vec<_> = input.items.iter().map(triple).collect();
        let items = builder.create_vector(&structs);

        let args = generated::TriplesArgs { items: Some(items) };
        let triples = generated::Triples::create(&mut builder, &args);
        builder.finish(triples, None);

        Ok(collapse(builder))
    }

    fn deserialize(self, bytes: &[u8]) -> Result<List, Error> {
        let triples = flatbuffers::root::<generated::Triples>(bytes)?;
        let vector = triples.items().ok_or("The array has no items")?;

        let items = vector
            .iter()
            .map(|triple| Item {
                value: triple.value(),
                first: triple.first(),
                second: triple.second(),
            })
            .collect();

        Ok(List { items })
    }
}

impl Workload<SampleShape> for FlatBuffers {
    type Bytes = Finished;

    fn serialize(self, input: &Sample) -> Result<Finished, Error> {
        let mut builder = FlatBufferBuilder::new();

        let args = generated::SampleArgs {
            key: input.key,
            a: input.a,
            b: input.b,
        };
        let sample = generated::Sample::create(&mut builder, &args);
        builder.finish(sample, None);

        Ok(collapse(builder))
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Sample, Error> {
        let sample = flatbuffers::root::<generated::Sample>(bytes)?;

        Ok(Sample {
            key: sample.key(),
            a: sample.a(),
            b: sample.b(),
        })
    }
}

impl Workload<StringsShape> for FlatBuffers {
    type Bytes = Finished;

    fn serialize(self, input: &Strings<'_>) -> Result<Finished, Error> {
        let mut builder = FlatBufferBuilder::new();
        let strings = input
            .items
            .each_ref()
            .map(|string| builder.create_string(string));
        let items = builder.create_vector(&strings);

        let args = generated::StringsArgs { items: Some(items) };
        let table = generated::Strings::create(&mut builder, &args);
        builder.finish(table, None);

        Ok(collapse(builder))
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Strings<'_>, Error> {
        let table = flatbuffers::root::<generated::Strings>(bytes)?;
        let strings = table.items().ok_or("The table has no strings")?;
        if strings.len() != COUNT {
            return Err(format!("The table has {} strings, not {COUNT}", strings.len()).into());
        }

        Ok(Strings {
            items: std::array::from_fn(|index| Cow::Borrowed(strings.get(index))),
        })
    }
}

/// Takes the finished message from its builder, without copying it
fn collapse(builder: FlatBufferBuilder<'_>) -> Finished {
    let (buffer, head) = builder.collapse();
    Finished { buffer, head }
}

fn copy_column<'a, T>(vector: Vector<'a, T>) -> Cow<'a, [T]>
where
    T: flatbuffers::Follow<'a, Inner = T> + Clone + 'a,
{
    Cow::Owned(vector.iter().collect())
}

fn create_item<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    item: &Item,
) -> flatbuffers::WIPOffset<generated::Item<'a>> {
    let pair_args = generated::PairArgs {
        first: item.first,
        second: item.second,
    };
    let pair = generated::Pair::create(builder, &pair_args);

    let args = generated::ItemArgs {
        value: item.value,
        pair: Some(pair),
    };
    generated::Item::create(builder, &args)
}

fn triple(item: &Item) -> generated::Triple {
    generated::Triple::new(item.value, item.first, item.second)
}
