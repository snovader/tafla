//! Swift for `Tafla`. Messages and tables are nonescapable views of their
//! bytes, whose throwing getters read fields lazily, and vectors and arrays of
//! structs views that read each item when it's asked for; they're written
//! from structs of their fields, which a `TableSizer` sizes and a
//! `TableWriter` writes, nested tables in place. Structs are plain structs
//! read and written at fixed offsets.
//!
//! The views' lifetimes depend on their bytes', which Swift 6.2 to 6.4
//! declare with the experimental feature `Lifetimes` and `@_lifetime`, so the
//! target that holds the generated code enables the feature, as the runtime's
//! does. Code that uses the views needs nothing.
//!
//! What reads is `@inlinable`, as the runtime's getters are, and the views'
//! stored properties are `@usableFromInline`: the package manager's release
//! build optimizes each module on its own, so that otherwise each getter of
//! each item would be a call into the generated module. The views and
//! structs are `@frozen`, so that a module built with library evolution, as a
//! binary framework is, can still set their stored properties in an
//! inlinable initializer. That commits them to those properties: a view's
//! reader of its bytes, and a struct's fields, which the format fixes. What
//! writes isn't inlinable: the optimizer doesn't inline a `serialize()` into
//! its caller, which then calls the same code, and nested tables' `add(to:)`
//! and `write(to:)` are called within the module.
//!
//! A message's `layout` is an inlinable computed property, which builds the
//! layout in the code that reads with it, as the runtime's
//! `TableLayout.default` is. A caller reaches a `static let` of another
//! module through an accessor that it can't see into, and the getters inlined
//! into it would then load the layout's offset type from memory and switch on
//! it at each field, rather than know it.
//!
//! An enum is a `RawRepresentable` struct of its integer, whose values are
//! inlinable computed static properties, for the same reason, so that a value
//! is a constant in the code that names it. Its initializer takes any
//! integer, so a field of the enum reads any integer of its type, named or
//! not, as the integer, wrapped where it's read and unwrapped where it's
//! written. An array of an enum is an array of its integer type, which is how
//! the bytes hold it.

use super::prose::indefinite_article;
use super::writer::{CodeWriter, Style, call, text};
use super::{GeneratedFile, Options};
use crate::model::{Definition, Enum, Field, FieldType, Kind, Layout, OffsetType, Scalar, Schema};
use crate::naming::camel_case;

/// The runtime's module, unless [`Options::runtime`] names another
const RUNTIME: &str = "Tafla";

/// swift-format's configuration in `swift/.swift-format`, whose line length
/// is 120. It keeps a call or a declaration broken before each argument and
/// its closing parenthesis, which is how a line that doesn't fit is broken.
const STYLE: Style = Style {
    width: 120,
    indent: "    ",
    trailing_comma: false,
    hug_arguments: false,
    max_arguments_width: None,
};

/// The keywords that a declaration's name can't be, which names take in
/// backticks, as Swift escapes them, but for `self`, which `MEMBERS` lists:
/// those of declarations, statements, expressions and types. Others are
/// keywords only where the grammar expects them.
const KEYWORDS: [&str; 55] = [
    "as",
    "associatedtype",
    "await",
    "borrowing",
    "break",
    "case",
    "catch",
    "class",
    "consuming",
    "continue",
    "default",
    "defer",
    "deinit",
    "do",
    "else",
    "enum",
    "extension",
    "fallthrough",
    "false",
    "fileprivate",
    "for",
    "func",
    "guard",
    "if",
    "import",
    "in",
    "init",
    "inout",
    "internal",
    "is",
    "let",
    "nil",
    "nonisolated",
    "open",
    "operator",
    "precedencegroup",
    "private",
    "protocol",
    "public",
    "repeat",
    "rethrows",
    "return",
    "static",
    "struct",
    "subscript",
    "super",
    "switch",
    "throw",
    "throws",
    "true",
    "try",
    "typealias",
    "var",
    "where",
    "while",
];

/// The names that fields take a `_` after: `serialize()` of fields structs
/// and structs, which a property of its name clashes with, and `self`, which
/// an initializer's parameter of its name would hide, even in backticks. The
/// stored properties of messages and tables, `_reader` and `_position`, start
/// with a `_`, which no field does.
const MEMBERS: [&str; 2] = ["self", "serialize"];

pub fn generate(
    schema: &Schema,
    stem: &str,
    header: &str,
    options: &Options,
) -> Vec<GeneratedFile> {
    let runtime = options.runtime.as_deref().unwrap_or(RUNTIME);
    let generator = Generator { schema };

    let mut writer = CodeWriter::new(STYLE.indent);
    writer.line(format!("// {header}"));
    // A schema without definitions imports nothing, since enums need only the
    // standard library
    if !schema.definitions.is_empty() {
        writer.blank();
        writer.line(format!("import {runtime}"));
    }

    for enumeration in &schema.enums {
        writer.blank();
        enumeration_struct(&mut writer, enumeration);
    }

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
        path: format!("{stem}.swift"),
        contents: writer.finish(),
    }]
}

struct Generator<'a> {
    schema: &'a Schema,
}

