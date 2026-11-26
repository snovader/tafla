//! Reads mutated tables as every valid table of the conformance suite
//! describes them, its cases of presence too, following their offsets into
//! nested tables, vectors, arrays, strings and structs, and counting the
//! fields that a later revision would find missing, as the conformance test
//! reads them.
//!
//! Its seed corpus is the suite's tables, `conformance/valid/*.bin`,
//! `conformance/equivalent/*.bin`, `conformance/invalid/*.bin` and those of
//! `conformance/presence/`, so that the fuzzer mutates tables that the
//! descriptions read deep into. Reads may fail, but only with the decode
//! error, and finding a vector's item by index must find what iterating
//! finds.

#![no_main]

// The conformance test's descriptions and reader, rather than a second model
// of tables
#[allow(dead_code)]
#[path = "../../core/tests/common/cases.rs"]
mod cases;
#[allow(dead_code)]
#[path = "../../core/tests/common/description.rs"]
mod description;
#[allow(dead_code)]
#[path = "../../core/tests/common/read.rs"]
mod read;

use std::sync::LazyLock;

use description::Table;
use libfuzzer_sys::fuzz_target;
use read::{ReadError, read_table};
use serde_json::Value;

const CASES: &str = include_str!("../../../conformance/cases.json");

/// The cases of absent fields and of later revisions, which lie apart
const PRESENCE_CASES: &str = include_str!("../../../conformance/presence/cases.json");

/// The descriptions of the suites' valid tables
static TABLES: LazyLock<Vec<Table>> = LazyLock::new(|| {
    let valid_tables = |json: &str| {
        let suite: Value = serde_json::from_str(json).expect("cases.json is JSON");
        let cases = suite["valid"]
            .as_array()
            .expect("cases.json lists valid cases");

        cases
            .iter()
            .map(|case| cases::table_of(&case["table"]))
            .collect::<Vec<_>>()
    };

    [CASES, PRESENCE_CASES]
        .into_iter()
        .flat_map(valid_tables)
        .collect()
});

fuzz_target!(|bytes: &[u8]| {
    for table in TABLES.iter() {
        match read_table(bytes, table) {
            // The bytes hold values of the described types where it reads
            // them, which may differ from the description's
            Ok(read) => assert_eq!(read.fields.len(), table.fields.len()),
            Err(ReadError::Decode(error)) => assert_ne!(error.to_string(), ""),
            // The bytes read, but with a size or a number of items other
            // than described, or hold a field described as absent
            Err(ReadError::Size { .. } | ReadError::Items { .. } | ReadError::Present { .. }) => {}
            Err(ReadError::Get { index }) => {
                panic!("A vector's item {index} read differently by index than by iterating")
            }
        }
    }
});
