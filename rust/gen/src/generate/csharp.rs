//! C# for the runtime's namespace `Tafla`, one file per schema, of C# 14 for
//! .NET 10. Messages and tables are `readonly struct`s that read their fields
//! lazily through a `TableReader`, and are written from `readonly record
//! struct`s of their fields, which a `TableSizer` counts and a `TableWriter`
//! then writes, nested tables and vector items in place. Structs are `readonly
//! record struct`s, read from and written into spans of their bytes with
//! `BinaryPrimitives`.
//!
//! The code names everything outside the schema in full, from `global::`, so
//! that no declaration of a schema can hide a name that it uses, and it never
//! names one of the schema's types where a property could hide it: within a
//! type of a definition's fields, as a static member's qualifier. It reads a
//! struct with its constructor, whose type a property can't hide, and writes
//! the sizes and alignments of structs and nested tables as numbers, so that a
//! field may take the name of any declaration but its own.

use super::fixed::{FixedSizes, NESTED_OFFSET_SIZE, offset_size};
use super::prose::indefinite_article;
use super::writer::{CodeWriter, Style, call, text};
use super::{GeneratedFile, Options};
use crate::model::{Definition, Field, FieldType, Kind, Layout, OffsetType, Scalar, Schema};
use crate::naming::pascal_case;

/// The runtime's namespace, unless [`Options::runtime`] names another
const RUNTIME: &str = "Tafla";

const INDENT: &str = "    ";

/// The longest table that the generated code lays out itself, whose
/// positions an `int` holds, as spans of bytes count them
const MAX_LAID_OUT: usize = i32::MAX as usize;

/// The standard library's little-endian reads and writes of scalars
const BINARY_PRIMITIVES: &str = "global::System.Buffers.Binary.BinaryPrimitives";

/// How the code breaks calls, one argument per line and the closing
/// parenthesis on a line of its own, as the runtime's code does. `dotnet
/// format` keeps the lines as they're broken.
const STYLE: Style = Style {
    width: 120,
    indent: INDENT,
    trailing_comma: false,
    hug_arguments: false,
    max_arguments_width: None,
};

/// The members that the generated types declare or inherit beside the
/// properties of the fields, which a property of the same name would clash
/// with or hide, so fields named after them take a `_` after their name:
/// those of `object`, the private `PrintMembers()` of a record, a message's
/// `FormatVersion`, `VersionOf()` and `TableLayout`, the `Serialize()` and
/// `SerializeInto()` of its fields, the `AddTo()` and `WriteTo()` with which a
/// table's fields are written in its parent, or `PutTo()`, with which a
/// table of fields of fixed sizes is written whole, and a struct's `Size`,
/// `Alignment`, `Write()` and `SerializeArray()`. A field named after its own
/// definition or its definition's fields takes one too, since C# reserves a
/// type's name for its constructors. `reserved.rs` keeps declarations from
/// taking the names of these members, which their own types would declare.
const MEMBERS: [&str; 20] = [
    "AddTo",
    "Alignment",
    "Equals",
    "Finalize",
    "FormatVersion",
    "GetHashCode",
    "GetType",
    "MemberwiseClone",
    "PrintMembers",
    "PutTo",
    "ReferenceEquals",
    "Serialize",
    "SerializeArray",
    "SerializeInto",
    "Size",
    "TableLayout",
    "ToString",
    "VersionOf",
    "Write",
    "WriteTo",
];

/// C#'s keywords, which a namespace's names take an `@` before, as a verbatim
/// identifier, and `global`, which would read as the alias of the global
/// namespace
const KEYWORDS: [&str; 78] = [
    "abstract",
    "as",
    "base",
    "bool",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "checked",
    "class",
    "const",
    "continue",
    "decimal",
    "default",
    "delegate",
    "do",
    "double",
    "else",
    "enum",
    "event",
    "explicit",
    "extern",
    "false",
    "finally",
    "fixed",
    "float",
    "for",
    "foreach",
    "global",
    "goto",
    "if",
    "implicit",
    "in",
    "int",
    "interface",
    "internal",
    "is",
    "lock",
    "long",
    "namespace",
    "new",
    "null",
    "object",
    "operator",
    "out",
    "override",
    "params",
    "private",
    "protected",
    "public",
    "readonly",
    "ref",
    "return",
    "sbyte",
    "sealed",
    "short",
    "sizeof",
    "stackalloc",
    "static",
    "string",
    "struct",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "uint",
    "ulong",
    "unchecked",
    "unsafe",
    "ushort",
    "using",
    "virtual",
    "void",
    "volatile",
    "while",
];

pub fn generate(
    schema: &Schema,
    stem: &str,
    header: &str,
    options: &Options,
) -> Vec<GeneratedFile> {
    let name = type_name_of(stem);
    let path = format!("{name}.cs");
    let namespace = options.package.as_ref().map_or(name, |package| {
        package
            .as_str()
            .split('.')
            .map(namespace_name)
            .collect::<Vec<_>>()
            .join(".")
    });
    let generator = Generator {
        schema,
        fixed: FixedSizes::new(schema, MAX_LAID_OUT),
        runtime: format!("global::{}", options.runtime.as_deref().unwrap_or(RUNTIME)),
    };

    // The header that C#'s tools take as generated code, whose analyzers and
    // formatting they leave to the generator
    let mut writer = CodeWriter::new(INDENT);
    writer.line("// <auto-generated>");
    writer.line(format!("// {header}"));
    writer.line("// </auto-generated>");
    if schema.definitions.is_empty() {
        return vec![GeneratedFile {
            path,
            contents: writer.finish(),
        }];
    }

    writer.blank();
    // Code that a tool generates has nullable annotations only when it says so
    writer.line("#nullable enable");
    writer.line("#pragma warning disable CS1591 // Members that the schema doesn't document");
    writer.blank();
    writer.line(format!("namespace {namespace};"));

    for (index, definition) in schema.definitions.iter().enumerate() {
        writer.blank();
        match definition.kind {
            Kind::Message(layout) => {
                generator.message(&mut writer, definition, layout);
                writer.blank();
                generator.fields(&mut writer, index);
            }
            Kind::Table => {
                generator.table(&mut writer, definition);
                writer.blank();
                generator.fields(&mut writer, index);
            }
            Kind::Struct(_) => generator.structure(&mut writer, definition),
        }

        if schema.is_item(index) {
            writer.blank();
            if definition.is_struct() {
                struct_array_view(&mut writer, definition);
            } else {
                generator.vector_view(&mut writer, definition);
            }
        }
    }

    vec![GeneratedFile {
        path,
        contents: writer.finish(),
    }]
}

struct Generator<'a> {
    schema: &'a Schema,
    /// The byte length of each table whose fields' sizes the schema fixes,
    /// which the code writes whole rather than field by field
    fixed: FixedSizes,
    /// The runtime's namespace from `global::`, which its types are named in
    runtime: String,
}