/// How a getter reads its field
enum Read {
    /// In one expression of the field's position, which the runtime's
    /// getter reads at
    Expression(String),
    /// From `field`, the field's position, in a statement after the one that
    /// finds it
    FromPosition(String),
}

impl Generator<'_> {
    fn message(&self, writer: &mut CodeWriter, definition: &Definition, layout: Layout) {
        let name = &definition.name;

        swift_doc(writer, &definition.docs);
        writer.line("@frozen");
        writer.open(format!("public struct {name}: ~Escapable {{"));
        if let Some(version) = layout.version {
            swift_doc(
                writer,
                &["The version of the format, which the header carries and the initializer checks"],
            );
            writer.line(format!(
                "public static let formatVersion: UInt32 = {version}"
            ));
            writer.blank();
        }
        swift_doc(
            writer,
            &[format!(
                "The layout that {} is written and read in",
                article(name)
            )],
        );
        writer.line("@inlinable");
        writer.line(format!(
            "public static var layout: TableLayout {{ {} }}",
            layout_expression(layout)
        ));
        writer.blank();
        writer.line("@usableFromInline");
        writer.line("let _reader: TableReader");
        writer.blank();

        if layout.version.is_some() {
            swift_doc(
                writer,
                &[
                    format!(
                        "Reads {} from `bytes`, which it can't outlive.",
                        article(name)
                    ),
                    String::new(),
                    "Throws a `DecodeError` if `bytes` are in another format version.".to_owned(),
                ],
            );
            writer.line("@inlinable");
            writer.line("@_lifetime(copy bytes)");
            writer.open("public init(_ bytes: RawSpan) throws(DecodeError) {");
            writer.line("_reader = TableReader(bytes, layout: Self.layout)");
            writer.blank();
            writer.line("let version = try _reader.version");
            writer.open("guard version == Self.formatVersion else {");
            writer
                .line("throw .unsupportedVersion(version: version, supported: Self.formatVersion)");
            writer.close("}");
            writer.close("}");
            writer.blank();

            swift_doc(
                writer,
                &[
                    "Returns the format version in the header of `bytes`, without checking it, so that an",
                    "application can choose the reader of that version.",
                    "",
                    "Throws a `DecodeError` if the header doesn't fit in `bytes`.",
                ],
            );
            writer.line("@inlinable");
            writer.open(
                "public static func version(of bytes: RawSpan) throws(DecodeError) -> UInt32 {",
            );
            writer.line("try TableReader(bytes, layout: Self.layout).version");
            writer.close("}");
        } else {
            swift_doc(
                writer,
                &[format!(
                    "Reads {} from `bytes`, which it can't outlive",
                    article(name)
                )],
            );
            writer.line("@inlinable");
            writer.line("@_lifetime(copy bytes)");
            writer.open("public init(_ bytes: RawSpan) {");
            writer.line("_reader = TableReader(bytes, layout: Self.layout)");
            writer.close("}");
        }

        self.getters(writer, definition, true);
        writer.close("}");
    }

    fn table(&self, writer: &mut CodeWriter, definition: &Definition) {
        swift_doc(writer, &definition.docs);
        writer.line("@frozen");
        writer.open(format!("public struct {}: ~Escapable {{", definition.name));
        writer.line("@usableFromInline");
        writer.line("let _reader: TableReader");
        writer.line("@usableFromInline");
        writer.line("let _position: Int");
        writer.blank();

        swift_doc(
            writer,
            &[
                "Reads the table at `position` in the bytes that `reader` reads, as its parent's getter finds it",
            ],
        );
        writer.line("@inlinable");
        writer.line("@_lifetime(copy reader)");
        writer.open("init(reader: TableReader, position: Int) {");
        writer.line("_reader = reader");
        writer.line("_position = position");
        writer.close("}");

        self.getters(writer, definition, false);
        writer.close("}");
    }

    /// A getter of each field, which reads it from the bytes, of the root
    /// table or of a table nested at `position`
    fn getters(&self, writer: &mut CodeWriter, definition: &Definition, root: bool) {
        for (index, field) in definition.fields.iter().enumerate() {
            writer.blank();

            let position = if root {
                format!("_reader.rootOffset({index})")
            } else {
                format!("_reader.fieldOffset({index}, inTableAt: _position)")
            };
            let (value_type, read) = self.read(field, &position);
            // Views of the bytes depend on the reader's
            let is_view = matches!(
                field.field_type,
                FieldType::Array(_)
                    | FieldType::StructArray(_)
                    | FieldType::Table(_)
                    | FieldType::Vector(_)
            );

            let mut docs = field.docs.clone();
            if !docs.is_empty() {
                docs.push(String::new());
            }
            docs.extend(self.getter_docs(field.field_type));
            swift_doc(writer, &docs);

            writer.line("@inlinable");
            writer.open(format!(
                "public var {}: {value_type} {{",
                identifier(&field.name)
            ));
            if is_view {
                writer.line("@_lifetime(copy self)");
            }
            match read {
                // On a line of its own where it's too long for the getter's,
                // as a field of an enum with a long name is
                Read::Expression(expression) => {
                    let getter = format!("get throws(DecodeError) {{ {expression} }}");
                    if writer.fits(&STYLE, &getter) {
                        writer.line(getter);
                    } else {
                        writer.open("get throws(DecodeError) {");
                        writer.line(expression);
                        writer.close("}");
                    }
                }
                Read::FromPosition(statement) => {
                    writer.open("get throws(DecodeError) {");
                    writer.line(format!("let field = try {position}"));
                    writer.line(format!("return {statement}"));
                    writer.close("}");
                }
            }
            writer.close("}");
        }
    }

    /// The type that a getter returns, and how it reads its field at
    /// `position`
    fn read(&self, field: &Field, position: &str) -> (String, Read) {
        match field.field_type {
            FieldType::StringVector => {
                unreachable!("compile() rejects arrays of strings for this target")
            }
            FieldType::Scalar(Scalar::Bool) => (
                "Bool".to_owned(),
                Read::Expression(format!("try _reader.uint8(at: {position}) != 0")),
            ),
            FieldType::Scalar(scalar) => {
                let integer = format!("_reader.{}(at: {position})", scalar.name());
                (
                    self.scalar_type(field, scalar).to_owned(),
                    Read::Expression(format!("try {}", self.scalar_value(field, integer))),
                )
            }
            FieldType::Array(scalar) => (
                format!("ArrayView<{}>", swift_type(scalar)),
                Read::Expression(format!(
                    "try _reader.{}Array(at: {position})",
                    scalar.name()
                )),
            ),
            FieldType::String => (
                "String".to_owned(),
                Read::Expression(format!("try _reader.string(at: {position})")),
            ),
            FieldType::Struct(index) => {
                let name = &self.schema.definition(index).name;
                (
                    name.clone(),
                    Read::FromPosition(format!(
                        "try {name}(_reader.struct(at: field, size: {name}.size), at: 0)"
                    )),
                )
            }
            FieldType::StructArray(index) => {
                let name = &self.schema.definition(index).name;
                (
                    format!("{name}Array"),
                    Read::FromPosition(format!(
                        "try {name}Array(_reader.structArray(at: field, size: {name}.size))"
                    )),
                )
            }
            FieldType::Table(index) => {
                let name = &self.schema.definition(index).name;
                (
                    name.clone(),
                    Read::FromPosition(format!("{name}(reader: _reader, position: field)")),
                )
            }
            FieldType::Vector(index) => {
                let name = &self.schema.definition(index).name;
                (
                    format!("{name}Vector"),
                    Read::FromPosition(format!("try {name}Vector(_reader.vector(at: field))")),
                )
            }
        }
    }

    /// What a getter throws, and what its view reads later
    fn getter_docs(&self, field_type: FieldType) -> Vec<String> {
        match field_type {
            FieldType::Vector(index) => vec![
                format!(
                    "A view of the items, which reads each `{}` when it's asked for, and throws then if",
                    self.schema.definition(index).name
                ),
                "its bytes are corrupt.".to_owned(),
                String::new(),
                "Throws a `DecodeError` if the field, or the vector's count or the ends of its items, lie"
                    .to_owned(),
                "outside the bytes.".to_owned(),
            ],
            FieldType::StructArray(index) => vec![
                format!(
                    "A view of the structs, which reads each `{}` when it's asked for.",
                    self.schema.definition(index).name
                ),
                String::new(),
                "Throws a `DecodeError` if the field, or its structs, lie outside the bytes.".to_owned(),
            ],
            _ => vec!["Throws a `DecodeError` if the field lies outside the bytes.".to_owned()],
        }
    }

    /// A struct of the values that write a message or a table, with the
    /// methods that write them
    fn fields_struct(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
        let name = &definition.name;

        swift_doc(
            writer,
            &[format!("The fields of {}, to write", article(name))],
        );
        writer.open(format!("public struct {name}Fields: Hashable, Sendable {{"));
        if definition.kind == Kind::Table {
            swift_doc(
                writer,
                &[
                    format!(
                        "The alignment that a parent pads {} to: the largest alignment of its arrays",
                        article(name)
                    ),
                    "and nested tables, or 1".to_owned(),
                ],
            );
            writer.line(format!(
                "public static let alignment: Alignment = {}",
                alignment(self.schema.table_alignment(index))
            ));
            writer.blank();
        }
        self.properties(writer, &definition.fields);
        writer.blank();
        self.initializer(writer, &definition.fields);
        writer.blank();

        match definition.kind {
            Kind::Message(layout) => self.serialize(writer, definition, layout),
            Kind::Table => self.size_and_write(writer, definition),
            Kind::Struct(_) => unreachable!("structs are written by their own methods"),
        }
        writer.close("}");
    }

    /// The stored properties of a fields struct or a struct
    fn properties(&self, writer: &mut CodeWriter, fields: &[Field]) {
        for field in fields {
            swift_doc(writer, &field.docs);
            writer.line(format!(
                "public var {}: {}",
                identifier(&field.name),
                self.input_type(field)
            ));
        }
    }

    /// The memberwise initializer, which a public struct declares itself,
    /// since Swift's own is internal
    fn initializer(&self, writer: &mut CodeWriter, fields: &[Field]) {
        let parameters = fields
            .iter()
            .map(|field| {
                text(format!(
                    "{}: {}",
                    identifier(&field.name),
                    self.input_type(field)
                ))
            })
            .collect();
        writer.statement(&STYLE, "public ", &call("init", parameters), " {");
        writer.indent();
        for field in fields {
            let name = identifier(&field.name);
            writer.line(format!("self.{name} = {name}"));
        }
        writer.close("}");
    }

    fn serialize(&self, writer: &mut CodeWriter, definition: &Definition, layout: Layout) {
        let name = &definition.name;

        swift_doc(
            writer,
            &[
                format!("Writes {} with every field.", article(name)),
                String::new(),
                format!(
                    "Throws a `BuildError` if it's longer than its offset type, `{}`, addresses.",
                    layout.offset_type.name()
                ),
            ],
        );
        writer.open("public func serialize() throws(BuildError) -> [UInt8] {");
        let arguments = vec![
            text(format!("fields: {}", definition.fields.len())),
            text(format!("layout: {name}.layout")),
        ];
        writer.statement(
            &STYLE,
            "var sizer = try ",
            &call("TableSizer", arguments),
            "",
        );
        self.adds(writer, definition);
        writer.blank();

        writer.open("return try sizer.bytes { writer in");
        self.writes(writer, definition);
        writer.close("}");
        writer.close("}");
    }

    /// The methods with which a table's parent sizes and writes it in place
    fn size_and_write(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;
        let count = definition.fields.len();

        swift_doc(
            writer,
            &[format!(
                "Adds {} to its parent's sizer, after the field or vector that holds it",
                article(name)
            )],
        );
        writer.open("public func add(to sizer: inout TableSizer) {");
        writer.line(format!("let parent = sizer.startTable(fields: {count})"));
        self.adds(writer, definition);
        writer.line("sizer.endTable(parent)");
        writer.close("}");
        writer.blank();

        swift_doc(
            writer,
            &[format!(
                "Writes {} where its parent's writer is, after the field or vector that holds it",
                article(name)
            )],
        );
        writer.open("public func write(to writer: inout TableWriter) {");
        writer.line(format!("let parent = writer.startTable(fields: {count})"));
        self.writes(writer, definition);
        writer.line("writer.endTable(parent)");
        writer.close("}");
    }

    /// Adds each field to `sizer`, with what decides its length
    fn adds(&self, writer: &mut CodeWriter, definition: &Definition) {
        for field in &definition.fields {
            let name = identifier(&field.name);
            match field.field_type {
                FieldType::StringVector => {
                    unreachable!("compile() rejects arrays of strings for this target")
                }
                FieldType::Scalar(scalar) => {
                    writer.line(format!("sizer.add{}()", method(scalar.stored())));
                }
                FieldType::Array(scalar) => {
                    writer.line(format!(
                        "sizer.add{}Array(count: self.{name}.count)",
                        method(scalar)
                    ));
                }
                FieldType::String => writer.line(format!("sizer.addString(self.{name})")),
                FieldType::Struct(index) => {
                    let structure = &self.schema.definition(index).name;
                    writer.line(format!("sizer.addStruct(size: {structure}.size)"));
                }
                FieldType::StructArray(index) => {
                    let structure = &self.schema.definition(index).name;
                    let arguments = vec![
                        text(format!("count: self.{name}.count")),
                        text(format!("size: {structure}.size")),
                        text(format!("alignment: {structure}.alignment")),
                    ];
                    writer.statement(&STYLE, "", &call("sizer.addStructArray", arguments), "");
                }
                FieldType::Table(index) => {
                    let table = &self.schema.definition(index).name;
                    writer.line(format!(
                        "sizer.addTable(alignment: {table}Fields.alignment)"
                    ));
                    writer.line(format!("self.{name}.add(to: &sizer)"));
                }
                FieldType::Vector(_) => {
                    writer.line(format!("sizer.addVector(count: self.{name}.count)"));
                    writer.open(format!("for item in self.{name} {{"));
                    writer.line("item.add(to: &sizer)");
                    writer.close("}");
                }
            }
        }
    }

    /// Writes each field with `writer`, by its index
    fn writes(&self, writer: &mut CodeWriter, definition: &Definition) {
        for (index, field) in definition.fields.iter().enumerate() {
            let name = identifier(&field.name);
            match field.field_type {
                FieldType::StringVector => {
                    unreachable!("compile() rejects arrays of strings for this target")
                }
                FieldType::Scalar(Scalar::Bool) => {
                    writer.line(format!("writer.setUInt8({index}, self.{name} ? 1 : 0)"));
                }
                FieldType::Scalar(scalar) => {
                    writer.line(format!(
                        "writer.set{}({index}, {})",
                        method(scalar),
                        scalar_integer(field)
                    ));
                }
                FieldType::Array(scalar) => {
                    writer.line(format!(
                        "writer.set{}Array({index}, self.{name})",
                        method(scalar)
                    ));
                }
                FieldType::String => writer.line(format!("writer.setString({index}, self.{name})")),
                FieldType::Struct(_) => {
                    writer.line(format!("writer.setStruct({index})"));
                    writer.line(format!("self.{name}.write(to: &writer)"));
                }
                FieldType::StructArray(item) => {
                    let structure = &self.schema.definition(item).name;
                    let arguments = vec![
                        text(index.to_string()),
                        text(format!("count: self.{name}.count")),
                        text(format!("alignment: {structure}.alignment")),
                    ];
                    writer.statement(&STYLE, "", &call("writer.setStructArray", arguments), "");
                    writer.open(format!("for item in self.{name} {{"));
                    writer.line("item.write(to: &writer)");
                    writer.close("}");
                }
                FieldType::Table(item) => {
                    let table = &self.schema.definition(item).name;
                    writer.line(format!(
                        "writer.setTable({index}, alignment: {table}Fields.alignment)"
                    ));
                    writer.line(format!("self.{name}.write(to: &writer)"));
                }
                FieldType::Vector(_) => {
                    // Named after the field's index, since a table may hold
                    // several vectors
                    writer.line(format!(
                        "let vector{index} = writer.setVector({index}, count: self.{name}.count)"
                    ));
                    writer.open(format!("for (index, item) in self.{name}.enumerated() {{"));
                    writer.line("item.write(to: &writer)");
                    writer.line(format!("writer.setItemEnd(vector{index}, item: index)"));
                    writer.close("}");
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn structure(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;
        let layout = definition.struct_layout();

        swift_doc(writer, &definition.docs);
        writer.line("@frozen");
        writer.open(format!("public struct {name}: Hashable, Sendable {{"));
        swift_doc(writer, &[format!("The byte length of {}", article(name))]);
        writer.line(format!("public static let size = {}", layout.size));
        swift_doc(
            writer,
            &[format!(
                "The alignment of an array of `{name}`s: its largest scalar's size"
            )],
        );
        writer.line(format!(
            "public static let alignment: Alignment = {}",
            alignment(layout.alignment)
        ));
        writer.blank();
        self.properties(writer, &definition.fields);
        writer.blank();
        self.initializer(writer, &definition.fields);
        writer.blank();

        swift_doc(
            writer,
            &[
                format!("Reads {} from the start of `bytes`.", article(name)),
                String::new(),
                "Throws a `DecodeError` if `bytes` are shorter than `size`.".to_owned(),
            ],
        );
        writer.line("@inlinable");
        writer.open("public init(_ bytes: RawSpan) throws(DecodeError) {");
        writer.line("self.init(try TableReader(bytes).struct(at: 0, size: Self.size), at: 0)");
        writer.close("}");
        writer.blank();

        swift_doc(
            writer,
            &[format!(
                "Reads {} at `offset` in `bytes`, which hold its scalars there",
                article(name)
            )],
        );
        writer.line("@inlinable");
        writer.open("init(_ bytes: RawSpan, at offset: Int) {");
        let mut offset = 0;
        for field in &definition.fields {
            let field_name = identifier(&field.name);
            let at = if offset == 0 {
                "offset".to_owned()
            } else {
                format!("offset + {offset}")
            };
            let (value, size) = match field.field_type {
                FieldType::Scalar(Scalar::Bool) => {
                    (format!("UInt8(littleEndianBytes: bytes, at: {at}) != 0"), 1)
                }
                FieldType::Scalar(scalar) => {
                    let integer =
                        format!("{}(littleEndianBytes: bytes, at: {at})", swift_type(scalar));
                    (
                        self.scalar_value(field, integer),
                        usize::from(scalar.size()),
                    )
                }
                FieldType::Struct(index) => {
                    let inner = self.schema.definition(index);
                    (
                        format!("{}(bytes, at: {at})", inner.name),
                        inner.struct_layout().size,
                    )
                }
                _ => unreachable!("structs hold scalars and structs only"),
            };
            writer.line(format!("self.{field_name} = {value}"));
            offset += size;
        }
        writer.close("}");
        writer.blank();

        swift_doc(
            writer,
            &[format!(
                "The bytes of {}: its scalars, little-endian, one after another",
                article(name)
            )],
        );
        writer.open("public func serialize() -> [UInt8] {");
        writer.line("var bytes: [UInt8] = []");
        writer.line("bytes.reserveCapacity(Self.size)");
        writer.line("self.append(to: &bytes)");
        writer.line("return bytes");
        writer.close("}");
        writer.blank();

        swift_doc(
            writer,
            &[format!(
                "The bytes of an array of `{name}`s, as a table holds them: theirs, one after another"
            )],
        );
        writer.open("public static func serializeArray(_ items: [Self]) -> [UInt8] {");
        writer.line("var bytes: [UInt8] = []");
        writer.line("bytes.reserveCapacity(items.count * Self.size)");
        writer.open("for item in items {");
        writer.line("item.append(to: &bytes)");
        writer.close("}");
        writer.line("return bytes");
        writer.close("}");
        writer.blank();

        swift_doc(writer, &["Appends the scalars to `bytes`, little-endian"]);
        writer.open("func append(to bytes: inout [UInt8]) {");
        for field in &definition.fields {
            let field_name = identifier(&field.name);
            let integer = scalar_integer(field);
            writer.line(match field.field_type {
                FieldType::Scalar(Scalar::Bool) => {
                    format!("bytes.append(self.{field_name} ? 1 : 0)")
                }
                FieldType::Scalar(Scalar::Uint8) => format!("bytes.append({integer})"),
                FieldType::Scalar(Scalar::Int8) => {
                    format!("bytes.append(UInt8(bitPattern: {integer}))")
                }
                FieldType::Scalar(Scalar::Float32 | Scalar::Float64) => format!(
                    "withUnsafeBytes(of: self.{field_name}.bitPattern.littleEndian) {{ bytes.append(contentsOf: $0) }}"
                ),
                FieldType::Scalar(_) => format!(
                    "withUnsafeBytes(of: {integer}.littleEndian) {{ bytes.append(contentsOf: $0) }}"
                ),
                FieldType::Struct(_) => format!("self.{field_name}.append(to: &bytes)"),
                _ => unreachable!("structs hold scalars and structs only"),
            });
        }
        writer.close("}");
        writer.blank();

        swift_doc(
            writer,
            &[
                "Writes the scalars where `writer` is, little-endian, after the `setStruct(_:)` or the",
                "`setStructArray(_:count:alignment:)` of the field that holds them",
            ],
        );
        writer.open("public func write(to writer: inout TableWriter) {");
        for field in &definition.fields {
            let field_name = identifier(&field.name);
            writer.line(match field.field_type {
                FieldType::Scalar(Scalar::Bool) => {
                    format!("writer.writeScalar(UInt8(self.{field_name} ? 1 : 0))")
                }
                FieldType::Scalar(_) => format!("writer.writeScalar({})", scalar_integer(field)),
                FieldType::Struct(_) => format!("self.{field_name}.write(to: &writer)"),
                _ => unreachable!("structs hold scalars and structs only"),
            });
        }
        writer.close("}");
        writer.close("}");
    }

    /// The type of a field in a fields struct or a struct
    fn input_type(&self, field: &Field) -> String {
        match field.field_type {
            FieldType::StringVector => {
                unreachable!("compile() rejects arrays of strings for this target")
            }
            FieldType::Scalar(scalar) => self.scalar_type(field, scalar).to_owned(),
            FieldType::Array(scalar) => format!("[{}]", swift_type(scalar)),
            FieldType::String => "String".to_owned(),
            FieldType::Struct(index) => self.schema.definition(index).name.clone(),
            FieldType::StructArray(index) => format!("[{}]", self.schema.definition(index).name),
            FieldType::Table(index) => format!("{}Fields", self.schema.definition(index).name),
            FieldType::Vector(index) => format!("[{}Fields]", self.schema.definition(index).name),
        }
    }

    /// The type of a scalar field: its enum, or the scalar's own
    fn scalar_type(&self, field: &Field, scalar: Scalar) -> &str {
        match field.enumeration {
            Some(index) => &self.schema.enumeration(index).name,
            None => swift_type(scalar),
        }
    }

    /// The value of a scalar field from `integer`, the expression that reads
    /// its integer: the value of its enum, which wraps any integer, or the
    /// integer itself
    fn scalar_value(&self, field: &Field, integer: String) -> String {
        match field.enumeration {
            Some(index) => format!(
                "{}(rawValue: {integer})",
                self.schema.enumeration(index).name
            ),
            None => integer,
        }
    }
}

/// The integer of a scalar field of `self`, which the runtime writes: an
/// enum's `rawValue`, or the scalar itself
fn scalar_integer(field: &Field) -> String {
    let name = identifier(&field.name);
    if field.enumeration.is_some() {
        format!("self.{name}.rawValue")
    } else {
        format!("self.{name}")
    }
}

/// An enum: a struct of its integer, which it holds as `rawValue`, with a
/// static property of each value that it names, by its name in `camelCase`,
/// escaped as a field's is. The initializer wraps any integer, without
/// failing, since a field of the enum holds any integer of its type, and the
/// properties are inlinable, so that each is a constant where it's named.
fn enumeration_struct(writer: &mut CodeWriter, enumeration: &Enum) {
    let name = &enumeration.name;
    let integer = swift_type(enumeration.underlying);

    swift_doc(writer, &enumeration.docs);
    writer.line("@frozen");
    writer.open(format!(
        "public struct {name}: RawRepresentable, Hashable, Sendable {{"
    ));
    swift_doc(writer, &["The integer that stores the value"]);
    writer.line(format!("public let rawValue: {integer}"));
    writer.blank();

    swift_doc(
        writer,
        &[format!(
            "The value of `rawValue`, which needn't be one that `{name}` names, since a field of it holds any `{integer}`"
        )],
    );
    writer.line("@inlinable");
    writer.open(format!("public init(rawValue: {integer}) {{"));
    writer.line("self.rawValue = rawValue");
    writer.close("}");

    for value in &enumeration.values {
        let member = identifier(&value.name);
        let constant = format!("Self(rawValue: {})", value.value);

        writer.blank();
        swift_doc(writer, &value.docs);
        writer.line("@inlinable");
        let property = format!("public static var {member}: Self {{ {constant} }}");
        // On a line of its own where it's too long for the property's
        if writer.fits(&STYLE, &property) {
            writer.line(property);
        } else {
            writer.open(format!("public static var {member}: Self {{"));
            writer.line(constant);
            writer.close("}");
        }
    }
    writer.close("}");
}

/// The view of a vector of a table, which reads each item in place when it's
/// asked for
fn vector_view(writer: &mut CodeWriter, definition: &Definition) {
    let name = &definition.name;

    swift_doc(
        writer,
        &[format!(
            "A vector of `{name}`s, which reads each item when it's asked for"
        )],
    );
    writer.line("@frozen");
    writer.open(format!("public struct {name}Vector: ~Escapable {{"));
    writer.line("@usableFromInline");
    writer.line("let vector: Vector");
    writer.blank();
    writer.line("@inlinable");
    writer.line("@_lifetime(copy vector)");
    writer.open("init(_ vector: Vector) {");
    writer.line("self.vector = vector");
    writer.close("}");
    writer.blank();
    swift_doc(writer, &["The number of items"]);
    writer.line("@inlinable");
    writer.line("public var count: Int { vector.count }");
    writer.blank();
    writer.line("@inlinable");
    writer.line("public var isEmpty: Bool { vector.isEmpty }");
    writer.blank();
    swift_doc(writer, &["The indices of the items, from 0 to `count`"]);
    writer.line("@inlinable");
    writer.line("public var indices: Swift.Range<Int> { vector.indices }");
    writer.blank();
    swift_doc(
        writer,
        &[
            "Reads item `index`, without reading the others. `index` must be one of `indices`: an index",
            "outside them is a programming error, which stops the program, as `Array`'s subscript does.",
            "",
            "Throws a `DecodeError` if the item lies outside the bytes.",
        ],
    );
    writer.line("@inlinable");
    writer.open(format!("public subscript(index: Int) -> {name} {{"));
    writer.line("@_lifetime(copy self)");
    writer.open("get throws(DecodeError) {");
    // In place, from the item's bytes and those after it in its parent's
    writer.line(format!(
        "try {name}(reader: TableReader(vector.itemInPlace(at: index)), position: 0)"
    ));
    writer.close("}");
    writer.close("}");
    writer.close("}");
}

/// The view of an array of a struct, which reads each struct when it's asked
/// for, and an array's initializer that reads them all
fn struct_array_view(writer: &mut CodeWriter, definition: &Definition) {
    let name = &definition.name;

    swift_doc(
        writer,
        &[format!(
            "An array of `{name}`s, which reads each struct when it's asked for"
        )],
    );
    writer.line("@frozen");
    writer.open(format!("public struct {name}Array: ~Escapable {{"));
    swift_doc(writer, &["The structs' bytes, one after another"]);
    writer.line("public let bytes: RawSpan");
    writer.blank();
    writer.line("@inlinable");
    writer.line("@_lifetime(copy bytes)");
    writer.open("init(_ bytes: RawSpan) {");
    writer.line("self.bytes = bytes");
    writer.close("}");
    writer.blank();
    swift_doc(writer, &["The number of structs"]);
    writer.line("@inlinable");
    writer.line(format!(
        "public var count: Int {{ bytes.byteCount / {name}.size }}"
    ));
    writer.blank();
    writer.line("@inlinable");
    writer.line("public var isEmpty: Bool { bytes.isEmpty }");
    writer.blank();
    swift_doc(writer, &["The indices of the structs, from 0 to `count`"]);
    writer.line("@inlinable");
    writer.line("public var indices: Swift.Range<Int> { 0..<count }");
    writer.blank();
    swift_doc(
        writer,
        &[
            "Reads struct `index`, which must be one of `indices`: an index outside them is a programming",
            "error, which stops the program, as `Array`'s subscript does.",
        ],
    );
    writer.line("@inlinable");
    writer.open(format!("public subscript(index: Int) -> {name} {{"));
    writer.line("precondition(UInt(bitPattern: index) < UInt(count), \"Index out of range\")");
    writer.blank();
    // The initializer reads the struct's own bytes, which the precondition
    // keeps within the array's, so that its loads' checks compare their
    // offsets with the struct's size, a constant, and fold away. Checked
    // against the array's length, they stay in a loop over the indices for
    // every struct wherever the compiler can't see where the bytes came
    // from, such as in a function that takes the view, and keep the loop
    // from being vectorized
    writer.line("// The precondition keeps the struct within the bytes");
    writer.line(format!("let offset = index &* {name}.size"));
    writer.line(format!(
        "return {name}(bytes.extracting(unchecked: offset..<offset &+ {name}.size), at: 0)"
    ));
    writer.close("}");
    writer.close("}");
    writer.blank();

    // `Element` is the array's own here, which the checker keeps a struct
    // from being named
    writer.open(format!("extension Array where Element == {name} {{"));
    swift_doc(
        writer,
        &["Reads every struct of an array read from a table, into an array of their own"],
    );
    writer.line("@inlinable");
    writer.open(format!("public init(_ view: borrowing {name}Array) {{"));
    writer.line("self = view.indices.map { view[$0] }");
    writer.close("}");
    writer.close("}");
}

/// A field's or an enum value's name in `camelCase`, with a `_` after it if
/// it would clash with a member of the generated code, or in backticks if
/// it's a keyword. A value takes the name that a field of its name would, so
/// `self` takes a `_`, which also keeps `Level.self` the type's, and
/// `serialize` does too, though an enum has no `serialize()`. Swift reads
/// other keywords in backticks as static members, `init` among them.
fn identifier(name: &str) -> String {
    let name = camel_case(name);
    if MEMBERS.contains(&name.as_str()) {
        format!("{name}_")
    } else if KEYWORDS.contains(&name.as_str()) {
        format!("`{name}`")
    } else {
        name
    }
}

/// A declaration's name in code, after its indefinite article
fn article(name: &str) -> String {
    format!("{} `{name}`", indefinite_article(name))
}

/// Documentation comments of `lines`, Markdown as the schema's are
fn swift_doc(writer: &mut CodeWriter, lines: &[impl AsRef<str>]) {
    for line in lines {
        let line = line.as_ref();
        if line.is_empty() {
            writer.line("///");
        } else {
            writer.line(format!("/// {line}"));
        }
    }
}

/// The layout of a message, with the arguments that differ from
/// `TableLayout`'s defaults, whose version is the type's `formatVersion`
fn layout_expression(layout: Layout) -> String {
    let mut arguments = Vec::new();
    if layout.sized {
        arguments.push("sized: true");
    }
    if layout.version.is_some() {
        arguments.push("version: formatVersion");
    }
    match layout.offset_type {
        OffsetType::Uint8 => arguments.push("offsetType: .uint8"),
        OffsetType::Uint16 => arguments.push("offsetType: .uint16"),
        OffsetType::Uint32 => {}
    }

    if arguments.is_empty() {
        "TableLayout.default".to_owned()
    } else {
        format!("TableLayout({})", arguments.join(", "))
    }
}

/// The runtime's `Alignment` of `bytes`
fn alignment(bytes: u8) -> &'static str {
    match bytes {
        1 => ".one",
        2 => ".two",
        4 => ".four",
        8 => ".eight",
        _ => unreachable!("alignments are 1, 2, 4 or 8 bytes"),
    }
}

/// The Swift type of a scalar
const fn swift_type(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::Bool => "Bool",
        Scalar::Int8 => "Int8",
        Scalar::Uint8 => "UInt8",
        Scalar::Int16 => "Int16",
        Scalar::Uint16 => "UInt16",
        Scalar::Int32 => "Int32",
        Scalar::Uint32 => "UInt32",
        Scalar::Int64 => "Int64",
        Scalar::Uint64 => "UInt64",
        Scalar::Float32 => "Float",
        Scalar::Float64 => "Double",
    }
}

/// The scalar's name in the runtime's adders and setters, such as `UInt8`
/// in `addUInt8()` and `Float32` in `setFloat32Array(_:_:)`
const fn method(scalar: Scalar) -> &'static str {
    match scalar {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identifiers() {
        assert_eq!(identifier("value"), "value");
        assert_eq!(identifier("max_value"), "maxValue");
        assert_eq!(identifier("class"), "`class`");
        assert_eq!(identifier("in"), "`in`");
        assert_eq!(identifier("self"), "self_");
        assert_eq!(identifier("reader"), "reader");
        assert_eq!(identifier("position"), "position");
        assert_eq!(identifier("serialize"), "serialize_");
        // Keywords only where the grammar expects them
        assert_eq!(identifier("get"), "get");
        assert_eq!(identifier("type"), "type");
    }

    /// A getter of an enum, or a value of one, too long for its line puts
    /// its expression on a line of its own, as swift-format breaks it
    #[test]
    fn test_long_lines_break_within_their_braces() {
        let value = "a_value_whose_name_takes_a_line_of_its_own_beyond_the_others_values";
        let source = format!(
            "message Root {{ nested: Nested; }}\n\
             table Nested {{ priority: Priority; }}\n\
             enum Priority: uint64 {{ {value} = 18446744073709551615; }}\n"
        );
        let schema = crate::parse(&source).unwrap();
        let code = &generate(&schema, "long", "header", &Options::default())[0].contents;

        let getter = "\
        get throws(DecodeError) {
            try Priority(rawValue: _reader.uint64(at: _reader.fieldOffset(0, inTableAt: _position)))
        }
";
        assert!(code.contains(getter), "{code}");
        let constant = format!(
            "    public static var {}: Self {{\n        Self(rawValue: 18446744073709551615)\n    }}\n",
            camel_case(value)
        );
        assert!(code.contains(&constant), "{code}");
    }

    #[test]
    fn test_keywords_are_sorted_and_distinct() {
        assert!(KEYWORDS.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(MEMBERS.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(MEMBERS.iter().all(|member| !KEYWORDS.contains(member)));
    }

    #[test]
    fn test_layouts() {
        let layout = |sized, version, offset_type| Layout {
            sized,
            version,
            offset_type,
        };
        assert_eq!(
            layout_expression(layout(false, None, OffsetType::Uint32)),
            "TableLayout.default"
        );
        assert_eq!(
            layout_expression(layout(true, Some(1), OffsetType::Uint8)),
            "TableLayout(sized: true, version: formatVersion, offsetType: .uint8)"
        );
    }
}
