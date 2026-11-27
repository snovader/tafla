//! Writes a message's or a struct's values as a table's bytes, through the
//! runtime's `TableSizer` and `TableWriter`, as the generated writers do:
//! every field in its order, nested tables in place in their parent, each
//! padded to its schema's alignment. So the bytes are the ones that the
//! code generated from the schema writes for the same values.

use tafla::{Alignment, BuildError, TableLayout, TableSizer, TableWriter};

use super::offset_type_of;
use super::value::{Elements, Value};
use crate::model::{Definition, FieldType, Kind, Layout, Schema};

/// Writes the message or struct at `root` in the schema from its value,
/// which is of its type
///
/// # Errors
///
/// The runtime's error of a table that can't be written, such as one too
/// long for its offset type.
pub fn write(schema: &Schema, root: usize, value: &Value) -> Result<Vec<u8>, BuildError> {
    let definition = schema.definition(root);

    match (definition.kind, value) {
        (Kind::Struct(_), Value::Struct(bytes)) => Ok(bytes.clone()),
        (Kind::Message(layout), Value::Table(values)) => {
            let mut sizer = sizer(definition.fields.len(), layout)?;
            size_fields(schema, definition, values, &mut sizer);

            let mut writer = sizer.writer()?;
            write_fields(schema, definition, values, &mut writer);
            writer.into_bytes()
        }
        _ => unreachable!("a root is a message of fields or a struct of bytes"),
    }
}

/// A sizer of a message of `fields` fields in its schema's layout
fn sizer(fields: usize, layout: Layout) -> Result<TableSizer, BuildError> {
    let unversioned = TableLayout::DEFAULT.with_offset_type(offset_type_of(layout.offset_type));
    let unversioned = if layout.sized {
        unversioned.sized()
    } else {
        unversioned
    };

    match layout.version {
        Some(version) => TableSizer::new(fields, unversioned.with_version(version)),
        None => TableSizer::new(fields, unversioned),
    }
}

/// The alignment that a parent pads a nested table or an array of structs to
fn alignment_of(bytes: u8) -> Alignment {
    match bytes {
        1 => Alignment::One,
        2 => Alignment::Two,
        4 => Alignment::Four,
        8 => Alignment::Eight,
        _ => unreachable!("an alignment is a scalar's size"),
    }
}

fn size_fields(schema: &Schema, definition: &Definition, values: &[Value], sizer: &mut TableSizer) {
    for (field, value) in definition.fields.iter().zip(values) {
        size_field(schema, field.field_type, value, sizer);
    }
}

fn size_table(schema: &Schema, index: usize, values: &[Value], sizer: &mut TableSizer) {
    let definition = schema.definition(index);

    let parent = sizer.start_table(definition.fields.len());
    size_fields(schema, definition, values, sizer);
    sizer.end_table(parent);
}

fn size_field(schema: &Schema, field_type: FieldType, value: &Value, sizer: &mut TableSizer) {
    match (field_type, value) {
        (FieldType::Scalar(scalar), Value::Scalar(_)) => {
            sizer.add_struct(usize::from(scalar.size()));
        }
        (FieldType::Array(_), Value::Array(elements)) => size_array(elements, sizer),
        (FieldType::String, Value::String(string)) => sizer.add_str(string),
        (FieldType::Struct(_), Value::Struct(bytes)) => sizer.add_struct(bytes.len()),
        (FieldType::StructArray(index), Value::StructArray(bytes)) => {
            let layout = schema.definition(index).struct_layout();
            let count = bytes.len() / layout.size;
            sizer.add_struct_array(count, layout.size, alignment_of(layout.alignment));
        }
        (FieldType::Table(index), Value::Table(values)) => {
            sizer.add_table(alignment_of(schema.table_alignment(index)));
            size_table(schema, index, values, sizer);
        }
        (FieldType::Vector(index), Value::Vector(items)) => {
            sizer.add_vector(items.len());
            for values in items {
                size_table(schema, index, values, sizer);
            }
        }
        _ => unreachable!("a value is of its field's type"),
    }
}