impl Generator<'_> {
    /// A type of the runtime, named in full
    fn runtime_type(&self, name: &str) -> String {
        format!("{}.{name}", self.runtime)
    }

    /// A message's reader, which reads its fields from bytes that it keeps
    fn message(&self, writer: &mut CodeWriter, definition: &Definition, layout: Layout) {
        let name = &definition.name;
        let table_layout = self.runtime_type("TableLayout");
        let decode_exception = self.runtime_type("DecodeException");

        summary(writer, &schema_docs(&definition.docs));
        open_block(writer, format!("public readonly struct {name}"));
        if let Some(version) = layout.version {
            summary(
                writer,
                &["The version of the format, which the header carries and the constructor checks."
                    .to_owned()],
            );
            writer.line(format!("public const uint FormatVersion = {version};"));
            writer.blank();
        }
        writer.line(format!(
            "internal static readonly {table_layout} TableLayout = {};",
            self.layout_expression(layout)
        ));
        writer.blank();
        writer.line(format!(
            "private readonly {} _reader;",
            self.runtime_type("TableReader")
        ));
        writer.blank();

        summary(
            writer,
            &[format!(
                "Reads {} {} from <paramref name=\"bytes\"/>, which it keeps rather than copying them.",
                indefinite_article(name),
                cref(name)
            )],
        );
        if layout.version.is_some() {
            writer.line(format!(
                "/// <exception cref=\"{decode_exception}\">The bytes don't hold the header, or are in another format version.</exception>"
            ));
        }
        writer.line(format!(
            "public {name}(global::System.ReadOnlyMemory<byte> bytes)"
        ));
        writer.line("{");
        writer.indent();
        writer.line(format!(
            "_reader = new {}(bytes, TableLayout);",
            self.runtime_type("TableReader")
        ));
        if layout.version.is_some() {
            writer.blank();
            writer.line("var version = _reader.GetVersion();");
            writer.line("if (version != FormatVersion)");
            writer.line("{");
            writer.indent();
            writer.line(format!(
                "throw {decode_exception}.UnsupportedVersion(version, FormatVersion);"
            ));
            writer.close("}");
        }
        writer.close("}");

        if layout.version.is_some() {
            writer.blank();
            summary(
                writer,
                &[
                    "Returns the format version in the header of <paramref name=\"bytes\"/>, without checking it, so that"
                        .to_owned(),
                    "an application can choose the reader of that version.".to_owned(),
                ],
            );
            writer.line(format!(
                "/// <exception cref=\"{decode_exception}\">The bytes don't hold the header.</exception>"
            ));
            writer
                .line("public static uint VersionOf(global::System.ReadOnlyMemory<byte> bytes) =>");
            writer.indent();
            writer.line(format!(
                "new {}(bytes, TableLayout).GetVersion();",
                self.runtime_type("TableReader")
            ));
            writer.dedent();
        }

        self.getters(writer, definition, true);
        writer.close("}");
    }

    /// A nested table's reader, which reads its fields at its position in
    /// its parent's reader
    fn table(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;
        let reader = self.runtime_type("TableReader");

        summary(writer, &schema_docs(&definition.docs));
        open_block(writer, format!("public readonly struct {name}"));
        writer.line(format!("private readonly {reader} _reader;"));
        writer.line("private readonly int _position;");
        writer.blank();
        writer.line(format!("internal {name}({reader} reader, int position)"));
        writer.line("{");
        writer.indent();
        writer.line("_reader = reader;");
        writer.line("_position = position;");
        writer.close("}");

        self.getters(writer, definition, false);
        writer.close("}");
    }

    /// The layout of a message, as an expression of the runtime's
    /// `TableLayout`
    fn layout_expression(&self, layout: Layout) -> String {
        let mut properties = Vec::new();
        if layout.sized {
            properties.push("Sized = true".to_owned());
        }
        if layout.version.is_some() {
            properties.push("Versioned = true".to_owned());
        }
        if layout.offset_type != OffsetType::Uint32 {
            properties.push(format!(
                "OffsetType = {}.{}",
                self.runtime_type("OffsetType"),
                offset_type_name(layout.offset_type)
            ));
        }

        if properties.is_empty() {
            format!("{}.Default", self.runtime_type("TableLayout"))
        } else {
            format!("new() {{ {} }}", properties.join(", "))
        }
    }

    /// The properties that read a message's or table's fields, by their
    /// offsets from the root or from the table's position
    fn getters(&self, writer: &mut CodeWriter, definition: &Definition, is_root: bool) {
        for (index, field) in definition.fields.iter().enumerate() {
            let offset = if is_root {
                format!("_reader.RootOffset({index})")
            } else {
                format!("_reader.FieldOffset(_position, {index})")
            };
            let value = match field.field_type {
                FieldType::Scalar(Scalar::Bool) => format!("_reader.GetUInt8({offset}) != 0"),
                FieldType::Scalar(scalar) => {
                    format!("_reader.Get{}({offset})", method_name(scalar))
                }
                FieldType::Array(scalar) => {
                    format!("_reader.Get{}Array({offset})", method_name(scalar))
                }
                FieldType::String => format!("_reader.GetString({offset})"),
                FieldType::Struct(index) => format!(
                    "new(_reader.GetStruct({offset}, {}))",
                    self.struct_size(index)
                ),
                FieldType::StructArray(index) => format!(
                    "new(_reader.GetStructArray({offset}, {}))",
                    self.struct_size(index)
                ),
                FieldType::Table(_) => format!("new(_reader, {offset})"),
                FieldType::Vector(_) => format!("new(_reader.GetVector({offset}))"),
            };

            writer.blank();
            self.getter_docs(writer, field);
            expression_body(
                writer,
                &format!(
                    "public {} {}",
                    self.value_type(field.field_type),
                    member(&field.name, &definition.name)
                ),
                &value,
            );
        }
    }

    /// A getter's documentation: the field's, and for a vector or an array of
    /// structs, that the getter returns a view whose items are read when
    /// they're asked for
    fn getter_docs(&self, writer: &mut CodeWriter, field: &Field) {
        let mut docs = schema_docs(&field.docs);
        let (view, exception) = match field.field_type {
            FieldType::Vector(_) => (
                format!(
                    "Views the items, reading each one when it's asked for, so that an item whose bytes are corrupt throws a {} when it's read, rather than here.",
                    cref(&self.runtime_type("DecodeException"))
                ),
                "The vector's count or the ends of its items don't lie within the bytes.",
            ),
            FieldType::StructArray(_) => (
                "Views the structs in place, reading each one when it's asked for.".to_owned(),
                "The array's count or its structs don't lie within the bytes.",
            ),
            _ => {
                summary(writer, &docs);
                return;
            }
        };

        if !docs.is_empty() {
            docs.push(String::new());
        }
        docs.push(view);
        summary(writer, &docs);
        writer.line(format!(
            "/// <exception cref=\"{}\">{exception}</exception>",
            self.runtime_type("DecodeException")
        ));
    }

    /// The fields of a message or table, a `readonly record struct` with a
    /// required property of each field, and the methods that write them: a
    /// message's `Serialize()` and `SerializeInto()`, and a table's `AddTo()`
    /// and `WriteTo()`, which its parent calls
    fn fields(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
        let name = &definition.name;

        summary(
            writer,
            &[format!(
                "The fields of {} {}, to write.",
                indefinite_article(name),
                cref(name)
            )],
        );
        open_block(
            writer,
            format!("public readonly record struct {name}Fields"),
        );
        for (index, field) in definition.fields.iter().enumerate() {
            if index > 0 {
                writer.blank();
            }
            summary(writer, &schema_docs(&field.docs));
            writer.line(format!(
                "public required {} {} {{ get; init; }}",
                self.input_type(field.field_type),
                member(&field.name, name)
            ));
        }
        writer.blank();

        match definition.kind {
            Kind::Message(layout) => self.serialize(writer, definition, layout),
            Kind::Table => match self.fixed.table(index) {
                Some(size) => self.put_to(writer, definition, size),
                None => self.add_to_and_write_to(writer, definition),
            },
            Kind::Struct(_) => unreachable!("structs are their own fields"),
        }
        writer.close("}");
    }

    /// A message's `Serialize()` into new bytes and `SerializeInto()` the
    /// caller's
    fn serialize(&self, writer: &mut CodeWriter, definition: &Definition, layout: Layout) {
        let name = &definition.name;
        let build_exception = format!(
            "/// <exception cref=\"{}\">The message is longer than its offset type, {}, addresses.</exception>",
            self.runtime_type("BuildException"),
            layout.offset_type.name()
        );

        summary(
            writer,
            &[format!(
                "Writes {} {} with every field into new bytes of its length.",
                indefinite_article(name),
                cref(name)
            )],
        );
        writer.line(&build_exception);
        writer.line("public byte[] Serialize()");
        writer.line("{");
        writer.indent();
        match self.whole_message(definition, layout) {
            Some(whole) => self.serialize_whole(writer, definition, layout, &whole),
            None => self.serialize_body(writer, definition, layout, false),
        }
        writer.close("}");
        writer.blank();

        summary(
            writer,
            &[
                format!(
                    "Writes {} {} with every field into the start of <paramref name=\"bytes\"/>, as",
                    indefinite_article(name),
                    cref(name)
                ),
                "<see cref=\"Serialize\"/> writes it into new bytes, and returns its length. The bytes needn't be zero,"
                    .to_owned(),
                "and those after the message keep their values. The caller owns them: writing into them again, as the"
                    .to_owned(),
                "next message does, overwrites this one, and what a reader of them reads.".to_owned(),
            ],
        );
        writer.line(
            "/// <returns>The number of bytes that hold the message, from the start of <paramref name=\"bytes\"/>.</returns>",
        );
        writer.line(
            "/// <exception cref=\"global::System.ArgumentException\">The bytes are shorter than the message, before any of them is written.</exception>",
        );
        writer.line(&build_exception);
        writer.line("public int SerializeInto(byte[] bytes)");
        writer.line("{");
        writer.indent();
        self.serialize_body(writer, definition, layout, true);
        writer.close("}");
    }

    /// The layout of a message that `Serialize()` writes whole, when its
    /// fields' sizes are all fixed but for a string after them, and their
    /// positions lie within its offset type, as they do unless the message is
    /// always too long for it
    fn whole_message(&self, definition: &Definition, layout: Layout) -> Option<WholeMessage> {
        let width = offset_size(layout.offset_type);
        let header = (usize::from(layout.sized) + usize::from(layout.version.is_some())) * width;
        let last = definition.fields.len() - 1;

        let mut position = header + definition.fields.len() * width;
        let mut fields = Vec::new();
        let mut string = None;
        for (index, field) in definition.fields.iter().enumerate() {
            let property = member(&field.name, &definition.name);
            fields.push((property.clone(), field.field_type, position));
            if field.field_type == FieldType::String && index == last {
                string = Some(property);
                position += width;
            } else {
                position += self.fixed.field(self.schema, field.field_type)?;
            }
        }

        let max = layout.offset_type.max_value().min(MAX_LAID_OUT as u64);
        (position as u64 <= max).then_some(WholeMessage {
            fields,
            length: position,
            string,
        })
    }

    /// A message's `Serialize()` that writes it whole, each field and its
    /// offset at a position that the generator works out, into the bytes that
    /// the runtime's `TableWriter.CreateTableBytes()` returns with the header,
    /// without a sizer and a writer
    fn serialize_whole(
        &self,
        writer: &mut CodeWriter,
        definition: &Definition,
        layout: Layout,
        whole: &WholeMessage,
    ) {
        let name = &definition.name;
        let width = offset_size(layout.offset_type);
        let slot = match layout.offset_type {
            OffsetType::Uint8 => Scalar::Uint8,
            OffsetType::Uint16 => Scalar::Uint16,
            OffsetType::Uint32 => Scalar::Uint32,
        };
        let header = (usize::from(layout.sized) + usize::from(layout.version.is_some())) * width;

        let mut length = whole.length.to_string();
        if let Some(string) = &whole.string {
            writer.line(format!(
                "var length = {}.Utf8Length({string});",
                self.runtime_type("TableSizer")
            ));
            length = format!("{length} + length");
        }
        let mut arguments = vec![
            text(definition.fields.len().to_string()),
            text(format!("{name}.TableLayout")),
        ];
        if layout.version.is_some() {
            arguments.push(text(format!("{name}.FormatVersion")));
        }
        arguments.push(text(length));
        writer.statement(
            &STYLE,
            "var table = ",
            &call(
                format!("{}.CreateTableBytes", self.runtime_type("TableWriter")),
                arguments,
            ),
            ";",
        );
        writer.line("global::System.Span<byte> bytes = table;");
        writer.blank();

        for (index, (_, _, position)) in whole.fields.iter().enumerate() {
            writer.line(scalar_write(
                slot,
                header + index * width,
                &position.to_string(),
            ));
        }
        for (field, field_type, position) in &whole.fields {
            let at = *position;
            match field_type {
                FieldType::Scalar(scalar) => writer.line(scalar_write(*scalar, at, field)),
                FieldType::Struct(_) => writer.line(format!("{field}.Write({});", bytes_from(at))),
                FieldType::Table(_) => writer.line(format!("{field}.PutTo({});", bytes_from(at))),
                FieldType::String => {
                    let count = match layout.offset_type {
                        OffsetType::Uint8 => "(byte)length",
                        OffsetType::Uint16 => "(ushort)length",
                        OffsetType::Uint32 => "(uint)length",
                    };
                    writer.line(scalar_write(slot, at, count));
                    writer.line(format!(
                        "global::System.Text.Encoding.UTF8.GetBytes({field}, bytes[{}..]);",
                        at + width
                    ));
                }
                _ => {
                    unreachable!("a message written whole holds fields of fixed sizes and a string")
                }
            }
        }
        writer.blank();
        writer.line("return table;");
    }

    /// The body of a message's `Serialize()` or, `into` the caller's bytes,
    /// `SerializeInto()`: the sizer, the writer, the fields, and what the
    /// method returns
    fn serialize_body(
        &self,
        writer: &mut CodeWriter,
        definition: &Definition,
        layout: Layout,
        into: bool,
    ) {
        let name = &definition.name;
        let mut arguments = vec![
            text(definition.fields.len().to_string()),
            text(format!("{name}.TableLayout")),
        ];
        if layout.version.is_some() {
            arguments.push(text(format!("{name}.FormatVersion")));
        }
        writer.statement(
            &STYLE,
            "var sizer = ",
            &call(
                format!("new {}", self.runtime_type("TableSizer")),
                arguments,
            ),
            ";",
        );
        self.sizes(writer, definition);
        writer.blank();

        writer.line(if into {
            "var writer = sizer.CreateWriter(bytes);"
        } else {
            "var writer = sizer.CreateWriter();"
        });
        self.writes(writer, definition);
        writer.blank();
        writer.line(if into {
            "return writer.ByteLength;"
        } else {
            "return writer.Bytes;"
        });
    }

    /// A table's `AddTo()`, which adds its fields to its parent's sizer, and
    /// `WriteTo()`, which writes them where its parent's writer is
    fn add_to_and_write_to(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;
        let fields = definition.fields.len();

        summary(
            writer,
            &[format!(
                "Adds {} {} to its parent's sizer, after the field or vector that holds it.",
                indefinite_article(name),
                cref(name)
            )],
        );
        writer.line(format!(
            "internal void AddTo(ref {} sizer)",
            self.runtime_type("TableSizer")
        ));
        writer.line("{");
        writer.indent();
        writer.line(format!("var parent = sizer.StartTable({fields});"));
        self.sizes(writer, definition);
        writer.line("sizer.EndTable(parent);");
        writer.close("}");
        writer.blank();

        summary(
            writer,
            &[format!(
                "Writes {} {} where its parent's writer is, after the field or vector that holds it.",
                indefinite_article(name),
                cref(name)
            )],
        );
        writer.line(format!(
            "internal void WriteTo(ref {} writer)",
            self.runtime_type("TableWriter")
        ));
        writer.line("{");
        writer.indent();
        writer.line(format!("var parent = writer.StartTable({fields});"));
        self.writes(writer, definition);
        writer.line("writer.EndTable(parent);");
        writer.close("}");
    }

    /// The `PutTo()` of a table whose fields' sizes the schema fixes, which
    /// writes it whole, its offsets and its fields at positions that the
    /// generator works out, where its parent's writer reserved its bytes. It
    /// has no padding, since it holds no arrays, and its parent's sizer adds
    /// its constant length, for every item of a vector of it at once.
    fn put_to(&self, writer: &mut CodeWriter, definition: &Definition, size: usize) {
        let name = &definition.name;

        summary(
            writer,
            &[format!(
                "Writes {} {} whole, its offsets and its fields, into the start of <paramref name=\"bytes\"/>, which hold its {size} bytes, where its parent's writer reserved them.",
                indefinite_article(name),
                cref(name)
            )],
        );
        open_block(
            writer,
            "internal void PutTo(global::System.Span<byte> bytes)",
        );

        // Each field lies after the offsets and the fields before it
        let mut offset = definition.fields.len() * NESTED_OFFSET_SIZE;
        let mut fields = Vec::new();
        for (index, field) in definition.fields.iter().enumerate() {
            writer.line(scalar_write(
                Scalar::Uint32,
                index * NESTED_OFFSET_SIZE,
                &offset.to_string(),
            ));
            fields.push((member(&field.name, name), field.field_type, offset));
            offset += self
                .fixed
                .field(self.schema, field.field_type)
                .expect("a table of fixed fields holds only fields of fixed sizes");
        }
        for (field, field_type, at) in fields {
            writer.line(match field_type {
                FieldType::Scalar(scalar) => scalar_write(scalar, at, &field),
                FieldType::Struct(_) => format!("{field}.Write({});", bytes_from(at)),
                FieldType::Table(_) => format!("{field}.PutTo({});", bytes_from(at)),
                _ => unreachable!("a table of fixed fields holds only fields of fixed sizes"),
            });
        }
        writer.close("}");
    }

    /// The calls that add a table's fields to its sizer, in order
    fn sizes(&self, writer: &mut CodeWriter, definition: &Definition) {
        for field in &definition.fields {
            let value = member(&field.name, &definition.name);
            let call = match field.field_type {
                FieldType::Scalar(scalar) => format!("sizer.Add{}();", method_name(scalar)),
                FieldType::Array(scalar) => {
                    format!("sizer.Add{}Array({value}.Length);", method_name(scalar))
                }
                FieldType::String => format!("sizer.AddString({value});"),
                FieldType::Struct(index) => {
                    format!("sizer.AddStruct(size: {});", self.struct_size(index))
                }
                FieldType::StructArray(index) => format!(
                    "sizer.AddStructArray({value}.Length, size: {}, alignment: {});",
                    self.struct_size(index),
                    self.schema.definition(index).struct_layout().alignment
                ),
                FieldType::Table(index) => {
                    writer.line(format!(
                        "sizer.AddTable(alignment: {});",
                        self.schema.table_alignment(index)
                    ));
                    match self.fixed.table(index) {
                        Some(size) => format!("sizer.AddTableBytes(1, size: {size});"),
                        None => format!("{value}.AddTo(ref sizer);"),
                    }
                }
                FieldType::Vector(index) => {
                    writer.line(format!("sizer.AddVector({value}.Length);"));
                    // Tables of fixed fields are all of the same length
                    if let Some(size) = self.fixed.table(index) {
                        writer.line(format!(
                            "sizer.AddTableBytes({value}.Length, size: {size});"
                        ));
                        continue;
                    }
                    writer.line(format!("foreach (ref readonly var item in {value}.Span)"));
                    writer.line("{");
                    writer.indent();
                    writer.line("item.AddTo(ref sizer);");
                    writer.close("}");
                    continue;
                }
            };
            writer.line(call);
        }
    }

    /// The calls that write a table's fields, in order, each with its index.
    /// The locals of a vector or an array of structs are named after the
    /// field's index, so that they're distinct.
    fn writes(&self, writer: &mut CodeWriter, definition: &Definition) {
        for (index, field) in definition.fields.iter().enumerate() {
            let value = member(&field.name, &definition.name);
            let call = match field.field_type {
                FieldType::Scalar(Scalar::Bool) => {
                    format!("writer.SetUInt8({index}, {value} ? (byte)1 : (byte)0);")
                }
                FieldType::Scalar(scalar) => {
                    format!("writer.Set{}({index}, {value});", method_name(scalar))
                }
                FieldType::Array(scalar) => {
                    format!(
                        "writer.Set{}Array({index}, {value}.Span);",
                        method_name(scalar)
                    )
                }
                FieldType::String => format!("writer.SetString({index}, {value});"),
                FieldType::Struct(structure) => format!(
                    "{value}.Write(writer.ReserveStruct({index}, size: {}));",
                    self.struct_size(structure)
                ),
                // Each struct is written straight into the table, rather than
                // into bytes of their own that are then copied
                FieldType::StructArray(structure) => {
                    let size = self.struct_size(structure);
                    let alignment = self.schema.definition(structure).struct_layout().alignment;
                    writer.line(format!("var structs{index} = {value}.Span;"));
                    writer.statement(
                        &STYLE,
                        &format!("var bytes{index} = "),
                        &call(
                            "writer.ReserveStructArray",
                            vec![
                                text(index.to_string()),
                                text(format!("structs{index}.Length")),
                                text(format!("size: {size}")),
                                text(format!("alignment: {alignment}")),
                            ],
                        ),
                        ";",
                    );
                    writer.line(format!(
                        "for (var index = 0; index < structs{index}.Length; index++)"
                    ));
                    writer.line("{");
                    writer.indent();
                    writer.line(format!(
                        "structs{index}[index].Write(bytes{index}.Slice(index * {size}, {size}));"
                    ));
                    writer.close("}");
                    continue;
                }
                FieldType::Table(table) => {
                    writer.line(format!(
                        "writer.SetTable({index}, alignment: {});",
                        self.schema.table_alignment(table)
                    ));
                    match self.fixed.table(table) {
                        Some(size) => format!("{value}.PutTo(writer.ReserveTableBytes({size}));"),
                        None => format!("{value}.WriteTo(ref writer);"),
                    }
                }
                FieldType::Vector(table) => {
                    let write = match self.fixed.table(table) {
                        Some(size) => {
                            format!("items{index}[item].PutTo(writer.ReserveTableBytes({size}));")
                        }
                        None => format!("items{index}[item].WriteTo(ref writer);"),
                    };
                    writer.line(format!("var items{index} = {value}.Span;"));
                    writer.line(format!(
                        "var vector{index} = writer.SetVector({index}, items{index}.Length);"
                    ));
                    writer.line(format!(
                        "for (var item = 0; item < items{index}.Length; item++)"
                    ));
                    writer.line("{");
                    writer.indent();
                    writer.line(write);
                    writer.line(format!("writer.SetItemEnd(vector{index}, item);"));
                    writer.close("}");
                    continue;
                }
            };
            writer.line(call);
        }
    }

    /// A struct: a `readonly record struct` of its scalars and structs, read
    /// from the start of a span of bytes by its constructor and written into
    /// one, little-endian
    fn structure(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;
        let layout = definition.struct_layout();
        let fields = self.struct_fields(definition);

        summary(writer, &schema_docs(&definition.docs));
        open_block(writer, format!("public readonly record struct {name}"));
        summary(
            writer,
            &[format!(
                "The byte length of {} {}.",
                indefinite_article(name),
                cref(name)
            )],
        );
        writer.line(format!("public const int Size = {};", layout.size));
        writer.blank();
        summary(
            writer,
            &[format!(
                "The alignment of an array of {}s: its largest scalar's size.",
                cref(name)
            )],
        );
        writer.line(format!(
            "public const int Alignment = {};",
            layout.alignment
        ));
        writer.blank();

        self.struct_constructor(writer, name, &fields);
        for (index, field) in definition.fields.iter().enumerate() {
            writer.blank();
            summary(writer, &schema_docs(&field.docs));
            writer.line(format!(
                "public required {} {} {{ get; init; }}",
                self.value_type(field.field_type),
                fields[index].property
            ));
        }
        writer.blank();

        struct_writes(writer, name, &fields);
        writer.close("}");
    }

    /// A struct's fields, each with its property and offset in the struct
    fn struct_fields(&self, definition: &Definition) -> Vec<StructField> {
        let mut offset = 0;
        definition
            .fields
            .iter()
            .map(|field| {
                let at = offset;
                offset += match field.field_type {
                    FieldType::Scalar(scalar) => usize::from(scalar.size()),
                    FieldType::Struct(index) => self.struct_size(index),
                    _ => unreachable!("structs hold scalars and structs only"),
                };
                StructField {
                    property: member(&field.name, &definition.name),
                    field_type: field.field_type,
                    offset: at,
                }
            })
            .collect()
    }

    /// The constructor that reads a struct from the start of a span, which
    /// fails with the decode error if the span is shorter than the struct
    fn struct_constructor(&self, writer: &mut CodeWriter, name: &str, fields: &[StructField]) {
        let decode_exception = self.runtime_type("DecodeException");

        summary(
            writer,
            &[format!(
                "Reads {} {} from the start of <paramref name=\"bytes\"/>.",
                indefinite_article(name),
                cref(name)
            )],
        );
        writer.line(format!(
            "/// <exception cref=\"{decode_exception}\">The bytes are shorter than {} {}.</exception>",
            indefinite_article(name),
            cref(name)
        ));
        writer.line("[global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]");
        open_block(
            writer,
            format!("public {name}(global::System.ReadOnlySpan<byte> bytes)"),
        );
        open_block(writer, "if (bytes.Length < Size)");
        writer.line(format!(
            "throw {decode_exception}.OutOfRange(0, Size, bytes.Length);"
        ));
        writer.close("}");
        writer.blank();
        for field in fields {
            let value = match field.field_type {
                FieldType::Scalar(scalar) => scalar_read(scalar, field.offset),
                FieldType::Struct(_) => format!("new({})", bytes_from(field.offset)),
                _ => unreachable!("structs hold scalars and structs only"),
            };
            assignment(writer, &format!("{} = ", field.property), &value);
        }
        writer.close("}");
    }
    /// The view of a vector of a table, an `IReadOnlyList` whose indexer reads
    /// an item from its end and the end of the item before it, and whose
    /// enumerator reads each item's end once
    fn vector_view(&self, writer: &mut CodeWriter, definition: &Definition) {
        let table = &definition.name;
        let name = format!("{table}Vector");
        let reader = self.runtime_type("TableReader");
        let decode_exception = self.runtime_type("DecodeException");
        let generic = "global::System.Collections.Generic";

        summary(
            writer,
            &[format!(
                "A vector of {}s, which reads each item when it's asked for.",
                cref(table)
            )],
        );
        open_block(
            writer,
            format!("public readonly struct {name} : {generic}.IReadOnlyList<{table}>"),
        );
        writer.line(format!(
            "private readonly {} _vector;",
            self.runtime_type("Vector")
        ));
        writer.blank();
        writer.line(format!(
            "internal {name}({} vector)",
            self.runtime_type("Vector")
        ));
        writer.line("{");
        writer.indent();
        writer.line("_vector = vector;");
        writer.close("}");
        writer.blank();
        summary(writer, &["The number of items.".to_owned()]);
        writer.line("public int Count => _vector.Count;");
        writer.blank();
        summary(
            writer,
            &["Reads item <paramref name=\"index\"/>, without reading the others.".to_owned()],
        );
        writer.line(format!(
            "/// <exception cref=\"{decode_exception}\">The item ends before it starts or past the bytes.</exception>"
        ));
        writer.line(
            "/// <exception cref=\"global::System.ArgumentOutOfRangeException\">The vector has no item <paramref name=\"index\"/>.</exception>",
        );
        expression_body(
            writer,
            &format!("public {table} this[int index]"),
            &format!("new(new {reader}(_vector.GetInPlace(index)), 0)"),
        );
        writer.blank();
        summary(
            writer,
            &[
                "Returns an enumerator of the items in order, which reads each item's end once."
                    .to_owned(),
            ],
        );
        writer.line(
            "public Enumerator GetEnumerator() => new(_vector.ItemsInPlace().GetEnumerator());",
        );
        writer.blank();
        expression_body(
            writer,
            &format!(
                "{generic}.IEnumerator<{table}> {generic}.IEnumerable<{table}>.GetEnumerator()"
            ),
            "GetEnumerator()",
        );
        writer.blank();
        writer.line(
            "global::System.Collections.IEnumerator global::System.Collections.IEnumerable.GetEnumerator() => GetEnumerator();",
        );
        writer.blank();

        self.vector_enumerator(writer, table, &name);
        writer.close("}");
    }

    /// The enumerator of a vector's view, a struct that wraps the runtime's
    /// enumerator of the items' bytes and reads each item that it reaches
    fn vector_enumerator(&self, writer: &mut CodeWriter, table: &str, name: &str) {
        let reader = self.runtime_type("TableReader");
        let decode_exception = self.runtime_type("DecodeException");
        let generic = "global::System.Collections.Generic";

        summary(
            writer,
            &[
                format!(
                    "Reads the items of {} {} in order. Its <see cref=\"MoveNext\"/> throws a",
                    indefinite_article(name),
                    cref(name)
                ),
                format!(
                    "{} if the item ends before it starts or past the bytes, and can move on to the next item after it.",
                    cref(&decode_exception)
                ),
            ],
        );
        open_block(
            writer,
            format!("public struct Enumerator : {generic}.IEnumerator<{table}>"),
        );
        writer.line(format!(
            "private {} _items;",
            self.runtime_type("VectorItemEnumerator")
        ));
        writer.blank();
        writer.line(format!(
            "internal Enumerator({} items)",
            self.runtime_type("VectorItemEnumerator")
        ));
        writer.line("{");
        writer.indent();
        writer.line("_items = items;");
        writer.line("Current = default;");
        writer.close("}");
        writer.blank();
        summary(
            writer,
            &["The item that <see cref=\"MoveNext\"/> moved to.".to_owned()],
        );
        writer.line(format!("public {table} Current {{ get; private set; }}"));
        writer.blank();
        writer.line("readonly object global::System.Collections.IEnumerator.Current => Current;");
        writer.blank();
        summary(
            writer,
            &["Moves to the next item, and returns false after the last.".to_owned()],
        );
        writer.line(format!(
            "/// <exception cref=\"{decode_exception}\">The item ends before it starts or past the bytes.</exception>"
        ));
        writer.line("public bool MoveNext()");
        writer.line("{");
        writer.indent();
        writer.line("if (!_items.MoveNext())");
        writer.line("{");
        writer.indent();
        writer.line("return false;");
        writer.close("}");
        writer.blank();
        writer.line(format!(
            "Current = new {table}(new {reader}(_items.Current), 0);"
        ));
        writer.line("return true;");
        writer.close("}");
        writer.blank();
        writer.line(
            "readonly void global::System.Collections.IEnumerator.Reset() => throw new global::System.NotSupportedException();",
        );
        writer.blank();
        writer.line("readonly void global::System.IDisposable.Dispose() { }");
        writer.close("}");
    }

    /// The bytes of the struct at `index`
    fn struct_size(&self, index: usize) -> usize {
        self.schema.definition(index).struct_layout().size
    }

    /// The type of a field's getter
    fn value_type(&self, field_type: FieldType) -> String {
        match field_type {
            FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
            FieldType::Array(scalar) => {
                format!("global::System.ReadOnlySpan<{}>", scalar_type(scalar))
            }
            FieldType::String => "string".to_owned(),
            FieldType::Struct(index) | FieldType::Table(index) => {
                self.schema.definition(index).name.clone()
            }
            // The views that the generated code declares for the items' types
            FieldType::StructArray(index) => format!("{}Array", self.schema.definition(index).name),
            FieldType::Vector(index) => format!("{}Vector", self.schema.definition(index).name),
        }
    }

    /// The type of a field in its definition's fields, which hold arrays,
    /// structs and items as ranges of memory that they don't copy
    fn input_type(&self, field_type: FieldType) -> String {
        match field_type {
            FieldType::Array(scalar) => {
                format!("global::System.ReadOnlyMemory<{}>", scalar_type(scalar))
            }
            FieldType::StructArray(index) => format!(
                "global::System.ReadOnlyMemory<{}>",
                self.schema.definition(index).name
            ),
            FieldType::Table(index) => format!("{}Fields", self.schema.definition(index).name),
            FieldType::Vector(index) => format!(
                "global::System.ReadOnlyMemory<{}Fields>",
                self.schema.definition(index).name
            ),
            _ => self.value_type(field_type),
        }
    }
}

