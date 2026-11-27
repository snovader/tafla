//! Writes a message's or a struct's values as JSON, as README.md's "Tables
//! as JSON" describes: an object of the fields by their names, in their
//! order, with each scalar as `ScalarValue::push_json()` writes it.

use super::scalar::ScalarValue;
use super::syntax::push_string;
use super::value::Value;
use crate::model::{Definition, FieldType, Schema};

const INDENT: &str = "    ";

/// The JSON of the value of the message or struct at `root`, which ends
/// with a line feed. `ascii` writes every character outside ASCII as an
/// escape.
pub fn print(schema: &Schema, root: usize, value: &Value, ascii: bool) -> String {
    let mut printer = Printer {
        schema,
        ascii,
        json: String::new(),
    };

    let root_type = if schema.definition(root).is_struct() {
        FieldType::Struct(root)
    } else {
        FieldType::Table(root)
    };
    printer.value(root_type, value, 0);
    printer.json.push('\n');
    printer.json
}

struct Printer<'a> {
    schema: &'a Schema,
    ascii: bool,
    json: String,
}

impl Printer<'_> {
    fn value(&mut self, field_type: FieldType, value: &Value, depth: usize) {
        match (field_type, value) {
            (FieldType::Scalar(_), Value::Scalar(value)) => value.push_json(&mut self.json),
            (FieldType::Array(_), Value::Array(elements)) => {
                self.json.push('[');
                let mut separator = "";
                elements.for_each(|element| {
                    self.json.push_str(separator);
                    element.push_json(&mut self.json);
                    separator = ", ";
                });
                self.json.push(']');
            }
            (FieldType::String, Value::String(string)) => {
                push_string(&mut self.json, string, self.ascii);
            }
            (FieldType::Struct(index), Value::Struct(bytes)) => {
                self.structure(self.schema.definition(index), bytes);
            }
            (FieldType::StructArray(index), Value::StructArray(bytes)) => {
                let definition = self.schema.definition(index);
                let structs = bytes.chunks_exact(definition.struct_layout().size);
                self.lines(structs, depth, |printer, bytes| {
                    printer.structure(definition, bytes);
                });
            }
            (FieldType::Table(index), Value::Table(values)) => self.table(index, values, depth),
            (FieldType::Vector(index), Value::Vector(items)) => {
                self.lines(items.iter(), depth, |printer, values| {
                    printer.table(index, values, depth + 1);
                });
            }
            _ => unreachable!("a value is of its field's type"),
        }
    }

    /// A table's fields, each on a line of its own
    fn table(&mut self, index: usize, values: &[Value], depth: usize) {
        let definition = self.schema.definition(index);

        self.json.push('{');
        for (position, (field, value)) in definition.fields.iter().zip(values).enumerate() {
            self.json.push_str(if position == 0 { "\n" } else { ",\n" });
            self.indent(depth + 1);
            push_string(&mut self.json, &field.name, self.ascii);
            self.json.push_str(": ");
            self.value(field.field_type, value, depth + 1);
        }
        self.json.push('\n');
        self.indent(depth);
        self.json.push('}');
    }

    /// A struct's fields on one line, read from its bytes, with the structs
    /// in it as objects of their own
    fn structure(&mut self, definition: &Definition, bytes: &[u8]) {
        let mut offset = 0;

        self.json.push_str("{ ");
        for (position, field) in definition.fields.iter().enumerate() {
            if position > 0 {
                self.json.push_str(", ");
            }
            push_string(&mut self.json, &field.name, self.ascii);
            self.json.push_str(": ");

            match field.field_type {
                FieldType::Scalar(scalar) => {
                    ScalarValue::from_le_bytes(scalar, &bytes[offset..]).push_json(&mut self.json);
                    offset += usize::from(scalar.size());
                }
                FieldType::Struct(index) => {
                    let inner = self.schema.definition(index);
                    self.structure(inner, &bytes[offset..]);
                    offset += inner.struct_layout().size;
                }
                _ => unreachable!("structs hold scalars and structs only"),
            }
        }
        self.json.push_str(" }");
    }

    /// An array of structs or tables, each on lines of its own
    fn lines<T>(
        &mut self,
        items: impl Iterator<Item = T>,
        depth: usize,
        mut print: impl FnMut(&mut Self, T),
    ) {
        self.json.push('[');
        let mut is_empty = true;
        for item in items {
            self.json.push_str(if is_empty { "\n" } else { ",\n" });
            self.indent(depth + 1);
            print(self, item);
            is_empty = false;
        }
        if !is_empty {
            self.json.push('\n');
            self.indent(depth);
        }
        self.json.push(']');
    }

    fn indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.json.push_str(INDENT);
        }
    }
}
