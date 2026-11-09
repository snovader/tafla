//! Java for `io.github.snovader.tafla`, one file per class. Messages
//! and tables read fields lazily from their bytes and are written from
//! records of their fields, which a `TableSizer` counts and a `TableWriter`
//! then writes, nested tables in place. Structs are records, read and
//! written in place with `VarHandle`s.

use std::fmt::Write;

use super::java_package::KEYWORDS;
use super::prose::article;
use super::writer::{CodeWriter, Style, call, doc_comment, escape_outside_code_spans, text};
use super::{GeneratedFile, JavaPackage, Options};
use crate::model::{Definition, Field, FieldType, Kind, Layout, OffsetType, Scalar, Schema};
use crate::naming::camel_case;

/// The runtime's package, unless [`Options::runtime`] names another
const RUNTIME: &str = "io.github.snovader.tafla";

/// The package of the generated classes, unless [`Options::package`] names
/// another. It's a placeholder that names no organization, since a project
/// puts generated classes in a package of its own.
const PACKAGE: &str = "generated";

const STYLE: Style = Style {
    width: 100,
    indent: "    ",
    trailing_comma: false,
    hug_arguments: false,
    max_arguments_width: None,
};

/// Names that getters and record components can't take, which names take a
/// `_` after too: the methods of `Object` without parameters, which they would
/// override, the methods of structs' records, and the parameters that they
/// write their components with
const MEMBERS: [&str; 12] = [
    "bytes",
    "clone",
    "finalize",
    "getClass",
    "hashCode",
    "notify",
    "notifyAll",
    "offset",
    "serialize",
    "toString",
    "wait",
    "write",
];

pub fn generate(schema: &Schema, header: &str, options: &Options) -> Vec<GeneratedFile> {
    // Java reads `\u` as a Unicode escape even in comments, such as in a
    // path on Windows, unless it's an escape of the backslash itself, which
    // javac doesn't read on
    let header = format!("// {}", header.replace("\\u", "\\u005cu"));
    let runtime = options.runtime.as_deref().unwrap_or(RUNTIME);
    let package = options
        .package
        .as_ref()
        .map_or(PACKAGE, JavaPackage::as_str);

    let file = |name: &str, contents: String| GeneratedFile {
        path: format!("{}/{name}.java", package.replace('.', "/")),
        contents,
    };

    let mut files = Vec::new();
    for (index, definition) in schema.definitions.iter().enumerate() {
        let mut writer = CodeWriter::new(STYLE.indent);
        writer.line(&header);
        writer.line(format!("package {package};"));
        writer.blank();

        match definition.kind {
            Kind::Message(layout) => {
                imports(&mut writer, runtime, definition, Some(layout));
                message(&mut writer, schema, definition, layout);
            }
            Kind::Table => {
                imports(&mut writer, runtime, definition, None);
                table(&mut writer, schema, index);
            }
            Kind::Struct(_) => {
                writer.line(format!("import {runtime}.DecodeException;"));
                writer.blank();
                writer.line("import java.lang.invoke.MethodHandles;");
                writer.line("import java.lang.invoke.VarHandle;");
                writer.line("import java.nio.ByteBuffer;");
                writer.line("import java.nio.ByteOrder;");
                writer.blank();
                structure(&mut writer, schema, definition);
            }
        }
        files.push(file(&definition.name, writer.finish()));

        if schema.is_item(index) {
            let mut writer = CodeWriter::new(STYLE.indent);
            writer.line(&header);
            writer.line(format!("package {package};"));
            writer.blank();

            let view = if definition.is_struct() {
                struct_array_view(&mut writer, runtime, definition)
            } else {
                vector_view(&mut writer, runtime, definition)
            };
            files.push(file(&view, writer.finish()));
        }
    }

    files
}

/// The view of a vector of a table, a lazy `List` whose `get()` reads an
/// item from its end and the end of the item before it, checked against the
/// count that the view read once, and returns the view's class name
fn vector_view(writer: &mut CodeWriter, runtime: &str, definition: &Definition) -> String {
    let table = &definition.name;
    let name = format!("{table}Vector");

    writer.line(format!("import {runtime}.ByteBuffer;"));
    writer.line(format!("import {runtime}.DecodeException;"));
    writer.blank();
    writer.line("import java.util.AbstractList;");
    writer.line("import java.util.RandomAccess;");
    writer.blank();

    writer.lines(&javadoc(&[format!(
        "A vector of `{table}`s, which reads each item when it's asked for"
    )]));
    writer.open(format!(
        "public final class {name} extends AbstractList<{table}> implements RandomAccess {{"
    ));
    writer.line("private final ByteBuffer buffer;");
    writer.line("private final int position;");
    writer.line("private final int size;");
    writer.blank();
    writer.lines(&javadoc(&[
        "Views the vector at `position`, as its parent's getter finds it".to_owned(),
        String::new(),
        "@throws DecodeException if the vector's count or the ends of its items aren't within the"
            .to_owned(),
        "    bytes".to_owned(),
    ]));
    writer.open(format!("{name}(ByteBuffer buffer, int position) {{"));
    writer.line("this.buffer = buffer;");
    writer.line("this.position = position;");
    writer.line("this.size = buffer.getVectorLength(position);");
    writer.close("}");
    writer.blank();

    view_size(writer, "items");

    writer.lines(&javadoc(&[
        "Reads item `index`, without reading the others".to_owned(),
        String::new(),
        "@throws DecodeException if the item ends before it starts or past the bytes".to_owned(),
        "@throws IndexOutOfBoundsException if the vector has no item `index`".to_owned(),
    ]));
    writer.line("@Override");
    writer.open(format!("public {table} get(int index) {{"));
    writer.line(format!(
        "return new {table}(buffer, buffer.getVectorItemOffset(position, size, index));"
    ));
    writer.close("}");
    writer.close("}");

    name
}

