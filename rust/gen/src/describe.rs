//! A schema's description: its declarations as JSON, with the layout that
//! the format gives each of them, for tools that aren't code generators.
//! README.md's "Description" is its contract.
//!
//! The sizes and offsets are the checker's and the generators' own, from
//! `model` and `generate::fixed`, so that a description can't disagree with
//! the code generated from the same schema.

use std::fmt::Write;

use crate::generate::fixed::{FixedSizes, NESTED_OFFSET_SIZE, header_size, offset_size};
use crate::model::{Definition, Field, FieldType, Kind, Layout, OffsetType, Scalar, Schema};

/// The version of the description's shape, which changes when a tool written
/// for the one before it would misread a description: a key that's removed
/// or means something else, or a new `kind`. New keys alone don't change it.
pub const DESCRIPTION_VERSION: u64 = 1;

/// The layout of every nested table: no header, and `uint32` offsets
const TABLE_LAYOUT: Layout = Layout {
    sized: false,
    version: None,
    offset_type: OffsetType::Uint32,
};

/// The description of schemas, each with its path, as indented JSON that
/// ends its last line
pub fn describe(schemas: &[(&str, &Schema)]) -> String {
    let schemas = schemas
        .iter()
        .map(|&(path, schema)| {
            Json::Object(vec![
                ("path", Json::string(path)),
                ("definitions", definitions(schema)),
            ])
        })
        .collect();
    let document = Json::Object(vec![
        ("description_version", Json::Number(DESCRIPTION_VERSION)),
        ("schemas", Json::Array(schemas)),
    ]);

    let mut json = String::new();
    document.write(&mut json, 0);
    json.push('\n');
    json
}

fn definitions(schema: &Schema) -> Json {
    // A table longer than `uint32` offsets address can't be written
    let max = usize::try_from(OffsetType::Uint32.max_value()).unwrap_or(usize::MAX);
    let fixed = FixedSizes::new(schema, max);

    let definitions = schema
        .definitions
        .iter()
        .enumerate()
        .map(|(index, definition)| match definition.kind {
            Kind::Message(layout) => table(schema, &fixed, index, "message", layout),
            Kind::Table => table(schema, &fixed, index, "table", TABLE_LAYOUT),
            Kind::Struct(_) => structure(schema, definition),
        })
        .collect();
    Json::Array(definitions)
}

/// A message or a table: its layout, and its fields with the offsets and
/// sizes that the schema fixes
fn table(
    schema: &Schema,
    fixed: &FixedSizes,
    index: usize,
    kind: &'static str,
    layout: Layout,
) -> Json {
    let definition = schema.definition(index);
    let offsets = fixed.offsets(schema, definition);

    let fields = definition
        .fields
        .iter()
        .zip(&offsets.fields)
        .enumerate()
        .map(|(index, (field, offset))| {
            let mut members = field_members(schema, index, field);
            members.push(("offset", Json::optional(offset.offset)));
            members.push(("size", Json::optional(offset.size)));
            members.push(("alignment", Json::size(alignment(schema, field.field_type))));
            Json::Object(members)
        })
        .collect();

    Json::Object(vec![
        ("kind", Json::string(kind)),
        ("name", Json::string(&definition.name)),
        ("docs", docs(&definition.docs)),
        ("layout", layout_members(layout)),
        (
            "alignment",
            Json::size(usize::from(schema.table_alignment(index))),
        ),
        ("size", Json::optional(offsets.size)),
        ("fields", Json::Array(fields)),
    ])
}

fn layout_members(layout: Layout) -> Json {
    debug_assert_eq!(offset_size(TABLE_LAYOUT.offset_type), NESTED_OFFSET_SIZE);

    Json::Object(vec![
        ("offset_type", Json::string(layout.offset_type.name())),
        ("offset_size", Json::size(offset_size(layout.offset_type))),
        ("sized", Json::Bool(layout.sized)),
        (
            "version",
            layout
                .version
                .map_or(Json::Null, |version| Json::Number(u64::from(version))),
        ),
        ("header_size", Json::size(header_size(layout))),
    ])
}

