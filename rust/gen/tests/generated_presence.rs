//! The code generated from `typescript/core/test/presence.tafla`: optional
//! fields, which its writers leave out with an offset of 0, and fields that a
//! later version of a message or a later revision of a table appends, which
//! its readers return as `None`, or a scalar's default, from the tables that
//! don't hold them. It writes the tables that `TableBuilder` writes by hand,
//! reads back what it writes, reads the tables of the first version with the
//! reader of the second, rejects other versions, and reads corrupted and
//! arbitrary bytes as values, `None`, defaults or decode errors without
//! panicking. `typescript/core/test/generated/presence.test.ts` checks the
//! generated TypeScript and JavaScript alike.

// The tests nest points and entries only through records, so their
// alignments and the views' lengths go unused, as does the helper of
// arrays of the struct, which no array holds
#[allow(dead_code)]
#[path = "golden/presence/rust/presence.rs"]
mod presence;
mod tables;

use presence::{
    Entry, EntryFields, EntryV1, EntryV1Fields, Extent, Point, PointFields, PointV1, PointV1Fields,
    Record, RecordFields, RecordV1, RecordV1Fields, Sample, SampleFields, SampleV1, SampleV1Fields,
};
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use tables::{
    Bits, CASES, MAX_LENGTH, arbitrary_bytes, float32s, float64s, integers, overwrites, placements,
    strings,
};
use tafla::{BuildError, DecodeError, TableBuilder, TableLayout};

/// The layout of a `Record`, from its attributes in the schema, and the
/// position of its version, after its length
const RECORD: TableLayout<true> = TableLayout::DEFAULT
    .sized()
    .with_version(Record::FORMAT_VERSION);
const VERSION_AT: usize = 4;

// The defaults that the schema gives the scalars that may be missing
const FLAG_DEFAULT: bool = true;
const TOTAL_DEFAULT: i64 = -1;
const Z_DEFAULT: f64 = -0.5;
const WEIGHT_DEFAULT: f32 = 0.1;
const COUNT_DEFAULT: u16 = 7;

/// The values of a `Record`'s fields, which `RecordFields` borrows
#[derive(Clone, Debug)]
struct OwnedRecord {
    id: u32,
    name: Option<String>,
    values: Option<Vec<f64>>,
    origin: Option<OwnedPoint>,
    flag: Option<bool>,
    entries: Option<Vec<OwnedEntry>>,
    total: Option<i64>,
    label: String,
    extent: Option<Extent>,
}

/// The values of a `Point`'s fields, which `PointFields` borrows
#[derive(Clone, Debug)]
struct OwnedPoint {
    x: f64,
    y: f64,
    z: f64,
    label: Option<String>,
}

impl OwnedPoint {
    fn fields(&self) -> PointFields<'_> {
        PointFields {
            x: self.x,
            y: self.y,
            z: self.z,
            label: self.label.as_deref(),
        }
    }
}

/// The values of an `Entry`'s fields, whose references convert into the
/// fields that a vector of entries writes, as its items of any type do
#[derive(Clone, Debug)]
struct OwnedEntry {
    key: u16,
    note: Option<String>,
    weight: f32,
}

impl<'a> From<&'a OwnedEntry> for EntryFields<'a> {
    fn from(entry: &'a OwnedEntry) -> Self {
        EntryFields {
            key: entry.key,
            note: entry.note.as_deref(),
            weight: entry.weight,
        }
    }
}

/// The first revision of an entry, without its weight
impl<'a> From<&'a OwnedEntry> for EntryV1Fields<'a> {
    fn from(entry: &'a OwnedEntry) -> Self {
        EntryV1Fields {
            key: entry.key,
            note: entry.note.as_deref(),
        }
    }
}

