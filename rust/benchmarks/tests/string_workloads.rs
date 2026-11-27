//! Checks that the strings comparison's generator makes the TypeScript
//! strings suite's strings, and how each codec reads them back

use std::borrow::Cow;

use tafla_benchmarks::codec::{Error, Workload};
use tafla_benchmarks::for_each_codec;
use tafla_benchmarks::string_workloads::{COUNT, Strings, StringsShape, string_workloads};

/// A shape that the TypeScript suite's generator printed: its name, the
/// UTF-8 length of each string, and a hash of their bytes
struct PrintedShape {
    name: &'static str,
    bytes: usize,
    hash: u32,
}

/// Printed by the TypeScript suite's `shape()`, with the seeds of
/// `string_workloads()`. The hashes of the shapes that the C++ suite has too
/// are the same as its test's.
const TYPESCRIPT_SHAPES: [PrintedShape; 22] = [
    printed("empty", 0, 0),
    printed("8 chars of ASCII", 8, 2_912_978_722),
    printed("8 chars, one of 2 bytes", 9, 1_437_720_515),
    printed("16 chars of ASCII", 16, 841_955_873),
    printed("16 chars, two of 2 bytes", 18, 629_651_658),
    printed("15 chars, one of 4 bytes", 18, 210_269_755),
    printed("8 chars of 3 bytes", 24, 1_717_679_536),
    printed("64 chars of ASCII", 64, 1_948_179_883),
    printed("32 chars of 2 bytes", 64, 1_560_204_926),
    printed("16 chars of 4 bytes", 64, 536_250_614),
    printed("22 chars of 3 bytes", 66, 1_249_301_547),
    printed("32 chars of 3 bytes", 96, 2_365_967_191),
    printed("256 chars of ASCII", 256, 1_342_101_721),
    printed("255 chars, first of 2 bytes", 256, 4_039_442_052),
    printed("255 chars, last of 2 bytes", 256, 156_920_576),
    printed("256 chars of 3 bytes", 768, 726_450_835),
    printed("1,024 chars of ASCII", 1024, 2_674_459_570),
    printed("1,023 chars, first of 2 bytes", 1024, 3_792_960_320),
    printed("1,023 chars, last of 2 bytes", 1024, 3_999_142_749),
    printed("512 chars of 2 bytes", 1024, 1_124_334_465),
    printed("256 chars of 4 bytes", 1024, 4_122_669_824),
    printed("342 chars of 3 bytes", 1026, 3_488_746_411),
];

const fn printed(name: &'static str, bytes: usize, hash: u32) -> PrintedShape {
    PrintedShape { name, bytes, hash }
}

#[test]
fn generates_the_typescript_strings() {
    let workloads = string_workloads();
    assert_eq!(workloads.len(), TYPESCRIPT_SHAPES.len());

    for (workload, expected) in workloads.iter().zip(&TYPESCRIPT_SHAPES) {
        assert_eq!(workload.name, expected.name);
        assert!(
            workload
                .strings
                .iter()
                .all(|string| string.len() == expected.bytes),
            "{}",
            workload.name
        );
        assert_eq!(
            hash_strings(&workload.strings),
            expected.hash,
            "{}",
            workload.name
        );
    }
    assert_eq!(workloads[1].strings[0], "2I8eEZQc");
}

/// Hashes the strings' UTF-8, byte by byte, as the TypeScript script that
/// printed these did with 32-bit integers
fn hash_strings(strings: &[String]) -> u32 {
    strings
        .iter()
        .flat_map(|string| string.bytes())
        .fold(0, |hash, byte| {
            hash.wrapping_mul(31).wrapping_add(u32::from(byte))
        })
}

/// tafla, tafla-gen, flatbuffers, rmp-serde and rkyv read the strings in
/// place, borrowed from the bytes, and prost reads copies that it owns
#[test]
fn only_prost_copies_the_strings() -> Result<(), Error> {
    let workloads = string_workloads();
    let workload = &workloads[11];
    assert_eq!(workload.name, "32 chars of 3 bytes");

    for_each_codec!(|codec| assert_reads_in_place(codec, &workload.input())?);
    Ok(())
}

fn assert_reads_in_place<C: Workload<StringsShape>>(
    codec: C,
    input: &Strings<'_>,
) -> Result<(), Error> {
    let bytes = codec.serialize(input)?;
    let bytes = bytes.as_ref();
    let output = codec.deserialize(bytes)?;

    let in_place = output.items.iter().all(|item| match item {
        Cow::Borrowed(string) => bytes.as_ptr_range().contains(&string.as_ptr()),
        Cow::Owned(_) => false,
    });
    assert_eq!(in_place, C::NAME != "prost", "{}", C::NAME);
    assert_eq!(output.items.len(), COUNT);

    Ok(())
}
