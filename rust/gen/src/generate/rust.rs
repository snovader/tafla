//! Rust for `tafla`. Messages and tables are `Copy` views that read
//! fields lazily, borrowing strings and arrays from the bytes; they're written
//! from structs of their fields, which a `TableSizer` sizes and a
//! `TableWriter` writes, nested tables in place. Structs are plain structs
//! read and written at fixed offsets.

use std::fmt::Display;

use super::prose::indefinite_article;
use super::writer::{CodeWriter, Style, call, text};
use super::{GeneratedFile, Options};
use crate::model::{Definition, Field, FieldType, Kind, Layout, OffsetType, Scalar, Schema};

/// The runtime's crate, unless [`Options::runtime`] names another
const RUNTIME: &str = "tafla";

const STYLE: Style = Style {
    width: 100,
    indent: "    ",
    trailing_comma: true,
    hug_arguments: false,
    max_arguments_width: Some(FN_CALL_WIDTH),
};

/// rustfmt's `chain_width`, past which it puts each call of a chain on a line
const CHAIN_WIDTH: usize = 60;

/// rustfmt's `fn_call_width`, past which it breaks a call's arguments
const FN_CALL_WIDTH: usize = 60;

/// The widest `use` statement with a list that rustfmt keeps on one line, two
/// columns less than its `max_width`
const USE_LIST_WIDTH: usize = 98;

/// Strict and reserved keywords, which names take as raw identifiers
const KEYWORDS: [&str; 48] = [
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn",
    "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
    "virtual", "where", "while", "yield",
];

/// Keywords that can't be raw identifiers, which names take a `_` after
const PATH_KEYWORDS: [&str; 3] = ["crate", "self", "super"];

/// The associated functions that messages and tables define beside their
/// getters, `new()` and versioned messages' `version_of()`, which getters
/// would clash with, so names take a `_` after them too
const MEMBERS: [&str; 2] = ["new", "version_of"];

pub fn generate(
    schema: &Schema,
    stem: &str,
    header: &str,
    options: &Options,
) -> Vec<GeneratedFile> {
    let runtime = options.runtime.as_deref().unwrap_or(RUNTIME);
    let generator = Generator {
        schema,
        borrows: borrowing(schema),
    };

    let mut writer = CodeWriter::new(STYLE.indent);
    writer.line(format!("// {header}"));
    writer.blank();
    generator.imports(&mut writer, runtime);

    for (index, definition) in schema.definitions.iter().enumerate() {
        writer.blank();
        match definition.kind {
            Kind::Message(layout) => generator.message(&mut writer, definition, layout),
            Kind::Table => generator.table(&mut writer, definition),
            Kind::Struct(_) => generator.structure(&mut writer, definition),
        }
        if !definition.is_struct() {
            writer.blank();
            generator.fields_struct(&mut writer, index);
        }

        if schema.is_item(index) {
            writer.blank();
            if definition.is_struct() {
                struct_array_view(&mut writer, definition);
            } else {
                vector_view(&mut writer, definition);
            }
        }
    }

    vec![GeneratedFile {
        path: format!("{stem}.rs"),
        contents: writer.finish(),
    }]
}

struct Generator<'a> {
    schema: &'a Schema,
    /// Whether each definition's fields struct borrows strings or arrays, by
    /// its index
    borrows: Vec<bool>,
}