/// The alignment that a writer pads a field's value to: an array's elements,
/// after its count, and a nested table's start
fn alignment(schema: &Schema, field_type: FieldType) -> usize {
    let alignment = match field_type {
        FieldType::Array(scalar) => scalar.size(),
        FieldType::StructArray(index) => schema.definition(index).struct_layout().alignment,
        FieldType::Table(index) => schema.table_alignment(index),
        FieldType::Scalar(_) | FieldType::String | FieldType::Struct(_) | FieldType::Vector(_) => 1,
    };
    usize::from(alignment)
}

/// A struct: its size and alignment, its fields with their offsets, and its
/// scalars in order, those of the structs in it in their place
fn structure(schema: &Schema, definition: &Definition) -> Json {
    let layout = definition.struct_layout();

    let mut offset = 0;
    let mut fields = Vec::with_capacity(definition.fields.len());
    for (index, field) in definition.fields.iter().enumerate() {
        let size = struct_field_size(schema, field.field_type);
        let mut members = field_members(schema, index, field);
        members.push(("offset", Json::size(offset)));
        members.push(("size", Json::size(size)));
        fields.push(Json::Object(members));
        offset += size;
    }

    let mut scalars = Vec::new();
    push_scalars(schema, definition, &mut Vec::new(), &mut 0, &mut scalars);

    Json::Object(vec![
        ("kind", Json::string("struct")),
        ("name", Json::string(&definition.name)),
        ("docs", docs(&definition.docs)),
        ("size", Json::size(layout.size)),
        ("alignment", Json::size(usize::from(layout.alignment))),
        ("fields", Json::Array(fields)),
        ("scalars", Json::Array(scalars)),
    ])
}

fn struct_field_size(schema: &Schema, field_type: FieldType) -> usize {
    match field_type {
        FieldType::Scalar(scalar) => usize::from(scalar.size()),
        FieldType::Struct(index) => schema.definition(index).struct_layout().size,
        _ => unreachable!("structs hold scalars and structs only"),
    }
}

/// Adds each scalar of a struct with the names of the fields that lead to
/// it and its offset from the outermost struct's start. The checker bounds
/// how deep structs nest and how many fields their paths take.
fn push_scalars<'a>(
    schema: &'a Schema,
    definition: &'a Definition,
    path: &mut Vec<&'a str>,
    offset: &mut usize,
    scalars: &mut Vec<Json>,
) {
    for field in &definition.fields {
        path.push(&field.name);
        match field.field_type {
            FieldType::Scalar(scalar) => {
                scalars.push(Json::Object(vec![
                    (
                        "path",
                        Json::Array(path.iter().map(|name| Json::string(name)).collect()),
                    ),
                    ("scalar", Json::string(scalar.name())),
                    ("offset", Json::size(*offset)),
                ]));
                *offset += usize::from(scalar.size());
            }
            FieldType::Struct(index) => {
                push_scalars(schema, schema.definition(index), path, offset, scalars);
            }
            _ => unreachable!("structs hold scalars and structs only"),
        }
        path.pop();
    }
}

/// The members that every field has: its index, name, docs and type
fn field_members(schema: &Schema, index: usize, field: &Field) -> Vec<(&'static str, Json)> {
    vec![
        ("index", Json::size(index)),
        ("name", Json::string(&field.name)),
        ("docs", docs(&field.docs)),
        ("type", field_type(schema, field.field_type)),
    ]
}

