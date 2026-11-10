//! TypeScript, and JavaScript with `JSDoc` types, for `tafla`. Messages
//! and tables are classes that read fields lazily from their bytes, and are
//! written from interfaces of their fields. Structs are read eagerly.

use super::prose::article;
use super::writer::{
    CodeWriter, Expr, Style, call, comment_text, doc_comment, escape_outside_code_spans, text,
};
use super::{GeneratedFile, Options};
use crate::model::{Definition, Field, FieldType, Kind, Layout, OffsetType, Scalar, Schema};
use crate::naming::camel_case;

/// The runtime's module, unless [`Options::runtime`] names another
const RUNTIME: &str = "tafla";

const STYLE: Style = Style {
    width: 80,
    indent: "    ",
    trailing_comma: false,
    hug_arguments: false,
    max_arguments_width: None,
};

/// Members that classes define or inherit, which names take a `_` after: the
/// constructor and the other methods of `Object.prototype`, which a getter
/// would replace, and `then`, which would make a reader a thenable that
/// `await` and `Promise.resolve()` call and never settle
const MEMBERS: [&str; 8] = [
    "constructor",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "then",
    "toLocaleString",
    "toString",
    "valueOf",
];

/// Where a message's generated serializer writes it
#[derive(Clone, Copy)]
enum Output {
    /// Into new bytes, as `serialize()` does
    New,
    /// Into the caller's bytes, as `serializeInto()` does
    Into,
}

pub fn generate(
    schema: &Schema,
    stem: &str,
    header: &str,
    options: &Options,
    typed: bool,
) -> Vec<GeneratedFile> {
    let generator = Generator { schema, typed };
    let runtime = options.runtime.as_deref().unwrap_or(RUNTIME);

    let mut writer = CodeWriter::new(STYLE.indent);
    writer.line(format!("// {header}"));
    writer.blank();
    generator.imports(&mut writer, runtime);

    for (index, definition) in schema.definitions.iter().enumerate() {
        writer.blank();
        match definition.kind {
            Kind::Message(layout) => generator.message(&mut writer, definition, layout),
            Kind::Table => generator.table(&mut writer, index),
            Kind::Struct(_) => generator.structure(&mut writer, definition),
        }
        writer.blank();
        generator.fields_type(&mut writer, definition);

        if schema.is_item(index) {
            writer.blank();
            if matches!(definition.kind, Kind::Struct(_)) {
                generator.struct_array_view(&mut writer, definition);
            } else {
                generator.vector_view(&mut writer, definition);
            }
        }
    }

    let extension = if typed { "ts" } else { "js" };
    vec![GeneratedFile {
        path: format!("{}.{extension}", camel_case(stem)),
        contents: writer.finish(),
    }]
}

struct Generator<'a> {
    schema: &'a Schema,
    /// TypeScript, rather than JavaScript with `JSDoc`
    typed: bool,
}