impl OwnedRecord {
    fn fields(&self) -> RecordFields<'_, OwnedEntry> {
        RecordFields {
            id: self.id,
            name: self.name.as_deref(),
            values: self.values.as_deref(),
            origin: self.origin.as_ref().map(OwnedPoint::fields),
            flag: self.flag,
            entries: self.entries.as_deref(),
            total: self.total,
            label: &self.label,
            extent: self.extent,
        }
    }

    /// The fields of the first version, without those that the second added
    fn first_version(&self) -> RecordV1Fields<'_, OwnedEntry> {
        let origin = self.origin.as_ref();

        RecordV1Fields {
            id: self.id,
            name: self.name.as_deref(),
            values: self.values.as_deref(),
            origin: origin.map(|origin| PointV1Fields {
                x: origin.x,
                y: origin.y,
            }),
            flag: self.flag,
            entries: self.entries.as_deref(),
            total: self.total,
        }
    }

    /// The record as a reader of its version reads it: absent fields `None`,
    /// and each absent scalar its default
    fn expected(&self) -> ReadRecord {
        let origin = self.origin.as_ref();
        let entry = |entry: &OwnedEntry| ReadEntry {
            key: entry.key,
            note: entry.note.clone(),
            weight: Some((Bits(entry.weight), true)),
        };

        ReadRecord {
            origin: origin.map(|origin| ReadPoint {
                x: Bits(origin.x),
                y: Bits(origin.y),
                z: Some((Bits(origin.z), true)),
                label: origin.label.clone(),
            }),
            entries: self
                .entries
                .as_ref()
                .map(|entries| entries.iter().map(entry).collect()),
            label: Some(self.label.clone()),
            extent: self
                .extent
                .map(|extent| (Bits(extent.width), Bits(extent.height))),
            ..self.expected_of_first_version()
        }
    }

    /// The fields of the first version, as a reader of either version reads
    /// them from a table of the first, with the fields that the second
    /// version, or the second revision of a nested table, added `None` or
    /// their defaults
    fn expected_of_first_version(&self) -> ReadRecord {
        let origin = self.origin.as_ref();
        let entry = |entry: &OwnedEntry| ReadEntry {
            key: entry.key,
            note: entry.note.clone(),
            weight: Some((Bits(WEIGHT_DEFAULT), false)),
        };

        ReadRecord {
            id: self.id,
            name: self.name.clone(),
            values: self
                .values
                .as_ref()
                .map(|values| values.iter().copied().map(Bits).collect()),
            origin: origin.map(|origin| ReadPoint {
                x: Bits(origin.x),
                y: Bits(origin.y),
                z: Some((Bits(Z_DEFAULT), false)),
                label: None,
            }),
            flag: self.flag.map_or((FLAG_DEFAULT, false), |flag| (flag, true)),
            entries: self
                .entries
                .as_ref()
                .map(|entries| entries.iter().map(entry).collect()),
            total: self
                .total
                .map_or((TOTAL_DEFAULT, false), |total| (total, true)),
            label: None,
            extent: None,
        }
    }

    /// Writes the record by hand with `TableBuilder`, as the generated code
    /// writes it, setting each field that it leaves out absent
    fn build(&self) -> Result<Vec<u8>, BuildError> {
        let mut builder = TableBuilder::new(9, RECORD)?;
        builder.set_u32(self.id)?;
        match &self.name {
            Some(name) => builder.set_str(name.as_str())?,
            None => builder.set_absent()?,
        }
        match &self.values {
            Some(values) => builder.set_f64_array(values)?,
            None => builder.set_absent()?,
        }
        match &self.origin {
            Some(origin) => builder.set_table_with(4, |point| {
                point.set_f64(origin.x)?;
                point.set_f64(origin.y)?;
                point.set_f64(origin.z)?;
                match &origin.label {
                    Some(label) => point.set_str(label.as_str()),
                    None => point.set_absent(),
                }
            })?,
            None => builder.set_absent()?,
        }
        match self.flag {
            Some(flag) => builder.set_u8(u8::from(flag))?,
            None => builder.set_absent()?,
        }
        match &self.entries {
            Some(entries) => builder.set_vector_with(entries.len(), 3, |index, item| {
                let entry = &entries[index];
                item.set_u16(entry.key)?;
                match &entry.note {
                    Some(note) => item.set_str(note.as_str())?,
                    None => item.set_absent()?,
                }
                item.set_f32(entry.weight)
            })?,
            None => builder.set_absent()?,
        }
        match self.total {
            Some(total) => builder.set_i64(total)?,
            None => builder.set_absent()?,
        }
        builder.set_str(self.label.as_str())?;
        match self.extent {
            Some(extent) => builder.set_struct(extent.to_bytes().to_vec())?,
            None => builder.set_absent()?,
        }

        builder.to_bytes()
    }
}