impl Generator<'_> {
    fn imports(&self, writer: &mut CodeWriter, runtime: &str) {
        let definitions = &self.schema.definitions;
        // A schema without definitions imports nothing
        if definitions.is_empty() {
            return;
        }

        let has_tables = definitions.iter().any(|definition| !definition.is_struct());
        let has_messages = definitions
            .iter()
            .any(|definition| matches!(definition.kind, Kind::Message(_)));
        let has_nested = definitions
            .iter()
            .any(|definition| definition.kind == Kind::Table);
        let has_array_view = definitions
            .iter()
            .flat_map(|definition| &definition.fields)
            .any(|field| {
                matches!(field.field_type, FieldType::Array(scalar) if scalar != Scalar::Uint8)
            });
        let has_offset_type = definitions.iter().any(|definition| {
            definition
                .layout()
                .is_some_and(|layout| layout.offset_type != OffsetType::Uint32)
        });
        let has_structs = definitions.iter().any(Definition::is_struct);
        let has_vector = definitions
            .iter()
            .flat_map(|definition| &definition.fields)
            .any(|field| matches!(field.field_type, FieldType::Vector(_)));

        let mut names = Vec::new();
        // Arrays of structs and nested tables are padded to their alignment
        if has_structs || has_nested {
            names.push("Alignment");
        }
        if has_array_view {
            names.push("ArrayView");
        }
        if has_messages {
            names.push("BuildError");
        }
        if has_tables {
            names.push("ByteBuffer");
        }
        names.push("DecodeError");
        if has_offset_type {
            names.push("OffsetType");
        }
        // Messages make sizers, and tables take them and their writers
        if has_tables {
            names.extend(["TableLayout", "TableSizer"]);
        }
        if has_nested {
            names.push("TableWriter");
        }
        if has_vector {
            names.push("Vector");
        }

        let statement = format!("use {runtime}::{{{}}};", names.join(", "));
        if statement.len() <= USE_LIST_WIDTH {
            writer.line(statement);
            return;
        }

        // rustfmt fills the lines of a list that doesn't fit, each as wide as
        // a `use` statement on one line
        writer.open(format!("use {runtime}::{{"));
        let mut line = String::new();
        for name in names {
            let width = STYLE.indent.len() + line.len() + 1 + name.len() + 1;
            if !line.is_empty() && width > USE_LIST_WIDTH {
                writer.line(&line);
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(name);
            line.push(',');
        }
        writer.line(line);
        writer.close("};");
    }

    fn message(&self, writer: &mut CodeWriter, definition: &Definition, layout: Layout) {
        let name = &definition.name;
        let versioned = layout.version.is_some();
        let generics = if versioned { "<true>" } else { "" };

        rust_doc(writer, &definition.docs);
        writer.line("#[derive(Clone, Copy, Debug)]");
        writer.open(format!("pub struct {name}<'a> {{"));
        writer.line("bytes: &'a [u8],");
        writer.close("}");
        writer.blank();

        writer.open(format!("impl<'a> {name}<'a> {{"));
        if let Some(version) = layout.version {
            rust_doc(
                writer,
                &[
                    "The version of the format, which the header carries and [`Self::new`] checks"
                        .to_owned(),
                ],
            );
            writer.line(format!(
                "pub const FORMAT_VERSION: u32 = {};",
                integer(&version)
            ));
            writer.blank();
        }
        layout_constant(writer, layout, generics);
        writer.blank();

        if versioned {
            let mut docs = vec![format!("Reads {} from `bytes`", article(name))];
            docs.extend(errors_doc("if `bytes` are in another format version"));
            rust_doc(writer, &docs);
            writer.open("pub fn new(bytes: &'a [u8]) -> Result<Self, DecodeError> {");
            writer.line("let buffer = ByteBuffer::new(bytes, Self::LAYOUT);");
            writer.blank();
            writer.line("let version = buffer.version()?;");
            writer.open("if version != Self::FORMAT_VERSION {");
            writer.open("return Err(DecodeError::UnsupportedVersion {");
            writer.line("version,");
            writer.line("supported: Self::FORMAT_VERSION,");
            writer.close("});");
            writer.close("}");
            writer.blank();
            writer.line("Ok(Self { bytes })");
            writer.close("}");
            writer.blank();

            let mut docs = vec![
                "Returns the format version in the header of `bytes`, without checking".to_owned(),
                "it, so that an application can choose the reader of that version".to_owned(),
            ];
            docs.extend(errors_doc("if the header doesn't fit in `bytes`"));
            rust_doc(writer, &docs);
            writer.open("pub fn version_of(bytes: &[u8]) -> Result<u32, DecodeError> {");
            writer.line("ByteBuffer::new(bytes, Self::LAYOUT).version()");
            writer.close("}");
        } else {
            rust_doc(writer, &[format!("Reads {} from `bytes`", article(name))]);
            writer.open("pub const fn new(bytes: &'a [u8]) -> Self {");
            writer.line("Self { bytes }");
            writer.close("}");
        }

        self.getters(writer, definition, true);
        writer.close("}");
    }

    fn table(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;

        rust_doc(writer, &definition.docs);
        writer.line("#[derive(Clone, Copy, Debug)]");
        writer.open(format!("pub struct {name}<'a> {{"));
        writer.line("bytes: &'a [u8],");
        writer.line("position: usize,");
        writer.close("}");
        writer.blank();

        writer.open(format!("impl<'a> {name}<'a> {{"));
        rust_doc(
            writer,
            &[
                "Reads the table at `position` in `bytes`, as its parent's getter finds it"
                    .to_owned(),
            ],
        );
        writer.open("const fn new(bytes: &'a [u8], position: usize) -> Self {");
        writer.line("Self { bytes, position }");
        writer.close("}");

        self.getters(writer, definition, false);
        writer.close("}");
    }

    #[allow(clippy::too_many_lines)]
    fn structure(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;
        let size = definition.struct_layout().size;
        // Each field, its range of bytes in the struct, and the type that
        // reads it from them
        let mut offset = 0;
        let fields: Vec<(String, usize, usize, String)> = definition
            .fields
            .iter()
            .map(|field| {
                let (end, read) = match field.field_type {
                    FieldType::Scalar(scalar) => {
                        (offset + usize::from(scalar.size()), scalar_type(scalar))
                    }
                    FieldType::Struct(index) => {
                        let inner = self.schema.definition(index);
                        (offset + inner.struct_layout().size, inner.name.as_str())
                    }
                    _ => unreachable!("structs hold scalars and structs only"),
                };
                let field = (identifier(&field.name), offset, end, read.to_owned());
                offset = end;
                field
            })
            .collect();

        rust_doc(writer, &definition.docs);
        writer.line("#[derive(Clone, Copy, Debug, PartialEq)]");
        writer.open(format!("pub struct {name} {{"));
        for field in &definition.fields {
            rust_doc(writer, &field.docs);
            writer.line(format!(
                "pub {}: {},",
                identifier(&field.name),
                self.input_type(field.field_type)
            ));
        }
        writer.close("}");
        writer.blank();

        writer.open(format!("impl {name} {{"));
        rust_doc(writer, &[format!("The byte length of {}", article(name))]);
        writer.line(format!("pub const SIZE: usize = {};", integer(&size)));
        writer.blank();
        rust_doc(
            writer,
            &[format!(
                "The alignment of an array of {}: its largest scalar's size",
                article(name)
            )],
        );
        writer.line(format!(
            "pub const ALIGNMENT: Alignment = Alignment::{};",
            alignment_variant(definition.struct_layout().alignment)
        ));
        writer.blank();

        let mut docs = vec![format!("Reads {} from the start of `bytes`", article(name))];
        docs.extend(errors_doc("if `bytes` are shorter than [`Self::SIZE`]"));
        rust_doc(writer, &docs);
        writer.open("pub fn read(bytes: &[u8]) -> Result<Self, DecodeError> {");
        writer.open("match bytes.first_chunk() {");
        writer.line("Some(bytes) => Ok(Self::from_bytes(bytes)),");
        writer.open("None => Err(DecodeError::OutOfRange {");
        writer.line("position: 0,");
        writer.line(format!("byte_length: {},", integer(&size)));
        writer.line("available: bytes.len(),");
        writer.close("}),");
        writer.close("}");
        writer.close("}");
        writer.blank();

        rust_doc(
            writer,
            &[format!(
                "Reads {} from its bytes, such as those of an array of them",
                article(name)
            )],
        );
        writer.line("#[must_use]");
        // By reference, as an array of structs iterates over them, however
        // small
        if size <= 8 {
            writer.line("#[allow(clippy::trivially_copy_pass_by_ref)]");
        }
        writer.open(format!(
            "pub fn from_bytes(bytes: &[u8; {}]) -> Self {{",
            integer(&size)
        ));
        self.struct_literal(writer, "Self {", definition, 0);
        writer.close("}");
        writer.blank();

        writer.line("#[must_use]");
        writer.open(format!(
            "pub fn to_bytes(self) -> [u8; {}] {{",
            integer(&size)
        ));
        writer.line("let mut bytes = [0; Self::SIZE];");
        for (field, start, end, read) in &fields {
            if read == "bool" {
                writer.line(format!(
                    "bytes[{}] = u8::from(self.{field});",
                    integer(start)
                ));
                continue;
            }
            let bytes = if read.starts_with(char::is_uppercase) {
                format!("self.{field}.to_bytes()")
            } else {
                format!("self.{field}.to_le_bytes()")
            };
            writer.line(format!(
                "bytes[{}..{}].copy_from_slice(&{bytes});",
                integer(start),
                integer(end)
            ));
        }
        writer.blank();
        writer.line("bytes");
        writer.close("}");
        writer.blank();

        rust_doc(
            writer,
            &[format!(
                "Writes [`{name}`]s one after another, as an array of them holds them"
            )],
        );
        writer.line("#[must_use]");
        writer.open("pub fn array_to_bytes(items: &[Self]) -> Vec<u8> {");
        writer.line("let mut bytes = Vec::with_capacity(items.len() * Self::SIZE);");
        writer.open("for item in items {");
        writer.line("bytes.extend_from_slice(&item.to_bytes());");
        writer.close("}");
        writer.blank();
        writer.line("bytes");
        writer.close("}");
        writer.close("}");
    }

    /// A struct of the scalars at `offset` in `bytes`, with a struct within it
    /// as a literal of its own, since an array only defaults up to 32 bytes.
    /// Each range of a scalar lies within the array, so converting it never
    /// fails.
    fn struct_literal(
        &self,
        writer: &mut CodeWriter,
        opening: &str,
        definition: &Definition,
        offset: usize,
    ) {
        writer.open(opening);
        let mut start = offset;
        for field in &definition.fields {
            let name = identifier(&field.name);
            match field.field_type {
                FieldType::Scalar(Scalar::Bool) => {
                    writer.line(format!("{name}: bytes[{}] != 0,", integer(&start)));
                    start += 1;
                }
                FieldType::Scalar(scalar) => {
                    let end = start + usize::from(scalar.size());
                    writer.line(format!(
                        "{name}: {}::from_le_bytes(bytes[{}..{}].try_into().unwrap_or_default()),",
                        scalar_type(scalar),
                        integer(&start),
                        integer(&end)
                    ));
                    start = end;
                }
                FieldType::Struct(index) => {
                    let inner = self.schema.definition(index);
                    self.struct_literal(
                        writer,
                        &format!("{name}: {} {{", inner.name),
                        inner,
                        start,
                    );
                    start += inner.struct_layout().size;
                }
                _ => unreachable!("structs hold scalars and structs only"),
            }
        }
        if opening == "Self {" {
            writer.close("}");
        } else {
            writer.close("},");
        }
    }

    /// The struct of the values that write a definition, with the methods
    /// that write them
    fn fields_struct(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
        let name = &definition.name;
        let borrows = self.borrows[index];
        let generics = if borrows { "<'a>" } else { "" };

        rust_doc(
            writer,
            &[format!("The fields of {}, to write", article(name))],
        );
        writer.line("#[derive(Clone, Debug)]");
        writer.open(format!("pub struct {name}Fields{generics} {{"));
        for field in &definition.fields {
            rust_doc(writer, &field.docs);
            writer.line(format!(
                "pub {}: {},",
                identifier(&field.name),
                self.input_type(field.field_type)
            ));
        }
        writer.close("}");
        writer.blank();

        if borrows {
            writer.open(format!("impl {name}Fields<'_> {{"));
        } else {
            writer.open(format!("impl {name}Fields {{"));
        }
        match definition.kind {
            Kind::Message(layout) => self.serialize(writer, definition, layout),
            Kind::Table => self.nested_size_and_write(writer, index),
            Kind::Struct(_) => unreachable!("structs are written by their own methods"),
        }
        writer.close("}");
    }

    /// A message's `serialize()`, which sizes its fields and then writes them
    /// into bytes of that length
    fn serialize(&self, writer: &mut CodeWriter, definition: &Definition, layout: Layout) {
        let name = &definition.name;

        let mut docs = vec![format!("Writes {} with every field", article(name))];
        docs.extend(error_doc(
            "a [`BuildError`]",
            &format!(
                "if it's longer than its offset type, `{}`, addresses",
                layout.offset_type.name()
            ),
        ));
        rust_doc(writer, &docs);
        writer.open("pub fn serialize(&self) -> Result<Vec<u8>, BuildError> {");
        let fields = text(integer(&definition.fields.len()));
        let layout_constant = text(format!("{name}::LAYOUT"));
        let sizer = if layout.version.is_some() {
            let version = text(format!("{name}::FORMAT_VERSION"));
            call(
                "TableSizer::versioned",
                vec![fields, layout_constant, version],
            )
        } else {
            call("TableSizer::new", vec![fields, layout_constant])
        };
        writer.statement(&STYLE, "let mut sizer = ", &sizer, "?;");
        self.sizes(writer, definition, "&mut sizer");
        writer.blank();
        writer.line("let mut writer = sizer.writer()?;");
        self.writes(writer, definition, "&mut writer");
        writer.blank();
        writer.line("writer.into_bytes()");
        writer.close("}");
    }

    /// The alignment of a nested table, and the methods with which its
    /// parent sizes and writes it
    fn nested_size_and_write(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
        let name = &definition.name;
        let fields = integer(&definition.fields.len());
        // The length of a table of scalars and structs doesn't depend on
        // their values
        let sized_by_values = definition.fields.iter().any(|field| {
            !matches!(
                field.field_type,
                FieldType::Scalar(_) | FieldType::Struct(_)
            )
        });

        rust_doc(
            writer,
            &[
                format!(
                    "The alignment that a parent pads {} to: the largest alignment of its",
                    article(name)
                ),
                "arrays and nested tables, or 1".to_owned(),
            ],
        );
        writer.line(format!(
            "pub const ALIGNMENT: Alignment = Alignment::{};",
            alignment_variant(self.schema.table_alignment(index))
        ));
        writer.blank();

        rust_doc(
            writer,
            &[format!(
                "Adds {} to its parent's sizer, after the field or vector that holds it",
                article(name)
            )],
        );
        if !sized_by_values {
            writer.line("#[allow(clippy::unused_self)]");
        }
        writer.open("pub fn size(&self, sizer: &mut TableSizer) {");
        writer.line(format!("let parent = sizer.start_table({fields});"));
        self.sizes(writer, definition, "sizer");
        writer.line("sizer.end_table(parent);");
        writer.close("}");
        writer.blank();

        rust_doc(
            writer,
            &[format!(
                "Writes {} where its parent's writer is, after the field or vector that holds it",
                article(name)
            )],
        );
        writer.open("pub fn write(&self, writer: &mut TableWriter) {");
        writer.line(format!("let parent = writer.start_table({fields});"));
        self.writes(writer, definition, "writer");
        writer.line("writer.end_table(parent);");
        writer.close("}");
    }

    /// The calls that add a table's fields to its sizer, in order, with
    /// `sizer`, the expression of the sizer that nested tables take. The
    /// locals don't clash with fields, whose values are `self`'s.
    fn sizes(&self, writer: &mut CodeWriter, definition: &Definition, sizer: &str) {
        for field in &definition.fields {
            let value = format!("self.{}", identifier(&field.name));
            let (method, arguments) = match field.field_type {
                FieldType::Scalar(scalar) => (format!("add_{}", method_type(scalar)), vec![]),
                FieldType::Array(scalar) => (
                    format!("add_{}_array", scalar_type(scalar)),
                    vec![text(format!("{value}.len()"))],
                ),
                FieldType::String => ("add_str".to_owned(), vec![text(value)]),
                FieldType::Struct(index) => {
                    let structure = &self.schema.definition(index).name;
                    (
                        "add_struct".to_owned(),
                        vec![text(format!("{structure}::SIZE"))],
                    )
                }
                FieldType::StructArray(index) => {
                    let structure = &self.schema.definition(index).name;
                    (
                        "add_struct_array".to_owned(),
                        vec![
                            text(format!("{value}.len()")),
                            text(format!("{structure}::SIZE")),
                            text(format!("{structure}::ALIGNMENT")),
                        ],
                    )
                }
                FieldType::Table(index) => {
                    let table = &self.schema.definition(index).name;
                    let alignment = text(format!("{table}Fields::ALIGNMENT"));
                    writer.statement(&STYLE, "", &call("sizer.add_table", vec![alignment]), ";");
                    writer.statement(
                        &STYLE,
                        "",
                        &call(format!("{value}.size"), vec![text(sizer)]),
                        ";",
                    );
                    continue;
                }
                FieldType::Vector(_) => {
                    let count = text(format!("{value}.len()"));
                    writer.statement(&STYLE, "", &call("sizer.add_vector", vec![count]), ";");
                    writer.open(format!("for fields in {value} {{"));
                    writer.line(format!("fields.size({sizer});"));
                    writer.close("}");
                    continue;
                }
            };
            writer.statement(&STYLE, "", &call(format!("sizer.{method}"), arguments), ";");
        }
    }

    /// The calls that write a table's fields, each with its index, in the
    /// order that `sizes()` added them, with `table_writer`, the expression of
    /// the writer that nested tables take
    fn writes(&self, writer: &mut CodeWriter, definition: &Definition, table_writer: &str) {
        for (index, field) in definition.fields.iter().enumerate() {
            let value = format!("self.{}", identifier(&field.name));
            let position = text(integer(&index));
            let (method, arguments) = match field.field_type {
                FieldType::Scalar(Scalar::Bool) => (
                    "set_u8".to_owned(),
                    vec![position, text(format!("u8::from({value})"))],
                ),
                FieldType::Scalar(scalar) => (
                    format!("set_{}", scalar_type(scalar)),
                    vec![position, text(value)],
                ),
                FieldType::Array(scalar) => (
                    format!("set_{}_array", scalar_type(scalar)),
                    vec![position, text(value)],
                ),
                FieldType::String => ("set_str".to_owned(), vec![position, text(value)]),
                FieldType::Struct(_) => (
                    "set_struct".to_owned(),
                    vec![position, text(format!("&{value}.to_bytes()"))],
                ),
                // The structs and their conversion to bytes, which the writer
                // appends without collecting them first
                FieldType::StructArray(struct_index) => {
                    let structure = &self.schema.definition(struct_index).name;
                    (
                        "set_structs".to_owned(),
                        vec![
                            position,
                            text(value),
                            text(format!("{structure}::to_bytes")),
                            text(format!("{structure}::ALIGNMENT")),
                        ],
                    )
                }
                FieldType::Table(table_index) => {
                    let table = &self.schema.definition(table_index).name;
                    let alignment = text(format!("{table}Fields::ALIGNMENT"));
                    writer.statement(
                        &STYLE,
                        "",
                        &call("writer.set_table", vec![position, alignment]),
                        ";",
                    );
                    writer.statement(
                        &STYLE,
                        "",
                        &call(format!("{value}.write"), vec![text(table_writer)]),
                        ";",
                    );
                    continue;
                }
                FieldType::Vector(_) => {
                    let count = text(format!("{value}.len()"));
                    writer.statement(
                        &STYLE,
                        "let vector = ",
                        &call("writer.set_vector", vec![position, count]),
                        ";",
                    );
                    writer.open(format!(
                        "for (item, fields) in {value}.iter().enumerate() {{"
                    ));
                    writer.line(format!("fields.write({table_writer});"));
                    writer.line("writer.set_item_end(vector, item);");
                    writer.close("}");
                    continue;
                }
            };
            writer.statement(
                &STYLE,
                "",
                &call(format!("writer.{method}"), arguments),
                ";",
            );
        }
    }

    fn getters(&self, writer: &mut CodeWriter, definition: &Definition, is_root: bool) {
        // Each getter reads with a buffer in the table's layout, a constant,
        // rather than one that the reader holds, so that its reads compile
        // for that layout's offset type wherever the reader is passed
        let buffer = if is_root {
            "let buffer = ByteBuffer::new(self.bytes, Self::LAYOUT);"
        } else {
            "let buffer = ByteBuffer::new(self.bytes, TableLayout::DEFAULT);"
        };

        for (index, field) in definition.fields.iter().enumerate() {
            let offset = if is_root {
                format!("buffer.root_offset({})?", integer(&index))
            } else {
                format!("buffer.field_offset(self.position, {})?", integer(&index))
            };

            let mut docs = field.docs.clone();
            docs.extend(errors_doc("if the field lies outside the bytes"));
            if field.docs.is_empty() {
                docs.remove(0);
            }

            writer.blank();
            rust_doc(writer, &docs);
            let name = identifier(&field.name);
            let result = format!("Result<{}, DecodeError>", self.value_type(field.field_type));
            open_method(writer, &format!("pub fn {name}"), &["&self"], &result);
            writer.line(buffer);
            writer.line(format!("let position = {offset};"));
            match field.field_type {
                FieldType::Scalar(Scalar::Bool) => writer.line("Ok(buffer.get_u8(position)? != 0)"),
                FieldType::Scalar(scalar) => {
                    writer.line(format!("buffer.get_{}(position)", scalar_type(scalar)));
                }
                FieldType::Array(scalar) => {
                    writer.line(format!(
                        "buffer.get_{}_array(position)",
                        scalar_type(scalar)
                    ));
                }
                FieldType::String => writer.line("buffer.get_str(position)"),
                FieldType::Table(table) => {
                    let table = &self.schema.definition(table).name;
                    let read = format!("{table}::new(self.bytes, position)");

                    // rustfmt breaks a call wider than its `fn_call_width`
                    // as `Ok`'s argument
                    if read.len() <= FN_CALL_WIDTH {
                        writer.line(format!("Ok({read})"));
                    } else {
                        writer.line(format!("Ok({table}::new("));
                        writer.indent();
                        writer.line("self.bytes,");
                        writer.line("position,");
                        writer.dedent();
                        writer.line("))");
                    }
                }
                FieldType::Struct(index) => {
                    let structure = &self.schema.definition(index).name;
                    writer.line(format!(
                        "let bytes = buffer.get_struct(position, {structure}::SIZE)?;"
                    ));
                    writer.blank();
                    writer.line(format!("{structure}::read(bytes)"));
                }
                FieldType::StructArray(index) => {
                    let structure = &self.schema.definition(index).name;
                    writer.statement(
                        &STYLE,
                        "let bytes = ",
                        &call(
                            "buffer.get_struct_array",
                            vec![text("position"), text(format!("{structure}::SIZE"))],
                        ),
                        "?;",
                    );
                    // The bytes are a whole number of structs
                    writer.line(format!(
                        "let (structs, _) = bytes.as_chunks::<{{ {structure}::SIZE }}>();"
                    ));
                    writer.blank();
                    writer.line(format!("Ok(structs.iter().map({structure}::from_bytes))"));
                }
                FieldType::Vector(table) => {
                    let table = &self.schema.definition(table).name;
                    writer.line("let vector = buffer.get_vector(position)?;");
                    // Collecting into a `Result` would grow the items from
                    // nothing, since its size hint starts at zero
                    writer.line("let mut items = Vec::with_capacity(vector.len());");
                    writer.open("for item in vector.iter_in_place() {");
                    // Each item is read in place, from its bytes and those
                    // after it in the message
                    writer.line(format!("items.push({table}::new(item?, 0));"));
                    writer.close("}");
                    writer.blank();
                    writer.line("Ok(items)");
                }
            }
            writer.close("}");

            if matches!(
                field.field_type,
                FieldType::Vector(_) | FieldType::StructArray(_)
            ) {
                self.view_getter(writer, field, buffer, &offset);
            }
        }
    }

    /// The getter of a view of a vector or an array of structs, which reads
    /// each item when it's asked for, rather than all of them as the
    /// field's own getter does. It finds the field as the field's getter
    /// does, with `buffer`'s line and the `offset` it reads.
    fn view_getter(&self, writer: &mut CodeWriter, field: &Field, buffer: &str, offset: &str) {
        let (view, errors) = match field.field_type {
            FieldType::Vector(index) => (
                format!("{}Vector", self.schema.definition(index).name),
                "if the field, or the vector's count or the ends of its items, lie outside the bytes",
            ),
            FieldType::StructArray(index) => (
                format!("{}Array", self.schema.definition(index).name),
                "if the field or its structs lie outside the bytes",
            ),
            _ => unreachable!("only vectors and arrays of structs have views"),
        };

        let mut docs = field.docs.clone();
        if !docs.is_empty() {
            docs.push(String::new());
        }
        docs.push(format!(
            "Views the items of `{}()`, reading each one when it's asked for",
            identifier(&field.name)
        ));
        docs.extend(errors_doc(errors));

        writer.blank();
        rust_doc(writer, &docs);
        open_method(
            writer,
            &format!("pub fn {}_view", field.name),
            &["&self"],
            &format!("Result<{view}<'a>, DecodeError>"),
        );
        writer.line(buffer);
        writer.line(format!("let position = {offset};"));
        match field.field_type {
            FieldType::Vector(_) => {
                writer.open(format!("Ok({view} {{"));
                writer.line("vector: buffer.get_vector(position)?,");
                writer.close("})");
            }
            FieldType::StructArray(index) => {
                let structure = &self.schema.definition(index).name;
                writer.statement(
                    &STYLE,
                    "let bytes = ",
                    &call(
                        "buffer.get_struct_array",
                        vec![text("position"), text(format!("{structure}::SIZE"))],
                    ),
                    "?;",
                );
                // The bytes are a whole number of structs
                writer.line(format!(
                    "let (structs, _) = bytes.as_chunks::<{{ {structure}::SIZE }}>();"
                ));
                writer.blank();
                writer.line(format!("Ok({view} {{ structs }})"));
            }
            _ => unreachable!("only vectors and arrays of structs have views"),
        }
        writer.close("}");
    }

    fn value_type(&self, field_type: FieldType) -> String {
        match field_type {
            FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
            FieldType::Array(Scalar::Uint8) => "&'a [u8]".to_owned(),
            FieldType::Array(scalar) => format!("ArrayView<'a, {}>", scalar_type(scalar)),
            FieldType::String => "&'a str".to_owned(),
            FieldType::Struct(index) => self.schema.definition(index).name.clone(),
            FieldType::StructArray(index) => format!(
                "impl ExactSizeIterator<Item = {}> + use<'a>",
                self.schema.definition(index).name
            ),
            FieldType::Table(index) => format!("{}<'a>", self.schema.definition(index).name),
            FieldType::Vector(index) => format!("Vec<{}<'a>>", self.schema.definition(index).name),
        }
    }

    fn input_type(&self, field_type: FieldType) -> String {
        let fields = |index: usize| {
            let definition = self.schema.definition(index);
            let generics = if self.borrows[index] { "<'a>" } else { "" };
            format!("{}Fields{generics}", definition.name)
        };

        match field_type {
            FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
            FieldType::Array(scalar) => format!("&'a [{}]", scalar_type(scalar)),
            FieldType::String => "&'a str".to_owned(),
            FieldType::Struct(index) => self.schema.definition(index).name.clone(),
            FieldType::StructArray(index) => {
                format!("&'a [{}]", self.schema.definition(index).name)
            }
            FieldType::Table(index) => fields(index),
            FieldType::Vector(index) => format!("&'a [{}]", fields(index)),
        }
    }
}