impl Generator<'_> {
    fn imports(&self, writer: &mut CodeWriter, runtime: &str) {
        let kinds = |predicate: fn(&Kind) -> bool| {
            self.schema
                .definitions
                .iter()
                .any(|definition| predicate(&definition.kind))
        };
        let has_message = kinds(|kind| matches!(kind, Kind::Message(_)));
        let has_nested = kinds(|kind| matches!(kind, Kind::Table));
        let has_table = has_message || has_nested;
        let has_struct = kinds(|kind| matches!(kind, Kind::Struct(_)));
        let versioned = kinds(|kind| {
            matches!(
                kind,
                Kind::Message(Layout {
                    version: Some(_),
                    ..
                })
            )
        });

        let readers = self.struct_scalar_functions("read");
        let writers = self.struct_scalar_functions("write");

        // (name, whether it's only a type here)
        let mut names = Vec::new();
        if has_table {
            names.push(("ByteBuffer", !has_message));
        }
        if versioned {
            names.push(("DecodeError", false));
        }
        names.extend(readers.iter().map(|reader| (reader.as_str(), false)));
        if has_struct {
            names.push(("StructWriter", false));
        }
        if has_message {
            names.push(("TableLayout", false));
        }
        // Messages make sizers, and tables take them and their writers
        if has_table {
            names.push(("TableSizer", !has_message));
        }
        if has_nested {
            names.push(("TableWriter", true));
        }
        names.extend(writers.iter().map(|writer| (writer.as_str(), false)));

        let mut values = Vec::new();
        let mut types = Vec::new();
        for (name, type_only) in names {
            match (type_only, self.typed) {
                (false, _) => values.push(name.to_owned()),
                (true, true) => values.push(format!("type {name}")),
                (true, false) => types.push(name),
            }
        }

        if !types.is_empty() {
            writer.line(format!(
                "/** @import {{ {} }} from '{runtime}' */",
                types.join(", ")
            ));
        }
        // A schema without definitions imports nothing
        if values.is_empty() {
            return;
        }
        let statement = format!("import {{ {} }} from '{runtime}';", values.join(", "));
        if statement.len() <= STYLE.width {
            writer.line(statement);
        } else {
            writer.open("import {");
            let last = values.len() - 1;
            for (index, value) in values.iter().enumerate() {
                writer.line(if index < last {
                    format!("{value},")
                } else {
                    value.clone()
                });
            }
            writer.close(format!("}} from '{runtime}';"));
        }
    }

    /// The functions that read or write the scalars of structs, such as
    /// `readUint32` or `writeUint32`, in the order that the runtime exports
    /// them
    fn struct_scalar_functions(&self, verb: &str) -> Vec<String> {
        // A bool is read and written as the uint8 that stores it
        let mut scalars: Vec<Scalar> = Scalar::ALL
            .into_iter()
            .filter(|&scalar| {
                self.schema
                    .definitions
                    .iter()
                    .filter(|definition| definition.is_struct())
                    .flat_map(|definition| &definition.fields)
                    .any(|field| match field.field_type {
                        FieldType::Scalar(field_scalar) => field_scalar.stored() == scalar,
                        _ => false,
                    })
            })
            .collect();
        // `float32` before `float64`, `int8` before `int16`, and so on
        scalars.sort_by_key(|scalar| {
            let name = scalar.name();
            let digits = name
                .find(|c: char| c.is_ascii_digit())
                .unwrap_or(name.len());
            (&name[..digits], scalar.size())
        });

        scalars
            .into_iter()
            .map(|scalar| format!("{verb}{}", scalar_method(scalar)))
            .collect()
    }

    fn message(&self, writer: &mut CodeWriter, definition: &Definition, layout: Layout) {
        let name = &definition.name;

        writer.lines(&doc_comment(&schema_docs(&definition.docs)));
        writer.open(format!("export class {name} {{"));

        if let Some(version) = layout.version {
            writer.lines(&doc_comment(&[
                "The version of the format, which the header carries and the".to_owned(),
                "constructor checks".to_owned(),
            ]));
            writer.line(format!(
                "{}FORMAT_VERSION = {version};",
                self.static_readonly()
            ));
            writer.blank();
        }

        writer.statement(
            &STYLE,
            &format!("{}#LAYOUT = ", self.static_readonly()),
            &layout_expression(layout),
            ";",
        );
        writer.blank();
        self.private_field(writer, "buffer", "ByteBuffer");
        writer.blank();

        let mut tags = vec!["@param {Uint8Array} bytes".to_owned()];
        if layout.version.is_some() {
            tags.push("@throws {DecodeError} If `bytes` are in another format version.".to_owned());
        }
        self.doc(writer, &[], &tags);
        writer.open(format!(
            "constructor(bytes{}) {{",
            self.annotation("Uint8Array")
        ));
        writer.statement(
            &STYLE,
            "this.#buffer = ",
            &call(
                "new ByteBuffer",
                vec![text("bytes"), text(format!("{name}.#LAYOUT"))],
            ),
            ";",
        );
        if layout.version.is_some() {
            writer.blank();
            writer.line("const version = this.#buffer.getVersion();");
            writer.open(format!("if (version !== {name}.FORMAT_VERSION) {{"));
            let message = format!(
                "`Expected format version ${{{name}.FORMAT_VERSION}} of {name}, got ${{version}}`"
            );
            writer.statement(
                &STYLE,
                "throw ",
                &call("new DecodeError", vec![text(message)]),
                ";",
            );
            writer.close("}");
        }
        writer.close("}");
        writer.blank();

        if layout.version.is_some() {
            self.version_of(writer, name);
            writer.blank();
        }

        self.serializer(writer, definition, layout, Output::New);
        writer.blank();
        self.serializer(writer, definition, layout, Output::Into);

        self.getters(writer, definition, true);
        writer.close("}");
    }

    /// A message's `serialize()`, which writes it into new bytes, or its
    /// `serializeInto()`, which writes it into the caller's. Each has a body
    /// of its own, rather than calling one that both share, so that adding
    /// `serializeInto()` leaves `serialize()`'s code, and how V8 inlines the
    /// writers' calls into it, as it was.
    fn serializer(
        &self,
        writer: &mut CodeWriter,
        definition: &Definition,
        layout: Layout,
        output: Output,
    ) {
        let name = &definition.name;
        let fields = ("fields", format!("{name}Fields"));

        match output {
            Output::New => {
                self.doc(
                    writer,
                    &[format!("Writes a `{name}` with every field")],
                    &[
                        format!("@param {{{name}Fields}} fields"),
                        "@returns {Uint8Array}".to_owned(),
                    ],
                );
                self.signature(writer, "static serialize", &[fields], "Uint8Array");
            }
            Output::Into => {
                self.doc(
                    writer,
                    &[
                        format!("Writes a `{name}` with every field into the start of `bytes`,"),
                        "rather than into new bytes, and returns the view of them that holds"
                            .to_owned(),
                        "it. The view, and what's read from it, change when the bytes are"
                            .to_owned(),
                        "written again. Reusing bytes saves allocating and zeroing new ones,"
                            .to_owned(),
                        "which takes longest for large tables.".to_owned(),
                    ],
                    &[
                        format!("@param {{{name}Fields}} fields"),
                        "@param {Uint8Array} bytes".to_owned(),
                        "@returns {Uint8Array}".to_owned(),
                        "@throws {RangeError} If `bytes` are shorter than the table, before"
                            .to_owned(),
                        "any of them is written.".to_owned(),
                    ],
                );
                self.signature(
                    writer,
                    "static serializeInto",
                    &[fields, ("bytes", "Uint8Array".to_owned())],
                    "Uint8Array",
                );
            }
        }

        let mut arguments = vec![
            text(definition.fields.len().to_string()),
            text(format!("{name}.#LAYOUT")),
        ];
        if layout.version.is_some() {
            arguments.push(text(format!("{name}.FORMAT_VERSION")));
        }
        writer.statement(
            &STYLE,
            "const sizer = ",
            &call("new TableSizer", arguments),
            ";",
        );
        self.sizes(writer, definition);
        writer.blank();
        writer.line(match output {
            Output::New => "const writer = sizer.writer();",
            Output::Into => "const writer = sizer.writer(bytes);",
        });
        self.writes(writer, definition);
        writer.blank();
        writer.line("return writer.bytes();");
        writer.close("}");
    }

    fn table(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
        let name = &definition.name;

        writer.lines(&doc_comment(&schema_docs(&definition.docs)));
        writer.open(format!("export class {name} {{"));
        // A static of JavaScript is a number, rather than the literal that
        // addTable() and setTable() take, without its type
        let alignment = self.schema.table_alignment(index);
        self.doc(
            writer,
            &[
                format!(
                    "The alignment that a parent pads {} to: the largest",
                    article(name)
                ),
                "alignment of its arrays and nested tables, or 1".to_owned(),
            ],
            &[format!("@type {{{alignment}}}")],
        );
        writer.line(format!(
            "{}ALIGNMENT = {alignment};",
            self.static_readonly()
        ));
        writer.blank();
        self.private_field(writer, "buffer", "ByteBuffer");
        self.private_field(writer, "position", "number");
        writer.blank();

        self.doc(
            writer,
            &["Reads the table at `position`, as its parent's getter finds it".to_owned()],
            &[
                "@param {ByteBuffer} buffer".to_owned(),
                "@param {number} position".to_owned(),
            ],
        );
        writer.open(format!(
            "constructor(buffer{}, position{}) {{",
            self.annotation("ByteBuffer"),
            self.annotation("number")
        ));
        writer.line("this.#buffer = buffer;");
        writer.line("this.#position = position;");
        writer.close("}");
        writer.blank();

        self.nested_size_and_write(writer, definition);
        self.getters(writer, definition, false);
        writer.close("}");
    }

    /// The static functions with which a parent sizes and writes a nested
    /// table
    fn nested_size_and_write(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;
        let fields = definition.fields.len();
        // The length of a table of scalars and structs doesn't depend on
        // their values
        let sized_by_values = definition.fields.iter().any(|field| {
            !matches!(
                field.field_type,
                FieldType::Scalar(_) | FieldType::Struct(_)
            )
        });
        let values = if sized_by_values { "fields" } else { "_fields" };
        self.doc(
            writer,
            &[
                format!(
                    "Adds {} to its parent's sizer, after the field or",
                    article(name)
                ),
                "vector that holds it".to_owned(),
            ],
            &[
                "@param {TableSizer} sizer".to_owned(),
                format!("@param {{{name}Fields}} {values}"),
                "@returns {void}".to_owned(),
            ],
        );
        writer.open(format!(
            "static size(sizer{}, {values}{}){} {{",
            self.annotation("TableSizer"),
            self.annotation(&format!("{name}Fields")),
            self.annotation("void")
        ));
        writer.line(format!("const parent = sizer.startTable({fields});"));
        self.sizes(writer, definition);
        writer.line("sizer.endTable(parent);");
        writer.close("}");
        writer.blank();

        self.doc(
            writer,
            &[
                format!(
                    "Writes {} where its parent's writer is, after the",
                    article(name)
                ),
                "field or vector that holds it".to_owned(),
            ],
            &[
                "@param {TableWriter} writer".to_owned(),
                format!("@param {{{name}Fields}} fields"),
                "@returns {void}".to_owned(),
            ],
        );
        writer.open(format!(
            "static write(writer{}, fields{}){} {{",
            self.annotation("TableWriter"),
            self.annotation(&format!("{name}Fields")),
            self.annotation("void")
        ));
        writer.line(format!("const parent = writer.startTable({fields});"));
        self.writes(writer, definition);
        writer.line("writer.endTable(parent);");
        writer.close("}");
    }

    #[allow(clippy::too_many_lines)]
    fn structure(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;

        writer.lines(&doc_comment(&schema_docs(&definition.docs)));
        writer.open(format!("export class {name} {{"));
        writer.lines(&doc_comment(&[format!("The byte length of a `{name}`")]));
        writer.line(format!(
            "{}SIZE = {};",
            self.static_readonly(),
            definition.struct_layout().size
        ));
        writer.blank();
        // A static of JavaScript is a number, rather than the literal that
        // setStructArray() takes, without its type
        let alignment = definition.struct_layout().alignment;
        self.doc(
            writer,
            &[format!(
                "The alignment of an array of `{name}`: its largest scalar's size"
            )],
            &[format!("@type {{{alignment}}}")],
        );
        writer.line(format!(
            "{}ALIGNMENT = {alignment};",
            self.static_readonly()
        ));
        writer.blank();
        for field in &definition.fields {
            self.private_field(
                writer,
                &identifier(&field.name),
                &value_type(field.field_type, self.schema),
            );
        }
        writer.blank();

        self.doc(
            writer,
            &[
                format!("Reads a `{name}` at `offset` in `bytes`, eagerly, so that it"),
                "doesn't keep them alive".to_owned(),
            ],
            &[
                "@param {Uint8Array} bytes".to_owned(),
                "@param {number} [offset]".to_owned(),
                format!("@throws {{DecodeError}} If `bytes` end before the `{name}` does."),
            ],
        );
        writer.open(format!(
            "constructor(bytes{}, offset = 0) {{",
            self.annotation("Uint8Array")
        ));
        // The runtime's read functions, which check each read, skip making a
        // ByteBuffer and its DataView, which took five times as long
        let mut offset = 0;
        for field in &definition.fields {
            let position = struct_position(offset);
            let target = format!("this.#{} = ", identifier(&field.name));
            let (read, size) = match field.field_type {
                FieldType::Scalar(Scalar::Bool) => {
                    let read = call("readUint8", vec![text("bytes"), position]);
                    assign_bool(writer, &target, &read);
                    offset += 1;
                    continue;
                }
                FieldType::Scalar(scalar) => (
                    call(
                        format!("read{}", scalar_method(scalar)),
                        vec![text("bytes"), position],
                    ),
                    usize::from(scalar.size()),
                ),
                FieldType::Struct(index) => {
                    let inner = self.schema.definition(index);
                    (
                        call(format!("new {}", inner.name), vec![text("bytes"), position]),
                        inner.struct_layout().size,
                    )
                }
                _ => unreachable!("structs hold scalars and structs only"),
            };
            writer.statement(&STYLE, &target, &read, ";");
            offset += size;
        }
        writer.close("}");
        writer.blank();

        self.doc(
            writer,
            &[format!("Writes a `{name}`")],
            &[
                format!("@param {{{name}Fields}} fields"),
                "@returns {Uint8Array}".to_owned(),
            ],
        );
        // Each call writes into new bytes with the runtime's write functions,
        // which it returns, rather than with a StructWriter that every call
        // shares, whose bytes it would copy. A call from within another, from
        // a getter of its fields or a valueOf() that a write converts, then
        // writes bytes of its own. In Node 26, writing the comparison suites'
        // Sample took 31 ns so, and 41 ns with the shared writer.
        self.signature(
            writer,
            "static serialize",
            &[("fields", format!("{name}Fields"))],
            "Uint8Array",
        );
        writer.line(format!("const bytes = new Uint8Array({name}.SIZE);"));
        writer.line(format!("{name}.writeAt(bytes, 0, fields);"));
        writer.blank();
        writer.line("return bytes;");
        writer.close("}");
        writer.blank();

        self.doc(
            writer,
            &[format!(
                "Writes `{name}`s one after another, as an array of them holds them"
            )],
            &[
                format!("@param {{readonly {name}Fields[]}} items"),
                "@returns {Uint8Array}".to_owned(),
            ],
        );
        // A StructWriter's DataView writes each scalar into the bytes of an
        // array, which lie outside V8's heap, faster than the write functions,
        // which check each scalar's bounds themselves: in Node 26, 100 Triples
        // took 780 ns so, and 1.3 µs with the write functions
        self.signature(
            writer,
            "static serializeArray",
            &[("items", format!("readonly {name}Fields[]"))],
            "Uint8Array",
        );
        writer.line(format!(
            "const bytes = new Uint8Array(items.length * {name}.SIZE);"
        ));
        writer.line("const writer = new StructWriter(bytes);");
        writer.open("for (const fields of items) {");
        writer.line(format!("{name}.write(writer, fields);"));
        writer.close("}");
        writer.blank();
        writer.line("return bytes;");
        writer.close("}");
        writer.blank();

        self.doc(
            writer,
            &[format!("Writes a `{name}` at the writer's position")],
            &[
                "@param {StructWriter} writer".to_owned(),
                format!("@param {{{name}Fields}} fields"),
                "@returns {void}".to_owned(),
            ],
        );
        self.signature(
            writer,
            "static write",
            &[
                ("writer", "StructWriter".to_owned()),
                ("fields", format!("{name}Fields")),
            ],
            "void",
        );
        for field in &definition.fields {
            let value = format!("fields.{}", identifier(&field.name));
            match field.field_type {
                FieldType::Scalar(scalar) => {
                    let value = stored_value(scalar, &value);
                    writer.statement(
                        &STYLE,
                        "",
                        &call(format!("writer.set{}", scalar_method(scalar)), vec![value]),
                        ";",
                    );
                }
                FieldType::Struct(index) => {
                    let inner = &self.schema.definition(index).name;
                    writer.line(format!("{inner}.write(writer, {value});"));
                }
                _ => unreachable!("structs hold scalars and structs only"),
            }
        }
        writer.close("}");
        writer.blank();

        self.doc(
            writer,
            &[format!("Writes a `{name}` at `offset` in `bytes`")],
            &[
                "@param {Uint8Array} bytes".to_owned(),
                "@param {number} offset".to_owned(),
                format!("@param {{{name}Fields}} fields"),
                "@returns {void}".to_owned(),
                format!("@throws {{RangeError}} If `bytes` end before the `{name}` does."),
            ],
        );
        self.signature(
            writer,
            "static writeAt",
            &[
                ("bytes", "Uint8Array".to_owned()),
                ("offset", "number".to_owned()),
                ("fields", format!("{name}Fields")),
            ],
            "void",
        );
        let mut offset = 0;
        for field in &definition.fields {
            let position = struct_position(offset);
            let value = format!("fields.{}", identifier(&field.name));
            let (write, size) = match field.field_type {
                FieldType::Scalar(scalar) => (
                    call(
                        format!("write{}", scalar_method(scalar)),
                        vec![text("bytes"), position, stored_value(scalar, &value)],
                    ),
                    usize::from(scalar.size()),
                ),
                FieldType::Struct(index) => {
                    let inner = self.schema.definition(index);
                    (
                        call(
                            format!("{}.writeAt", inner.name),
                            vec![text("bytes"), position, text(value)],
                        ),
                        inner.struct_layout().size,
                    )
                }
                _ => unreachable!("structs hold scalars and structs only"),
            };
            writer.statement(&STYLE, "", &write, ";");
            offset += size;
        }
        writer.close("}");

        for field in &definition.fields {
            let field_name = identifier(&field.name);
            writer.blank();
            self.getter_doc(
                writer,
                &schema_docs(&field.docs),
                &value_type(field.field_type, self.schema),
            );
            writer.open(format!(
                "{field_name}(){} {{",
                self.annotation(&value_type(field.field_type, self.schema))
            ));
            writer.line(format!("return this.#{field_name};"));
            writer.close("}");
        }
        writer.close("}");
    }

    /// The view of a vector of a table, which reads each item when it's
    /// asked for, from the item's end and the end of the item before it
    fn vector_view(&self, writer: &mut CodeWriter, definition: &Definition) {
        let table = &definition.name;

        writer.lines(&doc_comment(&[format!(
            "A vector of `{table}`s, which reads each item when it's asked for"
        )]));
        writer.open(format!("export class {table}Vector {{"));
        self.private_field(writer, "buffer", "ByteBuffer");
        self.private_field(writer, "position", "number");
        self.private_field(writer, "length", "number");
        writer.blank();

        self.doc(
            writer,
            &["Views the vector at `position`, as its parent's getter finds it".to_owned()],
            &[
                "@param {ByteBuffer} buffer".to_owned(),
                "@param {number} position".to_owned(),
                "@throws {DecodeError} If the vector's count or the ends of its items lie"
                    .to_owned(),
                "outside the bytes.".to_owned(),
            ],
        );
        writer.open(format!(
            "constructor(buffer{}, position{}) {{",
            self.annotation("ByteBuffer"),
            self.annotation("number")
        ));
        writer.line("this.#buffer = buffer;");
        writer.line("this.#position = position;");
        writer.line("this.#length = buffer.getVectorLength(position);");
        writer.close("}");
        writer.blank();

        self.view_length(writer, "The number of items");
        writer.blank();

        self.doc(
            writer,
            &["Reads item `index`, without reading the others".to_owned()],
            &[
                "@param {number} index".to_owned(),
                format!("@returns {{{table}}}"),
                "@throws {RangeError} If the vector has no item `index`.".to_owned(),
                "@throws {DecodeError} If the item lies outside the bytes.".to_owned(),
            ],
        );
        writer.open(format!(
            "get(index{}){} {{",
            self.annotation("number"),
            self.annotation(table)
        ));
        // The count that the constructor checked, rather than reading and
        // checking it for each item, as getVectorOffset() would
        let offset = call(
            "this.#buffer.getVectorItemOffset",
            vec![text("this.#position"), text("this.#length"), text("index")],
        );
        writer.statement(
            &STYLE,
            "return ",
            &call(format!("new {table}"), vec![text("this.#buffer"), offset]),
            ";",
        );
        writer.close("}");
        writer.close("}");
    }

    /// The view of an array of a struct, which reads each struct when it's
    /// asked for, from a view of their bytes
    fn struct_array_view(&self, writer: &mut CodeWriter, definition: &Definition) {
        let structure = &definition.name;

        writer.lines(&doc_comment(&[format!(
            "An array of `{structure}`s, which reads each struct when it's asked for"
        )]));
        writer.open(format!("export class {structure}Array {{"));
        self.private_field(writer, "bytes", "Uint8Array");
        self.private_field(writer, "length", "number");
        writer.blank();

        self.doc(
            writer,
            &["Views the array at `position`, as its parent's getter finds it".to_owned()],
            &[
                "@param {ByteBuffer} buffer".to_owned(),
                "@param {number} position".to_owned(),
                "@throws {DecodeError} If the array lies outside the bytes.".to_owned(),
            ],
        );
        writer.open(format!(
            "constructor(buffer{}, position{}) {{",
            self.annotation("ByteBuffer"),
            self.annotation("number")
        ));
        writer.statement(
            &STYLE,
            "this.#bytes = ",
            &call(
                "buffer.getStructArray",
                vec![text("position"), text(format!("{structure}.SIZE"))],
            ),
            ";",
        );
        writer.line(format!(
            "this.#length = this.#bytes.length / {structure}.SIZE;"
        ));
        writer.close("}");
        writer.blank();

        self.view_length(writer, "The number of structs");
        writer.blank();

        self.doc(
            writer,
            &["Reads struct `index`".to_owned()],
            &[
                "@param {number} index".to_owned(),
                format!("@returns {{{structure}}}"),
                "@throws {RangeError} If the array has no struct `index`.".to_owned(),
            ],
        );
        writer.open(format!(
            "get(index{}){} {{",
            self.annotation("number"),
            self.annotation(structure)
        ));
        writer.open("if (!(Number.isInteger(index) && index >= 0 && index < this.#length)) {");
        writer.statement(
            &STYLE,
            "throw ",
            &call(
                "new RangeError",
                vec![text(
                    "`Struct ${index} is outside the array of ${this.#length}`",
                )],
            ),
            ";",
        );
        writer.close("}");
        writer.blank();
        writer.statement(
            &STYLE,
            "return ",
            &call(
                format!("new {structure}"),
                vec![
                    text("this.#bytes"),
                    text(format!("index * {structure}.SIZE")),
                ],
            ),
            ";",
        );
        writer.close("}");
        writer.close("}");
    }

    /// A view's `length()`, which it read when it was made
    fn view_length(&self, writer: &mut CodeWriter, summary: &str) {
        self.getter_doc(writer, &[summary.to_owned()], "number");
        writer.open(format!("length(){} {{", self.annotation("number")));
        writer.line("return this.#length;");
        writer.close("}");
    }

    /// The interface, or `JSDoc` typedef, of the values that write a definition
    fn fields_type(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = format!("{}Fields", definition.name);
        let summary = format!("The fields of a `{}`, to write", definition.name);

        if self.typed {
            writer.lines(&doc_comment(&[summary]));
            writer.open(format!("export interface {name} {{"));
            for field in &definition.fields {
                writer.lines(&doc_comment(&schema_docs(&field.docs)));
                writer.line(format!(
                    "{}: {};",
                    identifier(&field.name),
                    input_type(field.field_type, self.schema)
                ));
            }
            writer.close("}");
        } else {
            let mut lines = vec![
                summary,
                String::new(),
                format!("@typedef {{object}} {name}"),
            ];
            for field in &definition.fields {
                let description = schema_docs(&field.docs).join(" ");
                let property = format!(
                    "@property {{{}}} {} {description}",
                    input_type(field.field_type, self.schema),
                    identifier(&field.name)
                );
                lines.push(property.trim_end().to_owned());
            }
            writer.lines(&doc_comment(&lines));
        }
    }

    /// The calls that add a table's fields to its sizer, in order
    fn sizes(&self, writer: &mut CodeWriter, definition: &Definition) {
        for field in &definition.fields {
            let value = format!("fields.{}", identifier(&field.name));
            let (method, arguments) = match field.field_type {
                FieldType::Scalar(scalar) => (format!("add{}", scalar_method(scalar)), vec![]),
                FieldType::Array(scalar) => (
                    format!("add{}Array", scalar_method(scalar)),
                    vec![text(format!("{value}.length"))],
                ),
                FieldType::String => ("addString".to_owned(), vec![text(value)]),
                FieldType::Struct(index) => {
                    let structure = &self.schema.definition(index).name;
                    (
                        "addStruct".to_owned(),
                        vec![text(format!("{structure}.SIZE"))],
                    )
                }
                FieldType::StructArray(index) => {
                    let structure = &self.schema.definition(index).name;
                    (
                        "addStructArray".to_owned(),
                        vec![
                            text(format!("{value}.length")),
                            text(format!("{structure}.SIZE")),
                            text(format!("{structure}.ALIGNMENT")),
                        ],
                    )
                }
                FieldType::Table(index) => {
                    let table = &self.schema.definition(index).name;
                    writer.line(format!("sizer.addTable({table}.ALIGNMENT);"));
                    writer.statement(
                        &STYLE,
                        "",
                        &call(format!("{table}.size"), vec![text("sizer"), text(value)]),
                        ";",
                    );
                    continue;
                }
                FieldType::Vector(index) => {
                    let table = &self.schema.definition(index).name;
                    writer.line(format!("sizer.addVector({value}.length);"));
                    writer.open(format!("for (const item of {value}) {{"));
                    writer.line(format!("{table}.size(sizer, item);"));
                    writer.close("}");
                    continue;
                }
            };
            writer.statement(&STYLE, "", &call(format!("sizer.{method}"), arguments), ";");
        }
    }

    /// The calls that write a table's fields, in order
    fn writes(&self, writer: &mut CodeWriter, definition: &Definition) {
        for (index, field) in definition.fields.iter().enumerate() {
            let value = format!("fields.{}", identifier(&field.name));
            let position = text(index.to_string());
            let (method, arguments) = match field.field_type {
                FieldType::Scalar(scalar) => (
                    format!("set{}", scalar_method(scalar)),
                    vec![position, stored_value(scalar, &value)],
                ),
                FieldType::Array(scalar) => (
                    format!("set{}Array", scalar_method(scalar)),
                    vec![position, text(value)],
                ),
                FieldType::String => ("setString".to_owned(), vec![position, text(value)]),
                FieldType::Struct(struct_index) => {
                    let structure = &self.schema.definition(struct_index).name;
                    (
                        "setStruct".to_owned(),
                        vec![
                            position,
                            call(format!("{structure}.serialize"), vec![text(value)]),
                        ],
                    )
                }
                FieldType::StructArray(struct_index) => {
                    let structure = &self.schema.definition(struct_index).name;
                    (
                        "setStructArray".to_owned(),
                        vec![
                            position,
                            call(format!("{structure}.serializeArray"), vec![text(value)]),
                            text(format!("{structure}.SIZE")),
                            text(format!("{structure}.ALIGNMENT")),
                        ],
                    )
                }
                FieldType::Table(table_index) => {
                    let table = &self.schema.definition(table_index).name;
                    writer.line(format!("writer.setTable({index}, {table}.ALIGNMENT);"));
                    writer.statement(
                        &STYLE,
                        "",
                        &call(format!("{table}.write"), vec![text("writer"), text(value)]),
                        ";",
                    );
                    continue;
                }
                FieldType::Vector(table_index) => {
                    // The vector's local is named after the field's index,
                    // since its name may be a reserved word
                    let table = &self.schema.definition(table_index).name;
                    let vector = format!("vector{index}");
                    writer.statement(
                        &STYLE,
                        &format!("const {vector} = "),
                        &call(
                            "writer.setVector",
                            vec![position, text(format!("{value}.length"))],
                        ),
                        ";",
                    );
                    writer.open(format!("for (let i = 0; i < {value}.length; i++) {{"));
                    writer.line(format!("{table}.write(writer, {value}[i]);"));
                    writer.line(format!("writer.setItemEnd({vector}, i);"));
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
        for (index, field) in definition.fields.iter().enumerate() {
            let offset = if is_root {
                call("this.#buffer.rootOffset", vec![text(index.to_string())])
            } else {
                call(
                    "this.#buffer.fieldOffset",
                    vec![text("this.#position"), text(index.to_string())],
                )
            };
            let return_type = value_type(field.field_type, self.schema);
            let field_name = identifier(&field.name);
            // A view of a vector or an array of structs follows its getter
            let view = view_type(field.field_type, self.schema).map(|view| (view, offset.clone()));

            writer.blank();
            self.getter_doc(writer, &schema_docs(&field.docs), &return_type);
            writer.open(format!(
                "{field_name}(){} {{",
                self.annotation(&return_type)
            ));

            match field.field_type {
                FieldType::Scalar(scalar) => {
                    let get = call(
                        format!("this.#buffer.get{}", scalar_method(scalar)),
                        vec![offset],
                    );
                    if scalar == Scalar::Bool {
                        return_bool(writer, &get);
                    } else {
                        writer.statement(&STYLE, "return ", &get, ";");
                    }
                }
                FieldType::Array(scalar) => {
                    let get = call(
                        format!("this.#buffer.get{}Array", scalar_method(scalar)),
                        vec![offset],
                    );
                    writer.statement(&STYLE, "return ", &get, ";");
                }
                FieldType::String => {
                    writer.statement(
                        &STYLE,
                        "return ",
                        &call("this.#buffer.getString", vec![offset]),
                        ";",
                    );
                }
                FieldType::Struct(index) => {
                    let structure = &self.schema.definition(index).name;
                    let read = call(
                        format!("new {structure}"),
                        vec![text("this.#buffer.bytes()"), offset],
                    );
                    writer.statement(&STYLE, "return ", &read, ";");
                }
                FieldType::StructArray(index) => {
                    let structure = &self.schema.definition(index).name;
                    self.struct_array_getter(writer, offset, structure);
                }
                FieldType::Table(table) => {
                    let table = &self.schema.definition(table).name;
                    let read = call(format!("new {table}"), vec![text("this.#buffer"), offset]);
                    writer.statement(&STYLE, "return ", &read, ";");
                }
                FieldType::Vector(table) => {
                    let table = &self.schema.definition(table).name;
                    let array = if self.typed {
                        format!("new Array<{table}>(offsets.length)")
                    } else {
                        "new Array(offsets.length)".to_owned()
                    };

                    writer.statement(
                        &STYLE,
                        "const offsets = ",
                        &call("this.#buffer.getVectorOffsets", vec![offset]),
                        ";",
                    );
                    // The locals aren't named after the field, whose name may
                    // be a reserved word
                    writer.line(format!("const items = {array};"));
                    writer.blank();
                    writer.open("for (let i = 0; i < offsets.length; i++) {");
                    writer.statement(
                        &STYLE,
                        "items[i] = ",
                        &call(
                            format!("new {table}"),
                            vec![text("this.#buffer"), text("offsets[i]")],
                        ),
                        ";",
                    );
                    writer.close("}");
                    writer.blank();
                    writer.line("return items;");
                }
            }

            writer.close("}");

            if let Some((view, offset)) = view {
                self.view_getter(writer, field, &field_name, &view, offset);
            }
        }
    }

    /// The getter of a view of a vector or an array of structs, which reads
    /// each item when it's asked for, rather than all of them as the
    /// field's own getter does
    fn view_getter(
        &self,
        writer: &mut CodeWriter,
        field: &Field,
        field_name: &str,
        view: &str,
        offset: Expr,
    ) {
        let mut docs = schema_docs(&field.docs);
        if !docs.is_empty() {
            docs.push(String::new());
        }
        docs.push(format!(
            "Views the items of `{field_name}()`, reading each one when it's asked for"
        ));

        writer.blank();
        self.getter_doc(writer, &docs, view);
        writer.open(format!(
            "{}View(){} {{",
            camel_case(&field.name),
            self.annotation(view)
        ));
        writer.statement(
            &STYLE,
            "return ",
            &call(format!("new {view}"), vec![text("this.#buffer"), offset]),
            ";",
        );
        writer.close("}");
    }

    /// The body of a getter of an array of structs, which reads each struct
    /// eagerly from a view of their bytes
    fn struct_array_getter(&self, writer: &mut CodeWriter, offset: Expr, structure: &str) {
        let array = if self.typed {
            format!("new Array<{structure}>(bytes.length / {structure}.SIZE)")
        } else {
            format!("new Array(bytes.length / {structure}.SIZE)")
        };

        writer.statement(
            &STYLE,
            "const bytes = ",
            &call(
                "this.#buffer.getStructArray",
                vec![offset, text(format!("{structure}.SIZE"))],
            ),
            ";",
        );
        writer.statement(&STYLE, "const items = ", &text(array), ";");
        writer.blank();
        writer.open("for (let i = 0; i < items.length; i++) {");
        writer.statement(
            &STYLE,
            "items[i] = ",
            &call(
                format!("new {structure}"),
                vec![text("bytes"), text(format!("i * {structure}.SIZE"))],
            ),
            ";",
        );
        writer.close("}");
        writer.blank();
        writer.line("return items;");
    }

    /// A getter's documentation, with its return type in JavaScript
    fn getter_doc(&self, writer: &mut CodeWriter, docs: &[String], return_type: &str) {
        if self.typed {
            writer.lines(&doc_comment(docs));
            return;
        }

        // `@returns {type} description` on one line when it fits
        if let [line] = docs {
            let folded = format!("/** @returns {{{return_type}}} {} */", comment_text(line));
            if writer_fits(writer, &folded) {
                writer.line(folded);
                return;
            }
        }
        let tags = [format!("@returns {{{return_type}}}")];
        self.doc(writer, docs, &tags);
    }

    /// Documentation with `JSDoc` type tags, which TypeScript leaves out. A
    /// line that doesn't start with `@` continues the tag before it.
    /// A versioned message's `versionOf()`, which reads the version in a
    /// header without checking it, so that an application can choose the
    /// reader of that version before it constructs one
    fn version_of(&self, writer: &mut CodeWriter, name: &str) {
        self.doc(
            writer,
            &[
                "Returns the format version in the header of `bytes`, without checking".to_owned(),
                "it, so that an application can choose the reader of that version".to_owned(),
            ],
            &[
                "@param {Uint8Array} bytes".to_owned(),
                "@returns {number}".to_owned(),
                "@throws {DecodeError} If the header doesn't fit in `bytes`.".to_owned(),
            ],
        );
        writer.open(format!(
            "static versionOf(bytes{}){} {{",
            self.annotation("Uint8Array"),
            self.annotation("number")
        ));
        writer.statement(
            &STYLE,
            "return ",
            &call(
                "new ByteBuffer",
                vec![text("bytes"), text(format!("{name}.#LAYOUT"))],
            ),
            ".getVersion();",
        );
        writer.close("}");
    }

    fn doc(&self, writer: &mut CodeWriter, docs: &[String], tags: &[String]) {
        let mut kept = false;
        let tags: Vec<&String> = tags
            .iter()
            .filter(|tag| {
                if tag.starts_with('@') {
                    kept = !self.typed || tag.starts_with("@throws");
                }
                kept
            })
            .collect();

        let mut lines = docs.to_vec();
        if !lines.is_empty() && !tags.is_empty() {
            lines.push(String::new());
        }
        lines.extend(tags.into_iter().cloned());
        writer.lines(&doc_comment(&lines));
    }

    fn private_field(&self, writer: &mut CodeWriter, name: &str, field_type: &str) {
        if self.typed {
            writer.line(format!("readonly #{name}: {field_type};"));
        } else {
            writer.line(format!("/** @type {{{field_type}}} */"));
            writer.line(format!("#{name};"));
        }
    }

    fn static_readonly(&self) -> &'static str {
        if self.typed {
            "static readonly "
        } else {
            "static "
        }
    }

    /// Opens a method of `parameters`, each a name and its type, broken over
    /// lines when it's too long, as Biome breaks it
    fn signature(
        &self,
        writer: &mut CodeWriter,
        method: &str,
        parameters: &[(&str, String)],
        return_type: &str,
    ) {
        let parameters = parameters
            .iter()
            .map(|(name, parameter_type)| {
                text(format!("{name}{}", self.annotation(parameter_type)))
            })
            .collect();
        writer.statement(
            &STYLE,
            "",
            &call(method, parameters),
            &format!("{} {{", self.annotation(return_type)),
        );
        writer.indent();
    }

    fn annotation(&self, type_name: &str) -> String {
        if self.typed {
            format!(": {type_name}")
        } else {
            String::new()
        }
    }
}

fn writer_fits(_writer: &CodeWriter, line: &str) -> bool {
    // Getters are indented once, in a class
    STYLE.indent.len() + line.len() <= STYLE.width
}

/// A schema's documentation for `JSDoc`, with `@` outside code spans as an
/// HTML entity, which editors render as `@`, so that it can't start a tag,
/// such as `@deprecated` or `@internal`, which TypeScript reads, or `@type`,
/// which would change the type of JavaScript
fn schema_docs(docs: &[String]) -> Vec<String> {
    docs.iter()
        .map(|line| escape_outside_code_spans(line, |text| text.replace('@', "&#64;")))
        .collect()
}

fn identifier(name: &str) -> String {
    let name = camel_case(name);
    if MEMBERS.contains(&name.as_str()) {
        format!("{name}_")
    } else {
        name
    }
}

fn layout_expression(layout: Layout) -> Expr {
    let mut entries = Vec::new();
    if layout.sized {
        entries.push(("sized".to_owned(), "true".to_owned()));
    }
    if layout.version.is_some() {
        entries.push(("versioned".to_owned(), "true".to_owned()));
    }
    if layout.offset_type != OffsetType::Uint32 {
        entries.push((
            "offsetType".to_owned(),
            format!("'{}'", layout.offset_type.name()),
        ));
    }

    if entries.is_empty() {
        text("TableLayout.DEFAULT")
    } else {
        call("TableLayout.create", vec![Expr::Object(entries)])
    }
}

/// Where a struct's field lies, `offset` bytes into the struct
fn struct_position(offset: usize) -> Expr {
    if offset == 0 {
        text("offset")
    } else {
        text(format!("offset + {offset}"))
    }
}

/// `Float32` in `getFloat32()`, and `Uint8` for a bool, which a uint8 stores
fn scalar_method(scalar: Scalar) -> String {
    let name = scalar.stored().name();
    name[..1].to_ascii_uppercase() + &name[1..]
}

/// The value that a setter of a scalar takes: a bool's 1 or 0
fn stored_value(scalar: Scalar, value: &str) -> Expr {
    match scalar {
        Scalar::Bool => text(format!("{value} ? 1 : 0")),
        _ => text(value),
    }
}

/// Returns the bool that `read` reads from its byte, true when it isn't 0, in
/// parentheses when it doesn't fit on a line, as Biome breaks a comparison
fn return_bool(writer: &mut CodeWriter, read: &Expr) {
    let line = format!("return {} !== 0;", read.flat());
    if writer.fits(&STYLE, &line) {
        writer.line(line);
    } else {
        writer.open("return (");
        writer.statement(&STYLE, "", read, " !== 0");
        writer.close(");");
    }
}

/// Assigns the bool that `read` reads from its byte to `target`, broken after
/// the `=` when it doesn't fit on a line, as Biome breaks a comparison
fn assign_bool(writer: &mut CodeWriter, target: &str, read: &Expr) {
    let line = format!("{target}{} !== 0;", read.flat());
    if writer.fits(&STYLE, &line) {
        writer.line(line);
    } else {
        writer.line(target.trim_end());
        writer.indent();
        writer.statement(&STYLE, "", read, " !== 0;");
        writer.dedent();
    }
}

/// The type that a getter returns
fn value_type(field_type: FieldType, schema: &Schema) -> String {
    match field_type {
        FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
        FieldType::Array(scalar) => typed_array(scalar).to_owned(),
        FieldType::String => "string".to_owned(),
        FieldType::Struct(index) | FieldType::Table(index) => schema.definition(index).name.clone(),
        FieldType::StructArray(index) | FieldType::Vector(index) => {
            format!("{}[]", schema.definition(index).name)
        }
    }
}

/// The view of a vector or an array of structs, which their view getters
/// return
fn view_type(field_type: FieldType, schema: &Schema) -> Option<String> {
    match field_type {
        FieldType::Vector(index) => Some(format!("{}Vector", schema.definition(index).name)),
        FieldType::StructArray(index) => Some(format!("{}Array", schema.definition(index).name)),
        _ => None,
    }
}

/// The type that a field's setter takes
fn input_type(field_type: FieldType, schema: &Schema) -> String {
    match field_type {
        FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
        FieldType::Array(scalar) => {
            format!(
                "ArrayLike<{}> | {}",
                scalar_type(scalar),
                typed_array(scalar)
            )
        }
        FieldType::String => "string".to_owned(),
        FieldType::Struct(index) | FieldType::Table(index) => {
            format!("{}Fields", schema.definition(index).name)
        }
        FieldType::StructArray(index) | FieldType::Vector(index) => {
            format!("readonly {}Fields[]", schema.definition(index).name)
        }
    }
}

const fn scalar_type(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::Bool => "boolean",
        Scalar::Int64 | Scalar::Uint64 => "bigint",
        _ => "number",
    }
}

const fn typed_array(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::Bool => panic!("the checker rejects arrays of bools"),
        Scalar::Int8 => "Int8Array",
        Scalar::Uint8 => "Uint8Array",
        Scalar::Int16 => "Int16Array",
        Scalar::Uint16 => "Uint16Array",
        Scalar::Int32 => "Int32Array",
        Scalar::Uint32 => "Uint32Array",
        Scalar::Int64 => "BigInt64Array",
        Scalar::Uint64 => "BigUint64Array",
        Scalar::Float32 => "Float32Array",
        Scalar::Float64 => "Float64Array",
    }
}
