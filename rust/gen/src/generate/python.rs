//! Python for `tafla`. Messages and tables are classes that read
//! fields lazily from their bytes, written from `TypedDict`s of their fields,
//! and structs are frozen dataclasses packed with `struct`.

use super::prose::article;
use super::writer::{CodeWriter, Expr, Style, call, text};
use super::{GeneratedFile, Options};
use crate::model::{Definition, Field, FieldType, Kind, Layout, OffsetType, Scalar, Schema};

/// The runtime's package, unless [`Options::runtime`] names another
const RUNTIME: &str = "tafla";

const STYLE: Style = Style {
    width: 88,
    indent: "    ",
    trailing_comma: true,
    hug_arguments: true,
    max_arguments_width: None,
};

/// Python keywords and soft keywords, which names get a `_` after
const KEYWORDS: [&str; 38] = [
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "case", "class",
    "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
    "import", "in", "is", "lambda", "match", "nonlocal", "not", "or", "pass", "raise", "return",
    "try", "type", "while", "with", "yield",
];

/// Names that a field's getter or attribute would shadow, which names get a
/// `_` after too: the classes' own methods, the parameter of `serialize()`
/// beside its keyword arguments, and the built-in types that getters return,
/// which their annotations find in the class
const MEMBERS: [&str; 13] = [
    "bool",
    "cls",
    "float",
    "int",
    "list",
    "memoryview",
    "read",
    "read_array",
    "serialize",
    "serialize_array",
    "str",
    "version_of",
    "write",
];

pub fn generate(
    schema: &Schema,
    stem: &str,
    header: &str,
    options: &Options,
) -> Vec<GeneratedFile> {
    let runtime = options.runtime.as_deref().unwrap_or(RUNTIME);

    let mut writer = CodeWriter::new(STYLE.indent);
    writer.line(format!("# {header}"));
    writer.blank();
    imports(&mut writer, schema, runtime);

    for (index, definition) in schema.definitions.iter().enumerate() {
        writer.blanks(2);
        match definition.kind {
            Kind::Message(layout) => message(&mut writer, schema, definition, layout),
            Kind::Table => table(&mut writer, schema, index),
            Kind::Struct(_) => structure(&mut writer, schema, definition),
        }

        if !definition.is_struct() {
            writer.blanks(2);
            fields_type(&mut writer, schema, definition);
        }

        if schema.is_item(index) {
            writer.blanks(2);
            if definition.is_struct() {
                struct_array_view(&mut writer, schema, definition);
            } else {
                vector_view(&mut writer, definition);
            }
        }
    }

    vec![GeneratedFile {
        path: format!("{stem}.py"),
        contents: writer.finish(),
    }]
}

fn imports(writer: &mut CodeWriter, schema: &Schema, runtime: &str) {
    let has = |predicate: fn(&Definition) -> bool| schema.definitions.iter().any(predicate);
    let has_message = has(|definition| matches!(definition.kind, Kind::Message(_)));
    let has_table = has(|definition| matches!(definition.kind, Kind::Message(_) | Kind::Table));
    let has_nested = has(|definition| definition.kind == Kind::Table);
    let has_struct = has(Definition::is_struct);
    let has_field = |predicate: fn(FieldType) -> bool| {
        schema
            .definitions
            .iter()
            .flat_map(|definition| &definition.fields)
            .any(|field| predicate(field.field_type))
    };
    let has_array = has_field(|field_type| matches!(field_type, FieldType::Array(_)));
    // Vectors and arrays of structs have views, which iterate their items
    let has_view = has_field(|field_type| {
        matches!(field_type, FieldType::Vector(_) | FieldType::StructArray(_))
    });
    let has_sequence = has_struct || has_view;
    let versioned = has(|definition| {
        matches!(
            definition.kind,
            Kind::Message(Layout {
                version: Some(_),
                ..
            })
        )
    });

    let mut abc = Vec::new();
    if has_message || has_array || has_struct {
        abc.push("Buffer");
    }
    if has_array {
        abc.push("Iterable");
    }
    if has_view {
        abc.push("Iterator");
    }
    if has_sequence {
        abc.push("Sequence");
    }

    let mut typing = Vec::new();
    if has_struct {
        typing.push("ClassVar");
    }
    if has_message || has_nested {
        typing.push("Final");
    }
    if has_struct {
        typing.push("Literal");
        typing.push("Self");
    }
    if has_table {
        typing.push("TypedDict");
    }
    if has_message {
        typing.push("Unpack");
    }

    let mut buffer = Vec::new();
    if has_table {
        buffer.push("ByteBuffer");
    }
    if versioned || has_struct {
        buffer.push("DecodeError");
    }
    if has_message {
        buffer.push("TableLayout");
    }
    if has_table {
        buffer.push("TableWriter");
    }

    let standard: Vec<(&str, Vec<&str>)> = vec![
        ("collections.abc", abc),
        (
            "dataclasses",
            if has_struct {
                vec!["dataclass"]
            } else {
                vec![]
            },
        ),
        (
            "struct",
            if has_struct {
                vec!["Struct", "error"]
            } else {
                vec![]
            },
        ),
        ("typing", typing),
    ];
    for (module, names) in standard {
        if !names.is_empty() {
            from_import(writer, module, &names);
        }
    }
    // A schema without definitions imports nothing
    if !buffer.is_empty() {
        writer.blank();
        from_import(writer, runtime, &buffer);
    }
}