/// The view of an array of a struct, a lazy `List` whose `get()` reads a
/// struct from a view of their bytes, and returns the view's class name
fn struct_array_view(writer: &mut CodeWriter, runtime: &str, definition: &Definition) -> String {
    let structure = &definition.name;
    let name = format!("{structure}Array");

    writer.line(format!("import {runtime}.ByteBuffer;"));
    writer.line(format!("import {runtime}.DecodeException;"));
    writer.blank();
    writer.line("import java.util.AbstractList;");
    writer.line("import java.util.Objects;");
    writer.line("import java.util.RandomAccess;");
    writer.blank();

    writer.lines(&javadoc(&[format!(
        "An array of `{structure}`s, which reads each struct when it's asked for"
    )]));
    writer.open(format!(
        "public final class {name} extends AbstractList<{structure}> implements RandomAccess {{"
    ));
    writer.line("private final java.nio.ByteBuffer view;");
    writer.line("private final int size;");
    writer.blank();
    writer.lines(&javadoc(&[
        "Views the array at `position`, as its parent's getter finds it".to_owned(),
        String::new(),
        "@throws DecodeException if the array's count or its structs aren't within the bytes"
            .to_owned(),
    ]));
    writer.open(format!("{name}(ByteBuffer buffer, int position) {{"));
    writer.line(format!(
        "this.view = buffer.getStructArray(position, {structure}.SIZE);"
    ));
    writer.line(format!("this.size = view.remaining() / {structure}.SIZE;"));
    writer.close("}");
    writer.blank();

    view_size(writer, "structs");

    writer.lines(&javadoc(&[
        "Reads struct `index`".to_owned(),
        String::new(),
        "@throws IndexOutOfBoundsException if the array has no struct `index`".to_owned(),
    ]));
    writer.line("@Override");
    writer.open(format!("public {structure} get(int index) {{"));
    writer.line("Objects.checkIndex(index, size);");
    writer.line(format!(
        "return {structure}.read(view, index * {structure}.SIZE);"
    ));
    writer.close("}");
    writer.close("}");

    name
}

/// A view's `size()`, of the number of items that it read when it was made
fn view_size(writer: &mut CodeWriter, items: &str) {
    writer.lines(&javadoc(&[format!("{{@return the number of {items}}}")]));
    writer.line("@Override");
    writer.open("public int size() {");
    writer.line("return size;");
    writer.close("}");
    writer.blank();
}

fn imports(
    writer: &mut CodeWriter,
    runtime: &str,
    definition: &Definition,
    layout: Option<Layout>,
) {
    let has = |predicate: fn(FieldType) -> bool| {
        definition
            .fields
            .iter()
            .any(|field| predicate(field.field_type))
    };
    let has_vector = has(|field_type| matches!(field_type, FieldType::Vector(_)));

    let mut names = vec!["ByteBuffer"];
    if layout.is_some_and(|layout| layout.version.is_some()) {
        names.push("DecodeException");
    }
    if layout.is_some_and(|layout| layout.offset_type != OffsetType::Uint32) {
        names.push("OffsetType");
    }
    // Messages make a sizer, and tables take a sizer and a writer
    if layout.is_some() {
        names.push("TableLayout");
    }
    names.push("TableSizer");
    if layout.is_none() {
        names.push("TableWriter");
    }
    for name in names {
        writer.line(format!("import {runtime}.{name};"));
    }

    if has_vector {
        writer.blank();
        writer.line("import java.util.List;");
    }
    writer.blank();
}

/// A versioned message's `versionOf()`, which reads the version in a header
/// without checking it, so that an application can choose the reader of that
/// version before it constructs one
fn version_of(writer: &mut CodeWriter) {
    writer.lines(&doc_comment(&[
        "Returns the format version in the header of {@code bytes}, without checking it, so that an"
            .to_owned(),
        "application can choose the reader of that version".to_owned(),
        String::new(),
        "@throws DecodeException if the header doesn't fit in {@code bytes}".to_owned(),
    ]));
    writer.open("public static long versionOf(byte[] bytes) {");
    writer.line("return new ByteBuffer(bytes, LAYOUT).getVersion();");
    writer.close("}");
}

