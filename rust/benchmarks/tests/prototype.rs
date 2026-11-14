//! Checks that the prototype `DirectTableBuilder` writes the tafla codec's
//! bytes of every workload, in each way that its bytes grow, with segments
//! and without, from `to_bytes()` and `into_bytes()`

#![cfg(feature = "prototype")]

use tafla::prototype::{Buffer, InlineBuffer, NO_SEGMENTS, VecBuffer};
use tafla_benchmarks::codec::{Error, Workload};
use tafla_benchmarks::codecs::Tafla;
use tafla_benchmarks::prototype;
use tafla_benchmarks::workloads::{
    self, DOCUMENT_ROWS, DocumentShape, LIST_ITEMS, ListShape, RECORD, RecordShape, StructsShape,
};

/// Checks every workload with a builder of bytes `B` and `N` segments
/// within it, from `to_bytes()` and `into_bytes()`
fn check<B: Buffer, const N: usize>() -> Result<(), Error> {
    check_taken::<B, N, false>()?;
    check_taken::<B, N, true>()
}

fn check_taken<B: Buffer, const N: usize, const INTO: bool>() -> Result<(), Error> {
    let expected = Workload::<RecordShape>::serialize(Tafla, &RECORD)?;
    assert_eq!(prototype::record::<B, N, INTO>(&RECORD)?, expected);

    for rows in DOCUMENT_ROWS {
        let document = workloads::document(rows);
        let expected = Workload::<DocumentShape>::serialize(Tafla, &document)?;
        assert_eq!(prototype::document::<B, N, INTO>(&document)?, expected);
    }

    let list = workloads::list(LIST_ITEMS);
    let expected = Workload::<ListShape>::serialize(Tafla, &list)?;
    assert_eq!(prototype::list::<B, N, INTO>(&list)?, expected);

    let expected = Workload::<StructsShape>::serialize(Tafla, &list)?;
    assert_eq!(prototype::structs::<B, N, INTO>(&list)?, expected);

    Ok(())
}

#[test]
fn direct_builders_write_the_codec_bytes_of_every_workload() -> Result<(), Error> {
    check::<VecBuffer, 4>()?;
    check::<VecBuffer<0>, 4>()?;
    check::<VecBuffer<256, false>, 4>()?;
    check::<VecBuffer<512>, 0>()?;
    check::<InlineBuffer, 4>()?;
    check::<VecBuffer<256, true, NO_SEGMENTS>, 4>()?;
    check::<InlineBuffer<NO_SEGMENTS>, 4>()?;
    // Every run a segment
    check::<VecBuffer<256, true, 0>, 0>()
}