fn from_import(writer: &mut CodeWriter, module: &str, names: &[&str]) {
    let statement = format!("from {module} import {}", names.join(", "));
    if statement.len() <= STYLE.width {
        writer.line(statement);
        return;
    }

    writer.open(format!("from {module} import ("));
    for name in names {
        writer.line(format!("{name},"));
    }
    writer.close(")");
}

fn message(writer: &mut CodeWriter, schema: &Schema, definition: &Definition, layout: Layout) {
    let name = &definition.name;

    writer.open(format!("class {name}:"));
    docstring(writer, &definition.docs);
    writer.blank();
    writer.line("__slots__ = (\"_buffer\",)");
    writer.blank();

    if let Some(version) = layout.version {
        writer.line(format!("FORMAT_VERSION: Final = {version}"));
        docstring(
            writer,
            &[
                "The version of the format, which the header carries and the constructor"
                    .to_owned(),
                "checks".to_owned(),
            ],
        );
        writer.blank();
    }
    writer.line(format!("_LAYOUT: Final = {}", layout_expression(layout)));
    writer.blank();

    writer.open("def __init__(self, data: Buffer) -> None:");
    if layout.version.is_some() {
        docstring(
            writer,
            &[
                "Raises `TypeError` if `data` isn't C-contiguous, and `DecodeError` if".to_owned(),
                "it's in another format version.".to_owned(),
            ],
        );
    } else {
        docstring(
            writer,
            &["Raises `TypeError` if `data` isn't C-contiguous.".to_owned()],
        );
    }
    writer.line("self._buffer = ByteBuffer(data, self._LAYOUT)");
    if layout.version.is_some() {
        writer.blank();
        writer.line("version = self._buffer.get_version()");
        writer.open("if version != self.FORMAT_VERSION:");
        let message = format!(
            "f\"Expected format version {{self.FORMAT_VERSION}} of {name}, got {{version}}\""
        );
        writer.statement(
            &STYLE,
            "raise ",
            &call("DecodeError", vec![text(message)]),
            "",
        );
        writer.dedent();
    }
    writer.dedent();
    writer.blank();

    if layout.version.is_some() {
        version_of(writer);
        writer.blank();
    }

    writer.line("@classmethod");
    writer.statement(
        &STYLE,
        "def ",
        &call(
            "serialize",
            vec![text("cls"), text(format!("**fields: Unpack[{name}Fields]"))],
        ),
        " -> memoryview:",
    );
    writer.indent();
    docstring(
        writer,
        &[format!("Writes {} with every field", article(name))],
    );
    let mut arguments = vec![
        text(definition.fields.len().to_string()),
        text("cls._LAYOUT"),
    ];
    if layout.version.is_some() {
        arguments.push(text("cls.FORMAT_VERSION"));
    }
    writer.statement(&STYLE, "writer = ", &call("TableWriter", arguments), "");
    writes(writer, schema, definition);
    writer.blank();
    writer.line("return writer.bytes()");
    writer.dedent();

    getters(writer, schema, definition, true);
    writer.dedent();
}

