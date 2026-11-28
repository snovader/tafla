//! Reads mutated tables as every valid table of the conformance suite
//! describes them, following their offsets into nested tables, vectors,
//! arrays, strings and structs, as the conformance test reads them.
//!
//! Its seed corpus is the suite's tables, `conformance/valid/*.bin`,
//! `conformance/equivalent/*.bin` and `conformance/invalid/*.bin`, so that
//! the fuzzer mutates tables that the descriptions read deep into. Reads may
//! fail, but only with the decode error, finding a vector's item by index
//! must find what iterating finds, and reading every string of a vector of
//! strings must read what reading each string reads.

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

/// The descriptions of the suite's valid tables
static TABLES: LazyLock<Vec<Table>> = LazyLock::new(|| {
    let suite: Value = serde_json::from_str(CASES).expect("cases.json is JSON");
    let cases = suite["valid"]
        .as_array()
        .expect("cases.json lists valid cases");

    cases
        .iter()
        .map(|case| cases::table_of(&case["table"]))
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
            // than described
            Err(ReadError::Size { .. } | ReadError::Items { .. }) => {}
            Err(ReadError::Get { index }) => {
                panic!("A vector's item {index} read differently by index than by iterating")
            }
            Err(ReadError::Strings) => {
                panic!("A vector of strings read differently whole than a string at a time")
            }
        }
    }
});