/// Whether each definition's fields struct borrows strings or arrays, which
/// it does if it holds any, or nests a table that does. Each definition is
/// visited once, from a stack rather than by recursion, so that neither
/// tables nested in many fields nor long chains of them take long.
fn borrowing(schema: &Schema) -> Vec<bool> {
    let definitions = &schema.definitions;
    let mut borrows: Vec<Option<bool>> = vec![None; definitions.len()];
    let mut stack = Vec::new();

    for root in 0..definitions.len() {
        stack.push(root);
        while let Some(&index) = stack.last() {
            if borrows[index].is_some() {
                stack.pop();
                continue;
            }

            // Nested tables come first, and the checker rejects cycles of them
            let unknown: Vec<usize> = nested_tables(&definitions[index])
                .filter(|&nested| borrows[nested].is_none())
                .collect();
            if !unknown.is_empty() {
                stack.extend(unknown);
                continue;
            }

            let borrowing = definitions[index]
                .fields
                .iter()
                .any(|field| match field.field_type {
                    FieldType::Scalar(_) | FieldType::Struct(_) => false,
                    FieldType::Array(_)
                    | FieldType::String
                    | FieldType::StructArray(_)
                    | FieldType::Vector(_) => true,
                    FieldType::Table(nested) => borrows[nested] == Some(true),
                });
            borrows[index] = Some(borrowing);
            stack.pop();
        }
    }

    borrows.into_iter().map(Option::unwrap_or_default).collect()
}