/// A versioned message's `version_of()`, which reads the version in a header
/// without checking it, so that an application can choose the reader of that
/// version before it constructs one
fn version_of(writer: &mut CodeWriter) {
    writer.line("@classmethod");
    writer.open("def version_of(cls, data: Buffer) -> int:");
    docstring(
        writer,
        &[
            "Returns the format version in the header of `data`, without checking".to_owned(),
            "it, so that an application can choose the reader of that version.".to_owned(),
            "Raises `TypeError` if `data` isn't C-contiguous, and `DecodeError` if".to_owned(),
            "the header doesn't fit in it.".to_owned(),
        ],
    );
    writer.line("return ByteBuffer(data, cls._LAYOUT).get_version()");
    writer.dedent();
}

fn table(writer: &mut CodeWriter, schema: &Schema, index: usize) {
    let definition = schema.definition(index);
    let name = &definition.name;

    writer.open(format!("class {name}:"));
    if !definition.docs.is_empty() {
        docstring(writer, &definition.docs);
        writer.blank();
    }
    writer.line("__slots__ = (\"_buffer\", \"_position\")");
    writer.blank();

    let alignment = schema.table_alignment(index);
    writer.line(format!("ALIGNMENT: Final = {alignment}"));
    docstring(
        writer,
        &[
            format!(
                "The alignment that a parent pads {} to: the largest",
                article(name)
            ),
            "alignment of its arrays and nested tables, or 1".to_owned(),
        ],
    );
    writer.blank();

    writer.open("def __init__(self, buffer: ByteBuffer, position: int) -> None:");
    docstring(
        writer,
        &["Reads the table at `position`, as its parent's getter finds it".to_owned()],
    );
    writer.line("self._buffer = buffer");
    writer.line("self._position = position");
    writer.dedent();
    writer.blank();

    writer.line("@staticmethod");
    writer.open(format!(
        "def write(writer: TableWriter, fields: {name}Fields) -> None:"
    ));
    docstring(
        writer,
        &[
            format!(
                "Writes {} where its parent's writer is, after the",
                article(name)
            ),
            "field or vector that holds it".to_owned(),
        ],
    );
    writer.line(format!(
        "parent = writer.start_table({})",
        definition.fields.len()
    ));
    writes(writer, schema, definition);
    writer.line("writer.end_table(parent)");
    writer.dedent();

    getters(writer, schema, definition, false);
    writer.dedent();
}