/// A record as its getters read it, every field of it and of the tables in
/// it: `None` for each field that it doesn't hold but a scalar, which reads
/// as its getter's value and whether `has_<field>()` finds it. A field that
/// a reader doesn't know is `None` too.
#[derive(Debug, PartialEq)]
struct ReadRecord {
    id: u32,
    name: Option<String>,
    values: Option<Vec<Bits<f64>>>,
    origin: Option<ReadPoint>,
    flag: (bool, bool),
    entries: Option<Vec<ReadEntry>>,
    total: (i64, bool),
    label: Option<String>,
    extent: Option<(Bits<f32>, Bits<f32>)>,
}

impl ReadRecord {
    /// The record as the reader of the first version reads it, which knows
    /// none of the fields that the second version, or the second revision
    /// of a nested table, added
    fn of_first_reader(mut self) -> Self {
        if let Some(point) = &mut self.origin {
            point.z = None;
            point.label = None;
        }
        for entry in self.entries.iter_mut().flatten() {
            entry.weight = None;
        }

        self
    }
}

#[derive(Debug, PartialEq)]
struct ReadPoint {
    x: Bits<f64>,
    y: Bits<f64>,
    z: Option<(Bits<f64>, bool)>,
    label: Option<String>,
}

#[derive(Debug, PartialEq)]
struct ReadEntry {
    key: u16,
    note: Option<String>,
    weight: Option<(Bits<f32>, bool)>,
}

/// Reads every field of a record of the second version through its getters,
/// and each item of its vector both from iterating and by index
fn read_record(record: &Record<'_>) -> Result<ReadRecord, DecodeError> {
    let entries = match record.entries()? {
        Some(entries) => {
            let mut read = Vec::with_capacity(entries.len());
            for (index, entry) in entries.iter().enumerate() {
                let entry = read_entry(&entry?)?;
                let by_index = entries.get(index).expect("an item within the count");
                assert_eq!(read_entry(&by_index?)?, entry);
                read.push(entry);
            }
            assert!(entries.get(entries.len()).is_none());
            Some(read)
        }
        None => None,
    };

    Ok(ReadRecord {
        id: record.id()?,
        name: record.name()?.map(str::to_owned),
        values: record
            .values()?
            .map(|values| values.iter().map(Bits).collect()),
        origin: record
            .origin()?
            .map(|point| read_point(&point))
            .transpose()?,
        flag: (record.flag()?, record.has_flag()?),
        entries,
        total: (record.total()?, record.has_total()?),
        label: record.label()?.map(str::to_owned),
        extent: record
            .extent()?
            .map(|extent| (Bits(extent.width), Bits(extent.height))),
    })
}

fn read_point(point: &Point<'_>) -> Result<ReadPoint, DecodeError> {
    Ok(ReadPoint {
        x: Bits(point.x()?),
        y: Bits(point.y()?),
        z: Some((Bits(point.z()?), point.has_z()?)),
        label: point.label()?.map(str::to_owned),
    })
}

fn read_entry(entry: &Entry<'_>) -> Result<ReadEntry, DecodeError> {
    Ok(ReadEntry {
        key: entry.key()?,
        note: entry.note()?.map(str::to_owned),
        weight: Some((Bits(entry.weight()?), entry.has_weight()?)),
    })
}

