//! A schema's description: the description of each example schema in
//! `schemas/` matches `tests/descriptions/<schema>.json`, which
//! `TAFLA_GEN_UPDATE=1` rewrites, and its sizes and offsets are those of the
//! tables that the generated code writes, as `conformance/generated` holds
//! them.

mod common;

use std::path::Path;

use common::{
    REPOSITORY, as_usize, assert_no_differences, check_described_offsets, compare_file, described,
    example_schemas, fields_of, files_with_extension, update,
};
use serde_json::{Value, json};
use tafla_gen::{DESCRIPTION_VERSION, describe};

const DESCRIPTIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/descriptions");

/// The definitions of a schema's description
fn definitions(file_name: &str, source: &str) -> Vec<Value> {
    let description: Value = serde_json::from_str(&describe(file_name, source).unwrap()).unwrap();
    assert_eq!(description["description_version"], DESCRIPTION_VERSION);
    assert_eq!(description["schemas"][0]["path"], file_name);
    description["schemas"][0]["definitions"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn every_example_schema_has_its_expected_description() {
    let mut differences = Vec::new();

    for schema in example_schemas() {
        let description = describe(&schema.file_name, &schema.source).unwrap();
        let expected = Path::new(DESCRIPTIONS).join(format!("{}.json", schema.name));
        differences.extend(compare_file(&expected, &description));
    }

    assert_no_differences(&differences);
}

#[test]
fn every_expected_description_has_a_schema() {
    let schemas: Vec<String> = example_schemas()
        .into_iter()
        .map(|schema| schema.name)
        .collect();
    let stale: Vec<_> = files_with_extension(Path::new(DESCRIPTIONS), "json")
        .into_iter()
        .filter(|(name, _)| !schemas.contains(name))
        .map(|(_, path)| path)
        .collect();

    if update() {
        for path in &stale {
            std::fs::remove_file(path).unwrap();
        }
        return;
    }
    assert!(stale.is_empty(), "descriptions without a schema: {stale:?}");
}

#[test]
fn the_offsets_are_those_of_the_tables_that_the_generated_code_writes() {
    let generated = Path::new(REPOSITORY).join("conformance/generated");
    let cases: Value =
        serde_json::from_str(&std::fs::read_to_string(generated.join("shapes.json")).unwrap())
            .unwrap();
    let schema = cases["schema"].as_str().unwrap();
    let source = std::fs::read_to_string(Path::new(REPOSITORY).join(schema)).unwrap();
    let definitions = definitions(schema, &source);

    let mut checked = 0;
    for case in cases["cases"].as_array().unwrap() {
        let name = case["type"].as_str().unwrap();
        let bytes = std::fs::read(generated.join(case["file"].as_str().unwrap())).unwrap();
        let definition = described(&definitions, name);

        if definition["kind"] == "struct" {
            check_struct(definition, &bytes, &case["fields"]);
        } else {
            check_described_offsets(&definitions, name, &bytes, 0)
                .unwrap_or_else(|error| panic!("{}: {error}", case["name"]));
            if let Some(size) = definition["size"].as_u64() {
                assert_eq!(bytes.len() as u64, size, "{}", case["name"]);
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 7);
}

/// Checks that a struct is as long as its size, and that each scalar's
/// value lies at the offset that the description gives it
fn check_struct(definition: &Value, bytes: &[u8], values: &Value) {
    assert_eq!(bytes.len(), as_usize(&definition["size"]));

    for scalar in definition["scalars"].as_array().unwrap() {
        let path: Vec<&str> = scalar["path"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect();
        let value = path.iter().fold(values, |value, name| &value[name]);
        let offset = as_usize(&scalar["offset"]);

        match scalar["scalar"].as_str().unwrap() {
            "uint32" => {
                let read = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                assert_eq!(u64::from(read), value.as_u64().unwrap(), "{path:?}");
            }
            "float32" => {
                let read = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                // The cases spell the floats that JSON can't as strings
                let expected: f32 = match value.as_str() {
                    Some(text) => text.parse().unwrap(),
                    #[allow(clippy::cast_possible_truncation)]
                    None => value.as_f64().unwrap() as f32,
                };
                if expected.is_nan() {
                    assert!(read.is_nan(), "{path:?}");
                } else {
                    assert_eq!(read.to_bits(), expected.to_bits(), "{path:?}");
                }
            }
            other => panic!("no case holds a struct of {other}"),
        }
    }
}

#[test]
fn a_table_of_fixed_fields_has_its_size_and_positions() {
    let source = "message Frame (sized, version: 2) {\n\
                  \x20   flag: bool;\n\
                  \x20   origin: Vec3;\n\
                  \x20   pair: Pair;\n\
                  \x20   values: [float64];\n\
                  \x20   after: uint8;\n\
                  }\n\
                  struct Vec3 { x: float32; y: float32; z: float32; }\n\
                  table Pair { first: int16; second: Vec3; }";
    let definitions = definitions("frame.tafla", source);

    let frame = described(&definitions, "Frame");
    assert_eq!(frame["layout"]["header_size"], 8);
    assert_eq!(frame["size"], Value::Null);
    // After the header and 5 offsets of 4 bytes, a bool, a Vec3 and a Pair,
    // and then the array, whose count is padded so that its first element
    // starts at a multiple of 8, at 72, and after which nothing is fixed
    let offsets: Vec<&Value> = fields_of(frame)
        .iter()
        .map(|field| &field["offset"])
        .collect();
    assert_eq!(
        offsets,
        [&json!(28), &json!(29), &json!(41), &json!(68), &Value::Null]
    );
    let sizes: Vec<&Value> = fields_of(frame)
        .iter()
        .map(|field| &field["size"])
        .collect();
    assert_eq!(
        sizes,
        [&json!(1), &json!(12), &json!(22), &Value::Null, &json!(1)]
    );

    let pair = described(&definitions, "Pair");
    assert_eq!(pair["layout"]["offset_type"], "uint32");
    assert_eq!((&pair["size"], &pair["alignment"]), (&22.into(), &1.into()));
}

#[test]
fn a_struct_has_the_offsets_of_its_scalars() {
    let source = "struct Segment { start: Vec2; end: Vec2; id: uint16; }\n\
                  struct Vec2 { x: float64; y: float64; }";
    let definitions = definitions("segment.tafla", source);

    let segment = described(&definitions, "Segment");
    assert_eq!(
        (&segment["size"], &segment["alignment"]),
        (&34.into(), &8.into())
    );
    let scalars: Vec<(String, u64)> = segment["scalars"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scalar| {
            let path: Vec<&str> = scalar["path"]
                .as_array()
                .unwrap()
                .iter()
                .map(|name| name.as_str().unwrap())
                .collect();
            (path.join("."), scalar["offset"].as_u64().unwrap())
        })
        .collect();
    assert_eq!(
        scalars,
        [
            ("start.x".to_owned(), 0),
            ("start.y".to_owned(), 8),
            ("end.x".to_owned(), 16),
            ("end.y".to_owned(), 24),
            ("id".to_owned(), 32)
        ]
    );
}

#[test]
fn strings_are_escaped_as_json_strings() {
    let source = "/// \"quoted\" \\ back\tslash 🦀\nmessage Point { x: float64; }";
    let path = "a\u{2028}b\n\u{1}.tafla";

    let definitions = definitions(path, source);

    assert_eq!(
        described(&definitions, "Point")["docs"][0],
        "\"quoted\" \\ back\tslash 🦀"
    );
    // A line separator ends a line of JavaScript before ES2019
    let description = describe(path, source).unwrap();
    assert!(
        description.contains(r#""path": "a\u2028b\n\u0001.tafla""#),
        "{description}"
    );
}

#[test]
fn a_schema_with_errors_has_no_description() {
    let errors = describe("point.tafla", "message Point { x: flaot64; }").unwrap_err();

    assert_eq!(errors[0].message, "unknown type `flaot64`");
}