#[allow(clippy::too_many_lines)]
fn structure(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    let name = &definition.name;
    let format: String = schema
        .struct_scalars(definition)
        .into_iter()
        .map(struct_code)
        .collect();
    // The struct's values as its Struct unpacks them, a struct within it as
    // a struct of its own
    let construction = construction(schema, definition, "cls", &mut 0);
    // The runtime builds the error, which formats an offset of any size
    let does_not_fit = call(
        "DecodeError.out_of_range",
        vec![
            text("offset"),
            text("cls.SIZE"),
            text("memoryview(data).nbytes"),
        ],
    );
    let values_do_not_fit = call(
        "ValueError",
        vec![text(format!("\"{name}'s values don't fit its fields\""))],
    );
    let not_contiguous = call(
        "TypeError",
        vec![text(format!(
            "\"{name} can't read a buffer that isn't C-contiguous\""
        ))],
    );

    writer.line("@dataclass(frozen=True, slots=True)");
    writer.open(format!("class {name}:"));
    docstring(writer, &definition.docs);
    writer.blank();
    writer.line(format!(
        "SIZE: ClassVar[int] = {}",
        definition.struct_layout().size
    ));
    docstring(writer, &[format!("The byte length of {}", article(name))]);
    let alignment = definition.struct_layout().alignment;
    writer.line(format!(
        "ALIGNMENT: ClassVar[Literal[{alignment}]] = {alignment}"
    ));
    docstring(
        writer,
        &[format!(
            "The alignment of an array of {}: its largest scalar's size",
            article(name)
        )],
    );
    writer.line(format!("_STRUCT: ClassVar[Struct] = Struct(\"<{format}\")"));
    writer.blank();
    for field in &definition.fields {
        writer.line(format!(
            "{}: {}",
            identifier(&field.name),
            value_type(field.field_type, schema)
        ));
        docstring(writer, &field.docs);
    }
    writer.blank();

    writer.line("@classmethod");
    writer.open("def read(cls, data: Buffer, offset: int = 0) -> Self:");
    docstring(
        writer,
        &[
            format!("Reads {} at `offset` in `data`.", article(name)),
            String::new(),
            "Raises `DecodeError` if `offset` is negative or `data` ends before it,".to_owned(),
            "and `TypeError` if `data` isn't C-contiguous.".to_owned(),
        ],
    );
    // unpack_from() counts a negative offset from the end of the data, raises
    // OverflowError for one beyond a C ssize_t, and BufferError for data that
    // isn't C-contiguous
    writer.open("if offset < 0:");
    writer.statement(&STYLE, "raise ", &does_not_fit, "");
    writer.dedent();
    writer.blank();
    writer.open("try:");
    writer.line("values = cls._STRUCT.unpack_from(data, offset)");
    writer.dedent();
    writer.open("except (error, OverflowError) as cause:");
    writer.statement(&STYLE, "raise ", &does_not_fit, " from cause");
    writer.dedent();
    writer.open("except BufferError as cause:");
    writer.statement(&STYLE, "raise ", &not_contiguous, " from cause");
    writer.dedent();
    writer.blank();
    writer.statement(&STYLE, "return ", &construction, "");
    writer.dedent();
    writer.blank();

    writer.line("@classmethod");
    writer.open("def read_array(cls, data: Buffer) -> list[Self]:");
    docstring(
        writer,
        &[
            format!("Reads the `{name}`s one after another in `data`."),
            String::new(),
            "Raises `DecodeError` if `data` isn't a whole number of them, and".to_owned(),
            "`TypeError` if it isn't C-contiguous.".to_owned(),
        ],
    );
    writer.open("try:");
    // As Ruff breaks it: on one line, or the brackets' contents on a line of
    // their own, or the construction and the loop on lines of their own
    let element = format!(
        "{} for values in cls._STRUCT.iter_unpack(data)",
        construction.flat()
    );
    let comprehension = format!("return [{element}]");
    if writer.fits(&STYLE, &comprehension) {
        writer.line(comprehension);
    } else {
        writer.open("return [");
        if writer.fits(&STYLE, &element) {
            writer.line(element);
        } else {
            writer.statement(&STYLE, "", &construction, "");
            writer.line("for values in cls._STRUCT.iter_unpack(data)");
        }
        writer.close("]");
    }
    writer.dedent();
    writer.open("except error as cause:");
    writer.statement(
        &STYLE,
        "raise ",
        &call(
            "DecodeError",
            vec![text(format!(
                "f\"{name}s don't fit in {{memoryview(data).nbytes}} bytes\""
            ))],
        ),
        " from cause",
    );
    writer.dedent();
    writer.open("except BufferError as cause:");
    writer.statement(&STYLE, "raise ", &not_contiguous, " from cause");
    writer.dedent();
    writer.dedent();
    writer.blank();

    writer.open("def serialize(self) -> bytes:");
    docstring(
        writer,
        &[
            format!("Returns the bytes of {}.", article(name)),
            String::new(),
            "Raises `ValueError` if a field's value doesn't fit its type.".to_owned(),
        ],
    );
    let mut fields = Vec::new();
    push_values(schema, definition, "self", &mut fields);
    // struct raises its error for an integer out of range and OverflowError
    // for a float too large, neither of which is the runtime's ValueError
    writer.open("try:");
    writer.statement(&STYLE, "return ", &call("self._STRUCT.pack", fields), "");
    writer.dedent();
    writer.open("except (error, OverflowError) as cause:");
    writer.statement(&STYLE, "raise ", &values_do_not_fit, " from cause");
    writer.dedent();
    writer.dedent();
    writer.blank();

    writer.line("@classmethod");
    writer.open("def serialize_array(cls, items: Sequence[Self]) -> bytearray:");
    docstring(
        writer,
        &[
            format!("Returns the bytes of `{name}`s one after another, as an array of them"),
            "holds them.".to_owned(),
            String::new(),
            "Raises `ValueError` if a field's value doesn't fit its type.".to_owned(),
        ],
    );
    // The size and the method in locals, rather than looked up on the class
    // for each struct, which took 13% longer
    writer.line("size = cls.SIZE");
    writer.line("pack_into = cls._STRUCT.pack_into");
    writer.line("data = bytearray(len(items) * size)");
    let mut arguments = vec![text("data"), text("index * size")];
    push_values(schema, definition, "item", &mut arguments);
    writer.open("try:");
    writer.open("for index, item in enumerate(items):");
    writer.statement(&STYLE, "", &call("pack_into", arguments), "");
    writer.dedent();
    writer.dedent();
    writer.open("except (error, OverflowError) as cause:");
    writer.statement(&STYLE, "raise ", &values_do_not_fit, " from cause");
    writer.dedent();
    writer.blank();
    writer.line("return data");
    writer.dedent();
    writer.dedent();
}