fn nested_tables(definition: &Definition) -> impl Iterator<Item = usize> + '_ {
    definition
        .fields
        .iter()
        .filter_map(|field| match field.field_type {
            FieldType::Table(index) => Some(index),
            _ => None,
        })
}

/// The layout constant, broken as rustfmt breaks a long chain
fn layout_constant(writer: &mut CodeWriter, layout: Layout, generics: &str) {
    let mut calls = Vec::new();
    if layout.sized {
        calls.push(".sized()".to_owned());
    }
    if layout.version.is_some() {
        calls.push(".versioned()".to_owned());
    }
    match layout.offset_type {
        OffsetType::Uint8 => calls.push(".with_offset_type(OffsetType::Uint8)".to_owned()),
        OffsetType::Uint16 => calls.push(".with_offset_type(OffsetType::Uint16)".to_owned()),
        OffsetType::Uint32 => {}
    }

    let prefix = format!("const LAYOUT: TableLayout{generics} = ");
    let chain = format!("TableLayout::DEFAULT{}", calls.concat());
    if chain.len() <= CHAIN_WIDTH {
        writer.line(format!("{prefix}{chain};"));
        return;
    }

    writer.line(format!("{prefix}TableLayout::DEFAULT"));
    writer.indent();
    let last = calls.len() - 1;
    for (index, call) in calls.iter().enumerate() {
        writer.line(if index == last {
            format!("{call};")
        } else {
            call.clone()
        });
    }
    writer.dedent();
}

