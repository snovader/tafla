use std::borrow::Cow;
use std::fmt;

use serde::de::{self, Deserializer, SeqAccess, Visitor};

use crate::codec::{Codec, DocumentWorkload, Error, Workload};
use crate::string_workloads::{COUNT, Strings, StringsShape};
use crate::workloads::{
    Document, DocumentShape, List, ListShape, Record, RecordShape, Sample, SampleShape,
    StructsShape, sum_column,
};

/// rmp-serde, which packs the workloads' own types through their serde derives.
///
/// The msgpack format is schemaless: a struct packs as a map with its field
/// names, as `to_vec_named()` writes it and msgpackr packs an object, and a
/// document packs as one map. Columns pack as arrays of float32s and uint32s,
/// element by element, since msgpack has no typed arrays. Reading borrows
/// strings from the bytes and copies columns into `Vec`s.
///
/// Structs have a fixed shape, so an array of them packs with `to_vec()`,
/// which packs a struct as an array of its fields in order, without their
/// names: each item, and the table that holds them.
///
/// The strings comparison's table packs as an array of strings, as msgpackr
/// packs a `string[]`. Reading checks each string's UTF-8 with
/// `str::from_utf8()` and borrows it from the bytes.
#[derive(Clone, Copy, Debug)]
pub struct RmpSerde;

impl Codec for RmpSerde {
    const NAME: &'static str = "rmp-serde";
}

impl Workload<RecordShape> for RmpSerde {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &Record<'_>) -> Result<Vec<u8>, Error> {
        Ok(rmp_serde::to_vec_named(input)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Record<'_>, Error> {
        Ok(rmp_serde::from_slice(bytes)?)
    }
}

impl Workload<DocumentShape> for RmpSerde {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &Document<'_>) -> Result<Vec<u8>, Error> {
        Ok(rmp_serde::to_vec_named(input)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Document<'_>, Error> {
        Ok(rmp_serde::from_slice(bytes)?)
    }
}

impl DocumentWorkload for RmpSerde {
    fn sum_a(self, bytes: &[u8]) -> Result<f64, Error> {
        let document: Document<'_> = rmp_serde::from_slice(bytes)?;
        Ok(sum_column(document.a.iter().copied()))
    }
}

impl Workload<ListShape> for RmpSerde {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &List) -> Result<Vec<u8>, Error> {
        Ok(rmp_serde::to_vec_named(input)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<List, Error> {
        Ok(rmp_serde::from_slice(bytes)?)
    }
}

impl Workload<StructsShape> for RmpSerde {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &List) -> Result<Vec<u8>, Error> {
        Ok(rmp_serde::to_vec(input)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<List, Error> {
        Ok(rmp_serde::from_slice(bytes)?)
    }
}

impl Workload<SampleShape> for RmpSerde {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &Sample) -> Result<Vec<u8>, Error> {
        Ok(rmp_serde::to_vec_named(input)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Sample, Error> {
        Ok(rmp_serde::from_slice(bytes)?)
    }
}

impl Workload<StringsShape> for RmpSerde {
    type Bytes = Vec<u8>;

    fn serialize(self, input: &Strings<'_>) -> Result<Vec<u8>, Error> {
        Ok(rmp_serde::to_vec(&input.items)?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Strings<'_>, Error> {
        let mut deserializer = rmp_serde::Deserializer::from_read_ref(bytes);
        let items = deserializer.deserialize_seq(BorrowedStrings)?;

        Ok(Strings { items })
    }
}

/// Reads an array of strings borrowed from the bytes. serde's array of
/// `Cow`s would copy each string, as it borrows only a field marked
/// `#[serde(borrow)]`.
struct BorrowedStrings;

impl<'de> Visitor<'de> for BorrowedStrings {
    type Value = [Cow<'de, str>; COUNT];

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "an array of {COUNT} strings")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut items = [const { Cow::Borrowed("") }; COUNT];
        for (index, item) in items.iter_mut().enumerate() {
            let string = seq.next_element::<&'de str>()?;
            *item = Cow::Borrowed(string.ok_or_else(|| de::Error::invalid_length(index, &self))?);
        }

        if seq.next_element::<de::IgnoredAny>()?.is_some() {
            return Err(de::Error::invalid_length(COUNT + 1, &self));
        }

        Ok(items)
    }
}
