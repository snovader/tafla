//! tafla-gen's tables as JSON: the tables of `conformance/generated` decode
//! as the fields of their cases in `shapes.json` and encode back byte for
//! byte; the tables of arbitrary schemas encode from JSON as `TableBuilder`
//! writes them and decode back into the same JSON; bytes that a writer
//! wouldn't write read as values; corrupted, arbitrary, deep and
//! self-sharing bytes decode as values or decode errors within the limits;
//! and errors in JSON name the path of their field.

mod tables;

use std::fmt::Write;
use std::fs;
use std::path::Path;

use proptest::prelude::*;
use serde_json::Value as Json;
use tables::{Bits, CASES, Header, Value, build, check_json_round_trip, struct_bytes};
use tafla::{Alignment, OffsetType, TableLayout};
use tafla_gen::{DecodeOptions, Diagnostic, MAX_DEPTH, Root, Schema, parse};

const REPOSITORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

const EXAMPLE_SCHEMAS: [&str; 4] = ["all_types", "fixed_fields", "layouts", "shapes"];

fn repository_file(path: &str) -> Vec<u8> {
    fs::read(Path::new(REPOSITORY).join(path)).unwrap()
}

fn example_schema(name: &str) -> String {
    String::from_utf8(repository_file(&format!("schemas/{name}.tafla"))).unwrap()
}

fn schema(source: &str) -> Schema {
    parse(source).unwrap_or_else(|errors| {
        panic!("{}", Diagnostic::render_all(&errors, "test.tafla", source))
    })
}

/// The messages and structs of a schema's source, by their names
fn roots(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let rest = line
                .strip_prefix("message ")
                .or_else(|| line.strip_prefix("struct "))?;
            rest.split([' ', '(', '{']).next().map(str::to_owned)
        })
        .collect()
}

/// The rendered errors of JSON that doesn't encode
fn encode_errors(root: &Root, json: &str) -> String {
    let errors = root.encode(json).expect_err(json);
    Diagnostic::render_all(&errors, "table.json", json)
}

// The tables of conformance/generated

#[test]
fn decodes_the_tables_of_conformance_generated_as_their_cases_and_encodes_them_back() {
    let source = example_schema("shapes");
    let schema = schema(&source);
    let cases: Json =
        serde_json::from_slice(&repository_file("conformance/generated/shapes.json")).unwrap();

    for case in cases["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let root = Root::find(&schema, case["type"].as_str().unwrap()).unwrap();
        let file = format!("conformance/generated/{}", case["file"].as_str().unwrap());
        let bytes = repository_file(&file);

        let json = root.decode(&bytes, &DecodeOptions::default()).unwrap();
        let read: Json = serde_json::from_str(&json).unwrap();
        assert_eq!(read, case["fields"], "{name}");

        // The description as serde_json writes it, whose numbers are written
        // otherwise, such as `3.4028234663852886e38`
        assert_eq!(
            root.encode(&case["fields"].to_string()),
            Ok(bytes.clone()),
            "{name}"
        );
        assert_eq!(root.encode(&json), Ok(bytes), "{name}");
    }
}

#[test]
fn decodes_strings_in_ascii_as_conformance_generated_writes_them() {
    let schema = schema(&example_schema("shapes"));
    let root = Root::find(&schema, "Point").unwrap();
    let bytes = repository_file("conformance/generated/shapes/point.bin");
    let options = DecodeOptions {
        ascii: true,
        ..DecodeOptions::default()
    };

    let json = root.decode(&bytes, &options).unwrap();

    assert_eq!(
        json,
        "{\n    \"x\": \"-0\",\n    \"y\": \"NaN\",\n    \"id\": \"-9223372036854775808\",\n    \
         \"name\": \"a\\u00e5\\u4e16\\ud83e\\udd80\"\n}\n"
    );
}

#[test]
fn shows_where_each_part_of_a_table_lies() {
    let schema = schema(&example_schema("shapes"));
    let root = Root::find(&schema, "ItemList").unwrap();
    let bytes = repository_file("conformance/generated/shapes/item-list.bin");

    let layout = root.layout(&bytes, &DecodeOptions::default());

    assert_eq!(layout.error, None);
    let lines: Vec<&str> = layout.text.lines().collect();
    assert_eq!(
        lines[..13],
        [
            "position    length  bytes                      field                 value",
            "       0         4  04 00 00 00                items                 offset 4",
            "       4         4  03 00 00 00                items                 count 3",
            "       8         4  2c 00 00 00                items[0]              end 44",
            "      12         4  48 00 00 00                items[1]              end 72",
            "      16         4  64 00 00 00                items[2]              end 100",
            "      20         4  08 00 00 00                items[0].value        offset 8",
            "      24         4  0c 00 00 00                items[0].pair         offset 12",
            "      28         4  00 00 80 3f                items[0].value        float32 1",
            "      32         4  08 00 00 00                items[0].pair.first   offset 8",
            "      36         4  0c 00 00 00                items[0].pair.second  offset 12",
            "      40         4  00 00 00 80                items[0].pair.first   float32 -0",
            "      44         4  00 00 c0 7f                items[0].pair.second  float32 NaN",
        ]
    );
    assert!(
        layout
            .text
            .ends_with("  items[2].pair.second  float32 -Infinity\n")
    );
}