/// Reads every field of a record of the first version with the reader of
/// the first version, which doesn't know the fields that the second added
fn read_record_v1(record: &RecordV1<'_>) -> Result<ReadRecord, DecodeError> {
    let read_point = |point: PointV1<'_>| {
        Ok::<_, DecodeError>(ReadPoint {
            x: Bits(point.x()?),
            y: Bits(point.y()?),
            z: None,
            label: None,
        })
    };
    let read_entry = |entry: Result<EntryV1<'_>, DecodeError>| {
        let entry = entry?;
        Ok(ReadEntry {
            key: entry.key()?,
            note: entry.note()?.map(str::to_owned),
            weight: None,
        })
    };

    Ok(ReadRecord {
        id: record.id()?,
        name: record.name()?.map(str::to_owned),
        values: record
            .values()?
            .map(|values| values.iter().map(Bits).collect()),
        origin: record.origin()?.map(read_point).transpose()?,
        flag: (record.flag()?, record.has_flag()?),
        entries: record
            .entries()?
            .map(|entries| entries.iter().map(read_entry).collect())
            .transpose()?,
        total: (record.total()?, record.has_total()?),
        label: None,
        extent: None,
    })
}

/// Calls every getter of a record, of its point and of each of its entries,
/// whatever the others return, and returns what each returned
fn every_getter(record: &Record<'_>) -> Vec<Result<String, DecodeError>> {
    let mut reads = vec![
        record.id().map(|id| format!("{id}")),
        record.name().map(|name| format!("{name:?}")),
        record
            .values()
            .map(|values| format!("{:?}", values.map(|values| values.to_vec()))),
        record.flag().map(|flag| format!("{flag}")),
        record.has_flag().map(|has| format!("{has}")),
        record.total().map(|total| format!("{total}")),
        record.has_total().map(|has| format!("{has}")),
        record.label().map(|label| format!("{label:?}")),
        record.extent().map(|extent| format!("{extent:?}")),
    ];

    if let Ok(Some(point)) = record.origin() {
        reads.extend([
            point.x().map(|x| format!("{:?}", Bits(x))),
            point.y().map(|y| format!("{:?}", Bits(y))),
            point.z().map(|z| format!("{:?}", Bits(z))),
            point.has_z().map(|has| format!("{has}")),
            point.label().map(|label| format!("{label:?}")),
        ]);
    }

    if let Ok(Some(entries)) = record.entries() {
        for entry in entries.iter() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    reads.push(Err(error));
                    continue;
                }
            };
            reads.extend([
                entry.key().map(|key| format!("{key}")),
                entry.note().map(|note| format!("{note:?}")),
                entry.weight().map(|weight| format!("{:?}", Bits(weight))),
                entry.has_weight().map(|has| format!("{has}")),
            ]);
        }
    }

    reads
}

/// Values of the type, or `None` as often as not
fn optional<S: Strategy>(values: S) -> impl Strategy<Value = Option<S::Value>> {
    option::weighted(0.5, values)
}

fn entries() -> impl Strategy<Value = OwnedEntry> {
    (integers(0, u16::MAX), optional(strings()), float32s())
        .prop_map(|(key, note, weight)| OwnedEntry { key, note, weight })
}

fn points() -> impl Strategy<Value = OwnedPoint> {
    (float64s(), float64s(), float64s(), optional(strings()))
        .prop_map(|(x, y, z, label)| OwnedPoint { x, y, z, label })
}

fn records() -> impl Strategy<Value = OwnedRecord> {
    let extents = (float32s(), float32s()).prop_map(|(width, height)| Extent { width, height });

    (
        (
            integers(0, u32::MAX),
            optional(strings()),
            optional(vec(float64s(), 0..=MAX_LENGTH)),
            optional(points()),
            optional(any::<bool>()),
        ),
        (
            optional(vec(entries(), 0..=3)),
            optional(integers(i64::MIN, i64::MAX)),
            strings(),
            optional(extents),
        ),
    )
        .prop_map(
            |((id, name, values, origin, flag), (entries, total, label, extent))| OwnedRecord {
                id,
                name,
                values,
                origin,
                flag,
                entries,
                total,
                label,
                extent,
            },
        )
}

