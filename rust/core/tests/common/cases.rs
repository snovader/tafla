//! Parses the descriptions of tables in the conformance suite's `cases.json`.
//! See `conformance/README.md` at the root of the repository.

use std::{fmt::Debug, str::FromStr};

use serde_json::Value;
use tafla::{Alignment, OffsetType};

use super::description::{Field, Layout, Nested, ScalarType, Source, Table};

/// Returns the table that a case's `table` describes
pub fn table_of(value: &Value) -> Table {
    let layout = &value["layout"];
    let offset_type = match layout["offsetType"].as_str() {
        Some("uint8") => OffsetType::Uint8,
        Some("uint16") => OffsetType::Uint16,
        Some("uint32") => OffsetType::Uint32,
        other => panic!("{other:?} isn't an offset type"),
    };

    Table {
        layout: Layout {
            sized: layout["sized"].as_bool().unwrap(),
            versioned: layout["versioned"].as_bool().unwrap(),
            offset_type,
        },
        version: value["version"]
            .as_u64()
            .map(|version| u32::try_from(version).unwrap()),
        fields: value["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(field_of)
            .collect(),
    }
}

/// Returns a field from the JSON that holds 64-bit integers as strings and
/// the floats that JSON has no number for by name
fn field_of(field: &Value) -> Field {
    let value = &field["value"];
    let type_name = field["type"].as_str().unwrap();

    if let Some(scalar) = scalar_type_of(type_name) {
        return scalar_of(scalar, value);
    }

    match type_name {
        "bool" => Field::Bool(value.as_bool().unwrap()),
        "int8[]" => Field::I8Array(elements(value, integer)),
        "uint8[]" => Field::U8Array(elements(value, integer)),
        "int16[]" => Field::I16Array(elements(value, integer)),
        "uint16[]" => Field::U16Array(elements(value, integer)),
        "int32[]" => Field::I32Array(elements(value, integer)),
        "uint32[]" => Field::U32Array(elements(value, integer)),
        "int64[]" => Field::I64Array(elements(value, parse)),
        "uint64[]" => Field::U64Array(elements(value, parse)),
        "float32[]" => Field::F32Array(elements(value, float32)),
        "float64[]" => Field::F64Array(elements(value, float64)),
        "string" => Field::String(value.as_str().unwrap().to_owned()),
        "struct" => Field::Struct(elements(&field["fields"], field_of)),
        "struct[]" => {
            let types = elements(&field["fields"], |name| {
                let name = name.as_str().unwrap();
                scalar_type_of(name).unwrap_or_else(|| panic!("{name} isn't a scalar type"))
            });
            let items = elements(&field["items"], |values| struct_of(&types, values));

            Field::StructArray { types, items }
        }
        "table" => Field::Table {
            nested: nested_of(field),
            alignment: field["alignment"].as_u64().map(alignment_of),
        },
        "vector" => Field::Vector(
            field["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(nested_of)
                .collect(),
        ),
        "string[]" => Field::StringVector(elements(value, |string| {
            string.as_str().unwrap().to_owned()
        })),
        other => panic!("Unknown type {other}"),
    }
}

/// Returns the scalar type that a description names, or `None` for any
/// other type
fn scalar_type_of(name: &str) -> Option<ScalarType> {
    Some(match name {
        "int8" => ScalarType::I8,
        "uint8" => ScalarType::U8,
        "int16" => ScalarType::I16,
        "uint16" => ScalarType::U16,
        "int32" => ScalarType::I32,
        "uint32" => ScalarType::U32,
        "int64" => ScalarType::I64,
        "uint64" => ScalarType::U64,
        "float32" => ScalarType::F32,
        "float64" => ScalarType::F64,
        _ => return None,
    })
}

fn scalar_of(scalar: ScalarType, value: &Value) -> Field {
    match scalar {
        ScalarType::I8 => Field::I8(integer(value)),
        ScalarType::U8 => Field::U8(integer(value)),
        ScalarType::I16 => Field::I16(integer(value)),
        ScalarType::U16 => Field::U16(integer(value)),
        ScalarType::I32 => Field::I32(integer(value)),
        ScalarType::U32 => Field::U32(integer(value)),
        ScalarType::I64 => Field::I64(parse(value)),
        ScalarType::U64 => Field::U64(parse(value)),
        ScalarType::F32 => Field::F32(float32(value)),
        ScalarType::F64 => Field::F64(float64(value)),
    }
}

/// Returns the scalars of a struct of `types` from the values of an item of
/// an array of structs
fn struct_of(types: &[ScalarType], values: &Value) -> Vec<Field> {
    let values = values.as_array().unwrap();
    assert_eq!(
        values.len(),
        types.len(),
        "A struct of {types:?}: {values:?}"
    );

    let scalars = types.iter().zip(values);
    scalars
        .map(|(&scalar, value)| scalar_of(scalar, value))
        .collect()
}

fn nested_of(value: &Value) -> Nested {
    let from = match value["from"].as_str() {
        Some("builder") => Source::Builder,
        Some("bytes") => Source::Bytes,
        other => panic!("Tables can't be nested from {other:?}"),
    };

    Nested {
        from,
        table: table_of(&value["table"]),
    }
}

fn alignment_of(alignment: u64) -> Alignment {
    match alignment {
        1 => Alignment::One,
        2 => Alignment::Two,
        4 => Alignment::Four,
        8 => Alignment::Eight,
        other => panic!("Tables can't be aligned to {other}"),
    }
}

fn elements<T>(value: &Value, element: impl Fn(&Value) -> T) -> Vec<T> {
    value.as_array().unwrap().iter().map(element).collect()
}

fn integer<T: TryFrom<i64, Error: Debug>>(value: &Value) -> T {
    T::try_from(value.as_i64().unwrap()).unwrap()
}

fn parse<T: FromStr<Err: Debug>>(value: &Value) -> T {
    value.as_str().unwrap().parse().unwrap()
}

fn float64(value: &Value) -> f64 {
    match value {
        Value::Number(number) => number.as_f64().unwrap(),
        Value::String(name) => match name.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            other => panic!("Not a number: {other}"),
        },
        other => panic!("Not a number: {other}"),
    }
}

/// Returns a float32 that JSON holds as the f64 that equals it, or by name
fn float32(value: &Value) -> f32 {
    let float = float64(value);

    // NaN converts to an unspecified NaN, rather than the quiet one that the
    // tables hold
    if float.is_nan() {
        return f32::NAN;
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