/// The fields of a message that `Serialize()` writes whole, each with its
/// property and position, the length of the table but for the string after
/// them, and the string's property
struct WholeMessage {
    fields: Vec<(String, FieldType, usize)>,
    length: usize,
    string: Option<String>,
}

/// A field of a struct, with its property and its offset in the struct
struct StructField {
    property: String,
    field_type: FieldType,
    offset: usize,
}

/// A struct's `SerializeArray()` of structs, `Serialize()` of itself and
/// `Write()` into the start of a span
fn struct_writes(writer: &mut CodeWriter, name: &str, fields: &[StructField]) {
    summary(
        writer,
        &[format!(
            "Returns the bytes of <paramref name=\"structs\"/> one after another, as an array of {}s holds them.",
            cref(name)
        )],
    );
    writer.line(
        "/// <exception cref=\"global::System.OverflowException\">They take more bytes than an <see cref=\"int\"/> counts.</exception>",
    );
    open_block(
        writer,
        format!("public static byte[] SerializeArray(global::System.ReadOnlySpan<{name}> structs)"),
    );
    writer.line("var bytes = new byte[checked(structs.Length * Size)];");
    open_block(
        writer,
        "for (var index = 0; index < structs.Length; index++)",
    );
    writer.line("structs[index].Write(new global::System.Span<byte>(bytes, index * Size, Size));");
    writer.close("}");
    writer.blank();
    writer.line("return bytes;");
    writer.close("}");
    writer.blank();

    summary(
        writer,
        &[format!(
            "Returns the bytes of the {}: its scalars, little-endian, one after another.",
            cref(name)
        )],
    );
    open_block(writer, "public byte[] Serialize()");
    writer.line("var bytes = new byte[Size];");
    writer.line("Write(bytes);");
    writer.blank();
    writer.line("return bytes;");
    writer.close("}");
    writer.blank();

    summary(
        writer,
        &[format!(
            "Writes the {} into the start of <paramref name=\"bytes\"/>.",
            cref(name)
        )],
    );
    writer.line(format!(
        "/// <exception cref=\"global::System.ArgumentOutOfRangeException\">The bytes are shorter than {} {}, before any of them is written.</exception>",
        indefinite_article(name),
        cref(name)
    ));
    open_block(writer, "public void Write(global::System.Span<byte> bytes)");
    writer.line(
        "global::System.ArgumentOutOfRangeException.ThrowIfLessThan(bytes.Length, Size, nameof(bytes));",
    );
    writer.blank();
    for field in fields {
        writer.line(match field.field_type {
            FieldType::Scalar(scalar) => scalar_write(scalar, field.offset, &field.property),
            FieldType::Struct(_) => {
                format!("{}.Write({});", field.property, bytes_from(field.offset))
            }
            _ => unreachable!("structs hold scalars and structs only"),
        });
    }
    writer.close("}");
}