/// A record with only its required fields
fn record_of_required_fields(id: u32, label: &str) -> OwnedRecord {
    OwnedRecord {
        id,
        name: None,
        values: None,
        origin: None,
        flag: None,
        entries: None,
        total: None,
        label: label.to_owned(),
        extent: None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn optional_fields_read_back_as_written_or_as_none_or_their_defaults_wherever_the_bytes_lie(
        record in records(),
        placement in placements(),
    ) {
        let bytes = record.fields().serialize()?;
        let placed = placement.place(&bytes);

        let read = read_record(&Record::new(placed.bytes())?);
        prop_assert_eq!(read, Ok(record.expected()));
    }

    #[test]
    fn absent_fields_are_written_as_table_builder_sets_them_absent(record in records()) {
        prop_assert_eq!(record.fields().serialize(), record.build());
    }

    /// The fields that the second version, and the second revisions of the
    /// point and the entries, added are `None` or their defaults
    #[test]
    fn a_record_of_the_first_version_reads_with_the_fields_of_the_second_missing(
        record in records(),
        placement in placements(),
    ) {
        let bytes = record.first_version().serialize()?;
        let placed = placement.place(&bytes);

        prop_assert_eq!(Record::version_of(placed.bytes()), Ok(1));
        let read = read_record(&Record::new(placed.bytes())?);
        prop_assert_eq!(read, Ok(record.expected_of_first_version()));
        let read = read_record_v1(&RecordV1::new(placed.bytes())?);
        prop_assert_eq!(read, Ok(record.expected_of_first_version().of_first_reader()));
    }

    #[test]
    fn a_record_of_a_version_before_the_first_or_after_its_own_is_rejected(
        record in records(),
        version in prop_oneof![0..=3_u32, any::<u32>()],
    ) {
        let mut bytes = record.fields().serialize()?;
        bytes[VERSION_AT..VERSION_AT + 4].copy_from_slice(&version.to_le_bytes());

        prop_assert_eq!(Record::version_of(&bytes), Ok(version));
        let read = Record::new(&bytes);
        if (Record::FIRST_VERSION..=Record::FORMAT_VERSION).contains(&version) {
            prop_assert!(read.is_ok());
        } else {
            let error = DecodeError::VersionOutsideRange { version, first: 1, last: 2 };
            prop_assert_eq!(read.map(|_| ()), Err(error));
        }
    }

    /// A message without a version finds the field that its second revision
    /// appended from its first field's offset, and a reader of the first
    /// revision reads its fields from a table of the second
    #[test]
    fn a_sample_of_the_first_revision_reads_the_default_for_the_field_that_the_second_appended(
        key in integers(0, u32::MAX),
        count in integers(0, u16::MAX),
    ) {
        let first = SampleV1Fields { key }.serialize()?;
        let second = SampleFields { key, count }.serialize()?;

        let sample = Sample::new(&first);
        prop_assert_eq!(sample.key(), Ok(key));
        prop_assert_eq!(sample.count(), Ok(COUNT_DEFAULT));
        prop_assert_eq!(sample.has_count(), Ok(false));

        let sample = Sample::new(&second);
        prop_assert_eq!(sample.count(), Ok(count));
        prop_assert_eq!(sample.has_count(), Ok(true));
        prop_assert_eq!(SampleV1::new(&second).key(), Ok(key));
    }

    /// Overwritten and cut bytes read as values, `None`, defaults or decode
    /// errors, alike wherever they lie
    #[test]
    fn corrupted_records_read_as_values_none_defaults_or_decode_errors(
        record in records(),
        overwrites in vec(overwrites(), 1..=3),
        cut in any::<prop::sample::Index>(),
        placements in (placements(), placements()),
    ) {
        let mut bytes = record.fields().serialize()?;
        for overwrite in overwrites {
            bytes = overwrite.apply(&bytes);
        }
        bytes.truncate(cut.index(bytes.len() + 1));

        let (first, second) = (placements.0.place(&bytes), placements.1.place(&bytes));
        let read = |bytes| Record::new(bytes).map(|record| every_getter(&record));
        prop_assert_eq!(read(first.bytes()), read(second.bytes()));
    }

    /// Arbitrary bytes, with a version that the reader reads in their slot,
    /// read as a record, and without a version as a sample, as values,
    /// `None`, defaults or decode errors
    #[test]
    fn arbitrary_bytes_read_as_values_none_defaults_or_decode_errors(
        bytes in arbitrary_bytes(),
        version in 1..=2_u32,
    ) {
        let header = tables::Header::versioned(TableLayout::DEFAULT.sized(), version);
        let versioned = header.write_version(&bytes);
        if let Ok(record) = Record::new(&versioned) {
            every_getter(&record);
        }

        let sample = Sample::new(&bytes);
        let _ = (sample.key(), sample.count(), sample.has_count());
    }
}

#[test]
fn an_absent_field_is_written_as_table_builder_set_absent_writes_it() {
    let written = record_of_required_fields(7, "a").fields().serialize();

    let mut builder = TableBuilder::new(9, RECORD).unwrap();
    builder.set_u32(7).unwrap();
    for _ in 1..7 {
        builder.set_absent().unwrap();
    }
    builder.set_str("a").unwrap();
    builder.set_absent().unwrap();

    assert_eq!(written, builder.to_bytes());
}

#[test]
fn a_reader_of_one_version_rejects_the_others() {
    let bytes = record_of_required_fields(1, "")
        .fields()
        .serialize()
        .unwrap();

    let error = DecodeError::UnsupportedVersion {
        version: 2,
        supported: 1,
    };
    assert_eq!(RecordV1::new(&bytes).map(|_| ()), Err(error));
    assert_eq!(RecordV1::version_of(&bytes), Ok(2));
}

/// A field whose offset is 0 reads as absent, and a field that a later
/// revision appended reads as missing when its offset lies in the padding
/// before the table's first value, which is 0
#[test]
fn a_field_of_a_later_revision_is_missing_where_its_offset_reads_as_0() {
    // A Sample of the first revision whose first value lies after 4 bytes of
    // zeros, where the offset of the field of the second revision would be
    let bytes = [8, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0];

    let sample = Sample::new(&bytes);
    assert_eq!(sample.key(), Ok(42));
    assert_eq!(sample.count(), Ok(COUNT_DEFAULT));
    assert_eq!(sample.has_count(), Ok(false));
}

/// A nested table's field that a later revision appended, whose offset is 0
/// from the table's start, reads as missing, as it would in the padding
/// before the table's first value
#[test]
fn a_field_of_a_later_revision_is_missing_from_a_nested_table_where_its_offset_is_0() {
    let mut builder = TableBuilder::new(9, RECORD).unwrap();
    builder.set_u32(1).unwrap();
    builder.set_absent().unwrap();
    builder.set_absent().unwrap();
    builder
        .set_table_with(4, |point| {
            point.set_f64(2.0)?;
            point.set_f64(3.0)?;
            point.set_absent()?;
            point.set_absent()
        })
        .unwrap();
    builder.set_absent().unwrap();
    builder
        .set_vector_with(1, 3, |_, entry| {
            entry.set_u16(4)?;
            entry.set_absent()?;
            entry.set_absent()
        })
        .unwrap();
    builder.set_absent().unwrap();
    builder.set_str("").unwrap();
    builder.set_absent().unwrap();
    let bytes = builder.to_bytes().unwrap();

    let record = Record::new(&bytes).unwrap();
    let point = record.origin().unwrap().unwrap();
    assert_eq!((point.z(), point.has_z()), (Ok(Z_DEFAULT), Ok(false)));
    assert_eq!(point.label(), Ok(None));
    let entry = record.entries().unwrap().unwrap().get(0).unwrap().unwrap();
    assert_eq!(
        (entry.weight(), entry.has_weight()),
        (Ok(WEIGHT_DEFAULT), Ok(false))
    );
}
