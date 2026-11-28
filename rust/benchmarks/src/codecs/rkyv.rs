use std::borrow::Cow;

use rkyv::api::high::to_bytes_in;
use rkyv::rancor;
use rkyv::util::AlignedVec;

use crate::codec::{Codec, DocumentWorkload, Error, Workload};
use crate::string_workloads::{ArchivedStrings, Strings, StringsShape};
use crate::workloads::{
    ArchivedDocument, ArchivedList, ArchivedRecord, ArchivedSample, Document, DocumentShape, Item,
    List, ListShape, Record, RecordShape, Sample, SampleShape, Shape, StructsShape, sum_column,
};

/// rkyv, which archives the workloads' own types through their derives.
///
/// As its documentation shows, an archive is written into an `AlignedVec`,
/// with `to_bytes_in()`, which takes one, here with the shape's capacity, as
/// `to_bytes()` writes into a new one, and read in place with `access()`,
/// which validates all of it with bytecheck first, as untrusted bytes need.
///
/// Archived numbers are little-endian wrappers, whose `to_native()` reads
/// them. Strings are read in place, and columns are copied into `Vec`s
/// through `to_native()`, since rkyv has no slices of native numbers.
///
/// rkyv archives a `Vec` of a struct inline, as an array of structs, so the
/// vector of nested tables and the array of structs are the same archive.
///
/// The strings comparison's table archives as an array of 16 strings, whose
/// UTF-8 bytecheck checks with simdutf8 in `access()`.
#[derive(Clone, Copy, Debug)]
pub struct Rkyv;

impl Codec for Rkyv {
    const NAME: &'static str = "rkyv";
}

impl Workload<RecordShape> for Rkyv {
    type Bytes = AlignedVec;

    fn serialize(self, input: &Record<'_>) -> Result<AlignedVec, Error> {
        let bytes = AlignedVec::with_capacity(RecordShape::capacity(input));
        Ok(to_bytes_in::<_, rancor::Error>(input, bytes)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Record<'_>, Error> {
        let record = rkyv::access::<ArchivedRecord, rancor::Error>(bytes)?;

        Ok(Record {
            x: record.x.to_native(),
            y: record.y.to_native(),
            id: record.id.to_native(),
            name: Cow::Borrowed(record.name.as_str()),
        })
    }
}

impl Workload<DocumentShape> for Rkyv {
    type Bytes = AlignedVec;

    fn serialize(self, input: &Document<'_>) -> Result<AlignedVec, Error> {
        let bytes = AlignedVec::with_capacity(DocumentShape::capacity(input));
        Ok(to_bytes_in::<_, rancor::Error>(input, bytes)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Document<'_>, Error> {
        let document = rkyv::access::<ArchivedDocument, rancor::Error>(bytes)?;

        Ok(Document {
            x: document.x.to_native(),
            y: document.y.to_native(),
            name: Cow::Borrowed(document.name.as_str()),
            base: document.base.to_native(),
            keys: document.keys.iter().map(|key| key.to_native()).collect(),
            a: document.a.iter().map(|value| value.to_native()).collect(),
            b: document.b.iter().map(|value| value.to_native()).collect(),
        })
    }
}

impl DocumentWorkload for Rkyv {
    fn sum_a(self, bytes: &[u8]) -> Result<f64, Error> {
        let document = rkyv::access::<ArchivedDocument, rancor::Error>(bytes)?;
        Ok(sum_column(document.a.iter().map(|value| value.to_native())))
    }
}

impl Workload<ListShape> for Rkyv {
    type Bytes = AlignedVec;

    fn serialize(self, input: &List) -> Result<AlignedVec, Error> {
        let bytes = AlignedVec::with_capacity(ListShape::capacity(input));
        Ok(to_bytes_in::<_, rancor::Error>(input, bytes)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<List, Error> {
        read_list(bytes)
    }
}

impl Workload<StructsShape> for Rkyv {
    type Bytes = AlignedVec;

    fn serialize(self, input: &List) -> Result<AlignedVec, Error> {
        let bytes = AlignedVec::with_capacity(StructsShape::capacity(input));
        Ok(to_bytes_in::<_, rancor::Error>(input, bytes)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<List, Error> {
        read_list(bytes)
    }
}

impl Workload<SampleShape> for Rkyv {
    type Bytes = AlignedVec;

    fn serialize(self, input: &Sample) -> Result<AlignedVec, Error> {
        let bytes = AlignedVec::with_capacity(SampleShape::capacity(input));
        Ok(to_bytes_in::<_, rancor::Error>(input, bytes)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Sample, Error> {
        let sample = rkyv::access::<ArchivedSample, rancor::Error>(bytes)?;

        Ok(Sample {
            key: sample.key.to_native(),
            a: sample.a.to_native(),
            b: sample.b.to_native(),
        })
    }
}

impl Workload<StringsShape> for Rkyv {
    type Bytes = AlignedVec;

    fn serialize(self, input: &Strings<'_>) -> Result<AlignedVec, Error> {
        let bytes = AlignedVec::with_capacity(StringsShape::capacity(input));
        Ok(to_bytes_in::<_, rancor::Error>(input, bytes)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Strings<'_>, Error> {
        let strings = rkyv::access::<ArchivedStrings, rancor::Error>(bytes)?;

        Ok(Strings {
            items: strings
                .items
                .each_ref()
                .map(|string| Cow::Borrowed(string.as_str())),
        })
    }
}

fn read_list(bytes: &[u8]) -> Result<List, Error> {
    let list = rkyv::access::<ArchivedList, rancor::Error>(bytes)?;

    let items = list
        .items
        .iter()
        .map(|item| Item {
            value: item.value.to_native(),
            first: item.first.to_native(),
            second: item.second.to_native(),
        })
        .collect();

    Ok(List { items })
}