fn size_array(elements: &Elements, sizer: &mut TableSizer) {
    match elements {
        Elements::Int8(elements) => sizer.add_i8_array(elements.len()),
        Elements::Uint8(elements) => sizer.add_u8_array(elements.len()),
        Elements::Int16(elements) => sizer.add_i16_array(elements.len()),
        Elements::Uint16(elements) => sizer.add_u16_array(elements.len()),
        Elements::Int32(elements) => sizer.add_i32_array(elements.len()),
        Elements::Uint32(elements) => sizer.add_u32_array(elements.len()),
        Elements::Int64(elements) => sizer.add_i64_array(elements.len()),
        Elements::Uint64(elements) => sizer.add_u64_array(elements.len()),
        Elements::Float32(elements) => sizer.add_f32_array(elements.len()),
        Elements::Float64(elements) => sizer.add_f64_array(elements.len()),
    }
}

fn write_fields(
    schema: &Schema,
    definition: &Definition,
    values: &[Value],
    writer: &mut TableWriter,
) {
    for (index, (field, value)) in definition.fields.iter().zip(values).enumerate() {
        write_field(schema, index, field.field_type, value, writer);
    }
}

fn write_table(schema: &Schema, index: usize, values: &[Value], writer: &mut TableWriter) {
    let definition = schema.definition(index);

    let parent = writer.start_table(definition.fields.len());
    write_fields(schema, definition, values, writer);
    writer.end_table(parent);
}

fn write_field(
    schema: &Schema,
    index: usize,
    field_type: FieldType,
    value: &Value,
    writer: &mut TableWriter,
) {
    match (field_type, value) {
        (FieldType::Scalar(scalar), Value::Scalar(value)) => {
            let mut bytes = [0; 8];
            value.write_le(&mut bytes);
            writer.set_struct(index, &bytes[..usize::from(scalar.size())]);
        }
        (FieldType::Array(_), Value::Array(elements)) => write_array(index, elements, writer),
        (FieldType::String, Value::String(string)) => writer.set_str(index, string),
        (FieldType::Struct(_), Value::Struct(bytes)) => writer.set_struct(index, bytes),
        (FieldType::StructArray(item), Value::StructArray(bytes)) => {
            let layout = schema.definition(item).struct_layout();
            writer.set_struct_array(index, bytes, layout.size, alignment_of(layout.alignment));
        }
        (FieldType::Table(table), Value::Table(values)) => {
            writer.set_table(index, alignment_of(schema.table_alignment(table)));
            write_table(schema, table, values, writer);
        }
        (FieldType::Vector(table), Value::Vector(items)) => {
            let vector = writer.set_vector(index, items.len());
            for (item, values) in items.iter().enumerate() {
                write_table(schema, table, values, writer);
                writer.set_item_end(vector, item);
            }
        }
        _ => unreachable!("a value is of its field's type"),
    }
}

fn write_array(index: usize, elements: &Elements, writer: &mut TableWriter) {
    match elements {
        Elements::Int8(elements) => writer.set_i8_array(index, elements),
        Elements::Uint8(elements) => writer.set_u8_array(index, elements),
        Elements::Int16(elements) => writer.set_i16_array(index, elements),
        Elements::Uint16(elements) => writer.set_u16_array(index, elements),
        Elements::Int32(elements) => writer.set_i32_array(index, elements),
        Elements::Uint32(elements) => writer.set_u32_array(index, elements),
        Elements::Int64(elements) => writer.set_i64_array(index, elements),
        Elements::Uint64(elements) => writer.set_u64_array(index, elements),
        Elements::Float32(elements) => writer.set_f32_array(index, elements),
        Elements::Float64(elements) => writer.set_f64_array(index, elements),
    }
}