/// Reads a scalar at `offset` in `bytes`, little-endian
fn scalar_read(scalar: Scalar, offset: usize) -> String {
    match scalar {
        Scalar::Bool => format!("bytes[{offset}] != 0"),
        Scalar::Int8 => format!("(sbyte)bytes[{offset}]"),
        Scalar::Uint8 => format!("bytes[{offset}]"),
        scalar => format!(
            "{BINARY_PRIMITIVES}.Read{}LittleEndian({})",
            binary_name(scalar),
            bytes_from(offset)
        ),
    }
}

/// Writes `value`, a scalar, at `offset` in `bytes`, little-endian: a
/// statement
fn scalar_write(scalar: Scalar, offset: usize, value: &str) -> String {
    match scalar {
        Scalar::Bool => format!("bytes[{offset}] = {value} ? (byte)1 : (byte)0;"),
        Scalar::Int8 => format!("bytes[{offset}] = (byte){value};"),
        Scalar::Uint8 => format!("bytes[{offset}] = {value};"),
        scalar => format!(
            "{BINARY_PRIMITIVES}.Write{}LittleEndian({}, {value});",
            binary_name(scalar),
            bytes_from(offset)
        ),
    }
}

/// The view of an array of a struct, a `ref struct` over the array's bytes in
/// place, whose indexer and enumerator read a struct each. Its sizes are
/// numbers, so that a struct named after one of its members, such as `Count`,
/// can't hide them.
fn struct_array_view(writer: &mut CodeWriter, definition: &Definition) {
    let structure = &definition.name;
    let name = format!("{structure}Array");
    let size = definition.struct_layout().size;

    summary(
        writer,
        &[format!(
            "An array of {}s in place, which reads each struct when it's asked for.",
            cref(structure)
        )],
    );
    open_block(writer, format!("public readonly ref struct {name}"));
    writer.line("private readonly global::System.ReadOnlySpan<byte> _bytes;");
    writer.blank();
    writer.line(format!(
        "internal {name}(global::System.ReadOnlySpan<byte> bytes)"
    ));
    writer.line("{");
    writer.indent();
    writer.line("_bytes = bytes;");
    writer.close("}");
    writer.blank();
    summary(writer, &["The number of structs.".to_owned()]);
    writer.line(format!("public int Count => _bytes.Length / {size};"));
    writer.blank();
    summary(
        writer,
        &["Reads struct <paramref name=\"index\"/>.".to_owned()],
    );
    writer.line(
        "/// <exception cref=\"global::System.ArgumentOutOfRangeException\">The array has no struct <paramref name=\"index\"/>.</exception>",
    );
    writer.line(format!("public {structure} this[int index]"));
    writer.line("{");
    writer.indent();
    writer.line("get");
    writer.line("{");
    writer.indent();
    writer.line("global::System.ArgumentOutOfRangeException.ThrowIfNegative(index);");
    writer.line(
        "global::System.ArgumentOutOfRangeException.ThrowIfGreaterThanOrEqual(index, Count);",
    );
    writer.blank();
    writer.line(format!(
        "return new {structure}(_bytes.Slice(index * {size}, {size}));"
    ));
    writer.close("}");
    writer.close("}");
    writer.blank();

    summary(writer, &["Reads every struct into a new array.".to_owned()]);
    writer.line(format!("public {structure}[] ToArray()"));
    writer.line("{");
    writer.indent();
    writer.line(format!("var structs = new {structure}[Count];"));
    writer.line("for (var index = 0; index < structs.Length; index++)");
    writer.line("{");
    writer.indent();
    writer.line(format!(
        "structs[index] = new {structure}(_bytes.Slice(index * {size}, {size}));"
    ));
    writer.close("}");
    writer.blank();
    writer.line("return structs;");
    writer.close("}");
    writer.blank();

    summary(
        writer,
        &["Returns an enumerator of the structs in order.".to_owned()],
    );
    writer.line("public Enumerator GetEnumerator() => new(_bytes);");
    writer.blank();

    struct_array_enumerator(writer, structure, &name, size);
    writer.close("}");
}

