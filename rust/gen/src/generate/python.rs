//! Python for `tafla`. Messages and tables are classes that read
//! fields lazily from their bytes, written from `TypedDict`s of their fields,
//! and structs are frozen dataclasses packed with `struct`. A table's leading
//! fields of fixed size, and a string after them, are written in one call,
//! from the runtime's `FixedFields`, and a table of only those fields is read
//! at once by `fields()` too, and a vector of such tables in one call each.
//! Tables of scalars and a string, and structs of scalars, are also read
//! into tuples of their values and written from them.

use std::collections::HashSet;

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
/// `_` after too: the classes' own methods, such as `fields()`, the
/// parameter of `serialize()` beside its keyword arguments, and the built-in
/// types that getters return, which their annotations find in the class
const MEMBERS: [&str; 18] = [
    "bool",
    "cls",
    "field_values",
    "fields",
    "float",
    "int",
    "list",
    "memoryview",
    "read",
    "read_array",
    "read_array_values",
    "read_values",
    "serialize",
    "serialize_array",
    "serialize_values",
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
            Kind::Message(layout) => message(&mut writer, schema, index, layout),
            Kind::Table => table(&mut writer, schema, index),
            Kind::Struct(_) => structure(&mut writer, schema, definition),
        }

        writer.blanks(2);
        if definition.is_struct() {
            fields_protocol(&mut writer, schema, definition);
        } else {
            fields_type(&mut writer, schema, definition);
            writer.blanks(2);
            values_type(&mut writer, schema, definition);
        }

        if schema.is_item(index) {
            writer.blanks(2);
            if definition.is_struct() {
                struct_array_view(&mut writer, schema, definition);
            } else {
                vector_view(&mut writer, schema, index);
            }
        }
    }

    vec![GeneratedFile {
        path: format!("{stem}.py"),
        contents: writer.finish(),
    }]
}

