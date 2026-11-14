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
/// would replace, `fields`, which reads every field of a table, and `then`,
/// which would make a reader a thenable that `await` and `Promise.resolve()`
/// call and never settle
const MEMBERS: [&str; 9] = [
    "constructor",
    "fields",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "then",
    "toLocaleString",
    "toString",
    "valueOf",
];

/// The read of a field's value, which `suffix` follows
struct Read<'d> {
    field: &'d Field,
    expression: Expr,
    suffix: &'static str,
}

/// Where a table's `fields()` reads it from
#[derive(Clone, Copy)]
enum TableAt {
    /// A message's root, through its reader
    Root,
    /// A nested table's position, as its static `read()` takes it
    Position,
}

/// What a struct's fields that are structs are read into
#[derive(Clone, Copy)]
enum StructRead {
    /// Their readers, as the struct's constructor reads them
    Reader,
    /// Their fields, as the struct's static `read()` reads them
    Fields,
}

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
    let generator = Generator {
        schema,
        typed,
        reads: Reads::of(schema),
    };
    let runtime = options.runtime.as_deref().unwrap_or(RUNTIME);

    let mut writer = CodeWriter::new(STYLE.indent);
    writer.line(format!("// {header}"));
    writer.blank();
    generator.imports(&mut writer, runtime);

    for (index, definition) in schema.definitions.iter().enumerate() {
        writer.blank();
        match definition.kind {
            Kind::Message(layout) => generator.message(&mut writer, index, layout),
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
                generator.vector_view(&mut writer, index);
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
    reads: Reads,
}

/// Which tables `fields()` reads whole in one call, which allocates in
/// proportion to the bytes. Every table that a vector holds would read its
/// strings, arrays and vectors each time, which corrupted offsets can point
/// at the same bytes for every item, or at the vector itself, so a vector's
/// items are read whole only when each takes the same bytes to read.
struct Reads {
    /// Tables of scalars, structs and such tables, whose reads take the same
    /// bytes whatever the bytes hold
    fixed: Vec<bool>,
    /// Tables whose `fields()` reads them whole: those whose vectors hold
    /// only fixed tables
    whole: Vec<bool>,
}

impl Reads {
    fn of(schema: &Schema) -> Self {
        let tables = schema.definitions.len();
        let mut fixed = vec![None; tables];
        let mut whole = vec![None; tables];
        for index in 0..tables {
            Self::fixed(schema, index, &mut fixed);
            Self::whole(schema, index, &mut fixed, &mut whole);
        }

        Self {
            fixed: fixed.into_iter().map(|fixed| fixed == Some(true)).collect(),
            whole: whole.into_iter().map(|whole| whole == Some(true)).collect(),
        }
    }

    // Each is worked out once, since a table can nest another in many
    // fields, and walking every path would take exponential time. Only
    // vectors can nest a table in itself, and neither follows one into a
    // table that isn't fixed, so both end.

    fn fixed(schema: &Schema, index: usize, fixed: &mut [Option<bool>]) -> bool {
        if let Some(known) = fixed[index] {
            return known;
        }

        let mut result = true;
        for field in &schema.definition(index).fields {
            result = match field.field_type {
                FieldType::Scalar(_) | FieldType::Struct(_) => true,
                FieldType::Table(table) => Self::fixed(schema, table, fixed),
                _ => false,
            };
            if !result {
                break;
            }
        }
        fixed[index] = Some(result);

        result
    }

    fn whole(
        schema: &Schema,
        index: usize,
        fixed: &mut [Option<bool>],
        whole: &mut [Option<bool>],
    ) -> bool {
        if let Some(known) = whole[index] {
            return known;
        }

        let mut result = true;
        for field in &schema.definition(index).fields {
            result = match field.field_type {
                FieldType::Table(table) => Self::whole(schema, table, fixed, whole),
                FieldType::Vector(table) => Self::fixed(schema, table, fixed),
                _ => true,
            };
            if !result {
                break;
            }
        }
        whole[index] = Some(result);

        result
    }
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

    fn message(&self, writer: &mut CodeWriter, index: usize, layout: Layout) {
        let definition = self.schema.definition(index);
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
        if self.reads.whole[index] {
            writer.blank();
            self.fields_reader(writer, definition);
        }
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
                    &[format!("Writes {} with every field", article(name))],
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
                        format!(
                            "Writes {} with every field into the start of `bytes`,",
                            article(name)
                        ),
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

        let whole = self.reads.whole[index];
        if whole {
            self.static_read(writer, definition);
            writer.blank();
        }
        self.nested_size_and_write(writer, definition);
        self.getters(writer, definition, false);
        if whole {
            writer.blank();
            self.fields_reader(writer, definition);
        }
        writer.close("}");
    }

    /// A table's `fields()`, which reads every field at once: a message's
    /// from its root, and a nested table's with its static `read()`
    fn fields_reader(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;

        self.doc(
            writer,
            &[
                format!("Reads every field into the `{name}Fields` that writes it, nested"),
                "tables and the items of vectors and arrays of structs included".to_owned(),
            ],
            &[format!("@returns {{{name}Fields}}")],
        );
        writer.open(format!(
            "fields(){} {{",
            self.annotation(&format!("{name}Fields"))
        ));
        if matches!(definition.kind, Kind::Message(_)) {
            return_object(writer, &self.table_reads(definition, TableAt::Root));
        } else {
            writer.line(format!("return {name}.read(this.#buffer, this.#position);"));
        }
        writer.close("}");
    }

    /// A nested table's static `read()`, which reads every field at once
    /// without creating its reader, for its parent's `fields()` and a view's
    fn static_read(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;

        self.doc(
            writer,
            &[
                "Reads every field of the table at `position`, as `fields()` does,".to_owned(),
                "without creating its reader".to_owned(),
            ],
            &[
                "@param {ByteBuffer} buffer".to_owned(),
                "@param {number} position".to_owned(),
                format!("@returns {{{name}Fields}}"),
            ],
        );
        self.signature(
            writer,
            "static read",
            &[
                ("buffer", "ByteBuffer".to_owned()),
                ("position", "number".to_owned()),
            ],
            &format!("{name}Fields"),
        );
        return_object(writer, &self.table_reads(definition, TableAt::Position));
        writer.close("}");
    }

    /// The reads of every field of a table, in order, each of which reads a
    /// nested table, vector or array of structs whole
    fn table_reads<'d>(&self, definition: &'d Definition, at: TableAt) -> Vec<Read<'d>> {
        let buffer = match at {
            TableAt::Root => "this.#buffer",
            TableAt::Position => "buffer",
        };

        definition
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let offset = match at {
                    TableAt::Root => call(
                        format!("{buffer}.rootOffset"),
                        vec![text(index.to_string())],
                    ),
                    TableAt::Position => call(
                        format!("{buffer}.fieldOffset"),
                        vec![text("position"), text(index.to_string())],
                    ),
                };
                let mut suffix = "";
                let read = match field.field_type {
                    FieldType::Scalar(scalar) => call(
                        format!("{buffer}.get{}", scalar_method(scalar)),
                        vec![offset],
                    ),
                    FieldType::Array(scalar) => call(
                        format!("{buffer}.get{}Array", scalar_method(scalar)),
                        vec![offset],
                    ),
                    FieldType::String => call(format!("{buffer}.getString"), vec![offset]),
                    FieldType::Struct(index) => call(
                        format!("{}.read", self.schema.definition(index).name),
                        vec![text(format!("{buffer}.bytes()")), offset],
                    ),
                    FieldType::Table(index) => call(
                        format!("{}.read", self.schema.definition(index).name),
                        vec![text(buffer), offset],
                    ),
                    // The view's fields(), which reads every item
                    FieldType::StructArray(_) | FieldType::Vector(_) => {
                        suffix = ".fields()";
                        let view = value_type(field.field_type, self.schema);
                        call(format!("new {view}"), vec![text(buffer), offset])
                    }
                };
                Read {
                    field,
                    expression: read,
                    suffix,
                }
            })
            .collect()
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
        writer.lines(&doc_comment(&[format!(
            "The byte length of {}",
            article(name)
        )]));
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
                format!(
                    "Reads {} at `offset` in `bytes`, eagerly, so that it",
                    article(name)
                ),
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
        for Read {
            field,
            expression: read,
            ..
        } in self.struct_reads(definition, StructRead::Reader, "bytes", "offset")
        {
            let target = format!("this.#{} = ", identifier(&field.name));
            match field.field_type {
                FieldType::Scalar(Scalar::Bool) => assign_bool(writer, &target, &read, ";"),
                _ => writer.statement(&STYLE, &target, &read, ";"),
            }
        }
        writer.close("}");
        writer.blank();

        self.doc(
            writer,
            &[
                format!(
                    "Reads the fields of {} at `offset` in `bytes`,",
                    article(name)
                ),
                format!("without creating {}", article(name)),
            ],
            &[
                "@param {Uint8Array} bytes".to_owned(),
                "@param {number} [offset]".to_owned(),
                format!("@returns {{{name}Fields}}"),
                format!("@throws {{DecodeError}} If `bytes` end before the `{name}` does."),
            ],
        );
        writer.open(format!(
            "static read(bytes{}, offset = 0){} {{",
            self.annotation("Uint8Array"),
            self.annotation(&format!("{name}Fields"))
        ));
        return_object(
            writer,
            &self.struct_reads(definition, StructRead::Fields, "bytes", "offset"),
        );
        writer.close("}");
        writer.blank();

        self.doc(
            writer,
            &[format!("Writes {}", article(name))],
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
            &[format!("Writes {} at the writer's position", article(name))],
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
            &[format!("Writes {} at `offset` in `bytes`", article(name))],
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
            let position = struct_position("offset", offset);
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

    /// The reads of a struct's fields at `offset` in `bytes`, in order, with a
    /// bool's byte to compare with 0, and a struct's as its `read` says
    fn struct_reads<'d>(
        &self,
        definition: &'d Definition,
        read: StructRead,
        bytes: &str,
        base: &str,
    ) -> Vec<Read<'d>> {
        let mut offset = 0;
        let mut reads = Vec::new();
        for field in &definition.fields {
            let position = struct_position(base, offset);
            let (expression, size) = match field.field_type {
                FieldType::Scalar(scalar) => (
                    call(
                        format!("read{}", scalar_method(scalar)),
                        vec![text(bytes), position],
                    ),
                    usize::from(scalar.size()),
                ),
                FieldType::Struct(index) => {
                    let inner = self.schema.definition(index);
                    let callee = match read {
                        StructRead::Reader => format!("new {}", inner.name),
                        StructRead::Fields => format!("{}.read", inner.name),
                    };
                    (
                        call(callee, vec![text(bytes), position]),
                        inner.struct_layout().size,
                    )
                }
                _ => unreachable!("structs hold scalars and structs only"),
            };
            reads.push(Read {
                field,
                expression,
                suffix: "",
            });
            offset += size;
        }

        reads
    }

    /// The view of a vector of a table, which reads each item when it's
    /// asked for, from the item's end and the end of the item before it
    fn vector_view(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
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
        if self.reads.fixed[index] {
            writer.blank();
            self.vector_fields(writer, table);
        }
        writer.close("}");
    }

    /// A vector's `fields()`, which reads every item's fields
    fn vector_fields(&self, writer: &mut CodeWriter, table: &str) {
        self.view_fields(
            writer,
            table,
            &["@throws {DecodeError} If an item lies outside the bytes.".to_owned()],
        );
        writer.statement(
            &STYLE,
            "const offsets = ",
            &call(
                "this.#buffer.getVectorOffsets",
                vec![text("this.#position")],
            ),
            ";",
        );
        writer.line(format!(
            "const items = {};",
            self.new_array(table, "offsets.length")
        ));
        writer.blank();
        writer.open("for (let i = 0; i < offsets.length; i++) {");
        writer.statement(
            &STYLE,
            "items[i] = ",
            &call(
                format!("{table}.read"),
                vec![text("this.#buffer"), text("offsets[i]")],
            ),
            ";",
        );
        writer.close("}");
        writer.blank();
        writer.line("return items;");
        writer.close("}");
    }

    /// Opens a view's `fields()`, which reads every item's fields at once
    fn view_fields(&self, writer: &mut CodeWriter, item: &str, throws: &[String]) {
        let mut tags = vec![format!("@returns {{{item}Fields[]}}")];
        tags.extend_from_slice(throws);
        self.doc(
            writer,
            &[format!(
                "Reads the `{item}Fields` of every item, from the first to the last"
            )],
            &tags,
        );
        writer.open(format!(
            "fields(){} {{",
            self.annotation(&format!("{item}Fields[]"))
        ));
    }

    /// A new array of `length` items of a type, whose type TypeScript gives
    fn new_array(&self, item: &str, length: &str) -> String {
        if self.typed {
            format!("new Array<{item}Fields>({length})")
        } else {
            format!("new Array({length})")
        }
    }

    /// The view of an array of a struct, which reads each struct when it's
    /// asked for, from a view of their bytes
    fn struct_array_view(&self, writer: &mut CodeWriter, definition: &Definition) {
        let structure = &definition.name;
        // Struct `index`'s offset in the array's bytes, which bound its reads,
        // rather than a check of `index` in each getter, which took 12% longer
        // to sum an array's structs field by field
        let offset = format!("index * {}", definition.struct_layout().size);

        writer.lines(&doc_comment(&[
            format!("An array of `{structure}`s, which reads each struct when it's asked for."),
            String::new(),
            "Its getters take a struct's index, an integer, which they check only by".to_owned(),
            "reading within the array's bytes.".to_owned(),
        ]));
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
                "@throws {DecodeError} If the array has no struct `index`.".to_owned(),
            ],
        );
        writer.open(format!(
            "get(index{}){} {{",
            self.annotation("number"),
            self.annotation(structure)
        ));
        writer.statement(
            &STYLE,
            "return ",
            &call(
                format!("new {structure}"),
                vec![text("this.#bytes"), text(&offset)],
            ),
            ";",
        );
        writer.close("}");

        let reads = self.struct_reads(definition, StructRead::Reader, "this.#bytes", &offset);
        for Read {
            field,
            expression: read,
            ..
        } in reads
        {
            let return_type = value_type(field.field_type, self.schema);
            writer.blank();
            self.struct_getter_doc(writer, field, &return_type);
            writer.open(format!(
                "{}(index{}){} {{",
                view_identifier(&field.name),
                self.annotation("number"),
                self.annotation(&return_type)
            ));
            if field.field_type == FieldType::Scalar(Scalar::Bool) {
                return_bool(writer, &read);
            } else {
                writer.statement(&STYLE, "return ", &read, ";");
            }
            writer.close("}");
        }
        writer.blank();

        self.struct_array_fields(writer, definition);
        writer.close("}");
    }

    /// An array of structs' `fields()`, which reads every struct's fields
    fn struct_array_fields(&self, writer: &mut CodeWriter, definition: &Definition) {
        let structure = &definition.name;

        self.view_fields(writer, structure, &[]);
        // The struct's reads in the loop, from the bytes in a local, rather
        // than its read() of this.#bytes, which took a quarter longer
        writer.line("const bytes = this.#bytes;");
        writer.line(format!(
            "const items = {};",
            self.new_array(structure, "this.#length")
        ));
        writer.blank();
        writer.open("for (let i = 0; i < items.length; i++) {");
        writer.line(format!(
            "const offset = i * {};",
            definition.struct_layout().size
        ));
        let reads = self.struct_reads(definition, StructRead::Fields, "bytes", "offset");
        write_object(writer, "items[i] = ", &reads);
        writer.close("}");
        writer.blank();
        writer.line("return items;");
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
        let summary = format!("The fields of {}, to write", article(&definition.name));

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

    fn getters(&self, writer: &mut CodeWriter, definition: &Definition, root: bool) {
        for (index, field) in definition.fields.iter().enumerate() {
            let offset = if root {
                call("this.#buffer.rootOffset", vec![text(index.to_string())])
            } else {
                call(
                    "this.#buffer.fieldOffset",
                    vec![text("this.#position"), text(index.to_string())],
                )
            };
            let return_type = value_type(field.field_type, self.schema);

            writer.blank();
            self.getter_doc(writer, &schema_docs(&field.docs), &return_type);
            writer.open(format!(
                "{}(){} {{",
                identifier(&field.name),
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
                FieldType::Table(table) => {
                    let table = &self.schema.definition(table).name;
                    let read = call(format!("new {table}"), vec![text("this.#buffer"), offset]);
                    writer.statement(&STYLE, "return ", &read, ";");
                }
                FieldType::StructArray(_) | FieldType::Vector(_) => {
                    // A view, which reads each item when it's asked for
                    let read = call(
                        format!("new {return_type}"),
                        vec![text("this.#buffer"), offset],
                    );
                    writer.statement(&STYLE, "return ", &read, ";");
                }
            }

            writer.close("}");
        }
    }

    /// The documentation of a view's getter of a field of struct `index`,
    /// with its parameter and return type in JavaScript
    fn struct_getter_doc(&self, writer: &mut CodeWriter, field: &Field, return_type: &str) {
        let mut docs = schema_docs(&field.docs);
        if !docs.is_empty() {
            docs.push(String::new());
        }
        docs.push(format!(
            "Reads `{}` of struct `index`, without reading the others",
            identifier(&field.name)
        ));
        let tags = [
            "@param {number} index".to_owned(),
            format!("@returns {{{return_type}}}"),
            "@throws {DecodeError} If the array has no struct `index`.".to_owned(),
        ];
        self.doc(writer, &docs, &tags);
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

/// The own members of an array of structs' view, which the getters of its
/// structs' fields take a `_` after, beside those that every class does
const VIEW_MEMBERS: [&str; 3] = ["fields", "get", "length"];

/// The name of the getter of its structs' field on an array of structs' view
fn view_identifier(name: &str) -> String {
    let name = identifier(name);
    if VIEW_MEMBERS.contains(&name.as_str()) {
        format!("{name}_")
    } else {
        name
    }
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
fn struct_position(base: &str, offset: usize) -> Expr {
    if offset == 0 {
        text(base)
    } else {
        text(format!("{base} + {offset}"))
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

/// Returns an object of each field's value that `reads` reads, a bool's
/// compared with 0, broken over a line for each, as Biome keeps an object
/// that's broken after its brace
fn return_object(writer: &mut CodeWriter, reads: &[Read]) {
    write_object(writer, "return ", reads);
}

/// Writes `target` and an object of each field's value that `reads` reads, as
/// [`return_object`] returns it
fn write_object(writer: &mut CodeWriter, target: &str, reads: &[Read]) {
    writer.open(format!("{target}{{"));
    for (index, read) in reads.iter().enumerate() {
        let comma = if index + 1 < reads.len() { "," } else { "" };
        let key = format!("{}: ", identifier(&read.field.name));
        if read.field.field_type == FieldType::Scalar(Scalar::Bool) {
            assign_bool(writer, &key, &read.expression, comma);
        } else {
            let suffix = format!("{}{comma}", read.suffix);
            writer.statement(&STYLE, &key, &read.expression, &suffix);
        }
    }
    writer.close("};");
}

/// Assigns the bool that `read` reads from its byte to `target`, broken after
/// the `=` or `:` when it doesn't fit on a line, as Biome breaks a comparison,
/// and ends it with `suffix`
fn assign_bool(writer: &mut CodeWriter, target: &str, read: &Expr, suffix: &str) {
    let line = format!("{target}{} !== 0{suffix}", read.flat());
    if writer.fits(&STYLE, &line) {
        writer.line(line);
    } else {
        writer.line(target.trim_end());
        writer.indent();
        writer.statement(&STYLE, "", read, &format!(" !== 0{suffix}"));
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
        // A view of the items, which reads each one when it's asked for
        FieldType::Vector(index) => format!("{}Vector", schema.definition(index).name),
        FieldType::StructArray(index) => format!("{}Array", schema.definition(index).name),
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