#[test]
fn shows_the_layout_up_to_the_read_that_fails() {
    let schema = schema(&example_schema("shapes"));
    let root = Root::find(&schema, "Point").unwrap();
    let bytes = repository_file("conformance/generated/shapes/point.bin");

    let layout = root.layout(&bytes[..30], &DecodeOptions::default());

    let error = layout.error.unwrap();
    assert_eq!(error.path(), "y");
    assert_eq!(
        layout.text,
        "\
position    length  bytes                      field  value
       0         4  10 00 00 00                x      offset 16
       4         4  18 00 00 00                y      offset 24
       8         8  20 00 00 00 28 00 00 00           unread
      16         8  00 00 00 00 00 00 00 80    x      float64 -0
      24         6  00 00 00 00 00 00                 unread
"
    );
}

// Versions and the bytes that readers accept

#[test]
fn rejects_another_version_unless_any_version_is_set() {
    let schema = schema(&example_schema("shapes"));
    let root = Root::find(&schema, "Document").unwrap();
    let bytes = repository_file("conformance/generated/shapes/document.bin");
    let mut other = bytes.clone();
    other[4] = 2;

    let error = root.decode(&other, &DecodeOptions::default()).unwrap_err();
    assert!(error.is_unsupported_version());
    assert_eq!(error.path(), "");
    assert_eq!(
        error.to_string(),
        "Unsupported format version 2; the reader reads version 1"
    );

    let any_version = DecodeOptions {
        any_version: true,
        ..DecodeOptions::default()
    };
    let json = root.decode(&other, &any_version).unwrap();
    assert_eq!(
        Ok(json.clone()),
        root.decode(&bytes, &DecodeOptions::default())
    );
    // Encoding writes the schema's version
    assert_eq!(root.encode(&json), Ok(bytes));
}

#[test]
fn reads_bools_of_any_byte_and_unaligned_arrays_and_encodes_them_as_writers_do() {
    let schema = schema("message Flags { flag: bool; values: [uint32]; }");
    let root = Root::find(&schema, "Flags").unwrap();
    let written = [
        8, 0, 0, 0, 12, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0,
    ];
    // A bool of 0x80, and the array's count right after it, without padding
    let accepted = [
        8, 0, 0, 0, 9, 0, 0, 0, 0x80, 2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0,
    ];

    let json = root.decode(&accepted, &DecodeOptions::default()).unwrap();

    assert_eq!(json, "{\n    \"flag\": true,\n    \"values\": [1, 2]\n}\n");
    assert_eq!(
        root.decode(&written, &DecodeOptions::default()),
        Ok(json.clone())
    );
    assert_eq!(root.encode(&json), Ok(written.to_vec()));

    let layout = root.layout(&accepted, &DecodeOptions::default()).text;
    assert!(
        layout.contains("  flag    bool true, of the byte 128\n"),
        "{layout}"
    );
}

#[test]
fn decodes_a_struct_from_the_start_of_longer_bytes() {
    let schema = schema(&example_schema("shapes"));
    let root = Root::find(&schema, "Sample").unwrap();
    let mut bytes = repository_file("conformance/generated/shapes/sample.bin");
    let json = root.decode(&bytes, &DecodeOptions::default()).unwrap();

    bytes.extend([1, 2, 3]);

    assert_eq!(root.decode(&bytes, &DecodeOptions::default()), Ok(json));
    let error = root
        .decode(&bytes[..11], &DecodeOptions::default())
        .unwrap_err();
    assert_eq!(error.path(), "");
}

// Bounds on what corrupted bytes make decoding do

const FOREST: &str =
    "message Forest { trees: [Tree]; } table Tree { value: int32; children: [Tree]; }";

/// The JSON of a forest of one tree whose children are a chain of
/// `levels` trees, one within another
fn chain(levels: usize) -> String {
    let mut json = String::from("{\"trees\": [");
    for _ in 0..levels {
        json.push_str("{\"value\": 7, \"children\": [");
    }
    json.push_str(&"]}".repeat(levels));
    json.push_str("]}");
    json
}