/// The view of a vector of a table, which reads each item when it's asked
/// for, from the item's end and the end of the item before it
fn vector_view(writer: &mut CodeWriter, definition: &Definition) {
    let table = &definition.name;

    writer.open(format!("class {table}Vector:"));
    docstring(
        writer,
        &[format!(
            "A vector of `{table}`s, which reads each item when it's asked for"
        )],
    );
    writer.blank();
    writer.line("__slots__ = (\"_buffer\", \"_length\", \"_position\")");
    writer.blank();

    writer.open("def __init__(self, buffer: ByteBuffer, position: int) -> None:");
    docstring(
        writer,
        &[
            "Views the vector at `position`, as its parent's getter finds it.".to_owned(),
            String::new(),
            "Raises `DecodeError` if the vector's count or the ends of its items lie".to_owned(),
            "outside the bytes.".to_owned(),
        ],
    );
    writer.line("self._buffer = buffer");
    writer.line("self._position = position");
    writer.line("self._length = buffer.get_vector_length(position)");
    writer.dedent();
    writer.blank();

    view_length(writer);

    writer.open(format!("def __getitem__(self, index: int) -> {table}:"));
    docstring(
        writer,
        &[
            "Reads item `index`, without reading the others, counting from the end if".to_owned(),
            "it's negative.".to_owned(),
            String::new(),
            "Raises `IndexError` if the vector has no item `index`, and".to_owned(),
            "`DecodeError` if the item lies outside the bytes.".to_owned(),
        ],
    );
    writer.open("if index < 0:");
    writer.line("index += self._length");
    writer.dedent();
    writer.line("offset = self._buffer.get_vector_offset(self._position, index)");
    writer.line(format!("return {table}(self._buffer, offset)"));
    writer.dedent();
    writer.blank();

    writer.open(format!("def __iter__(self) -> Iterator[{table}]:"));
    docstring(
        writer,
        &[
            "Reads the items one after another, after reading every item's end.".to_owned(),
            String::new(),
            "Raises `DecodeError` if an item lies outside the bytes, before reading any."
                .to_owned(),
        ],
    );
    writer.line("buffer = self._buffer");
    writer.open("for offset in buffer.get_vector_offsets(self._position):");
    writer.line(format!("yield {table}(buffer, offset)"));
    writer.dedent();
    writer.dedent();
    writer.dedent();
}

/// The view of an array of a struct, which reads each struct when it's
/// asked for, from a view of their bytes
fn struct_array_view(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    let structure = &definition.name;

    writer.open(format!("class {structure}Array:"));
    docstring(
        writer,
        &[format!(
            "An array of `{structure}`s, which reads each struct when it's asked for"
        )],
    );
    writer.blank();
    writer.line("__slots__ = (\"_data\", \"_length\")");
    writer.blank();

    writer.open("def __init__(self, buffer: ByteBuffer, position: int) -> None:");
    docstring(
        writer,
        &[
            "Views the array at `position`, as its parent's getter finds it.".to_owned(),
            String::new(),
            "Raises `DecodeError` if the array lies outside the bytes.".to_owned(),
        ],
    );
    writer.line(format!(
        "self._data = buffer.get_struct_array(position, {structure}.SIZE)"
    ));
    writer.line(format!(
        "self._length = len(self._data) // {structure}.SIZE"
    ));
    writer.dedent();
    writer.blank();

    view_length(writer);

    writer.open(format!("def __getitem__(self, index: int) -> {structure}:"));
    docstring(
        writer,
        &[
            "Reads struct `index`, counting from the end if it's negative.".to_owned(),
            String::new(),
            "Raises `IndexError` if the array has no struct `index`.".to_owned(),
        ],
    );
    writer.open("if index < 0:");
    writer.line("index += self._length");
    writer.dedent();
    // The message leaves out the caller's index, as a list's does, since
    // converting one of more digits than Python's limit to a string fails
    writer.open("if not 0 <= index < self._length:");
    writer.statement(
        &STYLE,
        "raise ",
        &call(
            "IndexError",
            vec![text(
                "f\"Struct index is outside the array of {self._length}\"",
            )],
        ),
        "",
    );
    writer.dedent();
    writer.line(format!(
        "return {structure}.read(self._data, index * {structure}.SIZE)"
    ));
    writer.dedent();
    writer.blank();

    // The structs' values as their Struct unpacks them, as read_array()
    // reads them
    let construction = construction(schema, definition, structure, &mut 0);
    writer.open(format!("def __iter__(self) -> Iterator[{structure}]:"));
    docstring(writer, &["Reads the structs one after another".to_owned()]);
    writer.open(format!(
        "for values in {structure}._STRUCT.iter_unpack(self._data):"
    ));
    writer.statement(&STYLE, "yield ", &construction, "");
    writer.dedent();
    writer.dedent();
    writer.dedent();
}