#[allow(clippy::too_many_lines)]
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
        typing.push("Protocol");
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
    if (0..schema.definitions.len()).any(|index| fixed_fields(schema, index) > 0) {
        buffer.push("FixedFields");
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
    // Structs' protocols declare their fields with `builtins.property`, by
    // a name that no field takes, since fields start with a letter. A field
    // named `property` or `builtins` would otherwise hide the decorator from
    // the fields after it in the class.
    if has_struct {
        writer.line("import builtins as _builtins");
    }
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

fn message(writer: &mut CodeWriter, schema: &Schema, index: usize, layout: Layout) {
    let definition = schema.definition(index);
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
    let fixed = fixed_fields(schema, index);
    if fixed > 0 {
        let description = fixed_description(schema, definition, fixed, Some("_LAYOUT"));
        writer.statement(&STYLE, "_FIXED_FIELDS: Final = ", &description, "");
    }
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

    serialize(writer, schema, definition, layout, fixed);
    if has_serialize_values(schema, index) {
        writer.blank();
        serialize_values(writer, schema, index, layout);
    }

    getters(writer, schema, definition, true);
    if has_fields(schema, index) {
        field_reader(writer, schema, index, true, Form::Fields);
    }
    field_reader(writer, schema, index, true, Form::Values);
    writer.dedent();
}

/// A message's `serialize()`, which writes it from its fields, the first
/// `fixed` of them with its `FixedFields`
fn serialize(
    writer: &mut CodeWriter,
    schema: &Schema,
    definition: &Definition,
    layout: Layout,
    fixed: usize,
) {
    let name = &definition.name;

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
    if fixed > 0 {
        set_fixed_fields(
            writer,
            schema,
            definition,
            fixed,
            "writer.set_fields",
            "cls",
        );
    }
    writes(writer, schema, definition, fixed);
    writer.blank();
    writer.line("return writer.bytes()");
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

    let fixed = fixed_fields(schema, index);
    if fixed > 0 {
        let description = fixed_description(schema, definition, fixed, None);
        writer.statement(&STYLE, "_FIXED_FIELDS: Final = ", &description, "");
        writer.blank();
    }

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
    // A table whose fields are all fixed is written whole, without the
    // calls that start and end it
    if fixed == definition.fields.len() {
        set_fixed_fields(
            writer,
            schema,
            definition,
            fixed,
            "writer.write_table",
            name,
        );
    } else {
        writer.line(format!(
            "parent = writer.start_table({})",
            definition.fields.len()
        ));
        if fixed > 0 {
            set_fixed_fields(writer, schema, definition, fixed, "writer.set_fields", name);
        }
        writes(writer, schema, definition, fixed);
        writer.line("writer.end_table(parent)");
    }
    writer.dedent();

    getters(writer, schema, definition, false);
    if has_fields(schema, index) {
        field_reader(writer, schema, index, false, Form::Fields);
    }
    field_reader(writer, schema, index, false, Form::Values);
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

    writer.line("@dataclass(frozen=True, slots=True, init=False)");
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

    initializer(writer, schema, definition);
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
    writer.statement(
        &STYLE,
        "def ",
        &call(
            "serialize_array",
            vec![text("cls"), text(format!("items: Sequence[{name}Fields]"))],
        ),
        " -> bytearray:",
    );
    writer.indent();
    docstring(
        writer,
        &[
            format!("Returns the bytes of `{name}`s, or of any other objects of their"),
            "fields, one after another, as an array of them holds them.".to_owned(),
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
    if has_struct_values(definition) {
        struct_values(writer, schema, definition);
    }
    writer.dedent();

    writer.blanks(2);
    slot_setters(writer, definition);
}

/// A struct's `__init__()`, which sets each field's slot through the setter
/// that `slot_setters()` binds after the class. A frozen dataclass's own
/// `__init__()` sets each through `object.__setattr__()`, around the class's
/// `__setattr__()`, which raises: creating 100 `Triple`s took 24.1 µs that
/// way and 15.6 µs this way, and writing and reading an array of them 25%
/// and 23% less. The class stays a frozen dataclass of the same fields.
fn initializer(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    let name = &definition.name;
    let fields: Vec<String> = definition
        .fields
        .iter()
        .map(|field| identifier(&field.name))
        .collect();
    // Fields are snake_case, so none takes `_self`, which the instance is
    // named when a field takes `self`, as in a dataclass's own __init__()
    let instance = if fields.iter().any(|field| field == "self") {
        "_self"
    } else {
        "self"
    };

    let mut parameters = vec![text(instance)];
    for (field, identifier) in definition.fields.iter().zip(&fields) {
        let value_type = value_type(field.field_type, schema);
        parameters.push(text(format!("{identifier}: {value_type}")));
    }
    writer.statement(&STYLE, "def ", &call("__init__", parameters), " -> None:");
    writer.indent();
    docstring(
        writer,
        &["Sets each slot directly, not through `object.__setattr__()`".to_owned()],
    );
    for field in &fields {
        let setter = call(
            slot_setter(name, field),
            vec![text(instance), text(field.as_str())],
        );
        writer.statement(&STYLE, "", &setter, "");
    }
    writer.dedent();
}

/// Binds the `__set__()` of each slot's member descriptor, which `__init__()`
/// calls, to a module-level name after the class, since a dataclass with
/// slots replaces the class that its body defines
fn slot_setters(writer: &mut CodeWriter, definition: &Definition) {
    let name = &definition.name;
    for field in &definition.fields {
        let field = identifier(&field.name);
        let setter = slot_setter(name, &field);
        let line = format!("{setter} = {name}.__dict__[\"{field}\"].__set__");
        if writer.fits(&STYLE, &line) {
            writer.line(line);
        } else {
            // As Ruff breaks it, at the subscript
            writer.open(format!("{setter} = {name}.__dict__["));
            writer.line(format!("\"{field}\""));
            writer.close("].__set__");
        }
    }
}

/// The module-level name of the setter of `field`'s slot in the struct
/// `name`, which nothing else in the module takes: declarations are
/// `PascalCase` without `_`, so `_<name>_` starts no other struct's setters,
/// and fields are `snake_case`, and imports don't start with `_`
fn slot_setter(name: &str, field: &str) -> String {
    format!("_{name}_{field}")
}

/// The view of a vector of a table, which reads each item when it's asked
/// for, from the item's end and the end of the item before it, and every
/// item's fields at once, when the table has `fields()`
fn vector_view(writer: &mut CodeWriter, schema: &Schema, index: usize) {
    let definition = schema.definition(index);
    let table = &definition.name;

    writer.open(format!("class {table}Vector:"));
    docstring(
        writer,
        &[
            format!("A vector of `{table}`s, which reads each item when it's asked for, so"),
            "an item whose bytes are corrupt raises `DecodeError` when it's read, rather"
                .to_owned(),
            "than when its parent's getter returns the vector".to_owned(),
        ],
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
    // The count that the constructor checked, rather than reading it again
    writer.statement(
        &STYLE,
        "offset = ",
        &call(
            "self._buffer.get_vector_item_offset",
            vec![text("self._position"), text("self._length"), text("index")],
        ),
        "",
    );
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

    vector_readers(writer, schema, index);
    writer.dedent();
}

/// Reads the values of every item of a vector of `table` in one call, and
/// opens the block for items that it can't read so
fn vector_fields(writer: &mut CodeWriter, table: &str) {
    let read = call(
        "self._buffer.get_vector_fields",
        vec![
            text("self._position"),
            text(format!("{table}._FIXED_FIELDS")),
        ],
    );
    writer.statement(&STYLE, "items = ", &read, "");
    writer.open("if items is None:");
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
    writer.line("__slots__ = (\"_data\",)");
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
    writer.dedent();
    writer.blank();

    writer.open("def __len__(self) -> int:");
    writer.line(format!("return len(self._data) // {structure}.SIZE"));
    writer.dedent();
    writer.blank();

    // The structs' values as their Struct unpacks them, as read_array()
    // reads them
    let construction = construction(schema, definition, structure, &mut 0);
    writer.open(format!("def __getitem__(self, index: int) -> {structure}:"));
    docstring(
        writer,
        &[
            "Reads struct `index`, counting from the end if it's negative.".to_owned(),
            String::new(),
            "Raises `IndexError` if the array has no struct `index`.".to_owned(),
        ],
    );
    struct_item_values(writer, structure, "values = ");
    writer.statement(&STYLE, "return ", &construction, "");
    writer.dedent();
    writer.blank();

    writer.open(format!("def __iter__(self) -> Iterator[{structure}]:"));
    docstring(writer, &["Reads the structs one after another".to_owned()]);
    writer.open(format!(
        "for values in {structure}._STRUCT.iter_unpack(self._data):"
    ));
    writer.statement(&STYLE, "yield ", &construction, "");
    writer.dedent();
    writer.dedent();

    if has_struct_values(definition) {
        struct_array_values(writer, schema, definition);
    }
    writer.dedent();
}

/// The methods of the view of an array of a struct of scalars that read the
/// structs' values into tuples, as `read_values()` reads one, without
/// creating their dataclasses: one struct's, every struct's at once, and
/// every struct's one after another
fn struct_array_values(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    let structure = &definition.name;
    let types = value_types(schema, definition);

    writer.blank();
    def_line(
        writer,
        "item_field_values",
        &["self".to_owned(), "index: int".to_owned()],
        &tuple_annotation(types.clone()),
    );
    writer.indent();
    docstring(
        writer,
        &[
            format!("Reads the values of struct `index`, as `{structure}.read_values()` reads"),
            "them, counting from the end if it's negative.".to_owned(),
            String::new(),
            "Raises `IndexError` if the array has no struct `index`.".to_owned(),
        ],
    );
    struct_item_values(writer, structure, "return ");
    writer.dedent();

    writer.blank();
    def_line(
        writer,
        "field_values",
        &["self".to_owned()],
        &Annotation::Generic("list", vec![tuple_annotation(types.clone())]),
    );
    writer.indent();
    docstring(
        writer,
        &[format!(
            "Reads every struct's values at once, as `{structure}.read_values()` reads each"
        )],
    );
    writer.line(format!(
        "return list({structure}._STRUCT.iter_unpack(self._data))"
    ));
    writer.dedent();

    writer.blank();
    def_line(
        writer,
        "iter_field_values",
        &["self".to_owned()],
        &Annotation::Generic("Iterator", vec![tuple_annotation(types)]),
    );
    writer.indent();
    docstring(
        writer,
        &[
            "Reads every struct's values one after another, as `field_values()` reads".to_owned(),
            "them into a list".to_owned(),
        ],
    );
    writer.line(format!(
        "return {structure}._STRUCT.iter_unpack(self._data)"
    ));
    writer.dedent();
}

/// Unpacks the values of struct `index` of an array's view, after `prefix`.
/// `struct` reads a negative offset from the end of the array's bytes, which
/// are a whole number of structs, and raises `struct.error` for an offset
/// outside them, or `OverflowError` for one beyond a C `ssize_t`, so its
/// check of the offset is the check of the index.
fn struct_item_values(writer: &mut CodeWriter, structure: &str, prefix: &str) {
    writer.open("try:");
    writer.statement(
        &STYLE,
        prefix,
        &call(
            format!("{structure}._STRUCT.unpack_from"),
            vec![
                text("self._data"),
                text(format!("index * {structure}.SIZE")),
            ],
        ),
        "",
    );
    writer.dedent();
    // The message leaves out the caller's index, as a list's does, since
    // converting one of more digits than Python's limit to a string fails
    writer.open("except error, OverflowError:");
    writer.statement(
        &STYLE,
        "raise ",
        &call(
            "IndexError",
            vec![text(
                "f\"Struct index is outside the array of {len(self)}\"",
            )],
        ),
        " from None",
    );
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

/// The protocol of a struct's fields, which an array of it is written from:
/// the struct's own attributes, or those of any object of the same names, as
/// properties, which read-only attributes such as a frozen dataclass's have.
/// It's a `Protocol` rather than the dataclass, so that an application's own
/// objects type-check.
fn fields_protocol(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    let name = &definition.name;

    writer.open(format!("class {name}Fields(Protocol):"));
    docstring(
        writer,
        &[
            format!(
                "The fields of {}, which an array of them is written from: the",
                article(name)
            ),
            format!(
                "attributes of {}, or of any other object with attributes of",
                article(name)
            ),
            "the same names and types".to_owned(),
        ],
    );
    for field in &definition.fields {
        let value_type = match field.field_type {
            FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
            FieldType::Struct(index) => format!("{}Fields", schema.definition(index).name),
            _ => unreachable!("structs hold scalars and structs only"),
        };
        let signature = format!("def {}(self) -> {value_type}:", identifier(&field.name));

        writer.blank();
        writer.line("@_builtins.property");
        if field.docs.is_empty() {
            writer.line(format!("{signature} ..."));
        } else {
            writer.open(signature);
            docstring(writer, &field.docs);
            writer.dedent();
        }
    }
    writer.dedent();
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
/// The calls that write a table's fields, from its field `first`, after the
/// fields that `FixedFields` packs
fn writes(writer: &mut CodeWriter, schema: &Schema, definition: &Definition, first: usize) {
    for (index, field) in definition.fields.iter().enumerate().skip(first) {
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
            FieldType::Vector(table_index)
                if fixed_fields(schema, table_index)
                    == schema.definition(table_index).fields.len() =>
            {
                vector_tables(writer, schema, table_index, position, &value);
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

/// Writes a vector of tables that `FixedFields` describes whole, from
/// `value`, its items' `TypedDict`s, in one call that takes each item's values
/// as `write_table()` does, and its string after them
fn vector_tables(
    writer: &mut CodeWriter,
    schema: &Schema,
    table_index: usize,
    position: Expr,
    value: &str,
) {
    let table = schema.definition(table_index);
    let mut values = Vec::new();
    push_fixed_values(schema, table, table.fields.len(), "item", &mut values);
    if let Some(last) = table.fields.last()
        && last.field_type == FieldType::String
    {
        values.push(format!("item[\"{}\"]", identifier(&last.name)));
    }

    comprehension(
        writer,
        "items = ",
        &tuple_flat(&values),
        &format!("for item in {value}"),
        |writer| tuple_statement(writer, "", &values),
    );
    let arguments = vec![
        position,
        text(format!("{}._FIXED_FIELDS", table.name)),
        text("items"),
    ];
    writer.statement(&STYLE, "", &call("writer.set_vector_tables", arguments), "");
}

/// The most fields, those of nested tables included, that one `FixedFields`
/// describes. Tables may nest another in many fields, and that one the next,
/// so describing every nested table whole would grow the generated code and
/// the work of generating it with the product of their fields.
const MAX_FIXED_FIELDS: usize = 32;

/// How many of the leading fields of the message or table at `index` its
/// `FixedFields` describes, for `TableWriter` to pack with their offsets in
/// one call: those of fixed size, whose offsets depend on no value, and a
/// string after them, up to [`MAX_FIXED_FIELDS`]. It's 0 when packing them
/// wouldn't save a call: for fewer than two, unless they're a whole table's,
/// which its parent then writes without starting and ending it.
fn fixed_fields(schema: &Schema, index: usize) -> usize {
    let definition = schema.definition(index);
    if definition.is_struct() {
        return 0;
    }

    let mut budget = MAX_FIXED_FIELDS;
    let mut packed = 0;
    for field in &definition.fields {
        let Some(count) = fixed_count(schema, field.field_type, budget) else {
            break;
        };
        budget -= count;
        packed += 1;
    }
    if let Some(field) = definition.fields.get(packed)
        && field.field_type == FieldType::String
        && budget > 0
    {
        packed += 1;
    }

    let is_whole_table = definition.kind == Kind::Table && packed == definition.fields.len();
    if packed >= 2 || (is_whole_table && packed > 0) {
        packed
    } else {
        0
    }
}

/// The number of fields that a field adds to a `FixedFields`, itself and a
/// nested table's fields, when its bytes are of a size that the schema fixes
/// and they're at most `budget`: a scalar's, a struct's, or a nested table's
/// whose fields all are, which has no arrays to align. Each step spends some
/// of the budget or ends, so this ends soon, however tables nest.
fn fixed_count(schema: &Schema, field_type: FieldType, budget: usize) -> Option<usize> {
    if budget == 0 {
        return None;
    }

    match field_type {
        FieldType::Scalar(_) | FieldType::Struct(_) => Some(1),
        FieldType::Table(index) => {
            let mut count = 1;
            for field in &schema.definition(index).fields {
                count += fixed_count(schema, field.field_type, budget - count)?;
            }
            Some(count)
        }
        FieldType::Array(_)
        | FieldType::String
        | FieldType::StructArray(_)
        | FieldType::Vector(_) => None,
    }
}

/// The `FixedFields` of a table's first `count` fields, each a scalar's
/// `struct` format code, a struct's size, or a nested table's own, and then
/// the string, with a message's `layout`
fn fixed_description(
    schema: &Schema,
    definition: &Definition,
    count: usize,
    layout: Option<&str>,
) -> Expr {
    let mut arguments = vec![text(definition.fields.len().to_string())];
    let mut string = false;
    for field in &definition.fields[..count] {
        arguments.push(match field.field_type {
            FieldType::Scalar(scalar) => text(format!("\"{}\"", struct_code(scalar))),
            FieldType::Struct(index) => text(format!(
                "\"{}s\"",
                schema.definition(index).struct_layout().size
            )),
            FieldType::Table(index) => {
                let nested = schema.definition(index);
                fixed_description(schema, nested, nested.fields.len(), None)
            }
            FieldType::String => {
                string = true;
                continue;
            }
            FieldType::Array(_) | FieldType::StructArray(_) | FieldType::Vector(_) => {
                unreachable!("only fields of fixed size and a string are packed")
            }
        });
    }

    if string {
        arguments.push(text("string=True"));
    }
    if let Some(layout) = layout {
        arguments.push(text(format!("layout={layout}")));
    }
    call("FixedFields", arguments)
}

/// The call that writes a table's first `count` fields with its
/// `FixedFields`: their values, each nested table's in place of it, and the
/// string's
fn set_fixed_fields(
    writer: &mut CodeWriter,
    schema: &Schema,
    definition: &Definition,
    count: usize,
    method: &str,
    owner: &str,
) {
    let mut values = Vec::new();
    push_fixed_values(schema, definition, count, "fields", &mut values);

    let mut arguments = vec![text(format!("{owner}._FIXED_FIELDS"))];
    // A tuple of one value needs its comma
    arguments.push(match values.as_slice() {
        [value] => text(format!("({value},)")),
        _ => call("", values.into_iter().map(text).collect()),
    });
    let last = &definition.fields[count - 1];
    if last.field_type == FieldType::String {
        arguments.push(text(format!("fields[\"{}\"]", identifier(&last.name))));
    }

    writer.statement(&STYLE, "", &call(method, arguments), "");
}

/// Appends the values of a table's first `count` fields, but a string's,
/// from `fields`, with those of a nested table in place of it
fn push_fixed_values(
    schema: &Schema,
    definition: &Definition,
    count: usize,
    fields: &str,
    values: &mut Vec<String>,
) {
    for field in &definition.fields[..count] {
        let value = format!("{fields}[\"{}\"]", identifier(&field.name));
        match field.field_type {
            FieldType::Scalar(_) => values.push(value),
            FieldType::Struct(_) => values.push(format!("{value}.serialize()")),
            FieldType::Table(index) => {
                let nested = schema.definition(index);
                push_fixed_values(schema, nested, nested.fields.len(), &value, values);
            }
            // The string is an argument of its own
            FieldType::String => {}
            FieldType::Array(_) | FieldType::StructArray(_) | FieldType::Vector(_) => {
                unreachable!("only fields of fixed size and a string are packed")
            }
        }
    }
}

/// Whether the message at `index` has `serialize_values()`, which writes it
/// from the values that `field_values()` reads: whether its fields are
/// scalars, strings, arrays and vectors of tables of fixed size without
/// structs, whose items' values its vectors' views read into the tuples that
/// `TableWriter.set_vector_tables()` writes, as `ByteBuffer.get_fields()`
/// reads them. A struct's value would be its bytes there, and a nested
/// table's values would be written by code of its own.
fn has_serialize_values(schema: &Schema, index: usize) -> bool {
    let definition = schema.definition(index);
    matches!(definition.kind, Kind::Message(_))
        && definition
            .fields
            .iter()
            .all(|field| match field.field_type {
                FieldType::Scalar(_) | FieldType::String | FieldType::Array(_) => true,
                FieldType::Vector(item) => {
                    let table = schema.definition(item);
                    is_fixed(schema, item) && !holds_struct(schema, table, table.fields.len())
                }
                FieldType::Struct(_) | FieldType::StructArray(_) | FieldType::Table(_) => false,
            })
}

/// Whether a struct is of scalars alone, whose values `read_values()` reads
/// into a tuple, as its `Struct` unpacks them, and `serialize_values()` takes.
/// A tuple of a struct within it would hold its scalars in place of it, so
/// those have the dataclass alone.
fn has_struct_values(definition: &Definition) -> bool {
    definition
        .fields
        .iter()
        .all(|field| matches!(field.field_type, FieldType::Scalar(_)))
}

/// The types of a definition's values, as a tuple of them holds them
fn value_types(schema: &Schema, definition: &Definition) -> Vec<String> {
    definition
        .fields
        .iter()
        .map(|field| value_type(field.field_type, schema))
        .collect()
}

/// A field's name as a parameter of a method that writes it from its value,
/// with a `_` after it when the method has a local of that name. The
/// parameters are positional only, so callers don't see it.
fn parameter(field: &Field, locals: &[&str]) -> String {
    let name = identifier(&field.name);
    if locals.contains(&name.as_str()) {
        format!("{name}_")
    } else {
        name
    }
}

/// What a table's `field_values()` or `fields()` reads its fields into
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    /// A tuple of each field's value as its getter returns it, but a nested
    /// table's, whose values are in place of it when they're all of fixed
    /// size, as `ByteBuffer.get_fields()` reads them, and in a tuple of their
    /// own otherwise
    Values,
    /// The `TypedDict` that the table is written from, with the items of its
    /// vectors and arrays of structs
    Fields,
}

impl Form {
    /// The method of a nested table's reader that reads it in this form
    const fn method(self) -> &'static str {
        match self {
            Self::Values => "field_values",
            Self::Fields => "fields",
        }
    }
}

/// A field's value in what `field_values()` or `fields()` returns
enum Element {
    /// The value at a slot of [`Reads::slots`], spliced in place of the field
    /// in a tuple when it's the values of a nested table of fixed size
    Value {
        key: String,
        slot: usize,
        spliced: bool,
    },
    /// A nested table's fields, read in the same call, in place of it in a
    /// tuple when they're all of fixed size
    Table {
        key: String,
        flat: bool,
        elements: Vec<Element>,
    },
}

/// The expression of a value, which reads the bytes when `reads`, and the
/// local that it's assigned to when a later read must follow it
struct Slot {
    expression: Expr,
    hint: String,
    reads: bool,
    local: Option<String>,
}

/// The most fields of nested tables that a `field_values()` or `fields()`
/// reads in its own call, rather than through the nested tables' readers, so
/// that tables nested in many fields don't grow the generated code with the
/// product of their fields
const MAX_INLINE_FIELDS: usize = 32;

/// The reads of a table's `field_values()` or `fields()`, in the order that
/// its getters read them, so that it raises the getters' first error, and the
/// locals of the positions of the nested tables that it reads itself, which
/// the reads before them are assigned to locals before
struct Reads<'a> {
    schema: &'a Schema,
    form: Form,
    slots: Vec<Slot>,
    /// The locals that are assigned before the result, in order
    statements: Vec<(String, Expr)>,
    names: HashSet<String>,
    budget: usize,
}

impl<'a> Reads<'a> {
    fn new(schema: &'a Schema, form: Form) -> Self {
        Self {
            schema,
            form,
            slots: Vec::new(),
            statements: Vec::new(),
            // The method's own names
            names: ["self", "buffer", "values"].map(str::to_owned).into(),
            budget: MAX_INLINE_FIELDS,
        }
    }

    fn value(&mut self, key: String, expression: Expr, reads: bool, spliced: bool) -> Element {
        self.slots.push(Slot {
            expression,
            hint: key.clone(),
            reads,
            local: None,
        });
        Element::Value {
            key,
            slot: self.slots.len() - 1,
            spliced,
        }
    }

    /// A local's name after `hint`, which no other name of the method takes
    fn local(&mut self, hint: &str) -> String {
        let mut name = hint.to_owned();
        let mut suffix = 2;
        while !self.names.insert(name.clone()) {
            name = format!("{hint}_{suffix}");
            suffix += 1;
        }
        name
    }

    /// Assigns each read so far to a local, so that it reads the bytes before
    /// what's assigned next
    fn hoist(&mut self) {
        for index in 0..self.slots.len() {
            let slot = &self.slots[index];
            if !slot.reads || slot.local.is_some() {
                continue;
            }
            let (hint, expression) = (slot.hint.clone(), slot.expression.clone());
            let name = self.local(&hint);
            self.statements.push((name.clone(), expression));
            self.slots[index].local = Some(name);
        }
    }

    /// The elements of the first `count` fields of `definition`, which
    /// `ByteBuffer.get_fields()` read into `values` from `next` on, with
    /// structs as their bytes
    fn unpacked(
        &mut self,
        definition: &Definition,
        count: usize,
        values: &str,
        next: &mut usize,
    ) -> Vec<Element> {
        let mut elements = Vec::new();
        for field in &definition.fields[..count] {
            let key = identifier(&field.name);
            let value = match field.field_type {
                FieldType::Table(index) => {
                    let nested = self.schema.definition(index);
                    let elements = self.unpacked(nested, nested.fields.len(), values, next);
                    Element::Table {
                        key,
                        flat: true,
                        elements,
                    }
                }
                FieldType::Struct(index) => {
                    let structure = &self.schema.definition(index).name;
                    let read = format!("{structure}.read({values}[{next}])");
                    *next += 1;
                    self.value(key, text(read), false, false)
                }
                _ => {
                    let value = format!("{values}[{next}]");
                    *next += 1;
                    self.value(key, text(value), false, false)
                }
            };
            elements.push(value);
        }
        elements
    }

    /// The elements of the fields of `definition` from `first` on, read from
    /// the table at `position`, or from the root when it's `None`
    fn read(
        &mut self,
        definition: &Definition,
        first: usize,
        position: Option<&str>,
    ) -> Vec<Element> {
        let schema = self.schema;
        let mut elements = Vec::new();
        for (index, field) in definition.fields.iter().enumerate().skip(first) {
            let key = identifier(&field.name);
            let offset = match position {
                None => call("buffer.root_offset", vec![text(index.to_string())]),
                Some(position) => call(
                    "buffer.field_offset",
                    vec![text(position), text(index.to_string())],
                ),
            };

            let read = match field.field_type {
                FieldType::Scalar(Scalar::Bool) => text(format!(
                    "{} != 0",
                    call("buffer.get_uint8", vec![offset]).flat()
                )),
                FieldType::Scalar(scalar) => {
                    call(format!("buffer.get_{}", scalar.name()), vec![offset])
                }
                FieldType::Array(scalar) => {
                    call(format!("buffer.get_{}_array", scalar.name()), vec![offset])
                }
                FieldType::String => call("buffer.get_string", vec![offset]),
                FieldType::Struct(index) => {
                    let structure = &schema.definition(index).name;
                    let bytes = call(
                        "buffer.get_struct",
                        vec![offset, text(format!("{structure}.SIZE"))],
                    );
                    call(format!("{structure}.read"), vec![bytes])
                }
                FieldType::StructArray(index) => {
                    let structure = &schema.definition(index).name;
                    match self.form {
                        Form::Values => {
                            call(format!("{structure}Array"), vec![text("buffer"), offset])
                        }
                        Form::Fields => {
                            let bytes = call(
                                "buffer.get_struct_array",
                                vec![offset, text(format!("{structure}.SIZE"))],
                            );
                            call(format!("{structure}.read_array"), vec![bytes])
                        }
                    }
                }
                FieldType::Vector(index) => {
                    let view = call(
                        format!("{}Vector", schema.definition(index).name),
                        vec![text("buffer"), offset],
                    );
                    match self.form {
                        Form::Values => view,
                        Form::Fields => call(format!("{}.fields", view.flat()), vec![]),
                    }
                }
                FieldType::Table(index) => {
                    let nested = schema.definition(index);
                    let flat = is_flat(schema, index);
                    // A nested table without leading fields of fixed size is
                    // read here, from its position, and any other by its
                    // reader, which reads those in one call
                    if !flat
                        && fixed_fields(schema, index) == 0
                        && nested.fields.len() <= self.budget
                    {
                        self.budget -= nested.fields.len();
                        self.hoist();
                        let local = self.local(&key);
                        self.statements.push((local.clone(), offset));
                        let nested = self.read(nested, 0, Some(&local));
                        elements.push(Element::Table {
                            key,
                            flat: false,
                            elements: nested,
                        });
                        continue;
                    }

                    let reader = call(nested.name.clone(), vec![text("buffer"), offset]);
                    let read = call(format!("{}.{}", reader.flat(), self.form.method()), vec![]);
                    let spliced = flat && self.form == Form::Values;
                    elements.push(self.value(key, read, true, spliced));
                    continue;
                }
            };
            elements.push(self.value(key, read, true, false));
        }
        elements
    }

    /// The expression of a slot: its local, or its read
    fn expression(&self, slot: usize) -> Expr {
        let slot = &self.slots[slot];
        slot.local
            .as_ref()
            .map_or_else(|| slot.expression.clone(), |local| text(local.clone()))
    }

    /// The tuple of `elements`, as `field_values()` returns it
    fn tuple(&self, elements: &[Element]) -> Expr {
        let mut items = Vec::new();
        self.push_items(elements, &mut items);
        tuple_of(items)
    }

    fn push_items(&self, elements: &[Element], items: &mut Vec<Expr>) {
        for element in elements {
            match element {
                Element::Value { slot, spliced, .. } => {
                    let expression = self.expression(*slot);
                    items.push(if *spliced {
                        text(format!("*{}", expression.flat()))
                    } else {
                        expression
                    });
                }
                Element::Table {
                    flat: true,
                    elements,
                    ..
                } => self.push_items(elements, items),
                Element::Table { elements, .. } => items.push(self.tuple(elements)),
            }
        }
    }

    /// The dict of `elements`, as `fields()` returns it
    fn dict(&self, elements: &[Element]) -> DictValue {
        let entries = elements
            .iter()
            .map(|element| match element {
                Element::Value { key, slot, .. } => {
                    (key.clone(), DictValue::Expr(self.expression(*slot)))
                }
                Element::Table { key, elements, .. } => (key.clone(), self.dict(elements)),
            })
            .collect();
        DictValue::Dict(entries)
    }
}

/// A tuple of `items`, which breaks over lines as a call's arguments do, with
/// a comma after a sole item
fn tuple_of(items: Vec<Expr>) -> Expr {
    match items.as_slice() {
        [] => text("()"),
        [item] => text(format!("({},)", item.flat())),
        _ => call("", items),
    }
}

/// Whether a nested table's fields are all of fixed size, which its
/// `FixedFields` describes whole, unless it has none, so that its values lie
/// in place of it in its parent's, as `ByteBuffer.get_fields()` reads them
fn is_flat(schema: &Schema, index: usize) -> bool {
    fixed_fields(schema, index) == schema.definition(index).fields.len()
}

/// Whether the table at `index` has fields, which its `FixedFields`
/// describes whole
fn is_fixed(schema: &Schema, index: usize) -> bool {
    fixed_fields(schema, index) > 0 && is_flat(schema, index)
}

/// Whether the message or table at `index` has `fields()`, which reads the
/// items of its vectors too: whether every vector that it or a table nested
/// in it holds is of tables of fixed size, which hold no vectors. Reading
/// the items of others could read without end, since corrupted bytes can
/// make an item hold the vector that holds it, or read more items than the
/// bytes hold, since the items of a vector can hold the same vector. Those
/// tables have `field_values()` alone, which returns views of vectors.
fn has_fields(schema: &Schema, index: usize) -> bool {
    // Each nested table once, since tables may nest the same one in many
    // fields of each other
    let mut tables = vec![index];
    let mut seen = HashSet::from([index]);
    while let Some(table) = tables.pop() {
        for field in &schema.definition(table).fields {
            match field.field_type {
                FieldType::Vector(item) if !is_flat(schema, item) => return false,
                // The checker has rejected tables that hold themselves
                FieldType::Table(nested) if seen.insert(nested) => tables.push(nested),
                _ => {}
            }
        }
    }
    true
}

/// The annotation of the values of the fields of `definition`, as
/// `field_values()` returns them
fn values_annotation(schema: &Schema, definition: &Definition) -> Annotation {
    let mut types = Vec::new();
    push_value_types(schema, definition, &mut types);
    Annotation::Generic("tuple", types)
}

/// Pushes the types of a table's values, a nested table's in place of it
/// when its fields are all of fixed size, which are few, and otherwise its
/// alias, so that the types of tables nested in many fields of each other
/// don't grow with the product of their fields
fn push_value_types(schema: &Schema, definition: &Definition, types: &mut Vec<Annotation>) {
    for field in &definition.fields {
        match field.field_type {
            FieldType::Table(index) if is_flat(schema, index) => {
                push_value_types(schema, schema.definition(index), types);
            }
            FieldType::Table(index) => types.push(values_alias(schema.definition(index))),
            field_type => types.push(Annotation::Name(value_type(field_type, schema))),
        }
    }
}

/// The alias of the tuple of a table's values, `<Name>FieldValues`, which
/// `field_values()` returns
fn values_alias(definition: &Definition) -> Annotation {
    Annotation::Name(format!("{}FieldValues", definition.name))
}

/// Declares a table's `<Name>FieldValues`, the alias of the tuple of its
/// values, as Ruff breaks it: on one line, or its types within the brackets,
/// all on one line when they fit and one on each otherwise
fn values_type(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    let Annotation::Generic(generic, types) = values_annotation(schema, definition) else {
        unreachable!("the values are a tuple")
    };
    let prefix = format!("type {}FieldValues = ", definition.name);
    let flat: Vec<String> = types.iter().map(Annotation::flat).collect();
    let line = format!("{prefix}{generic}[{}]", flat.join(", "));
    if types.is_empty() || writer.fits(&STYLE, &line) {
        writer.line(format!(
            "{prefix}{}",
            values_annotation(schema, definition).flat()
        ));
        return;
    }

    writer.open(format!("{prefix}{generic}["));
    let together = flat.join(", ");
    if types.len() == 1 || writer.fits(&STYLE, &together) {
        writer.line(together);
    } else {
        for value_type in flat {
            writer.line(format!("{value_type},"));
        }
    }
    writer.close("]");
}

/// Whether the first `count` fields of `definition` hold a struct, nested
/// tables' included, which `ByteBuffer.get_fields()` reads as its bytes
fn holds_struct(schema: &Schema, definition: &Definition, count: usize) -> bool {
    definition.fields[..count]
        .iter()
        .any(|field| match field.field_type {
            FieldType::Struct(_) => true,
            FieldType::Table(index) => {
                let nested = schema.definition(index);
                holds_struct(schema, nested, nested.fields.len())
            }
            _ => false,
        })
}

/// A message's or table's `field_values()` or `fields()`, which reads its
/// leading fields of fixed size in one call when its offsets are the ones
/// that its writer writes, and its other fields after them, those of nested
/// tables included, in the order that its getters read them, so that its
/// values and errors are theirs. When its offsets aren't the ones that its
/// writer writes, it reads every field through the getters.
fn field_reader(writer: &mut CodeWriter, schema: &Schema, index: usize, is_root: bool, form: Form) {
    let definition = schema.definition(index);
    let fixed = fixed_fields(schema, index);
    let mut reads = Reads::new(schema, form);
    let mut elements = reads.unpacked(definition, fixed, "values", &mut 0);
    let rest = reads.read(definition, fixed, (!is_root).then_some("self._position"));
    elements.extend(rest);

    writer.blank();
    let annotation = match form {
        Form::Values => values_alias(definition),
        Form::Fields => Annotation::Name(format!("{}Fields", definition.name)),
    };
    def_line(writer, form.method(), &["self".to_owned()], &annotation);
    writer.indent();
    docstring(writer, &reader_docs(schema, index, is_root, form));

    // The leading fields of fixed size, as get_fields() returns them when
    // they're all the fields and none is a struct, which is read from its bytes
    let unpacked_whole = fixed > 0
        && fixed == definition.fields.len()
        && !holds_struct(schema, definition, fixed)
        && form == Form::Values;
    let buffer = if fixed == definition.fields.len() {
        "self._buffer"
    } else {
        writer.line("buffer = self._buffer");
        "buffer"
    };
    if fixed > 0 {
        let position = if is_root { "0" } else { "self._position" };
        writer.line(format!(
            "values = {buffer}.get_fields({position}, self._FIXED_FIELDS)"
        ));
        writer.open("if values is None:");
        getter_fallback(writer, schema, definition, form);
        writer.dedent();
        writer.blank();
    }

    for (name, expression) in &reads.statements {
        writer.statement(&STYLE, &format!("{name} = "), expression, "");
    }
    if unpacked_whole {
        writer.line("return values");
    } else {
        match form {
            Form::Values => writer.statement(&STYLE, "return ", &reads.tuple(&elements), ""),
            Form::Fields => dict_statement(writer, "return ", &reads.dict(&elements)),
        }
    }
    writer.dedent();
}

/// Returns the fields' values or fields as the getters read them, for a table
/// whose offsets aren't the ones that its writer writes
fn getter_fallback(writer: &mut CodeWriter, schema: &Schema, definition: &Definition, form: Form) {
    let getter = |field: &Field| format!("self.{}()", identifier(&field.name));
    match form {
        Form::Values => {
            let items = definition
                .fields
                .iter()
                .map(|field| match field.field_type {
                    FieldType::Table(index) if is_flat(schema, index) => {
                        format!("*{}.field_values()", getter(field))
                    }
                    FieldType::Table(_) => format!("{}.field_values()", getter(field)),
                    _ => getter(field),
                })
                .map(text)
                .collect();
            writer.statement(&STYLE, "return ", &tuple_of(items), "");
        }
        Form::Fields => {
            let entries = definition
                .fields
                .iter()
                .map(|field| {
                    let value = match field.field_type {
                        FieldType::Table(_) | FieldType::Vector(_) => {
                            format!("{}.fields()", getter(field))
                        }
                        FieldType::StructArray(_) => format!("list({})", getter(field)),
                        _ => getter(field),
                    };
                    (identifier(&field.name), DictValue::Text(value))
                })
                .collect();
            dict_statement(writer, "return ", &DictValue::Dict(entries));
        }
    }
}

/// The docstring of a `field_values()` or `fields()`, which describes the
/// values of the kinds of fields that the table holds
fn reader_docs(schema: &Schema, index: usize, is_root: bool, form: Form) -> Vec<String> {
    let definition = schema.definition(index);
    let holds = |predicate: fn(FieldType) -> bool| {
        definition
            .fields
            .iter()
            .any(|field| predicate(field.field_type))
    };
    let tables = holds(|field_type| matches!(field_type, FieldType::Table(_)));
    let items =
        holds(|field_type| matches!(field_type, FieldType::Vector(_) | FieldType::StructArray(_)));
    let vectors = holds(|field_type| matches!(field_type, FieldType::Vector(_)));

    let mut docs: Vec<String> = match form {
        // The values that serialize_values() takes, but for a vector's view
        Form::Values if is_root && has_serialize_values(schema, index) && !vectors => vec![
            "Reads every field's value at once into a tuple, in the fields' order, as".to_owned(),
            "`serialize_values()` takes them, as `fields()` reads them into a dict.".to_owned(),
        ],
        Form::Values if tables => vec![
            "Reads every field's value into a tuple, in the fields' order, as its getter"
                .to_owned(),
            "returns it, but a nested table's: its values, in place of it when they're".to_owned(),
            "all of fixed size, and in a tuple of their own otherwise.".to_owned(),
        ],
        Form::Values => vec![
            "Reads every field's value into a tuple, in the fields' order, as its getter"
                .to_owned(),
            "returns it.".to_owned(),
        ],
        Form::Fields => {
            let written_by = if is_root { "serialize()" } else { "write()" };
            let with = match (tables, items) {
                (true, true) => {
                    ", with a nested table's fields as its `fields()` reads them, and the \
                     items of vectors and arrays of structs in lists"
                }
                (true, false) => ", with a nested table's fields as its `fields()` reads them",
                (false, true) => ", with the items of vectors and arrays of structs in lists",
                (false, false) => "",
            };
            wrap(&format!(
                "Reads every field at once, as `{written_by}` takes them{with}."
            ))
        }
    };

    if fixed_fields(schema, index) > 0 {
        docs.extend([
            String::new(),
            "It reads the leading fields of fixed size in one call when the table's".to_owned(),
            "offsets are those that its writer writes, and otherwise every field".to_owned(),
            "through the getters, which raise `DecodeError` for bytes they can't read.".to_owned(),
        ]);
    } else if !definition.fields.is_empty() {
        docs.extend([
            String::new(),
            "It raises `DecodeError` for bytes that the getters can't read, as they do.".to_owned(),
        ]);
    }
    docs
}

/// The lines of a docstring's paragraph, wrapped at 76 characters, which the
/// docstring's indentation keeps within the formatter's width
fn wrap(paragraph: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in paragraph.split(' ') {
        if !line.is_empty() && line.len() + 1 + word.len() > 76 {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push(line);
    lines
}

/// A vector view's `field_values()` and `item_field_values()`, which read
/// its items' values as their table's `field_values()` reads each, and its
/// `fields()` when the table is of fixed size, which reads every item's fields
/// as the table's `fields()` reads each. A table of fixed size reads every
/// item in one call when the items' offsets are those that their writer
/// writes, and otherwise item by item.
fn vector_readers(writer: &mut CodeWriter, schema: &Schema, index: usize) {
    let definition = schema.definition(index);
    let table = &definition.name;
    let values = values_alias(definition);

    vector_item_values(writer, schema, index, &values);

    writer.blank();
    let list = Annotation::Generic("list", vec![values]);
    def_line(writer, "field_values", &["self".to_owned()], &list);
    writer.indent();
    docstring(
        writer,
        &[
            format!("Reads every item's values at once, as `{table}.field_values()` reads"),
            "each, in one call when the items' offsets are those that their writer".to_owned(),
            "writes, and otherwise item by item".to_owned(),
        ],
    );
    if is_fixed(schema, index) {
        vector_fields(writer, table);
        writer.line("return [item.field_values() for item in self]");
        writer.dedent();
        writer.blank();
        if holds_struct(schema, definition, definition.fields.len()) {
            let mut reads = Reads::new(schema, Form::Values);
            let elements = reads.unpacked(definition, definition.fields.len(), "values", &mut 0);
            let element = reads.tuple(&elements);
            comprehension(
                writer,
                "return ",
                &element.flat(),
                "for values in items",
                |writer| writer.statement(&STYLE, "", &element, ""),
            );
        } else {
            writer.line("return items");
        }
    } else {
        writer.line("return [item.field_values() for item in self]");
    }
    writer.dedent();

    vector_iter_values(writer, schema, index);

    if !is_flat(schema, index) {
        return;
    }
    writer.blank();
    writer.open(format!("def fields(self) -> list[{table}Fields]:"));
    docstring(
        writer,
        &[
            format!("Reads every item's fields at once, as `{table}.fields()` reads each,"),
            "in one call when the items' offsets are those that their writer writes,".to_owned(),
            "and otherwise item by item".to_owned(),
        ],
    );
    if is_fixed(schema, index) {
        vector_fields(writer, table);
        writer.line("return [item.fields() for item in self]");
        writer.dedent();
        writer.blank();
        let mut reads = Reads::new(schema, Form::Fields);
        let elements = reads.unpacked(definition, definition.fields.len(), "values", &mut 0);
        let element = reads.dict(&elements);
        comprehension(
            writer,
            "return ",
            &element.flat(),
            "for values in items",
            |writer| dict_statement(writer, "", &element),
        );
    } else {
        writer.line("return [item.fields() for item in self]");
    }
    writer.dedent();
}

/// A vector view's `iter_field_values()`, which iterates over its items'
/// values as its table's `field_values()` reads each: through the runtime's
/// `iter_vector_fields()` for a table of fixed size without structs, which
/// reads each item's end when it reaches it, and without creating a reader
/// of each item but those whose offsets aren't the ones that their writer
/// writes, and through the items' readers for any other table
fn vector_iter_values(writer: &mut CodeWriter, schema: &Schema, index: usize) {
    let definition = schema.definition(index);
    let table = &definition.name;
    let direct =
        is_fixed(schema, index) && !holds_struct(schema, definition, definition.fields.len());

    writer.blank();
    let iterator = Annotation::Generic("Iterator", vec![values_alias(definition)]);
    def_line(writer, "iter_field_values", &["self".to_owned()], &iterator);
    writer.indent();
    if direct {
        docstring(
            writer,
            &[
                format!("Reads every item's values one after another, as `{table}.field_values()`"),
                "reads each, reading each item when it reaches it, so that it raises".to_owned(),
                "`DecodeError` for an item that doesn't lie within the bytes when it".to_owned(),
                "reaches it".to_owned(),
            ],
        );
        writer.line("buffer = self._buffer");
        let read = call(
            "buffer.iter_vector_fields",
            vec![
                text("self._position"),
                text(format!("{table}._FIXED_FIELDS")),
                text(format!(
                    "lambda offset: {table}(buffer, offset).field_values()"
                )),
            ],
        );
        writer.statement(&STYLE, "return ", &read, "");
    } else {
        docstring(
            writer,
            &[format!(
                "Reads every item's values one after another, as `{table}.field_values()` reads each"
            )],
        );
        writer.line("return (item.field_values() for item in self)");
    }
    writer.dedent();
}

/// A vector view's `item_field_values()`, which reads one item's values, as
/// its table's `field_values()` reads them, without reading the others
fn vector_item_values(writer: &mut CodeWriter, schema: &Schema, index: usize, values: &Annotation) {
    let definition = schema.definition(index);
    let table = &definition.name;

    writer.blank();
    def_line(
        writer,
        "item_field_values",
        &["self".to_owned(), "index: int".to_owned()],
        values,
    );
    writer.indent();
    docstring(
        writer,
        &[
            format!("Reads the values of item `index`, as `{table}.field_values()` reads"),
            "them, without reading the others, counting from the end if it's negative.".to_owned(),
            String::new(),
            "Raises `IndexError` if the vector has no item `index`, and `DecodeError`".to_owned(),
            "for bytes that the item's getters can't read.".to_owned(),
        ],
    );
    if is_fixed(schema, index) {
        writer.open("if index < 0:");
        writer.line("index += self._length");
        writer.dedent();
        writer.statement(
            &STYLE,
            "offset = ",
            &call(
                "self._buffer.get_vector_item_offset",
                vec![text("self._position"), text("self._length"), text("index")],
            ),
            "",
        );
        writer.statement(
            &STYLE,
            "values = ",
            &call(
                "self._buffer.get_fields",
                vec![text("offset"), text(format!("{table}._FIXED_FIELDS"))],
            ),
            "",
        );
        writer.open("if values is None:");
        writer.line(format!(
            "return {table}(self._buffer, offset).field_values()"
        ));
        writer.dedent();
        writer.blank();
        return_converted(writer, schema, definition, "return ", "values");
    } else {
        writer.line("return self[index].field_values()");
    }
    writer.dedent();
}

/// Returns, after `prefix`, the values of a table of fixed size that
/// `ByteBuffer.get_fields()` read into `values`, with each struct read from
/// its bytes
fn return_converted(
    writer: &mut CodeWriter,
    schema: &Schema,
    definition: &Definition,
    prefix: &str,
    values: &str,
) {
    if holds_struct(schema, definition, definition.fields.len()) {
        let mut reads = Reads::new(schema, Form::Values);
        let elements = reads.unpacked(definition, definition.fields.len(), values, &mut 0);
        writer.statement(&STYLE, prefix, &reads.tuple(&elements), "");
    } else {
        writer.line(format!("{prefix}{values}"));
    }
}

/// A message's `serialize_values()`, which writes it from its fields' values
/// in their order, as positional arguments, as `field_values()` returns them,
/// but a vector's as its items' values, as its view's `field_values()` returns
/// them
fn serialize_values(writer: &mut CodeWriter, schema: &Schema, index: usize, layout: Layout) {
    let definition = schema.definition(index);
    let name = &definition.name;
    let names: Vec<String> = definition
        .fields
        .iter()
        .map(|field| parameter(field, &["writer"]))
        .collect();
    let mut parameters = vec!["cls".to_owned()];
    for (field, name) in definition.fields.iter().zip(&names) {
        let value_type = match field.field_type {
            FieldType::Vector(item) => {
                format!("Sequence[{}]", values_alias(schema.definition(item)).flat())
            }
            field_type => input_type(field_type, schema),
        };
        parameters.push(format!("{name}: {value_type}"));
    }
    parameters.push("/".to_owned());
    let has_vector = definition
        .fields
        .iter()
        .any(|field| matches!(field.field_type, FieldType::Vector(_)));

    writer.line("@classmethod");
    def_line(
        writer,
        "serialize_values",
        &parameters,
        &Annotation::Name("memoryview".to_owned()),
    );
    writer.indent();
    let mut docs = vec![format!(
        "Writes {} from every field's value, in their order, as",
        article(name)
    )];
    if has_vector {
        docs.extend([
            "`field_values()` returns them, but a vector's as its items' values, as its".to_owned(),
            "view's `field_values()` returns them".to_owned(),
        ]);
    } else {
        docs.push("`field_values()` returns them".to_owned());
    }
    docstring(writer, &docs);
    let mut arguments = vec![
        text(definition.fields.len().to_string()),
        text("cls._LAYOUT"),
    ];
    if layout.version.is_some() {
        arguments.push(text("cls.FORMAT_VERSION"));
    }
    writer.statement(&STYLE, "writer = ", &call("TableWriter", arguments), "");

    // The leading fields of fixed size in one call, a string's as an argument
    // of its own, after the packed values
    let fixed = fixed_fields(schema, index);
    if fixed > 0 {
        let has_string = definition.fields[fixed - 1].field_type == FieldType::String;
        let packed = &names[..fixed - usize::from(has_string)];
        let mut arguments = vec![text("cls._FIXED_FIELDS"), tuple_expr(packed)];
        if has_string {
            arguments.push(text(names[fixed - 1].clone()));
        }
        writer.statement(&STYLE, "", &call("writer.set_fields", arguments), "");
    }
    for (index, field) in definition.fields.iter().enumerate().skip(fixed) {
        let value = names[index].clone();
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
            FieldType::Vector(item) => (
                "set_vector_tables".to_owned(),
                vec![
                    position,
                    text(format!("{}._FIXED_FIELDS", schema.definition(item).name)),
                    text(value),
                ],
            ),
            FieldType::Struct(_) | FieldType::StructArray(_) | FieldType::Table(_) => {
                unreachable!("messages of these fields have no serialize_values()")
            }
        };
        writer.statement(&STYLE, "", &call(format!("writer.{method}"), arguments), "");
    }
    writer.blank();
    writer.line("return writer.bytes()");
    writer.dedent();
}

/// A struct of scalars' `read_values()`, `read_array_values()` and
/// `serialize_values()`, which read its values into tuples, as its `Struct`
/// unpacks them, and write it from them, without its dataclass
#[allow(clippy::too_many_lines)]
fn struct_values(writer: &mut CodeWriter, schema: &Schema, definition: &Definition) {
    let name = &definition.name;
    let types = value_types(schema, definition);
    let does_not_fit = call(
        "DecodeError.out_of_range",
        vec![
            text("offset"),
            text("cls.SIZE"),
            text("memoryview(data).nbytes"),
        ],
    );
    let not_contiguous = call(
        "TypeError",
        vec![text(format!(
            "\"{name} can't read a buffer that isn't C-contiguous\""
        ))],
    );

    writer.blank();
    writer.line("@classmethod");
    let parameters = ["cls", "data: Buffer", "offset: int = 0"].map(str::to_owned);
    def_line(
        writer,
        "read_values",
        &parameters,
        &tuple_annotation(types.clone()),
    );
    writer.indent();
    docstring(
        writer,
        &[
            format!(
                "Reads the values of {} at `offset` in `data`, in its fields'",
                article(name)
            ),
            "order, as `read()` reads them, and raises as it does".to_owned(),
        ],
    );
    writer.open("if offset < 0:");
    writer.statement(&STYLE, "raise ", &does_not_fit, "");
    writer.dedent();
    writer.blank();
    writer.open("try:");
    writer.line("return cls._STRUCT.unpack_from(data, offset)");
    writer.dedent();
    writer.open("except (error, OverflowError) as cause:");
    writer.statement(&STYLE, "raise ", &does_not_fit, " from cause");
    writer.dedent();
    writer.open("except BufferError as cause:");
    writer.statement(&STYLE, "raise ", &not_contiguous, " from cause");
    writer.dedent();
    writer.dedent();

    writer.blank();
    writer.line("@classmethod");
    let parameters = ["cls", "data: Buffer"].map(str::to_owned);
    let annotation = Annotation::Generic("list", vec![tuple_annotation(types.clone())]);
    def_line(writer, "read_array_values", &parameters, &annotation);
    writer.indent();
    docstring(
        writer,
        &[
            format!("Reads the values of the `{name}`s one after another in `data`, as"),
            "`read_array()` reads them, and raises as it does".to_owned(),
        ],
    );
    writer.open("try:");
    writer.line("return list(cls._STRUCT.iter_unpack(data))");
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

    // The values are parameters, positional only, so that a field named
    // after the struct module's error, which the method catches, takes
    // another name, which callers don't see
    let names: Vec<String> = definition
        .fields
        .iter()
        .map(|field| parameter(field, &["error"]))
        .collect();
    let mut parameters = vec!["cls".to_owned()];
    for (name, value_type) in names.iter().zip(&types) {
        parameters.push(format!("{name}: {value_type}"));
    }
    parameters.push("/".to_owned());

    writer.blank();
    writer.line("@classmethod");
    let annotation = Annotation::Name("bytes".to_owned());
    def_line(writer, "serialize_values", &parameters, &annotation);
    writer.indent();
    docstring(
        writer,
        &[
            format!(
                "Returns the bytes of {} of these values, in its fields' order,",
                article(name)
            ),
            "as `read_values()` returns them.".to_owned(),
            String::new(),
            "Raises `ValueError` if a value doesn't fit its field's type.".to_owned(),
        ],
    );
    let values_do_not_fit = call(
        "ValueError",
        vec![text(format!("\"{name}'s values don't fit its fields\""))],
    );
    let pack = call("cls._STRUCT.pack", names.iter().map(text).collect());
    writer.open("try:");
    writer.statement(&STYLE, "return ", &pack, "");
    writer.dedent();
    writer.open("except (error, OverflowError) as cause:");
    writer.statement(&STYLE, "raise ", &values_do_not_fit, " from cause");
    writer.dedent();
    writer.dedent();
}

/// A tuple of `values`, as Python writes one: with a comma after a sole value
fn tuple_flat(values: &[String]) -> String {
    match values {
        [value] => format!("({value},)"),
        _ => format!("({})", values.join(", ")),
    }
}

/// A tuple of `values`, which breaks over lines as a call's arguments do,
/// unless it's of one value
fn tuple_expr(values: &[String]) -> Expr {
    match values {
        [value] => text(format!("({value},)")),
        _ => call("", values.iter().map(text).collect()),
    }
}

/// Writes `prefix` + a tuple of `values`, broken over lines when it doesn't fit
fn tuple_statement(writer: &mut CodeWriter, prefix: &str, values: &[String]) {
    writer.statement(&STYLE, prefix, &tuple_expr(values), "");
}

/// Writes `prefix` + a list comprehension of an element and its `clause`, as
/// Ruff breaks it: on one line, or its contents on a line of their own, or
/// the element, which `write_element` writes, and the clause on lines of
/// their own
fn comprehension(
    writer: &mut CodeWriter,
    prefix: &str,
    element: &str,
    clause: &str,
    write_element: impl FnOnce(&mut CodeWriter),
) {
    let contents = format!("{element} {clause}");
    let line = format!("{prefix}[{contents}]");
    if writer.fits(&STYLE, &line) {
        writer.line(line);
        return;
    }

    writer.open(format!("{prefix}["));
    if writer.fits(&STYLE, &contents) {
        writer.line(contents);
    } else {
        write_element(writer);
        writer.line(clause);
    }
    writer.close("]");
}

/// A type annotation: a name, or a generic of type arguments, such as a tuple
/// of values, which Ruff breaks within its brackets when it doesn't fit, an
/// argument on each line, each broken in turn when it doesn't fit, and a
/// comma after each but a sole one
enum Annotation {
    Name(String),
    Generic(&'static str, Vec<Annotation>),
}

impl Annotation {
    fn flat(&self) -> String {
        match self {
            Self::Name(name) => name.clone(),
            Self::Generic(name, arguments) if arguments.is_empty() => format!("{name}[()]"),
            Self::Generic(name, arguments) => {
                let arguments: Vec<String> = arguments.iter().map(Self::flat).collect();
                format!("{name}[{}]", arguments.join(", "))
            }
        }
    }

    /// Writes `prefix` + the annotation + `suffix`, broken as Ruff breaks it
    fn write(&self, writer: &mut CodeWriter, prefix: &str, suffix: &str) {
        let line = format!("{prefix}{}{suffix}", self.flat());
        match self {
            Self::Generic(name, arguments)
                if !arguments.is_empty() && !writer.fits(&STYLE, &line) =>
            {
                writer.open(format!("{prefix}{name}["));
                let comma = if arguments.len() > 1 { "," } else { "" };
                for argument in arguments {
                    argument.write(writer, "", comma);
                }
                writer.close(format!("]{suffix}"));
            }
            _ => writer.line(line),
        }
    }
}

/// The annotation of a tuple of `types`
fn tuple_annotation(types: Vec<String>) -> Annotation {
    Annotation::Generic("tuple", types.into_iter().map(Annotation::Name).collect())
}

/// A method's `def` line, as Ruff breaks it: on one line, or with its
/// parameters on lines of their own, all on one when they fit, or one on
/// each, and a sole one with a comma, and then its return annotation, which
/// is broken in turn when it doesn't fit
fn def_line(writer: &mut CodeWriter, name: &str, parameters: &[String], annotation: &Annotation) {
    let together = parameters.join(", ");
    let line = format!("def {name}({together}) -> {}:", annotation.flat());
    if writer.fits(&STYLE, &line) {
        writer.line(line);
        return;
    }

    writer.open(format!("def {name}("));
    if parameters.len() > 1 && writer.fits(&STYLE, &together) {
        writer.line(together);
    } else {
        for parameter in parameters {
            writer.line(format!("{parameter},"));
        }
    }
    writer.dedent();

    annotation.write(writer, ") -> ", ":");
}

/// A value of a `TypedDict` of fields that `fields()` returns: an
/// expression, or a nested table's fields
enum DictValue {
    Text(String),
    /// A value that breaks over lines as a call does
    Expr(Expr),
    Dict(Vec<(String, DictValue)>),
}

impl DictValue {
    fn flat(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Expr(expression) => expression.flat(),
            Self::Dict(entries) => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| format!("\"{key}\": {}", value.flat()))
                    .collect();
                format!("{{{}}}", entries.join(", "))
            }
        }
    }
}

/// Writes `prefix` + a dict + `suffix`, broken over lines as Ruff breaks it,
/// an entry on each line, when it doesn't fit
fn dict_statement(writer: &mut CodeWriter, prefix: &str, value: &DictValue) {
    dict_lines(writer, prefix, value, "");
}

fn dict_lines(writer: &mut CodeWriter, prefix: &str, value: &DictValue, suffix: &str) {
    let line = format!("{prefix}{}{suffix}", value.flat());
    match value {
        DictValue::Dict(entries) if !entries.is_empty() && !writer.fits(&STYLE, &line) => {
            writer.open(format!("{prefix}{{"));
            for (key, value) in entries {
                dict_lines(writer, &format!("\"{key}\": "), value, ",");
            }
            writer.close(format!("}}{suffix}"));
        }
        DictValue::Expr(expression) => writer.statement(&STYLE, prefix, expression, suffix),
        _ => writer.line(line),
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
            FieldType::Table(index) => {
                let table = &schema.definition(index).name;
                let read = call(table.clone(), vec![text("self._buffer"), offset]);
                writer.statement(&STYLE, "return ", &read, "");
            }
            // The view of a vector or an array of structs, which reads each
            // item when it's asked for
            FieldType::StructArray(_) | FieldType::Vector(_) => {
                let view = value_type(field.field_type, schema);
                let read = call(view, vec![text("self._buffer"), offset]);
                writer.statement(&STYLE, "return ", &read, "");
            }
        }
        writer.dedent();
    }
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
        // Views, which read each item when it's asked for
        FieldType::Vector(index) => format!("{}Vector", schema.definition(index).name),
        FieldType::StructArray(index) => format!("{}Array", schema.definition(index).name),
    }
}

fn input_type(field_type: FieldType, schema: &Schema) -> String {
    match field_type {
        FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
        FieldType::Array(scalar) => format!("Buffer | Iterable[{}]", scalar_type(scalar)),
        FieldType::String => "str".to_owned(),
        // Structs are written from their dataclasses
        FieldType::Struct(index) => schema.definition(index).name.clone(),
        // Arrays of structs are written from any objects of their fields
        FieldType::StructArray(index) => {
            format!("Sequence[{}Fields]", schema.definition(index).name)
        }
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