fn field_type(schema: &Schema, field_type: FieldType) -> Json {
    let scalar = |kind, scalar: Scalar| {
        Json::Object(vec![
            ("kind", Json::string(kind)),
            ("scalar", Json::string(scalar.name())),
        ])
    };
    let definition = |kind, index| {
        Json::Object(vec![
            ("kind", Json::string(kind)),
            ("definition", Json::string(&schema.definition(index).name)),
        ])
    };

    match field_type {
        FieldType::Scalar(of) => scalar("scalar", of),
        FieldType::Array(of) => scalar("array", of),
        FieldType::String => Json::Object(vec![("kind", Json::string("string"))]),
        FieldType::Struct(index) => definition("struct", index),
        FieldType::StructArray(index) => definition("struct_array", index),
        FieldType::Table(index) => definition("table", index),
        FieldType::Vector(index) => definition("vector", index),
    }
}

fn docs(lines: &[String]) -> Json {
    Json::Array(lines.iter().map(|line| Json::string(line)).collect())
}

/// A JSON value, whose objects keep their members' order
enum Json {
    Null,
    Bool(bool),
    Number(u64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(&'static str, Json)>),
}

impl Json {
    fn string(text: &str) -> Self {
        Self::String(text.to_owned())
    }

    fn size(size: usize) -> Self {
        Self::Number(size as u64)
    }

    fn optional(size: Option<usize>) -> Self {
        size.map_or(Self::Null, Self::size)
    }

    /// Writes the value indented by two spaces a level, with each item of
    /// an array and each member of an object on a line of its own, but for
    /// those that hold only strings, numbers, bools and nulls
    fn write(&self, json: &mut String, depth: usize) {
        match self {
            Self::Null => json.push_str("null"),
            Self::Bool(value) => {
                let _ = write!(json, "{value}");
            }
            Self::Number(value) => {
                let _ = write!(json, "{value}");
            }
            Self::String(text) => write_string(json, text),
            Self::Array(items) => {
                let inline = items.iter().all(Self::is_leaf);
                write_items(json, depth, inline, ['[', ']'], items, |json, item| {
                    item.write(json, depth + 1);
                });
            }
            Self::Object(members) => {
                let inline = members.iter().all(|(_, value)| value.is_leaf());
                write_items(
                    json,
                    depth,
                    inline,
                    ['{', '}'],
                    members,
                    |json, (name, value)| {
                        write_string(json, name);
                        json.push_str(": ");
                        value.write(json, depth + 1);
                    },
                );
            }
        }
    }

    /// Whether the value holds no array or object, so that an array or
    /// object of such values fits on one line, as a field's type does
    const fn is_leaf(&self) -> bool {
        !matches!(self, Self::Array(_) | Self::Object(_))
    }
}

/// Writes the items of an array or the members of an object: on one line
/// when `inline`, and otherwise each on a line of its own
fn write_items<T>(
    json: &mut String,
    depth: usize,
    inline: bool,
    [open, close]: [char; 2],
    items: &[T],
    mut write: impl FnMut(&mut String, &T),
) {
    json.push(open);
    for (index, item) in items.iter().enumerate() {
        if inline {
            json.push_str(if index == 0 { "" } else { ", " });
        } else {
            json.push_str(if index == 0 { "\n" } else { ",\n" });
            indent(json, depth + 1);
        }
        write(json, item);
    }
    if !inline && !items.is_empty() {
        json.push('\n');
        indent(json, depth);
    }
    json.push(close);
}

fn indent(json: &mut String, depth: usize) {
    for _ in 0..depth {
        json.push_str("  ");
    }
}

/// Writes a string with the escapes that JSON requires, and those of the
/// line and paragraph separators, which a schema's path may hold and which
/// end a line in JavaScript before ES2019
fn write_string(json: &mut String, text: &str) {
    json.push('"');
    for character in text.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            character
                if u32::from(character) < 0x20 || matches!(character, '\u{2028}' | '\u{2029}') =>
            {
                let _ = write!(json, "\\u{:04x}", u32::from(character));
            }
            character => json.push(character),
        }
    }
    json.push('"');
}