/// The enumerator of the view of an array of structs, a `ref struct` that
/// reads each struct that it reaches
fn struct_array_enumerator(writer: &mut CodeWriter, structure: &str, name: &str, size: usize) {
    summary(
        writer,
        &[format!(
            "Reads the structs of {} {} in order.",
            indefinite_article(name),
            cref(name)
        )],
    );
    open_block(writer, "public ref struct Enumerator");
    writer.line("private readonly global::System.ReadOnlySpan<byte> _bytes;");
    writer.blank();
    writer.line("/// <summary>Where the current struct starts</summary>");
    writer.line("private int _start;");
    writer.blank();
    writer.line("internal Enumerator(global::System.ReadOnlySpan<byte> bytes)");
    writer.line("{");
    writer.indent();
    writer.line("_bytes = bytes;");
    writer.line(format!("_start = -{size};"));
    writer.close("}");
    writer.blank();
    summary(
        writer,
        &["The struct that <see cref=\"MoveNext\"/> moved to.".to_owned()],
    );
    writer.line(format!(
        "public readonly {structure} Current => new(_bytes.Slice(_start, {size}));"
    ));
    writer.blank();
    summary(
        writer,
        &["Moves to the next struct, and returns false after the last.".to_owned()],
    );
    writer.line("public bool MoveNext()");
    writer.line("{");
    writer.indent();
    writer.line(format!("var next = _start + {size};"));
    writer.line("if (next >= _bytes.Length)");
    writer.line("{");
    writer.indent();
    writer.line("return false;");
    writer.close("}");
    writer.blank();
    writer.line("_start = next;");
    writer.line("return true;");
    writer.close("}");
    writer.close("}");
}