/// A view's `__len__()`, of the length that it read when it was made
fn view_length(writer: &mut CodeWriter) {
    writer.open("def __len__(self) -> int:");
    writer.line("return self._length");
    writer.dedent();
    writer.blank();
}

/// The call that makes a struct of the values that its `Struct` unpacks, from
/// `values[*index]` on, which it moves past them. Each is a positional
/// argument, even in a struct of scalars alone: Python 3.14 specializes such a
/// call to enter the class's `__init__` directly, but not `cls(*values)`, with
/// which reading 100 structs of three scalars took 5% longer.
fn construction(
    schema: &Schema,
    definition: &Definition,
    constructor: &str,
    index: &mut usize,
) -> Expr {
    let arguments = definition
        .fields
        .iter()
        .map(|field| match field.field_type {
            FieldType::Scalar(_) => {
                let value = text(format!("values[{index}]"));
                *index += 1;
                value
            }
            FieldType::Struct(inner) => {
                let inner = schema.definition(inner);
                construction(schema, inner, &inner.name, index)
            }
            _ => unreachable!("structs hold scalars and structs only"),
        })
        .collect();

    call(constructor, arguments)
}

/// Pushes the attributes of a struct's scalars, those of a struct within it
/// in its place, as its `Struct` packs them
fn push_values(schema: &Schema, definition: &Definition, owner: &str, values: &mut Vec<Expr>) {
    for field in &definition.fields {
        let attribute = format!("{owner}.{}", identifier(&field.name));
        match field.field_type {
            FieldType::Scalar(_) => values.push(text(attribute)),
            FieldType::Struct(inner) => {
                push_values(schema, schema.definition(inner), &attribute, values);
            }
            _ => unreachable!("structs hold scalars and structs only"),
        }
    }
}

fn fields_type(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    writer.open(format!("class {}Fields(TypedDict):", definition.name));
    docstring(
        writer,
        &[format!(
            "The fields of {}, to write",
            article(&definition.name)
        )],
    );
    writer.blank();
    for field in &definition.fields {
        writer.line(format!(
            "{}: {}",
            identifier(&field.name),
            input_type(field.field_type, schema)
        ));
        docstring(writer, &field.docs);
    }
    writer.dedent();
}

