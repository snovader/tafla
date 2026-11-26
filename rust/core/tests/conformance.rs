//! Reads and rebuilds the tables of tafla's conformance suite, which the
//! TypeScript runtime writes, and its cases of presence, which lie apart from
//! the others in `presence/`. See `conformance/README.md` at the root of the
//! repository for the format of their descriptions in `cases.json`.

mod common;

use std::{fs, path::Path};

use common::{
    cases::table_of,
    description::Table,
    read::{ReadError, read_root_field, read_table},
};
use serde_json::Value;

const CONFORMANCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance");

/// The cases of absent fields and of tables that a later revision of their
/// format reads, whose descriptions have fields of `"type": "absent"` and
/// tables with a count of `missing` fields
const PRESENCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/presence");

struct ValidCase {
    name: String,
    bytes: Vec<u8>,
    table: Table,
}

/// The `cases.json` of a suite's directory
fn suite(directory: &str) -> Value {
    let json = fs::read_to_string(Path::new(directory).join("cases.json")).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn read_file(directory: &str, case: &Value) -> Vec<u8> {
    fs::read(Path::new(directory).join(case["file"].as_str().unwrap())).unwrap()
}

fn valid_cases(directory: &str, suite: &Value) -> Vec<ValidCase> {
    let cases = suite["valid"].as_array().unwrap();

    cases
        .iter()
        .map(|case| ValidCase {
            name: case["name"].as_str().unwrap().to_owned(),
            bytes: read_file(directory, case),
            table: table_of(&case["table"]),
        })
        .collect()
}

/// Returns the description of the valid case that an equivalent or invalid
/// case names as its `case`
fn table_of_case<'a>(valid: &'a [ValidCase], case: &Value) -> &'a Table {
    let name = case["case"].as_str().unwrap();
    let valid = valid.iter().find(|valid| valid.name == name);
    let valid = valid.unwrap_or_else(|| panic!("There's no valid case {name}"));

    &valid.table
}

/// Reads every valid table of the suite in `directory`, which must read as
/// its description, with its count of missing fields when it has one
fn reads_every_valid_table(directory: &str) {
    for case in valid_cases(directory, &suite(directory)) {
        assert_eq!(
            read_table(&case.bytes, &case.table),
            Ok(case.table),
            "{}",
            case.name
        );
    }
}

fn rebuilds_every_valid_table_byte_for_byte(directory: &str) {
    for case in valid_cases(directory, &suite(directory)) {
        assert_eq!(case.table.to_bytes(), Ok(case.bytes), "{}", case.name);
    }
}

/// Reads the root fields before each invalid case's `field`, which must hold
/// their described values, and then `field`, which must fail with a decode
/// error
fn reads_invalid_tables_up_to_the_field_that_fails(directory: &str) {
    let suite = suite(directory);
    let valid = valid_cases(directory, &suite);

    for case in suite["invalid"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let field = usize::try_from(case["field"].as_u64().unwrap()).unwrap();
        let table = table_of_case(&valid, case);
        let bytes = read_file(directory, case);

        for (index, expected) in table.fields[..field].iter().enumerate() {
            let read = read_root_field(&bytes, table.layout, index, expected);
            assert_eq!(read.as_ref(), Ok(expected), "{name}/{index}");
        }

        let read = read_root_field(&bytes, table.layout, field, &table.fields[field]);
        assert!(
            matches!(read, Err(ReadError::Decode(_))),
            "{name}/{field}: {read:?}"
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "reads files, which Miri isolates")]
fn test_reads_every_valid_table() {
    reads_every_valid_table(CONFORMANCE);
}

#[test]
#[cfg_attr(miri, ignore = "reads files, which Miri isolates")]
fn test_rebuilds_every_valid_table_byte_for_byte() {
    rebuilds_every_valid_table_byte_for_byte(CONFORMANCE);
}

/// Reads every equivalent table, whose bytes differ from its valid case's,
/// as the valid case's description, header included
#[test]
#[cfg_attr(miri, ignore = "reads files, which Miri isolates")]
fn test_reads_equivalent_tables_as_their_valid_case() {
    let suite = suite(CONFORMANCE);
    let valid = valid_cases(CONFORMANCE, &suite);

    for case in suite["equivalent"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let table = table_of_case(&valid, case);

        assert_eq!(
            read_table(&read_file(CONFORMANCE, case), table).as_ref(),
            Ok(table),
            "{name}"
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "reads files, which Miri isolates")]
fn test_reads_invalid_tables_up_to_the_field_that_fails() {
    reads_invalid_tables_up_to_the_field_that_fails(CONFORMANCE);
}

/// Absent fields must have the offset 0, and a reader of a later revision
/// must find as many fields missing as the description counts
#[test]
#[cfg_attr(miri, ignore = "reads files, which Miri isolates")]
fn test_reads_every_valid_table_of_presence() {
    reads_every_valid_table(PRESENCE);
}

#[test]
#[cfg_attr(miri, ignore = "reads files, which Miri isolates")]
fn test_rebuilds_every_valid_table_of_presence_byte_for_byte() {
    rebuilds_every_valid_table_byte_for_byte(PRESENCE);
}

#[test]
#[cfg_attr(miri, ignore = "reads files, which Miri isolates")]
fn test_reads_invalid_tables_of_presence_up_to_the_field_that_fails() {
    reads_invalid_tables_up_to_the_field_that_fails(PRESENCE);
}