/// A line that declares a type, and the `{` of its block on a line of its
/// own, after which the block's lines are indented
fn open_block(writer: &mut CodeWriter, declaration: impl AsRef<str>) {
    writer.line(declaration);
    writer.line("{");
    writer.indent();
}

/// `declaration => value;` of a property or method, broken after the `=>`
/// when it doesn't fit on one line
fn expression_body(writer: &mut CodeWriter, declaration: &str, value: &str) {
    let line = format!("{declaration} => {value};");
    if writer.fits(&STYLE, &line) {
        writer.line(line);
        return;
    }

    writer.line(format!("{declaration} =>"));
    writer.indent();
    writer.line(format!("{value};"));
    writer.dedent();
}

/// `target value;`, broken after the `=` of `target` when it doesn't fit on
/// one line
fn assignment(writer: &mut CodeWriter, target: &str, value: &str) {
    let line = format!("{target}{value};");
    if writer.fits(&STYLE, &line) {
        writer.line(line);
        return;
    }

    writer.line(target.trim_end());
    writer.indent();
    writer.line(format!("{value};"));
    writer.dedent();
}

/// The bytes of a struct from `offset`, as a span
fn bytes_from(offset: usize) -> String {
    if offset == 0 {
        "bytes".to_owned()
    } else {
        format!("bytes[{offset}..]")
    }
}

