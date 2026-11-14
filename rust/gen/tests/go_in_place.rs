//! Go reads an array of structs in place only where its Go struct lies in
//! memory as its bytes do on every platform: each scalar at a multiple of its
//! size, which is its alignment but for 64-bit scalars on 32-bit platforms,
//! each struct within it lying so too, at a multiple of its alignment, and
//! its size a multiple of its alignment, with no bool. The example schemas'
//! structs lie at offsets of 4 bytes or less, so these cover the others.

use tafla_gen::{Options, Target, compile};

const SCHEMA: &str = "\
message Arrays {
    aligned: [Aligned];
    wide_after_narrow: [WideAfterNarrow];
    padded: [Padded];
    flag: [Flag];
    nested: [Nested];
    odd_within: [OddWithin];
    padded_within: [PaddedWithin];
    single_bytes: [SingleBytes];
}

/// At 0, 8, 12, 14 and 15
struct Aligned {
    a: float64;
    b: uint32;
    c: uint16;
    d: int8;
    e: uint8;
}

/// A uint64 at 4, where 64-bit platforms would pad it to 8
struct WideAfterNarrow {
    a: uint32;
    b: uint64;
}

/// 6 bytes aligned to 4, which Go would pad to 8
struct Padded {
    a: uint32;
    b: uint16;
}

struct Flag {
    a: uint8;
    b: bool;
}

/// A struct of uint16s at 2, a multiple of its alignment
struct Nested {
    a: uint16;
    pair: Pair;
}

struct Pair {
    x: uint16;
    y: uint16;
}

/// Odd, aligned to 2, at 1
struct OddWithin {
    a: uint8;
    odd: Odd;
}

struct Odd {
    a: uint8;
    b: uint16;
}

/// Its scalars lie at 0, 4 and 6, each a multiple of its size, and its 8
/// bytes are a multiple of its alignment, but Go pads the Padded within it
/// to 8
struct PaddedWithin {
    padded: Padded;
    c: uint16;
}

/// Single bytes, aligned to 1
struct SingleBytes {
    a: int8;
    b: uint8;
    c: uint8;
}
";

/// The statement that the generated `<Struct>Array.AsSlice()` returns
fn as_slice_returns(go: &str, structure: &str) -> String {
    let declaration = format!("func (v {structure}Array) AsSlice() ([]{structure}, bool) {{\n");
    let start = go.find(&declaration).expect("AsSlice() is declared") + declaration.len();
    let end = start + go[start..].find('\n').unwrap();

    go[start..end].trim().to_owned()
}

#[test]
fn go_reads_an_array_in_place_only_where_its_struct_lies_as_its_bytes() {
    let target = Target::from_name("go").unwrap();
    let files = compile("in_place.tafla", SCHEMA, target, &Options::default()).unwrap();
    let go: String = files.into_iter().map(|file| file.contents).collect();

    for structure in ["Aligned", "Nested", "SingleBytes"] {
        assert_eq!(
            as_slice_returns(&go, structure),
            format!("return tafla.StructsInPlace[{structure}](v.bytes)"),
            "{structure}"
        );
    }
    for structure in [
        "WideAfterNarrow",
        "Padded",
        "Flag",
        "OddWithin",
        "PaddedWithin",
    ] {
        assert_eq!(
            as_slice_returns(&go, structure),
            "return nil, false",
            "{structure}"
        );
    }
}