fn message(writer: &mut CodeWriter, schema: &Schema, definition: &Definition, layout: Layout) {
    let name = &definition.name;

    writer.lines(&javadoc(&schema_docs(&definition.docs)));
    writer.open(format!("public final class {name} {{"));
    if let Some(version) = layout.version {
        writer.lines(&doc_comment(&[
            "The version of the format, which the header carries and the constructor checks"
                .to_owned(),
        ]));
        // A long, as the runtime reads and writes versions, since a version
        // of the offset type `uint32` may not fit in an int
        let suffix = if i32::try_from(version).is_ok() {
            ""
        } else {
            "L"
        };
        writer.line(format!(
            "public static final long FORMAT_VERSION = {version}{suffix};"
        ));
        writer.blank();
    }
    layout_constant(writer, layout);
    writer.blank();
    writer.line("private final ByteBuffer buffer;");
    writer.blank();

    if layout.version.is_some() {
        writer.lines(&doc_comment(&[
            "@throws DecodeException if {@code bytes} are in another format version".to_owned(),
        ]));
    }
    writer.open(format!("public {name}(byte[] bytes) {{"));
    writer.line("this.buffer = new ByteBuffer(bytes, LAYOUT);");
    if layout.version.is_some() {
        writer.blank();
        writer.line("long version = buffer.getVersion();");
        writer.open("if (version != FORMAT_VERSION) {");
        let message = format!(
            "\"Expected format version \" + FORMAT_VERSION + \" of {name}, got \" + version"
        );
        writer.statement(
            &STYLE,
            "throw ",
            &call("new DecodeException", vec![text(message)]),
            ";",
        );
        writer.close("}");
    }
    writer.close("}");
    writer.blank();

    if layout.version.is_some() {
        version_of(writer);
        writer.blank();
    }

    writer.lines(&javadoc(&[format!(
        "Writes {} with every field",
        article(name)
    )]));
    writer.open("public static byte[] serialize(Fields fields) {");
    let mut arguments = vec![text(definition.fields.len().to_string()), text("LAYOUT")];
    if layout.version.is_some() {
        arguments.push(text("FORMAT_VERSION"));
    }
    writer.statement(
        &STYLE,
        "var sizer = ",
        &call("new TableSizer", arguments),
        ";",
    );
    sizes(writer, schema, definition);
    writer.blank();
    writer.line("var writer = sizer.writer();");
    writes(writer, schema, definition);
    writer.blank();
    writer.line("return writer.bytes();");
    writer.close("}");

    getters(writer, schema, definition, true);
    writer.blank();
    fields_record(writer, schema, definition);
    writer.close("}");
}

/// The layout, from the runtime's builder of layouts
fn layout_constant(writer: &mut CodeWriter, layout: Layout) {
    let mut options = Vec::new();
    if layout.sized {
        options.push(".sized(true)".to_owned());
    }
    if layout.version.is_some() {
        options.push(".versioned(true)".to_owned());
    }
    if layout.offset_type != OffsetType::Uint32 {
        options.push(format!(
            ".offsetType(OffsetType.{})",
            layout.offset_type.name().to_ascii_uppercase()
        ));
    }

    if options.is_empty() {
        writer.line("private static final TableLayout LAYOUT = TableLayout.DEFAULT;");
        return;
    }
    writer.line("private static final TableLayout LAYOUT = TableLayout.builder()");
    writer.indent();
    for option in options {
        writer.line(option);
    }
    writer.line(".build();");
    writer.dedent();
}

fn table(writer: &mut CodeWriter, schema: &Schema, index: usize) {
    let definition = schema.definition(index);
    let name = &definition.name;

    writer.lines(&javadoc(&schema_docs(&definition.docs)));
    writer.open(format!("public final class {name} {{"));
    writer.lines(&javadoc(&[
        format!(
            "The alignment that a parent pads {} to: the largest",
            article(name)
        ),
        "alignment of its arrays and nested tables, or 1".to_owned(),
    ]));
    writer.line(format!(
        "public static final int ALIGNMENT = {};",
        schema.table_alignment(index)
    ));
    writer.blank();
    writer.line("private final ByteBuffer buffer;");
    writer.line("private final int position;");
    writer.blank();
    writer.open(format!("{name}(ByteBuffer buffer, int position) {{"));
    writer.line("this.buffer = buffer;");
    writer.line("this.position = position;");
    writer.close("}");
    writer.blank();

    nested_size_and_write(writer, schema, definition);
    getters(writer, schema, definition, false);
    writer.blank();
    fields_record(writer, schema, definition);
    writer.close("}");
}