/// The calls that write a table's fields, in order, each with its index
fn writes(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    for (index, field) in definition.fields.iter().enumerate() {
        let value = format!("fields[\"{}\"]", identifier(&field.name));
        let position = text(index.to_string());
        let (method, arguments) = match field.field_type {
            FieldType::Scalar(Scalar::Bool) => (
                "set_uint8".to_owned(),
                vec![position, text(format!("1 if {value} else 0"))],
            ),
            FieldType::Scalar(scalar) => (
                format!("set_{}", scalar.name()),
                vec![position, text(value)],
            ),
            FieldType::Array(scalar) => (
                format!("set_{}_array", scalar.name()),
                vec![position, text(value)],
            ),
            FieldType::String => ("set_string".to_owned(), vec![position, text(value)]),
            FieldType::Struct(_) => (
                "set_struct".to_owned(),
                vec![position, call(format!("{value}.serialize"), vec![])],
            ),
            FieldType::StructArray(struct_index) => {
                let structure = &schema.definition(struct_index).name;
                (
                    "set_struct_array".to_owned(),
                    vec![
                        position,
                        call(format!("{structure}.serialize_array"), vec![text(value)]),
                        text(format!("{structure}.SIZE")),
                        text(format!("{structure}.ALIGNMENT")),
                    ],
                )
            }
            FieldType::Table(table_index) => {
                let table = &schema.definition(table_index).name;
                writer.line(format!("writer.set_table({index}, {table}.ALIGNMENT)"));
                writer.statement(
                    &STYLE,
                    "",
                    &call(format!("{table}.write"), vec![text("writer"), text(value)]),
                    "",
                );
                continue;
            }
            FieldType::Vector(table_index) => {
                let table = &schema.definition(table_index).name;
                writer.statement(
                    &STYLE,
                    "vector = ",
                    &call(
                        "writer.set_vector",
                        vec![position, text(format!("len({value})"))],
                    ),
                    "",
                );
                writer.open(format!("for i, item in enumerate({value}):"));
                writer.line(format!("{table}.write(writer, item)"));
                writer.line("writer.set_item_end(vector, i)");
                writer.dedent();
                continue;
            }
        };
        writer.statement(&STYLE, "", &call(format!("writer.{method}"), arguments), "");
    }
}

fn getters(writer: &mut CodeWriter, schema: &Schema, definition: &Definition, is_root: bool) {
    for (index, field) in definition.fields.iter().enumerate() {
        let offset = offset_expression(is_root, index);

        writer.blank();
        writer.open(format!(
            "def {}(self) -> {}:",
            identifier(&field.name),
            value_type(field.field_type, schema)
        ));
        docstring(writer, &field.docs);

        match field.field_type {
            FieldType::Scalar(Scalar::Bool) => {
                return_bool(writer, &call("self._buffer.get_uint8", vec![offset]));
            }
            FieldType::Scalar(scalar) => {
                let get = call(format!("self._buffer.get_{}", scalar.name()), vec![offset]);
                writer.statement(&STYLE, "return ", &get, "");
            }
            FieldType::Array(scalar) => {
                let get = call(
                    format!("self._buffer.get_{}_array", scalar.name()),
                    vec![offset],
                );
                writer.statement(&STYLE, "return ", &get, "");
            }
            FieldType::String => {
                writer.statement(
                    &STYLE,
                    "return ",
                    &call("self._buffer.get_string", vec![offset]),
                    "",
                );
            }
            FieldType::Struct(index) => {
                let structure = &schema.definition(index).name;
                let bytes = call(
                    "self._buffer.get_struct",
                    vec![offset, text(format!("{structure}.SIZE"))],
                );
                let read = call(format!("{structure}.read"), vec![bytes]);
                writer.statement(&STYLE, "return ", &read, "");
            }
            FieldType::StructArray(index) => {
                let structure = &schema.definition(index).name;
                let bytes = call(
                    "self._buffer.get_struct_array",
                    vec![offset, text(format!("{structure}.SIZE"))],
                );
                let read = call(format!("{structure}.read_array"), vec![bytes]);
                writer.statement(&STYLE, "return ", &read, "");
            }
            FieldType::Table(index) => {
                let table = &schema.definition(index).name;
                let read = call(table.clone(), vec![text("self._buffer"), offset]);
                writer.statement(&STYLE, "return ", &read, "");
            }
            FieldType::Vector(index) => {
                let table = &schema.definition(index).name;
                writer.statement(
                    &STYLE,
                    "offsets = ",
                    &call("self._buffer.get_vector_offsets", vec![offset]),
                    "",
                );
                writer.line(format!(
                    "return [{table}(self._buffer, offset) for offset in offsets]"
                ));
            }
        }
        writer.dedent();

        if let Some(view) = view_type(field.field_type, schema) {
            view_getter(writer, field, &view, offset_expression(is_root, index));
        }
    }
}

/// The getter of a view of a vector or an array of structs, which reads each
/// item when it's asked for, rather than all of them as the field's own
/// getter does
fn view_getter(writer: &mut CodeWriter, field: &Field, view: &str, offset: Expr) {
    let mut docs = field.docs.clone();
    if !docs.is_empty() {
        docs.push(String::new());
    }
    docs.push(format!(
        "Views the items of `{}()`, reading each one when it's asked for",
        identifier(&field.name)
    ));

    writer.blank();
    writer.open(format!("def {}_view(self) -> {view}:", field.name));
    docstring(writer, &docs);
    writer.statement(
        &STYLE,
        "return ",
        &call(view.to_owned(), vec![text("self._buffer"), offset]),
        "",
    );
    writer.dedent();
}