/// The view of a vector of a table, which reads each item when it's asked
/// for, from the item's end and the end of the item before it
fn vector_view(writer: &mut CodeWriter, definition: &Definition) {
    let table = &definition.name;
    let item = format!("Result<{table}<'a>, DecodeError>");

    rust_doc(
        writer,
        &[format!(
            "A vector of [`{table}`]s, which reads each item when it's asked for"
        )],
    );
    writer.line("#[derive(Clone, Copy, Debug)]");
    writer.open(format!("pub struct {table}Vector<'a> {{"));
    writer.line("vector: Vector<'a>,");
    writer.close("}");
    writer.blank();

    writer.open(format!("impl<'a> {table}Vector<'a> {{"));
    view_length(writer, "vector", "items");

    let mut docs = vec![
        "Reads item `index`, without reading the others, or returns `None` past the end".to_owned(),
    ];
    docs.extend(error_doc(
        "`Some` of a [`DecodeError`]",
        "if the item lies outside the bytes",
    ));
    rust_doc(writer, &docs);
    open_method(
        writer,
        "pub fn get",
        &["&self", "index: usize"],
        &format!("Option<{item}>"),
    );
    writer.line("self.vector.get_in_place(index).map(Self::item)");
    writer.close("}");
    writer.blank();

    rust_doc(
        writer,
        &["Iterates over the items, reading each item's end once".to_owned()],
    );
    open_method(
        writer,
        "pub fn iter",
        &["&self"],
        &format!("impl ExactSizeIterator<Item = {item}> + use<'a>"),
    );
    writer.line("self.vector.iter_in_place().map(Self::item)");
    writer.close("}");
    writer.blank();

    rust_doc(
        writer,
        &["Reads an item in place, from its bytes and those after it in the message".to_owned()],
    );
    open_method(
        writer,
        "fn item",
        &["bytes: Result<&'a [u8], DecodeError>"],
        &item,
    );
    writer.line(format!("Ok({table}::new(bytes?, 0))"));
    writer.close("}");
    writer.close("}");
}