/// The static methods with which a parent sizes and writes a nested table
fn nested_size_and_write(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    let name = &definition.name;
    let fields = definition.fields.len();

    writer.lines(&javadoc(&[
        format!(
            "Adds {} to its parent's sizer, after the field or",
            article(name)
        ),
        "vector that holds it".to_owned(),
    ]));
    writer.open("public static void size(TableSizer sizer, Fields fields) {");
    writer.line(format!("var parent = sizer.startTable({fields});"));
    sizes(writer, schema, definition);
    writer.line("sizer.endTable(parent);");
    writer.close("}");
    writer.blank();

    writer.lines(&javadoc(&[
        format!(
            "Writes {} where its parent's writer is, after the",
            article(name)
        ),
        "field or vector that holds it".to_owned(),
    ]));
    writer.open("public static void write(TableWriter writer, Fields fields) {");
    writer.line(format!("var parent = writer.startTable({fields});"));
    writes(writer, schema, definition);
    writer.line("writer.endTable(parent);");
    writer.close("}");
}

#[allow(clippy::too_many_lines)]
fn structure(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    let name = &definition.name;
    // Each field, its offset in the struct, and its own struct's size
    let mut offset = 0;
    let fields: Vec<(String, FieldType, usize)> = definition
        .fields
        .iter()
        .map(|field| {
            let at = offset;
            offset += match field.field_type {
                FieldType::Scalar(scalar) => usize::from(scalar.size()),
                FieldType::Struct(index) => schema.definition(index).struct_layout().size,
                _ => unreachable!("structs hold scalars and structs only"),
            };
            (identifier(&field.name), field.field_type, at)
        })
        .collect();

    writer.lines(&javadoc(&schema_docs(&definition.docs)));
    let components = fields
        .iter()
        .map(|(field, field_type, _)| text(format!("{} {field}", value_type(*field_type, schema))))
        .collect();
    writer.statement(
        &STYLE,
        "public ",
        &call(format!("record {name}"), components),
        " {",
    );
    writer.indent();
    writer.lines(&javadoc(&[format!("The byte length of {}", article(name))]));
    writer.line(format!(
        "public static final int SIZE = {};",
        definition.struct_layout().size
    ));
    writer.blank();
    writer.lines(&javadoc(&[format!(
        "The alignment of an array of {}: its largest scalar's size",
        article(name)
    )]));
    writer.line(format!(
        "public static final int ALIGNMENT = {};",
        definition.struct_layout().alignment
    ));
    writer.blank();

    let mut handles: Vec<&str> = Vec::new();
    for (_, field_type, _) in &fields {
        if let FieldType::Scalar(scalar) = field_type
            && let Some((handle, array)) = var_handle(*scalar)
            && !handles.contains(&handle)
        {
            if handles.is_empty() {
                writer.line(
                    "// The fields lie at fixed offsets, so they're read and written in place, \
                     little-endian",
                );
            }
            handles.push(handle);
            writer.line(format!("private static final VarHandle {handle} ="));
            writer.indent();
            writer.line(format!(
                "MethodHandles.byteArrayViewVarHandle({array}.class, ByteOrder.LITTLE_ENDIAN);"
            ));
            writer.dedent();
        }
    }
    if !handles.is_empty() {
        writer.blank();
    }

    writer.lines(&javadoc(&[
        format!("Reads {} at `offset` in `bytes`", article(name)),
        String::new(),
        "@throws DecodeException if `bytes` end before it does".to_owned(),
    ]));
    writer.open(format!(
        "public static {name} read(byte[] bytes, int offset) {{"
    ));
    struct_bounds_check(writer, "bytes.length");
    let reads = fields
        .iter()
        .map(|(_, field_type, at)| {
            let at = offset_expression("offset", *at);
            text(match field_type {
                FieldType::Scalar(scalar) => struct_read(*scalar, &at),
                FieldType::Struct(index) => {
                    format!("{}.read(bytes, {at})", schema.definition(*index).name)
                }
                _ => unreachable!("structs hold scalars and structs only"),
            })
        })
        .collect();
    writer.statement(&STYLE, "return ", &call(format!("new {name}"), reads), ";");
    writer.close("}");
    writer.blank();

    writer.lines(&javadoc(&[
        format!(
            "Reads {} at `offset` in a little-endian view, such as of an array of them",
            article(name)
        ),
        String::new(),
        "@throws DecodeException if the view ends before it does".to_owned(),
    ]));
    writer.open(format!(
        "public static {name} read(ByteBuffer view, int offset) {{"
    ));
    struct_bounds_check(writer, "view.limit()");
    let reads = fields
        .iter()
        .map(|(_, field_type, at)| {
            let at = offset_expression("offset", *at);
            text(match field_type {
                FieldType::Scalar(scalar) => view_read(*scalar, &at),
                FieldType::Struct(index) => {
                    format!("{}.read(view, {at})", schema.definition(*index).name)
                }
                _ => unreachable!("structs hold scalars and structs only"),
            })
        })
        .collect();
    writer.statement(&STYLE, "return ", &call(format!("new {name}"), reads), ";");
    writer.close("}");
    writer.blank();

    writer.lines(&javadoc(&[format!(
        "{{@return the bytes of `{name}`s one after another, as an array of them holds them}}"
    )]));
    writer.open(format!(
        "public static byte[] serializeArray({name}[] items) {{"
    ));
    writer.line("var bytes = new byte[items.length * SIZE];");
    writer.open("for (int i = 0; i < items.length; i++) {");
    writer.line("items[i].write(bytes, i * SIZE);");
    writer.close("}");
    writer.blank();
    writer.line("return bytes;");
    writer.close("}");
    writer.blank();

    writer.open("public byte[] serialize() {");
    writer.line("var bytes = new byte[SIZE];");
    writer.line("write(bytes, 0);");
    writer.blank();
    writer.line("return bytes;");
    writer.close("}");
    writer.blank();

    writer.lines(&javadoc(&[format!(
        "Writes the `{name}` at `offset` in `bytes`, which must hold it"
    )]));
    writer.open("public void write(byte[] bytes, int offset) {");
    for (field, field_type, at) in &fields {
        let at = offset_expression("offset", *at);
        match field_type {
            FieldType::Scalar(scalar) => {
                writer.line(format!("{};", struct_write(*scalar, &at, field)));
            }
            FieldType::Struct(_) => writer.line(format!("{field}.write(bytes, {at});")),
            _ => unreachable!("structs hold scalars and structs only"),
        }
    }
    writer.close("}");
    writer.close("}");
}

