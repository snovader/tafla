//! The code generated from `schemas/shapes.tafla`, how the tests read its
//! tables back with every getter, and the values of the fields that it
//! writes, as `crate::tables` describes them.

// Each test crate uses a part of the generated code
#[allow(dead_code)]
#[path = "../golden/shapes/rust/shapes.rs"]
pub mod generated;

use generated::{
    Columns, ColumnsFields, Document, DocumentFields, Item, ItemFields, ItemList, ItemListFields,
    Pair, PairFields, Point, PointFields, Sample,
};

use crate::tables::{Fields, Value, table, to_vec, vector};

// Reading every field with the generated getters, including those after one
// that fails

pub fn read_point(point: &Point) -> Fields {
    vec![
        point.x().map(Value::f64),
        point.y().map(Value::f64),
        point.id().map(Value::I64),
        point.name().map(Value::string),
    ]
}

pub fn read_document(document: &Document) -> Fields {
    vec![
        document.x().map(Value::f64),
        document.y().map(Value::f64),
        document.name().map(Value::string),
        document
            .columns()
            .and_then(|columns| table(read_columns(&columns))),
    ]
}

fn read_columns(columns: &Columns) -> Fields {
    vec![
        columns.base().map(Value::I64),
        columns.keys().map(to_vec).map(Value::U32Array),
        columns.a().map(Value::f32_array),
        columns.b().map(Value::f32_array),
    ]
}

/// Reads the items in order through the view that the getter returns, or
/// the first error of their bytes, and then each item's fields
pub fn read_item_list(item_list: &ItemList) -> Fields {
    vec![item_list.items().and_then(|view| {
        let items: Vec<Item> = view.iter().collect::<Result<_, _>>()?;
        vector(items.iter().map(read_item))
    })]
}

pub fn read_item(item: &Item) -> Fields {
    vec![
        item.value().map(Value::f32),
        item.pair().and_then(|pair| table(read_pair(&pair))),
    ]
}

fn read_pair(pair: &Pair) -> Fields {
    vec![pair.first().map(Value::f32), pair.second().map(Value::f32)]
}

// The values of the fields, of their types in the schema

pub fn describe_point(fields: &PointFields) -> Vec<Value> {
    vec![
        Value::f64(fields.x),
        Value::f64(fields.y),
        Value::I64(fields.id),
        Value::string(fields.name),
    ]
}

pub fn describe_document(fields: &DocumentFields) -> Vec<Value> {
    vec![
        Value::f64(fields.x),
        Value::f64(fields.y),
        Value::string(fields.name),
        Value::Table(describe_columns(&fields.columns)),
    ]
}

fn describe_columns(fields: &ColumnsFields) -> Vec<Value> {
    vec![
        Value::I64(fields.base),
        Value::U32Array(fields.keys.to_vec()),
        Value::f32_array(fields.a.iter().copied()),
        Value::f32_array(fields.b.iter().copied()),
    ]
}

pub fn describe_item_list(fields: &ItemListFields) -> Vec<Value> {
    vec![Value::Vector(
        fields.items.iter().map(describe_item).collect(),
    )]
}

pub fn describe_item(fields: &ItemFields) -> Vec<Value> {
    vec![
        Value::f32(fields.value),
        Value::Table(describe_pair(&fields.pair)),
    ]
}

fn describe_pair(fields: &PairFields) -> Vec<Value> {
    vec![Value::f32(fields.first), Value::f32(fields.second)]
}

/// The fields of a `Sample`, which is read whole, to compare by their bits
pub fn describe_sample(sample: &Sample) -> Vec<Value> {
    vec![
        Value::U32(sample.key),
        Value::f32(sample.a),
        Value::f32(sample.b),
    ]
}
