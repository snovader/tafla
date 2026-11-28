//! The code generated from `schemas/shapes.tafla` writes the tables of
//! `conformance/generated`, which the generated TypeScript wrote, byte for
//! byte, and reads back their values. `conformance/generated/README.md`
//! describes the cases in `shapes.json`.

mod shapes;
mod tables;

use std::{fmt::Debug, fs, path::Path, str::FromStr};

use serde_json::Value as Json;
use shapes::generated::{
    ColumnsFields, Document, DocumentFields, ItemFields, ItemList, ItemListFields, PairFields,
    Point, PointFields, Priority, Sample, State, Step, Task, TaskFields,
};
use shapes::{
    describe_document, describe_item_list, describe_point, describe_sample, describe_task,
    read_document, read_item_list, read_point, read_task,
};
use tables::{Fields, expected};
use tafla::{BuildError, DecodeError};

const GENERATED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/generated");

/// The quiet NaNs that the cases' `"NaN"` stands for, which JavaScript
/// writes
const QUIET_NAN_32: u32 = 0x7fc0_0000;
const QUIET_NAN_64: u64 = 0x7ff8_0000_0000_0000;

/// A case, with what the generated code writes from its fields and reads
/// from its bytes
struct Case {
    name: String,
    bytes: Vec<u8>,
    written: Result<Vec<u8>, BuildError>,
    read: Result<Fields, DecodeError>,
    /// The values that the fields should read as
    values: Fields,
}

fn cases() -> Vec<Case> {
    let json = fs::read_to_string(Path::new(GENERATED).join("shapes.json")).unwrap();
    let suite: Json = serde_json::from_str(&json).unwrap();
    assert_eq!(suite["schema"], "schemas/shapes.tafla");

    suite["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(case_of)
        .collect()
}

fn case_of(case: &Json) -> Case {
    let name = case["name"].as_str().unwrap().to_owned();
    let bytes = fs::read(Path::new(GENERATED).join(case["file"].as_str().unwrap())).unwrap();
    let fields = &case["fields"];

    let (written, read, values) = match case["type"].as_str().unwrap() {
        "Point" => point(fields, &bytes),
        "Document" => document(fields, &bytes),
        "ItemList" => item_list(fields, &bytes),
        "Sample" => sample(fields, &bytes),
        "Task" => task(fields, &bytes),
        other => panic!("{name}: shapes.tafla has no {other}"),
    };

    Case {
        name,
        bytes,
        written,
        read,
        values,
    }
}

#[test]
fn the_generated_code_writes_every_case_byte_for_byte() {
    for case in cases() {
        assert_eq!(case.written, Ok(case.bytes), "{}", case.name);
    }
}

#[test]
fn the_generated_code_reads_the_values_of_every_case() {
    for case in cases() {
        assert_eq!(case.read, Ok(case.values), "{}", case.name);
    }
}

// Each type's fields from the JSON, what the generated code writes from them
// and reads from `bytes`, and the values that it should read

type Outcome = (
    Result<Vec<u8>, BuildError>,
    Result<Fields, DecodeError>,
    Fields,
);

fn point(json: &Json, bytes: &[u8]) -> Outcome {
    let name = string(&json["name"]);
    let fields = PointFields {
        x: float64(&json["x"]),
        y: float64(&json["y"]),
        id: parse(&json["id"]),
        name: &name,
    };

    (
        fields.serialize(),
        Ok(read_point(&Point::new(bytes))),
        expected(describe_point(&fields)),
    )
}

fn document(json: &Json, bytes: &[u8]) -> Outcome {
    let name = string(&json["name"]);
    let columns = &json["columns"];
    let keys = elements(&columns["keys"], integer);
    let a = elements(&columns["a"], float32);
    let b = elements(&columns["b"], float32);
    let fields = DocumentFields {
        x: float64(&json["x"]),
        y: float64(&json["y"]),
        name: &name,
        columns: ColumnsFields {
            base: parse(&columns["base"]),
            keys: &keys,
            a: &a,
            b: &b,
        },
    };

    (
        fields.serialize(),
        Document::new(bytes).map(|document| read_document(&document)),
        expected(describe_document(&fields)),
    )
}

fn item_list(json: &Json, bytes: &[u8]) -> Outcome {
    let items = elements(&json["items"], item);
    let fields = ItemListFields { items: &items };

    (
        fields.serialize(),
        Ok(read_item_list(&ItemList::new(bytes))),
        expected(describe_item_list(&fields)),
    )
}

fn item(json: &Json) -> ItemFields {
    let pair = &json["pair"];

    ItemFields {
        value: float32(&json["value"]),
        pair: PairFields {
            first: float32(&pair["first"]),
            second: float32(&pair["second"]),
        },
    }
}

fn sample(json: &Json, bytes: &[u8]) -> Outcome {
    let sample = Sample {
        key: integer(&json["key"]),
        a: float32(&json["a"]),
        b: float32(&json["b"]),
    };

    (
        Ok(sample.to_bytes().to_vec()),
        Sample::read(bytes).map(|read| expected(describe_sample(&read))),
        expected(describe_sample(&sample)),
    )
}

/// A task of the enums' integers in the JSON, whether the schema names them
/// or not
fn task(json: &Json, bytes: &[u8]) -> Outcome {
    let history = elements(&json["history"], integer);
    let name = string(&json["name"]);
    let step = &json["step"];
    let fields = TaskFields {
        priority: Priority(signed(&json["priority"])),
        history: &history,
        step: Step {
            state: State(integer(&step["state"])),
            priority: Priority(signed(&step["priority"])),
            seconds: integer(&step["seconds"]),
        },
        name: &name,
    };

    (
        fields.serialize(),
        Ok(read_task(&Task::new(bytes))),
        expected(describe_task(&fields)),
    )
}

// Values as the JSON holds them: 64-bit integers as decimal strings, floats
// that JSON has no number for by name, and float32s as the float64s that
// equal them

fn string(value: &Json) -> String {
    value.as_str().unwrap().to_owned()
}

fn elements<T>(value: &Json, element: fn(&Json) -> T) -> Vec<T> {
    value.as_array().unwrap().iter().map(element).collect()
}

fn integer<T: TryFrom<u64, Error: Debug>>(value: &Json) -> T {
    T::try_from(value.as_u64().unwrap()).unwrap()
}

fn signed<T: TryFrom<i64, Error: Debug>>(value: &Json) -> T {
    T::try_from(value.as_i64().unwrap()).unwrap()
}

fn parse<T: FromStr<Err: Debug>>(value: &Json) -> T {
    value.as_str().unwrap().parse().unwrap()
}

fn float64(value: &Json) -> f64 {
    match value {
        Json::Number(number) => number.as_f64().unwrap(),
        Json::String(name) => match name.as_str() {
            "NaN" => f64::from_bits(QUIET_NAN_64),
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            other => panic!("Not a number: {other}"),
        },
        other => panic!("Not a number: {other}"),
    }
}

fn float32(value: &Json) -> f32 {
    let float = float64(value);
    if float.is_nan() {
        return f32::from_bits(QUIET_NAN_32);
    }

    #[allow(clippy::cast_possible_truncation)]
    let narrowed = float as f32;
    assert_eq!(
        f64::from(narrowed).to_bits(),
        float.to_bits(),
        "{float} isn't a float32"
    );

    narrowed
}