/// The position of field `index`, in the root table or a nested one
fn offset_expression(is_root: bool, index: usize) -> Expr {
    if is_root {
        call("self._buffer.root_offset", vec![text(index.to_string())])
    } else {
        call(
            "self._buffer.field_offset",
            vec![text("self._position"), text(index.to_string())],
        )
    }
}

/// A docstring, with Markdown's code spans as reStructuredText's, and
/// backslashes and quotes escaped so that the text can't end the string
fn docstring(writer: &mut CodeWriter, docs: &[String]) {
    let docs: Vec<String> = docs
        .iter()
        .map(|line| {
            line.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('`', "``")
        })
        .collect();
    match docs.as_slice() {
        [] => {}
        [line] => writer.line(format!("\"\"\"{line}\"\"\"")),
        [first, rest @ ..] => {
            writer.line(format!("\"\"\"{first}"));
            for line in rest {
                writer.line(line);
            }
            writer.line("\"\"\"");
        }
    }
}

fn layout_expression(layout: Layout) -> String {
    let mut arguments = Vec::new();
    if layout.sized {
        arguments.push("sized=True".to_owned());
    }
    if layout.version.is_some() {
        arguments.push("versioned=True".to_owned());
    }
    if layout.offset_type != OffsetType::Uint32 {
        arguments.push(format!("offset_type=\"{}\"", layout.offset_type.name()));
    }

    if arguments.is_empty() {
        "TableLayout.DEFAULT".to_owned()
    } else {
        format!("TableLayout({})", arguments.join(", "))
    }
}

fn identifier(name: &str) -> String {
    if KEYWORDS.contains(&name) || MEMBERS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_owned()
    }
}

fn value_type(field_type: FieldType, schema: &Schema) -> String {
    match field_type {
        FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
        FieldType::Array(scalar) => format!("memoryview[{}]", scalar_type(scalar)),
        FieldType::String => "str".to_owned(),
        FieldType::Struct(index) | FieldType::Table(index) => schema.definition(index).name.clone(),
        FieldType::StructArray(index) | FieldType::Vector(index) => {
            format!("list[{}]", schema.definition(index).name)
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

fn input_type(field_type: FieldType, schema: &Schema) -> String {
    match field_type {
        FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
        FieldType::Array(scalar) => format!("Buffer | Iterable[{}]", scalar_type(scalar)),
        FieldType::String => "str".to_owned(),
        // Structs are written from their dataclasses
        FieldType::Struct(index) => schema.definition(index).name.clone(),
        FieldType::StructArray(index) => format!("Sequence[{}]", schema.definition(index).name),
        FieldType::Table(index) => format!("{}Fields", schema.definition(index).name),
        FieldType::Vector(index) => format!("Sequence[{}Fields]", schema.definition(index).name),
    }
}

/// Returns the bool that `read` reads from its byte, true when it isn't 0, in
/// parentheses when it doesn't fit on a line, as Ruff breaks a comparison
fn return_bool(writer: &mut CodeWriter, read: &Expr) {
    let line = format!("return {} != 0", read.flat());
    if writer.fits(&STYLE, &line) {
        writer.line(line);
    } else {
        writer.open("return (");
        writer.statement(&STYLE, "", read, " != 0");
        writer.close(")");
    }
}

const fn scalar_type(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::Bool => "bool",
        Scalar::Float32 | Scalar::Float64 => "float",
        _ => "int",
    }
}

/// The `struct` module's format character, which is little-endian with `<`
///
/// `?` packs a bool's truth as 1 or 0, and unpacks any byte but 0 as `True`.
const fn struct_code(scalar: Scalar) -> char {
    match scalar {
        Scalar::Bool => '?',
        Scalar::Int8 => 'b',
        Scalar::Uint8 => 'B',
        Scalar::Int16 => 'h',
        Scalar::Uint16 => 'H',
        Scalar::Int32 => 'i',
        Scalar::Uint32 => 'I',
        Scalar::Int64 => 'q',
        Scalar::Uint64 => 'Q',
        Scalar::Float32 => 'f',
        Scalar::Float64 => 'd',
    }
}