/// The view of an array of a struct, which reads each struct when it's
/// asked for, from their bytes
fn struct_array_view(writer: &mut CodeWriter, definition: &Definition) {
    let structure = &definition.name;

    rust_doc(
        writer,
        &[format!(
            "An array of [`{structure}`]s, which reads each struct when it's asked for"
        )],
    );
    writer.line("#[derive(Clone, Copy, Debug)]");
    writer.open(format!("pub struct {structure}Array<'a> {{"));
    writer.line(format!("structs: &'a [[u8; {structure}::SIZE]],"));
    writer.close("}");
    writer.blank();

    writer.open(format!("impl<'a> {structure}Array<'a> {{"));
    view_length(writer, "structs", "structs");

    rust_doc(
        writer,
        &["Reads struct `index`, or returns `None` past the end".to_owned()],
    );
    open_method(
        writer,
        "pub fn get",
        &["&self", "index: usize"],
        &format!("Option<{structure}>"),
    );
    chain(
        writer,
        "self.structs",
        &["get(index)", &format!("map({structure}::from_bytes)")],
    );
    writer.close("}");
    writer.blank();

    rust_doc(writer, &["Iterates over the structs".to_owned()]);
    open_method(
        writer,
        "pub fn iter",
        &["&self"],
        &format!("impl ExactSizeIterator<Item = {structure}> + use<'a>"),
    );
    chain(
        writer,
        "self.structs",
        &["iter()", &format!("map({structure}::from_bytes)")],
    );
    writer.close("}");
    writer.close("}");
}