fn fields_record(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    writer.lines(&javadoc(&[format!(
        "The fields of {}, to write",
        article(&definition.name)
    )]));
    let components = definition
        .fields
        .iter()
        .map(|field| {
            text(format!(
                "{} {}",
                input_type(field.field_type, schema),
                identifier(&field.name)
            ))
        })
        .collect();

    // A record of arrays compares them by identity, which doesn't matter for
    // values that are only written
    let has_array = definition.fields.iter().any(|field| {
        matches!(
            field.field_type,
            FieldType::Array(_) | FieldType::StructArray(_)
        )
    });
    if has_array {
        writer.line("@SuppressWarnings(\"ArrayRecordComponent\") // Written, not compared");
    }
    writer.statement(&STYLE, "public ", &call("record Fields", components), " {}");
}

/// The calls that add a table's fields to its sizer, in order
fn sizes(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    for field in &definition.fields {
        let value = format!("fields.{}()", identifier(&field.name));
        let (method, arguments) = match field.field_type {
            FieldType::Scalar(scalar) => (format!("add{}", method_name(scalar)), vec![]),
            FieldType::Array(scalar) => (
                format!("add{}Array", method_name(scalar)),
                vec![text(format!("{value}.length"))],
            ),
            FieldType::String => ("addString".to_owned(), vec![text(value)]),
            FieldType::Struct(index) => {
                let structure = &schema.definition(index).name;
                (
                    "addStruct".to_owned(),
                    vec![text(format!("{structure}.SIZE"))],
                )
            }
            FieldType::StructArray(index) => {
                let structure = &schema.definition(index).name;
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
                let table = &schema.definition(index).name;
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
                let table = &schema.definition(index).name;
                writer.line(format!("sizer.addVector({value}.size());"));
                writer.open(format!("for (var item : {value}) {{"));
                writer.line(format!("{table}.size(sizer, item);"));
                writer.close("}");
                continue;
            }
        };
        writer.statement(&STYLE, "", &call(format!("sizer.{method}"), arguments), ";");
    }
}

