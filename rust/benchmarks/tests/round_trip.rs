//! Checks that every codec reads back exactly what it wrote of every workload,
//! so that the benchmarks can't time a broken codec

use tafla_benchmarks::check;
use tafla_benchmarks::codec::{Error, Workload};
use tafla_benchmarks::codecs::{FlatBuffers, LeBytes, Prost, Rkyv, RmpSerde, Tafla, TaflaGen};
use tafla_benchmarks::item_access::{self, ItemBytes};
use tafla_benchmarks::sized_items;
use tafla_benchmarks::string_workloads::{StringsShape, string_workloads};
use tafla_benchmarks::workloads::{
    self, DOCUMENT_ROWS, DocumentShape, LIST_ITEMS, ListShape, RECORD, RecordShape, SAMPLE,
    SampleShape, Shape, StructsShape,
};
use tafla_benchmarks::{for_each_codec, for_each_item_access};

#[test]
fn every_codec_reads_back_the_record() -> Result<(), Error> {
    for_each_codec!(|codec| check::round_trip::<RecordShape, _>(codec, &RECORD)?);
    Ok(())
}

#[test]
fn every_codec_reads_back_the_documents() -> Result<(), Error> {
    for rows in DOCUMENT_ROWS {
        let document = workloads::document(rows);
        for_each_codec!(|codec| check::round_trip::<DocumentShape, _>(codec, &document)?);
    }

    Ok(())
}

#[test]
fn every_codec_sums_a_column_of_the_documents() -> Result<(), Error> {
    for rows in DOCUMENT_ROWS {
        let document = workloads::document(rows);
        for_each_codec!(|codec| check::sum_a(codec, &document)?);
    }

    Ok(())
}

#[test]
fn every_codec_reads_back_the_vector_of_nested_tables() -> Result<(), Error> {
    let list = workloads::list(LIST_ITEMS);
    for_each_codec!(|codec| check::round_trip::<ListShape, _>(codec, &list)?);

    Ok(())
}

#[test]
fn every_codec_reads_back_the_array_of_structs() -> Result<(), Error> {
    let list = workloads::list(LIST_ITEMS);
    for_each_codec!(|codec| check::round_trip::<StructsShape, _>(codec, &list)?);

    Ok(())
}

#[test]
fn every_codec_reads_back_the_fixed_size_record() -> Result<(), Error> {
    for_each_codec!(|codec| check::round_trip::<SampleShape, _>(codec, &SAMPLE)?);
    check::round_trip::<SampleShape, _>(LeBytes, &SAMPLE)
}

/// The codecs that reuse a builder or a buffer write what they write into
/// new bytes, again into the same state, and read it back, for every shape
/// of the reused rows
#[test]
fn every_reusing_codec_writes_the_same_bytes_into_its_state() -> Result<(), Error> {
    let list = workloads::list(LIST_ITEMS);
    macro_rules! each {
        ($($codec:expr),*) => {$(
            check::reused::<RecordShape, _>($codec, &RECORD)?;
            for rows in DOCUMENT_ROWS {
                check::reused::<DocumentShape, _>($codec, &workloads::document(rows))?;
            }
            check::reused::<ListShape, _>($codec, &list)?;
            check::reused::<StructsShape, _>($codec, &list)?;
        )*};
    }
    each!(Tafla, FlatBuffers, Prost, RmpSerde, Rkyv);
    check::reused::<SampleShape, _>(FlatBuffers, &SAMPLE)?;
    check::reused::<SampleShape, _>(Prost, &SAMPLE)?;
    check::reused::<SampleShape, _>(RmpSerde, &SAMPLE)?;
    check::reused::<SampleShape, _>(Rkyv, &SAMPLE)
}

#[test]
fn every_codec_reads_back_every_shape_of_strings() -> Result<(), Error> {
    for workload in string_workloads() {
        let input = workload.input();
        for_each_codec!(|codec| check::round_trip::<StringsShape, _>(codec, &input)?);
    }

    Ok(())
}

#[test]
fn every_access_sums_the_last_item_and_every_item() -> Result<(), Error> {
    let list = workloads::list(LIST_ITEMS);
    let bytes = ItemBytes::write(&list)?;
    for_each_item_access!(|access, _name| item_access::check(access, &list, &bytes)?);

    Ok(())
}

#[test]
fn every_sized_item_reads_back_as_a_table_of_its_own() -> Result<(), Error> {
    let list = workloads::list(LIST_ITEMS);
    let bytes = sized_items::write(&list)?;

    sized_items::check(&list, &bytes)
}

#[test]
fn generated_code_writes_the_builders_tables() -> Result<(), Error> {
    let list = workloads::list(LIST_ITEMS);

    assert_same_bytes::<RecordShape>(&RECORD)?;
    for rows in DOCUMENT_ROWS {
        assert_same_bytes::<DocumentShape>(&workloads::document(rows))?;
    }
    assert_same_bytes::<ListShape>(&list)?;
    assert_same_bytes::<StructsShape>(&list)?;
    assert_same_bytes::<SampleShape>(&SAMPLE)?;
    for workload in string_workloads() {
        assert_same_bytes::<StringsShape>(&workload.input())?;
    }

    Ok(())
}

fn assert_same_bytes<S: Shape>(input: &S::Value<'_>) -> Result<(), Error>
where
    Tafla: Workload<S>,
    TaflaGen: Workload<S>,
{
    let built = Tafla.serialize(input)?;
    let generated = TaflaGen.serialize(input)?;

    assert_eq!(generated.as_ref(), built.as_ref(), "the {}", S::NAME);
    Ok(())
}