#[test]
fn decodes_and_encodes_tables_nested_up_to_the_most_tables_deep() {
    let schema = schema(FOREST);
    let root = Root::find(&schema, "Forest").unwrap();
    // The forest is one table of the depth
    let deepest = chain(MAX_DEPTH - 1);

    let bytes = root.encode(&deepest).unwrap();
    let json = root.decode(&bytes, &DecodeOptions::default()).unwrap();
    assert_eq!(root.encode(&json), Ok(bytes));

    let errors = encode_errors(&root, &chain(MAX_DEPTH));
    assert!(
        errors.starts_with("error: tables nest more than 128 deep here\n"),
        "{errors}"
    );

    // A tree one deeper, as TableBuilder writes it
    let mut tree = Value::Vector(Vec::new());
    for _ in 0..MAX_DEPTH {
        tree = Value::Vector(vec![vec![Value::I32(7), tree]]);
    }
    let bytes = build(Header::new(TableLayout::DEFAULT), &[tree]).unwrap();
    let error = root.decode(&bytes, &DecodeOptions::default()).unwrap_err();
    assert!(
        error.to_string().contains("tables nest more than 128 deep"),
        "{error}"
    );
    assert!(error.path().starts_with("trees[0].children[0].children[0]"));
    assert!(
        root.layout(&bytes, &DecodeOptions::default())
            .error
            .is_some()
    );
}

/// A forest of two trees whose children are each the same vector of two
/// trees, and so on for `levels` levels, so that reading every tree reads
/// 2^`levels` of them from bytes in proportion to `levels`
fn shared_levels(levels: usize) -> Vec<u8> {
    const VECTOR: u32 = 36;
    let mut bytes = 4_u32.to_le_bytes().to_vec();

    for level in 0..levels {
        let position = 4 + level * VECTOR as usize;
        let next = position as u32 + VECTOR;
        for slot in [2, 24, 36] {
            bytes.extend(u32::to_le_bytes(slot));
        }
        // Two trees: the offsets of their value and their children, which
        // are the next level's vector, and the value
        for item_start in [position + 12, position + 24] {
            for slot in [8, next - item_start as u32, 7] {
                bytes.extend(u32::to_le_bytes(slot));
            }
        }
    }
    bytes.extend(0_u32.to_le_bytes());
    bytes
}

#[test]
fn bounds_the_values_that_offsets_to_shared_bytes_read() {
    let schema = schema(FOREST);
    let root = Root::find(&schema, "Forest").unwrap();

    // 2^4 trees at the deepest level, which a writer writes each in full
    let bytes = shared_levels(4);
    let json = root.decode(&bytes, &DecodeOptions::default()).unwrap();
    let written = root.encode(&json).unwrap();
    assert!(written.len() > 3 * bytes.len());
    assert_eq!(root.decode(&written, &DecodeOptions::default()), Ok(json));

    let bytes = shared_levels(60);
    let error = root.decode(&bytes, &DecodeOptions::default()).unwrap_err();
    assert!(
        error.to_string().contains("more than 8 times the bytes"),
        "{error}"
    );
}

#[test]
fn fails_at_a_vector_item_before_one_that_corrupted_ends_make_hold_itself() {
    let schema = schema(FOREST);
    let root = Root::find(&schema, "Forest").unwrap();
    // A vector of two trees whose first ends at 0, so that the second starts
    // at the vector's count, and its children, at the first end, are the
    // vector itself, as SPEC.md's "Validation" describes
    let bytes = [4, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 7, 0, 0, 0];

    let error = root.decode(&bytes, &DecodeOptions::default()).unwrap_err();

    assert_eq!(error.path(), "trees[0]");
    assert!(
        error.to_string().contains("ends before it starts"),
        "{error}"
    );
}

#[test]
fn decodes_every_truncation_of_the_conformance_tables_as_values_or_decode_errors() {
    let schema = schema(&example_schema("shapes"));
    let cases: Json =
        serde_json::from_slice(&repository_file("conformance/generated/shapes.json")).unwrap();

    for case in cases["cases"].as_array().unwrap() {
        let root = Root::find(&schema, case["type"].as_str().unwrap()).unwrap();
        let file = format!("conformance/generated/{}", case["file"].as_str().unwrap());
        let bytes = repository_file(&file);

        for length in 0..bytes.len() {
            let truncated = &bytes[..length];
            let decoded = root.decode(truncated, &DecodeOptions::default());
            let layout = root.layout(truncated, &DecodeOptions::default());
            assert_eq!(decoded.err(), layout.error);
        }
    }
}

// Errors in JSON