/// Opens a method of an `impl` block, with its parameters on lines of their
/// own when its signature is wider than rustfmt's `max_width`, as rustfmt
/// breaks it
fn open_method(writer: &mut CodeWriter, head: &str, parameters: &[&str], result: &str) {
    let signature = format!("{head}({}) -> {result} {{", parameters.join(", "));
    if STYLE.indent.len() + signature.len() <= STYLE.width {
        writer.open(signature);
        return;
    }

    writer.line(format!("{head}("));
    writer.indent();
    for parameter in parameters {
        writer.line(format!("{parameter},"));
    }
    writer.dedent();
    writer.open(format!(") -> {result} {{"));
}

/// The calls of a chain on `root`, on one line, or each on a line of its own
/// when the chain is wider than rustfmt's `chain_width`
fn chain(writer: &mut CodeWriter, root: &str, calls: &[&str]) {
    let line = format!("{root}.{}", calls.join("."));
    if line.len() <= CHAIN_WIDTH {
        writer.line(line);
        return;
    }

    writer.line(root);
    writer.indent();
    for call in calls {
        writer.line(format!(".{call}"));
    }
    writer.dedent();
}

/// A view's `len()` and `is_empty()`, of the items that `field` holds
fn view_length(writer: &mut CodeWriter, field: &str, items: &str) {
    rust_doc(writer, &[format!("The number of {items}")]);
    writer.open("pub const fn len(&self) -> usize {");
    writer.line(format!("self.{field}.len()"));
    writer.close("}");
    writer.blank();

    writer.open("pub const fn is_empty(&self) -> bool {");
    writer.line(format!("self.{field}.is_empty()"));
    writer.close("}");
    writer.blank();
}