/// The calls that write a table's fields, in order, each with its index
fn writes(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    for (index, field) in definition.fields.iter().enumerate() {
        let value = format!("fields.{}()", identifier(&field.name));
        let position = text(index.to_string());
        let (method, arguments) = match field.field_type {
            FieldType::Scalar(Scalar::Bool) => (
                "setUint8".to_owned(),
                vec![position, text(format!("(short) ({value} ? 1 : 0)"))],
            ),
            FieldType::Scalar(scalar) => (
                format!("set{}", method_name(scalar)),
                vec![position, text(value)],
            ),
            FieldType::Array(scalar) => (
                format!("set{}Array", method_name(scalar)),
                vec![position, text(value)],
            ),
            FieldType::String => ("setString".to_owned(), vec![position, text(value)]),
            FieldType::Struct(_) => (
                "setStruct".to_owned(),
                vec![position, text(format!("{value}.serialize()"))],
            ),
            // Each struct is written straight into the table, rather than
            // into an array of their bytes that's then copied, which took
            // 1.5 times as long to write 100 structs
            FieldType::StructArray(struct_index) => {
                let structure = &schema.definition(struct_index).name;
                (
                    "setStructs".to_owned(),
                    vec![
                        position,
                        text(value),
                        text(format!("{structure}.SIZE")),
                        text(format!("{structure}.ALIGNMENT")),
                        text(format!("{structure}::write")),
                    ],
                )
            }
            FieldType::Table(table_index) => {
                let table = &schema.definition(table_index).name;
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
                // The vector's locals are named after the field's index, since
                // its name may be a keyword or another local's
                let table = &schema.definition(table_index).name;
                let vector = format!("vector{index}");
                let written = format!("written{index}");
                writer.statement(
                    &STYLE,
                    &format!("var {vector} = "),
                    &call(
                        "writer.setVector",
                        vec![position, text(format!("{value}.size()"))],
                    ),
                    ";",
                );
                // Iterated rather than indexed, since get() of a List without
                // random access, such as a LinkedList, walks the list
                writer.line(format!("var {written} = 0;"));
                writer.open(format!("for (var item : {value}) {{"));
                writer.line(format!("{table}.write(writer, item);"));
                writer.line(format!("writer.setItemEnd({vector}, {written}++);"));
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

fn getters(writer: &mut CodeWriter, schema: &Schema, definition: &Definition, is_root: bool) {
    for (index, field) in definition.fields.iter().enumerate() {
        let offset = if is_root {
            format!("buffer.rootOffset({index})")
        } else {
            format!("buffer.fieldOffset(position, {index})")
        };

        writer.blank();
        writer.lines(&javadoc(&schema_docs(&field.docs)));
        writer.open(format!(
            "public {} {}() {{",
            value_type(field.field_type, schema),
            identifier(&field.name)
        ));
        match field.field_type {
            FieldType::Scalar(Scalar::Bool) => {
                writer.line(format!("return buffer.getUint8({offset}) != 0;"));
            }
            FieldType::Scalar(scalar) => {
                writer.line(format!(
                    "return buffer.get{}({offset});",
                    method_name(scalar)
                ));
            }
            FieldType::Array(scalar) => {
                writer.line(format!(
                    "return buffer.get{}Array({offset});",
                    method_name(scalar)
                ));
            }
            FieldType::String => writer.line(format!("return buffer.getString({offset});")),
            FieldType::Struct(index) => {
                let structure = &schema.definition(index).name;
                writer.line(format!(
                    "return {structure}.read(buffer.bytes(), {offset});"
                ));
            }
            FieldType::StructArray(index) => {
                // An array, as arrays of scalars are, from a loop rather than
                // a stream to a list, which took 2.6 times as long to read
                // 100 structs
                let structure = &schema.definition(index).name;
                writer.line(format!(
                    "var view = buffer.getStructArray({offset}, {structure}.SIZE);"
                ));
                writer.line(format!(
                    "var items = new {structure}[view.remaining() / {structure}.SIZE];"
                ));
                writer.blank();
                writer.open("for (int i = 0; i < items.length; i++) {");
                writer.line(format!(
                    "items[i] = {structure}.read(view, i * {structure}.SIZE);"
                ));
                writer.close("}");
                writer.blank();
                writer.line("return items;");
            }
            FieldType::Table(table) => {
                let table = &schema.definition(table).name;
                writer.line(format!("return new {table}(buffer, {offset});"));
            }
            FieldType::Vector(table) => {
                let table = &schema.definition(table).name;
                writer.line(format!("var offsets = buffer.getVectorOffsets({offset});"));
                writer.blank();
                // Qualified, so that a schema can declare its own `Arrays`
                writer.line("return java.util.Arrays.stream(offsets)");
                writer.indent();
                writer.line(format!(".mapToObj(item -> new {table}(buffer, item))"));
                writer.line(".toList();");
                writer.dedent();
            }
        }
        writer.close("}");

        if let Some(view) = view_type(field.field_type, schema) {
            view_getter(writer, field, &view, &offset);
        }
    }
}

/// Writes the getter of a field's view, `<field>View()`, beside its getter
fn view_getter(writer: &mut CodeWriter, field: &Field, view: &str, offset: &str) {
    let getter = identifier(&field.name);
    let (description, value) = match field.field_type {
        FieldType::Array(scalar) => {
            let elements =
                format!("Views the elements of `{getter}()` in place, read-only, without");
            let description = match signed_type(scalar) {
                Some(signed) => vec![
                    elements,
                    format!("copying them, as `{signed}`s with the same bits"),
                ],
                None => vec![format!("{elements} copying them")],
            };
            let value = format!("buffer.get{}ArrayView({offset})", method_name(scalar));
            (description, value)
        }
        _ => (
            vec![format!(
                "Views the items of `{getter}()`, reading each one when it's asked for"
            )],
            format!("new {view}(buffer, {offset})"),
        ),
    };

    let mut docs = schema_docs(&field.docs);
    if !docs.is_empty() {
        docs.push(String::new());
    }
    docs.extend(description);

    writer.blank();
    writer.lines(&javadoc(&docs));
    writer.open(format!(
        "public {view} {}View() {{",
        camel_case(&field.name)
    ));
    writer.line(format!("return {value};"));
    writer.close("}");
}

/// The view that a field's view getter returns: a vector's or an array of
/// structs', which reads each item when it's asked for, or the runtime's
/// read-only view of an array of scalars, which the getter copies
fn view_type(field_type: FieldType, schema: &Schema) -> Option<String> {
    match field_type {
        FieldType::Vector(index) => Some(format!("{}Vector", schema.definition(index).name)),
        FieldType::StructArray(index) => Some(format!("{}Array", schema.definition(index).name)),
        // Qualified, so that a schema can declare its own `FloatBuffer`, and
        // since `ByteBuffer` is tafla's
        FieldType::Array(scalar) => Some(format!("java.nio.{}", nio_buffer(scalar))),
        _ => None,
    }
}

/// The JDK's buffer that views an array of scalars, of the signed type of
/// their size
const fn nio_buffer(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::Bool | Scalar::Int8 | Scalar::Uint8 => "ByteBuffer",
        Scalar::Int16 | Scalar::Uint16 => "ShortBuffer",
        Scalar::Int32 | Scalar::Uint32 => "IntBuffer",
        Scalar::Int64 | Scalar::Uint64 => "LongBuffer",
        Scalar::Float32 => "FloatBuffer",
        Scalar::Float64 => "DoubleBuffer",
    }
}

/// The signed type of an unsigned scalar's size, which a view holds its
/// elements in, with the same bits, as the runtime's view getters do
const fn signed_type(scalar: Scalar) -> Option<&'static str> {
    match scalar {
        Scalar::Uint8 => Some("byte"),
        Scalar::Uint16 => Some("short"),
        Scalar::Uint32 => Some("int"),
        Scalar::Uint64 => Some("long"),
        _ => None,
    }
}

/// A schema's documentation as Javadoc's HTML, outside code spans: with
/// `&`, `<` and `>` as entities, and `@` too, so that it can't start a tag,
/// such as `@deprecated`, which would deprecate the class, or `{@link}`, but
/// means itself as it does in the schema
fn schema_docs(docs: &[String]) -> Vec<String> {
    let html = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('@', "&#64;")
    };
    docs.iter()
        .map(|line| escape_outside_code_spans(line, html))
        .collect()
}

/// Javadoc, with Markdown's code spans as `{@code ...}`
fn javadoc(docs: &[String]) -> Vec<String> {
    // Java reads `\u` as a Unicode escape even in comments
    let docs: Vec<String> = docs
        .iter()
        .map(|line| code_spans(&line.replace("\\u", "&#92;u")))
        .collect();
    doc_comment(&docs)
}

fn code_spans(line: &str) -> String {
    let mut converted = String::new();
    for (index, part) in line.split('`').enumerate() {
        if index % 2 == 1 {
            let _ = write!(converted, "{{@code {part}}}");
        } else {
            converted.push_str(part);
        }
    }
    converted
}

fn identifier(name: &str) -> String {
    let name = camel_case(name);
    if KEYWORDS.contains(&name.as_str()) || MEMBERS.contains(&name.as_str()) {
        format!("{name}_")
    } else {
        name
    }
}

/// `Float32` in `getFloat32()`, as the runtime names its getters and setters,
/// and `Uint8` for a bool, which a uint8 stores
fn method_name(scalar: Scalar) -> String {
    let name = scalar.stored().name();
    name[..1].to_ascii_uppercase() + &name[1..]
}

/// Unsigned values widen to the next signed type, as the runtime returns
/// them, except `uint64`, which is a `long` with its bits
const fn scalar_type(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::Bool => "boolean",
        Scalar::Int8 => "byte",
        Scalar::Uint8 | Scalar::Int16 => "short",
        Scalar::Uint16 | Scalar::Int32 => "int",
        Scalar::Uint32 | Scalar::Int64 | Scalar::Uint64 => "long",
        Scalar::Float32 => "float",
        Scalar::Float64 => "double",
    }
}