/// A `<summary>` of lines of XML text, on one line when it's one line, and
/// each paragraph, which an empty line ends, in a `<para>` when it has several
fn summary(writer: &mut CodeWriter, lines: &[String]) {
    let paragraphs: Vec<&[String]> = lines
        .split(String::is_empty)
        .filter(|paragraph| !paragraph.is_empty())
        .collect();

    match paragraphs.as_slice() {
        [] => {}
        [[line]] => writer.line(format!("/// <summary>{line}</summary>")),
        [paragraph] => {
            writer.line("/// <summary>");
            for line in *paragraph {
                writer.line(format!("/// {line}"));
            }
            writer.line("/// </summary>");
        }
        paragraphs => {
            writer.line("/// <summary>");
            for paragraph in paragraphs {
                writer.line("/// <para>");
                for line in *paragraph {
                    writer.line(format!("/// {line}"));
                }
                writer.line("/// </para>");
            }
            writer.line("/// </summary>");
        }
    }
}

/// A schema's documentation as the XML of C#'s documentation comments, with
/// Markdown's code spans as `<c>` elements. A code span starts at every other
/// backtick, and ends at the next or at the end of the line.
fn schema_docs(docs: &[String]) -> Vec<String> {
    docs.iter()
        .map(|line| {
            let mut converted = String::with_capacity(line.len());
            for (index, part) in line.split('`').enumerate() {
                if index % 2 == 1 {
                    converted.push_str("<c>");
                    converted.push_str(&xml_text(part));
                    converted.push_str("</c>");
                } else {
                    converted.push_str(&xml_text(part));
                }
            }
            converted.trim_end().to_owned()
        })
        .collect()
}