fn rust_doc(writer: &mut CodeWriter, docs: &[String]) {
    for line in docs {
        writer.line(format!("/// {line}").trim_end());
    }
}

fn errors_doc(condition: &str) -> Vec<String> {
    error_doc("a [`DecodeError`]", condition)
}

fn error_doc(error: &str, condition: &str) -> Vec<String> {
    vec![
        String::new(),
        "# Errors".to_owned(),
        String::new(),
        format!("Returns {error} {condition}."),
    ]
}

fn identifier(name: &str) -> String {
    if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else if PATH_KEYWORDS.contains(&name) || MEMBERS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_owned()
    }
}

/// An integer literal, with its digits in groups of three when it has more
/// than four, as clippy's `unreadable_literal` asks
fn integer(value: &impl Display) -> String {
    let digits = value.to_string();
    if digits.len() <= 4 {
        return digits;
    }

    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push('_');
        }
        grouped.push(digit);
    }
    grouped
}

/// "a [`Name`]" or "an [`Name`]", the name linked in documentation
fn article(name: &str) -> String {
    format!("{} [`{name}`]", indefinite_article(name))
}

/// The Rust type, which also names the crate's getters and setters, but for
/// `bool`'s, which [`method_type()`] names
const fn scalar_type(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::Bool => "bool",
        Scalar::Int8 => "i8",
        Scalar::Uint8 => "u8",
        Scalar::Int16 => "i16",
        Scalar::Uint16 => "u16",
        Scalar::Int32 => "i32",
        Scalar::Uint32 => "u32",
        Scalar::Int64 => "i64",
        Scalar::Uint64 => "u64",
        Scalar::Float32 => "f32",
        Scalar::Float64 => "f64",
    }
}

/// The type that names the crate's getters and setters of a scalar: `u8` for a
/// bool, which a `u8` stores
const fn method_type(scalar: Scalar) -> &'static str {
    scalar_type(scalar.stored())
}

/// The variant of the runtime's `Alignment` of `bytes`
fn alignment_variant(bytes: u8) -> &'static str {
    match bytes {
        1 => "One",
        2 => "Two",
        4 => "Four",
        _ => "Eight",
    }
}