#[test]
fn names_the_path_of_each_error_in_the_json() {
    let shapes = schema(&example_schema("shapes"));
    let point = Root::find(&shapes, "Point").unwrap();
    let document = Root::find(&shapes, "Document").unwrap();

    let errors = encode_errors(
        &point,
        r#"{"x": 1, "y": "2", "id": 1.5, "nme": "a", "x": 0}"#,
    );
    assert_eq!(
        errors,
        "\
error: `y` isn't a float64
 --> table.json:1:15
  |
1 | {\"x\": 1, \"y\": \"2\", \"id\": 1.5, \"nme\": \"a\", \"x\": 0}
  |               ^^^ a float64 is a number within its range, or \"NaN\", \"Infinity\", \"-Infinity\" or \"-0\"

error: `id` isn't an int64
 --> table.json:1:26
  |
1 | {\"x\": 1, \"y\": \"2\", \"id\": 1.5, \"nme\": \"a\", \"x\": 0}
  |                          ^^^ an int64 is an integer from -9223372036854775808 to 9223372036854775807, as a number or as a string of its digits

error: `Point` has no field `nme`
 --> table.json:1:31
  |
1 | {\"x\": 1, \"y\": \"2\", \"id\": 1.5, \"nme\": \"a\", \"x\": 0}
  |                               ^^^^^ its fields are `x`, `y`, `id` and `name`

error: the field `x` is given twice
 --> table.json:1:43
  |
1 | {\"x\": 1, \"y\": \"2\", \"id\": 1.5, \"nme\": \"a\", \"x\": 0}
  |                                           ^^^ each field is given once

error: missing the field `name`
 --> table.json:1:1
  |
1 | {\"x\": 1, \"y\": \"2\", \"id\": 1.5, \"nme\": \"a\", \"x\": 0}
  | ^ every field is required
"
    );

    let json = r#"{"x": 1, "y": 2, "name": "", "columns": {"base": "18446744073709551615", "keys": [1, -1], "a": [], "b": {}}}"#;
    let errors = encode_errors(&document, json);
    assert!(
        errors.starts_with("error: `columns.base` isn't an int64\n"),
        "{errors}"
    );
    assert!(
        errors.contains("error: `columns.keys[1]` isn't a uint32\n"),
        "{errors}"
    );
    assert!(
        errors.contains("error: `columns.b` isn't an array of float32\n"),
        "{errors}"
    );
    assert!(
        errors.contains(" ^^ a uint32 is an integer from 0 to 4294967295\n"),
        "{errors}"
    );

    let errors = encode_errors(&document, r#"{"x": 1, "y": 2, "name": "", "columns": 3}"#);
    assert!(
        errors.starts_with("error: `columns` isn't a `Columns`\n"),
        "{errors}"
    );
    let errors = encode_errors(&document, "[]");
    assert!(
        errors.starts_with("error: the JSON isn't a `Document`\n"),
        "{errors}"
    );
}

#[test]
fn stops_at_an_error_of_json_syntax() {
    let schema = schema(&example_schema("shapes"));
    let point = Root::find(&schema, "Point").unwrap();

    for (json, error) in [
        (r#"{"x": 1,}"#, "error: expected a field's name, a string\n"),
        (r#"{"x": 1 "y": 2}"#, "error: expected `,` or `}`\n"),
        (r#"{"x": NaN}"#, "error: this isn't JSON\n"),
        (r#"{"x": 01}"#, "error: this isn't a JSON number\n"),
        (
            r#"{"x": 1, "y": 2, "id": 3, "name": "a"} {}"#,
            "error: expected the end of the text\n",
        ),
        (
            r#"{"x": 1, "y": 2, "id": 3, "name": "\ud800"}"#,
            "error: a string can't hold half of a surrogate pair\n",
        ),
        (
            r#"{"x": 1, "y": 2, "id": 3, "name": "a"#,
            "error: the string doesn't end\n",
        ),
        (r#"{"x": 1"#, "error: expected `,` or `}`\n"),
        ("", "error: the JSON isn't a `Point`\n"),
    ] {
        let errors = encode_errors(&point, json);
        assert!(errors.starts_with(error), "{json}: {errors}");
    }
}

#[test]
fn reports_a_table_too_long_for_its_offset_type_at_the_field_that_makes_it_so() {
    let schema = schema(&example_schema("fixed_fields"));
    let overflow = Root::find(&schema, "Overflow").unwrap();
    let quad = r#"{"a": 0, "b": 0, "c": 0, "d": 0, "e": 0, "f": 0, "g": 0, "h": 0}"#;
    let json = format!(
        r#"{{"id": 1, "block": {{"first": {quad}, "second": {quad}, "third": {quad}, "fourth": {quad}}}}}"#
    );

    let errors = encode_errors(&overflow, &json);

    assert!(
        errors.starts_with(
            "error: the field `block` makes `Overflow` 259 bytes, longer than its offset type, \
             `uint8`, addresses\n --> table.json:1:11\n"
        ),
        "{errors}"
    );
    assert!(errors.ends_with(" a table of the offset type `uint8` takes at most 255 bytes\n"));
}

#[test]
fn finds_messages_and_structs_only() {
    let schema = schema(&example_schema("shapes"));

    assert!(Root::find(&schema, "Sample").is_ok());
    let error = Root::find(&schema, "Columns").err().unwrap();
    assert_eq!(
        error.to_string(),
        "`Columns` is a table, which is read within a message, not on its own"
    );
    let error = Root::find(&schema, "Pont").err().unwrap();
    assert_eq!(
        error.to_string(),
        "the schema declares no message or struct `Pont`"
    );
    assert_eq!(
        error.help(),
        "the schema's messages and structs are `Point`, `Document`, `ItemList`, `Sample`, \
         `Task` and `Step`"
    );
}

// Arbitrary schemas, whose tables TableBuilder writes from the same values

/// A small random generator, which a seed that proptest draws starts, since
/// a schema's values depend on its types
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, count: usize) -> usize {
        (self.next() % count as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scalar {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
}

const SCALARS: [Scalar; 11] = [
    Scalar::Bool,
    Scalar::I8,
    Scalar::U8,
    Scalar::I16,
    Scalar::U16,
    Scalar::I32,
    Scalar::U32,
    Scalar::I64,
    Scalar::U64,
    Scalar::F32,
    Scalar::F64,
];

impl Scalar {
    const fn name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::I8 => "int8",
            Self::U8 => "uint8",
            Self::I16 => "int16",
            Self::U16 => "uint16",
            Self::I32 => "int32",
            Self::U32 => "uint32",
            Self::I64 => "int64",
            Self::U64 => "uint64",
            Self::F32 => "float32",
            Self::F64 => "float64",
        }
    }

    const fn size(self) -> usize {
        match self {
            Self::Bool | Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
        }
    }
}

#[derive(Clone, Copy)]
enum Type {
    Scalar(Scalar),
    Array(Scalar),
    String,
    Struct(usize),
    StructArray(usize),
    Table(usize),
    Vector(usize),
}

/// A schema of `structs` and `tables`, by their indices, and a message
struct Model {
    structs: Vec<Vec<Type>>,
    tables: Vec<Vec<Type>>,
    message: Vec<Type>,
    offset_type: OffsetType,
    sized: bool,
    version: Option<u32>,
}

impl Model {
    fn new(random: &mut Random) -> Self {
        let mut structs: Vec<Vec<Type>> = Vec::new();
        for index in 0..random.below(3) {
            let fields = (0..=random.below(3))
                .map(|_| {
                    if index > 0 && random.chance(25) {
                        Type::Struct(random.below(index))
                    } else {
                        Type::Scalar(*random.pick(&SCALARS))
                    }
                })
                .collect();
            structs.push(fields);
        }

        let table_count = random.below(4);
        let tables = (0..table_count)
            .map(|index| {
                (0..=random.below(3))
                    .map(|_| {
                        Self::field_type(random, structs.len(), index + 1..table_count, table_count)
                    })
                    .collect()
            })
            .collect();
        let message: Vec<Type> = (0..=random.below(4))
            .map(|_| Self::field_type(random, structs.len(), 0..table_count, table_count))
            .collect();

        let nests = message
            .iter()
            .any(|field| matches!(field, Type::Table(_) | Type::Vector(_)));
        let offset_type = if nests {
            OffsetType::Uint32
        } else {
            *random.pick(&[OffsetType::Uint8, OffsetType::Uint16, OffsetType::Uint32])
        };
        let version = random
            .chance(40)
            .then(|| (random.next() % (u64::from(offset_type.max_value()) + 1)) as u32);

        Self {
            structs,
            tables,
            message,
            offset_type,
            sized: random.chance(50),
            version,
        }
    }

    /// A field's type, of which nested tables are among `nested` only, so
    /// that tables don't contain themselves in place
    fn field_type(
        random: &mut Random,
        structs: usize,
        nested: std::ops::Range<usize>,
        tables: usize,
    ) -> Type {
        loop {
            let field_type = match random.below(7) {
                0 | 1 => Type::Scalar(*random.pick(&SCALARS)),
                2 => Type::Array(*random.pick(&SCALARS[1..])),
                3 => Type::String,
                4 if structs > 0 => {
                    if random.chance(50) {
                        Type::Struct(random.below(structs))
                    } else {
                        Type::StructArray(random.below(structs))
                    }
                }
                5 if !nested.is_empty() => Type::Table(nested.start + random.below(nested.len())),
                6 if tables > 0 => Type::Vector(random.below(tables)),
                _ => continue,
            };
            return field_type;
        }
    }

    fn type_name(field_type: Type) -> String {
        match field_type {
            Type::Scalar(scalar) => scalar.name().to_owned(),
            Type::Array(scalar) => format!("[{}]", scalar.name()),
            Type::String => "string".to_owned(),
            Type::Struct(index) => format!("S{index}"),
            Type::StructArray(index) => format!("[S{index}]"),
            Type::Table(index) => format!("T{index}"),
            Type::Vector(index) => format!("[T{index}]"),
        }
    }

    fn source(&self) -> String {
        let mut attributes = vec![format!("offset_type: {}", self.offset_type.name())];
        if self.sized {
            attributes.push("sized".to_owned());
        }
        if let Some(version) = self.version {
            attributes.push(format!("version: {version}"));
        }

        let mut source = String::new();
        let mut declare = |keyword: &str, name: String, fields: &[Type]| {
            let _ = writeln!(source, "{keyword} {name} {{");
            for (index, field) in fields.iter().enumerate() {
                let _ = writeln!(source, "    f{index}: {};", Self::type_name(*field));
            }
            source.push_str("}\n");
        };
        declare(
            "message",
            format!("M ({})", attributes.join(", ")),
            &self.message,
        );
        for (index, fields) in self.tables.iter().enumerate() {
            declare("table", format!("T{index}"), fields);
        }
        for (index, fields) in self.structs.iter().enumerate() {
            declare("struct", format!("S{index}"), fields);
        }
        source
    }

    fn header(&self) -> Header {
        let layout = TableLayout::DEFAULT.with_offset_type(self.offset_type);
        let layout = if self.sized { layout.sized() } else { layout };
        match self.version {
            Some(version) => Header::versioned(layout, version),
            None => Header::new(layout),
        }
    }

    /// The size and alignment of a struct
    fn struct_layout(&self, index: usize) -> (usize, usize) {
        self.structs[index]
            .iter()
            .map(|field| match *field {
                Type::Scalar(scalar) => (scalar.size(), scalar.size()),
                Type::Struct(inner) => self.struct_layout(inner),
                _ => unreachable!("structs hold scalars and structs"),
            })
            .fold(
                (0, 1),
                |(size, alignment), (field_size, field_alignment)| {
                    (size + field_size, alignment.max(field_alignment))
                },
            )
    }

    /// The values of a table's fields, and their JSON, with the members in
    /// their order or reversed
    fn table(&self, random: &mut Random, fields: &[Type], depth: usize) -> (Vec<Value>, String) {
        let mut values = Vec::new();
        let mut members = Vec::new();
        for (index, field) in fields.iter().enumerate() {
            let (value, json) = self.value(random, *field, depth);
            values.push(value);
            members.push(format!("\"f{index}\": {json}"));
        }
        if random.chance(30) {
            members.reverse();
        }
        (values, format!("{{{}}}", members.join(", ")))
    }

    /// The scalars of a struct, those of the structs in it in their place,
    /// and its JSON
    fn structure(&self, random: &mut Random, index: usize, scalars: &mut Vec<Value>) -> String {
        let mut members = Vec::new();
        for (field, field_type) in self.structs[index].iter().enumerate() {
            let json = match *field_type {
                Type::Scalar(scalar) => {
                    let (value, json) = scalar_value(random, scalar);
                    scalars.push(value);
                    json
                }
                Type::Struct(inner) => self.structure(random, inner, scalars),
                _ => unreachable!("structs hold scalars and structs"),
            };
            members.push(format!("\"f{field}\": {json}"));
        }
        format!("{{{}}}", members.join(", "))
    }

    fn value(&self, random: &mut Random, field_type: Type, depth: usize) -> (Value, String) {
        let count = |random: &mut Random| random.below(4);

        match field_type {
            Type::Scalar(scalar) => scalar_value(random, scalar),
            Type::Array(scalar) => {
                let elements: Vec<(Value, String)> = (0..count(random))
                    .map(|_| scalar_value(random, scalar))
                    .collect();
                let (values, json): (Vec<Value>, Vec<String>) = elements.into_iter().unzip();
                (array(scalar, values), format!("[{}]", json.join(", ")))
            }
            Type::String => {
                let string = string_value(random);
                let json = serde_json::to_string(&string).unwrap();
                (Value::String(string), json)
            }
            Type::Struct(index) => {
                let mut scalars = Vec::new();
                let json = self.structure(random, index, &mut scalars);
                (Value::Struct(scalars), json)
            }
            Type::StructArray(index) => {
                let mut items = Vec::new();
                let mut json = Vec::new();
                for _ in 0..count(random) {
                    let mut scalars = Vec::new();
                    json.push(self.structure(random, index, &mut scalars));
                    items.push(scalars);
                }
                let (size, alignment) = self.struct_layout(index);
                let alignment = match alignment {
                    1 => Alignment::One,
                    2 => Alignment::Two,
                    4 => Alignment::Four,
                    _ => Alignment::Eight,
                };
                let value = Value::StructArray {
                    size,
                    alignment,
                    items,
                };
                (value, format!("[{}]", json.join(", ")))
            }
            Type::Table(index) => {
                let (values, json) = self.table(random, &self.tables[index], depth + 1);
                (Value::Table(values), json)
            }
            Type::Vector(index) => {
                // Vectors of a table that holds itself end within a few levels
                let items = if depth < 4 { count(random) } else { 0 };
                let (values, json): (Vec<_>, Vec<_>) = (0..items)
                    .map(|_| self.table(random, &self.tables[index], depth + 1))
                    .unzip();
                (Value::Vector(values), format!("[{}]", json.join(", ")))
            }
        }
    }
}

fn array(scalar: Scalar, values: Vec<Value>) -> Value {
    macro_rules! elements {
        ($variant:ident) => {
            values
                .into_iter()
                .map(|value| match value {
                    Value::$variant(value) => value,
                    _ => unreachable!(),
                })
                .collect()
        };
    }

    match scalar {
        Scalar::Bool => unreachable!("bools have no arrays"),
        Scalar::I8 => Value::I8Array(elements!(I8)),
        Scalar::U8 => Value::U8Array(elements!(U8)),
        Scalar::I16 => Value::I16Array(elements!(I16)),
        Scalar::U16 => Value::U16Array(elements!(U16)),
        Scalar::I32 => Value::I32Array(elements!(I32)),
        Scalar::U32 => Value::U32Array(elements!(U32)),
        Scalar::I64 => Value::I64Array(elements!(I64)),
        Scalar::U64 => Value::U64Array(elements!(U64)),
        Scalar::F32 => Value::F32Array(elements!(F32)),
        Scalar::F64 => Value::F64Array(elements!(F64)),
    }
}

/// A scalar of the type, at its limits or of random bits, and its JSON in
/// any of the forms that encoding takes
fn scalar_value(random: &mut Random, scalar: Scalar) -> (Value, String) {
    let bits = if random.chance(40) {
        *random.pick(&[
            0,
            1,
            u64::MAX,
            1 << 63,
            (1 << 63) - 1,
            1 << 53,
            (1 << 53) + 1,
        ])
    } else {
        random.next()
    };

    macro_rules! integer {
        ($variant:ident, $type:ty) => {{
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_possible_wrap,
                clippy::cast_sign_loss
            )]
            let value = bits as $type;
            let json = if size_of::<$type>() == 8 && random.chance(50) {
                format!("\"{value}\"")
            } else {
                value.to_string()
            };
            (Value::$variant(value), json)
        }};
    }

    match scalar {
        Scalar::Bool => {
            let value = bits % 2 == 1;
            (Value::Bool(value), value.to_string())
        }
        Scalar::I8 => integer!(I8, i8),
        Scalar::U8 => integer!(U8, u8),
        Scalar::I16 => integer!(I16, i16),
        Scalar::U16 => integer!(U16, u16),
        Scalar::I32 => integer!(I32, i32),
        Scalar::U32 => integer!(U32, u32),
        Scalar::I64 => integer!(I64, i64),
        Scalar::U64 => integer!(U64, u64),
        Scalar::F32 => {
            let value = if random.chance(50) {
                *random.pick(&[
                    0.0,
                    -0.0,
                    f32::NAN,
                    -f32::NAN,
                    f32::from_bits(0x7f80_0001),
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::from_bits(1),
                    f32::MAX,
                    f32::MIN_POSITIVE,
                    0.1,
                ])
            } else {
                #[allow(clippy::cast_possible_truncation)]
                f32::from_bits(bits as u32)
            };
            let json = float_json(
                f64::from(value),
                value.to_bits().into(),
                8,
                value.is_nan(),
                || format!("{value:?}"),
            );
            (Value::F32(Bits(value)), json)
        }
        Scalar::F64 => {
            let value = if random.chance(50) {
                *random.pick(&[
                    0.0,
                    -0.0,
                    f64::NAN,
                    f64::from_bits(0xfff8_0000_0000_0001),
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    5e-324,
                    f64::MAX,
                    0.1,
                ])
            } else {
                f64::from_bits(bits)
            };
            let json = float_json(value, value.to_bits(), 16, value.is_nan(), || {
                format!("{value:?}")
            });
            (Value::F64(Bits(value)), json)
        }
    }
}

/// A float's JSON: a number, as Rust's `Debug` writes it, or a name, with
/// the bits of any NaN
fn float_json(
    value: f64,
    bits: u64,
    digits: usize,
    is_nan: bool,
    number: impl FnOnce() -> String,
) -> String {
    if is_nan {
        format!("\"NaN:0x{bits:0digits$x}\"")
    } else if value.is_infinite() {
        if value > 0.0 {
            "\"Infinity\""
        } else {
            "\"-Infinity\""
        }
        .to_owned()
    } else {
        number()
    }
}

fn string_value(random: &mut Random) -> String {
    let pieces = [
        "",
        "a",
        "å",
        "世界",
        "🦀",
        "\u{feff}",
        "\"\\/",
        "\n\t\r\u{0}\u{1f}",
        "\u{2028}\u{202e}",
        "\u{7f}",
        "\u{10ffff}",
    ];
    (0..random.below(4))
        .map(|_| *random.pick(&pieces))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES * 4))]

    #[test]
    fn tables_of_any_schema_encode_as_table_builder_writes_them_and_decode_back(seed in any::<u64>()) {
        let mut random = Random(seed | 1);
        let model = Model::new(&mut random);
        let source = model.source();
        let schema = schema(&source);
        let root = Root::find(&schema, "M").unwrap();
        let (values, json) = model.table(&mut random, &model.message, 1);

        let by_hand = build(model.header(), &values);
        let encoded = root.encode(&json);

        match (by_hand, encoded) {
            (Ok(by_hand), Ok(encoded)) => {
                prop_assert_eq!(&encoded, &by_hand, "{}\n{}", source, json);
                check_json_round_trip(&source, "M", &encoded)?;
            }
            (Err(_), Err(errors)) => {
                prop_assert!(errors[0].message.contains("longer than its offset type"), "{:?}", errors);
            }
            (by_hand, encoded) => {
                return Err(TestCaseError::fail(format!("{source}\n{json}\n{by_hand:?}\n{encoded:?}")));
            }
        }

        for index in 0..model.structs.len() {
            let mut scalars = Vec::new();
            let json = model.structure(&mut random, index, &mut scalars);
            let root = Root::find(&schema, &format!("S{index}")).unwrap();
            let bytes = struct_bytes(&scalars);
            prop_assert_eq!(root.encode(&json), Ok(bytes.clone()));
            check_json_round_trip(&source, &format!("S{index}"), &bytes)?;
        }
    }

    #[test]
    fn arbitrary_bytes_decode_as_any_example_message_or_struct_as_values_or_decode_errors(
        bytes in proptest::collection::vec(any::<u8>(), 0..256),
        any_version in any::<bool>(),
    ) {
        for name in EXAMPLE_SCHEMAS {
            let source = example_schema(name);
            let schema = schema(&source);
            for root_name in roots(&source) {
                let root = Root::find(&schema, &root_name).unwrap();
                let options = DecodeOptions { any_version, ..DecodeOptions::default() };

                let decoded = root.decode(&bytes, &options);
                let layout = root.layout(&bytes, &options);
                prop_assert_eq!(decoded.as_ref().err(), layout.error.as_ref());

                // Values that offsets read more than once take more bytes
                // when they're written, which may be too many
                if let Ok(json) = decoded {
                    match root.encode(&json) {
                        Ok(written) => prop_assert_eq!(root.decode(&written, &options), Ok(json)),
                        Err(errors) => prop_assert!(
                            errors[0].message.contains("longer than its offset type"),
                            "{:?}",
                            errors
                        ),
                    }
                }
            }
        }
    }

    #[test]
    fn arbitrary_text_encodes_as_a_table_or_fails_with_errors_that_point_into_it(
        text in "[{}\\[\\]:, \"a-z0-9.eE+\\-\\\\]{0,64}",
    ) {
        let source = example_schema("all_types");
        let schema = schema(&source);
        for root_name in roots(&source) {
            let root = Root::find(&schema, &root_name).unwrap();
            if let Err(errors) = root.encode(&text) {
                for error in &errors {
                    prop_assert!(error.span.start <= error.span.end && error.span.end <= text.len());
                    prop_assert!(error.render("table.json", &text).starts_with("error: "));
                }
            }
        }
    }
}