/// Text in XML, with `&`, `<` and `>` as entities, and U+FFFE and U+FFFF,
/// which XML can't hold even as references, as replacement characters
fn xml_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\u{FFFE}' | '\u{FFFF}' => escaped.push(char::REPLACEMENT_CHARACTER),
            character => escaped.push(character),
        }
    }
    escaped
}

/// A reference to a type in documentation
fn cref(name: &str) -> String {
    format!("<see cref=\"{name}\"/>")
}

/// A field's property, in `PascalCase`, which can't be a keyword, with a `_`
/// after it if it would clash with a member of the generated code or name its
/// own definition's reader or fields
fn member(name: &str, definition: &str) -> String {
    let member = pascal_case(name);
    let names_its_type = member
        .strip_prefix(definition)
        .is_some_and(|rest| rest.is_empty() || rest == "Fields");
    if MEMBERS.contains(&member.as_str()) || names_its_type {
        format!("{member}_")
    } else {
        member
    }
}

/// The type and file name of a schema's file name, in `PascalCase`: its runs
/// of ASCII letters and digits, each from a capital letter, after `Schema`
/// when it doesn't start with a letter, such as `PointCloud` of
/// `point_cloud`. It's the schema's namespace, unless `--package` names one.
fn type_name_of(stem: &str) -> String {
    let name: String = stem
        .split(|character: char| !character.is_ascii_alphanumeric())
        .map(|word| {
            let mut characters = word.chars();
            characters
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
                .unwrap_or_default()
        })
        .collect();

    if name.starts_with(|character: char| character.is_ascii_alphabetic()) {
        name
    } else {
        format!("Schema{name}")
    }
}

/// One of `--package`'s names in a namespace: with a `_` for each `$`, which
/// Java's names may hold and C#'s can't, and as a verbatim identifier, after
/// an `@`, when it's a keyword
fn namespace_name(name: &str) -> String {
    let name = name.replace('$', "_");
    if KEYWORDS.contains(&name.as_str()) {
        format!("@{name}")
    } else {
        name
    }
}

/// The name of an offset type in the runtime's `OffsetType`
const fn offset_type_name(offset_type: OffsetType) -> &'static str {
    match offset_type {
        OffsetType::Uint8 => "UInt8",
        OffsetType::Uint16 => "UInt16",
        OffsetType::Uint32 => "UInt32",
    }
}

/// `Float32` in `GetFloat32()`, as the runtime names its getters and setters,
/// and `UInt8` for a bool, which a uint8 stores
const fn method_name(scalar: Scalar) -> &'static str {
    match scalar.stored() {
        Scalar::Bool | Scalar::Uint8 => "UInt8",
        Scalar::Int8 => "Int8",
        Scalar::Int16 => "Int16",
        Scalar::Uint16 => "UInt16",
        Scalar::Int32 => "Int32",
        Scalar::Uint32 => "UInt32",
        Scalar::Int64 => "Int64",
        Scalar::Uint64 => "UInt64",
        Scalar::Float32 => "Float32",
        Scalar::Float64 => "Float64",
    }
}

/// `UInt32` in `BinaryPrimitives.ReadUInt32LittleEndian()`, of the scalars
/// wider than a byte
fn binary_name(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::Int16 => "Int16",
        Scalar::Uint16 => "UInt16",
        Scalar::Int32 => "Int32",
        Scalar::Uint32 => "UInt32",
        Scalar::Int64 => "Int64",
        Scalar::Uint64 => "UInt64",
        Scalar::Float32 => "Single",
        Scalar::Float64 => "Double",
        Scalar::Bool | Scalar::Int8 | Scalar::Uint8 => {
            unreachable!("a byte is read and written as it is")
        }
    }
}

/// C#'s type of a scalar, which has unsigned types of every width
const fn scalar_type(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::Bool => "bool",
        Scalar::Int8 => "sbyte",
        Scalar::Uint8 => "byte",
        Scalar::Int16 => "short",
        Scalar::Uint16 => "ushort",
        Scalar::Int32 => "int",
        Scalar::Uint32 => "uint",
        Scalar::Int64 => "long",
        Scalar::Uint64 => "ulong",
        Scalar::Float32 => "float",
        Scalar::Float64 => "double",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_name_of() {
        let cases = [
            ("shapes", "Shapes"),
            ("all_types", "AllTypes"),
            ("point-cloud.v1", "PointCloudV1"),
            ("pointCloud", "PointCloud"),
            ("_hidden", "Hidden"),
            ("2d", "Schema2d"),
            ("", "Schema"),
            ("été", "T"),
        ];

        for (stem, name) in cases {
            assert_eq!(type_name_of(stem), name, "{stem}");
        }
    }

    #[test]
    fn test_namespace_name() {
        assert_eq!(namespace_name("Example"), "Example");
        assert_eq!(namespace_name("event"), "@event");
        assert_eq!(namespace_name("global"), "@global");
        assert_eq!(namespace_name("a$b"), "a_b");
    }

    #[test]
    fn test_schema_docs() {
        assert_eq!(
            schema_docs(&["A <b> & `x < y` and `open".to_owned()]),
            ["A &lt;b&gt; &amp; <c>x &lt; y</c> and <c>open</c>"]
        );
    }
}