fn value_type(field_type: FieldType, schema: &Schema) -> String {
    match field_type {
        FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
        FieldType::Array(scalar) => format!("{}[]", scalar_type(scalar)),
        FieldType::String => "String".to_owned(),
        FieldType::Struct(index) | FieldType::Table(index) => schema.definition(index).name.clone(),
        FieldType::StructArray(index) => format!("{}[]", schema.definition(index).name),
        FieldType::Vector(index) => format!("List<{}>", schema.definition(index).name),
    }
}

fn input_type(field_type: FieldType, schema: &Schema) -> String {
    match field_type {
        FieldType::Table(index) => format!("{}.Fields", schema.definition(index).name),
        FieldType::Vector(index) => format!("List<{}.Fields>", schema.definition(index).name),
        _ => value_type(field_type, schema),
    }
}

/// The `VarHandle` constant that reads a scalar, and the array type it views,
/// for those wider than a byte
const fn var_handle(scalar: Scalar) -> Option<(&'static str, &'static str)> {
    match scalar {
        Scalar::Bool | Scalar::Int8 | Scalar::Uint8 => None,
        Scalar::Int16 | Scalar::Uint16 => Some(("INT16", "short[]")),
        Scalar::Int32 | Scalar::Uint32 => Some(("INT32", "int[]")),
        Scalar::Int64 | Scalar::Uint64 => Some(("INT64", "long[]")),
        Scalar::Float32 => Some(("FLOAT32", "float[]")),
        Scalar::Float64 => Some(("FLOAT64", "double[]")),
    }
}

fn offset_expression(base: &str, offset: usize) -> String {
    if offset == 0 {
        base.to_owned()
    } else {
        format!("{base} + {offset}")
    }
}

/// Writes the check that a struct read at `offset` ends within `length`
/// bytes, which throws a `DecodeException` as the runtime's reads do
fn struct_bounds_check(writer: &mut CodeWriter, length: &str) {
    writer.open(format!("if (offset < 0 || offset > {length} - SIZE) {{"));
    writer.line("throw new DecodeException(");
    writer.indent();
    writer.line("String.format(");
    writer.indent();
    writer.line("\"Read exceeds the supplied buffer: %d bytes at %d of %d\",");
    writer.line("SIZE,");
    writer.line("offset,");
    writer.line(length);
    writer.dedent();
    writer.line(")");
    writer.dedent();
    writer.line(");");
    writer.close("}");
    writer.blank();
}

fn struct_read(scalar: Scalar, at: &str) -> String {
    match scalar {
        Scalar::Bool => format!("bytes[{at}] != 0"),
        Scalar::Int8 => format!("bytes[{at}]"),
        Scalar::Uint8 => format!("(short) Byte.toUnsignedInt(bytes[{at}])"),
        Scalar::Int16 => format!("(short) INT16.get(bytes, {at})"),
        Scalar::Uint16 => format!("Short.toUnsignedInt((short) INT16.get(bytes, {at}))"),
        Scalar::Int32 => format!("(int) INT32.get(bytes, {at})"),
        Scalar::Uint32 => format!("Integer.toUnsignedLong((int) INT32.get(bytes, {at}))"),
        Scalar::Int64 | Scalar::Uint64 => format!("(long) INT64.get(bytes, {at})"),
        Scalar::Float32 => format!("(float) FLOAT32.get(bytes, {at})"),
        Scalar::Float64 => format!("(double) FLOAT64.get(bytes, {at})"),
    }
}

/// Reads a scalar from a little-endian view, as the runtime's getters widen
/// it
fn view_read(scalar: Scalar, at: &str) -> String {
    match scalar {
        Scalar::Bool => format!("view.get({at}) != 0"),
        Scalar::Int8 => format!("view.get({at})"),
        Scalar::Uint8 => format!("(short) Byte.toUnsignedInt(view.get({at}))"),
        Scalar::Int16 => format!("view.getShort({at})"),
        Scalar::Uint16 => format!("Short.toUnsignedInt(view.getShort({at}))"),
        Scalar::Int32 => format!("view.getInt({at})"),
        Scalar::Uint32 => format!("Integer.toUnsignedLong(view.getInt({at}))"),
        Scalar::Int64 | Scalar::Uint64 => format!("view.getLong({at})"),
        Scalar::Float32 => format!("view.getFloat({at})"),
        Scalar::Float64 => format!("view.getDouble({at})"),
    }
}

fn struct_write(scalar: Scalar, at: &str, field: &str) -> String {
    match scalar {
        Scalar::Bool => format!("bytes[{at}] = (byte) ({field} ? 1 : 0)"),
        Scalar::Int8 => format!("bytes[{at}] = {field}"),
        Scalar::Uint8 => format!("bytes[{at}] = (byte) {field}"),
        Scalar::Int16 => format!("INT16.set(bytes, {at}, {field})"),
        Scalar::Uint16 => format!("INT16.set(bytes, {at}, (short) {field})"),
        Scalar::Int32 => format!("INT32.set(bytes, {at}, {field})"),
        Scalar::Uint32 => format!("INT32.set(bytes, {at}, (int) {field})"),
        Scalar::Int64 | Scalar::Uint64 => format!("INT64.set(bytes, {at}, {field})"),
        Scalar::Float32 => format!("FLOAT32.set(bytes, {at}, {field})"),
        Scalar::Float64 => format!("FLOAT64.set(bytes, {at}, {field})"),
    }
}
