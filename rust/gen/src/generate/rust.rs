//! Rust for `tafla`. Messages and tables are `Copy` views that read fields
//! lazily, borrowing strings and arrays from the bytes, and vectors and
//! arrays of structs as views that read each item when it's asked for;
//! they're written from structs of their fields, which a `TableSizer` sizes
//! and a `TableWriter` writes, nested tables in place. Structs are plain
//! structs read and written at fixed offsets. An enum is a `Copy` struct of
//! its integer, with a constant of each value that the schema names, so that
//! a field of it holds any integer of its type; an array of an enum is an
//! array of its integers.

use std::collections::HashSet;
use std::fmt::Display;

use super::fixed::{self, FixedSizes, NESTED_OFFSET_SIZE};
use super::prose::indefinite_article;
use super::writer::{CodeWriter, Expr, Style, call, text};
use super::{GeneratedFile, Options};
use crate::model::{
    Definition, Enum, Field, FieldType, Kind, Layout, OffsetType, Scalar, ScalarValue, Schema,
    strongly_connected_components,
};

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

/// The widest line of a `use` statement's list that rustfmt fills when the
/// statement doesn't fit on one line, a column less than its `max_width`
const USE_LIST_LINE_WIDTH: usize = 99;

/// Strict and reserved keywords, which names take as raw identifiers
const KEYWORDS: [&str; 48] = [
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn",
    "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
    "virtual", "where", "while", "yield",
];

/// The longest nested table that the generated code writes whole, as the
/// array of its bytes, which is as long as the longest struct: 1,024
/// scalars of 8 bytes. Longer ones are written field by field, so that no
/// array of their bytes is copied on the stack.
const MAX_LAID_OUT: usize = 8192;

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
    let definitions = schema.definitions.iter();
    let enums = schema.enums.iter();
    let generator = Generator {
        schema,
        fixed: FixedSizes::new(schema, MAX_LAID_OUT),
        borrows: borrowing(schema),
        vector_components: strongly_connected_components(&schema.definitions, vectors),
        declared: definitions
            .map(|definition| definition.name.as_str())
            .chain(enums.map(|enumeration| enumeration.name.as_str()))
            .collect(),
    };

    let mut writer = CodeWriter::new(STYLE.indent);
    writer.line(format!("// {header}"));
    writer.blank();
    generator.imports(&mut writer, runtime);

    for enumeration in &schema.enums {
        writer.blank();
        enum_struct(&mut writer, enumeration);
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
            generator.reference_conversion(&mut writer, index);
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
    /// The byte length of each nested table whose fields' sizes the schema
    /// fixes, which the generated code writes whole
    fixed: FixedSizes,
    /// Whether each definition's fields struct borrows strings or arrays, by
    /// its index
    borrows: Vec<bool>,
    /// The component of each definition among those that hold each other in
    /// vectors, by its index
    vector_components: Vec<usize>,
    /// The names that the schema declares, which type parameters don't take
    declared: HashSet<&'a str>,
}

/// A vector or an array of structs in a fields struct, which takes items of
/// any type whose reference converts into its item
struct GenericField {
    /// The type of its items
    parameter: String,
    /// What they convert into: the struct, or the table's fields struct
    item: String,
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
        // Whole messages are written without a sizer, into a writer's bytes
        let whole = |definition: &Definition| match definition.kind {
            Kind::Message(layout) => self.whole_message(definition, layout).is_some(),
            Kind::Table | Kind::Struct(_) => false,
        };
        let has_whole = definitions.iter().any(whole);
        let has_sized = definitions
            .iter()
            .any(|definition| !definition.is_struct() && !whole(definition));
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
        names.push("DecodeError");
        if has_offset_type {
            names.push("OffsetType");
        }
        // Messages make sizers, or the bytes of a whole one, and tables take
        // sizers and writers
        if has_tables {
            names.push("TableLayout");
            names.push("TableReader");
        }
        if has_sized {
            names.push("TableSizer");
        }
        if has_nested || has_whole {
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

        // rustfmt fills the lines of a list that doesn't fit
        writer.open(format!("use {runtime}::{{"));
        let mut line = String::new();
        for name in names {
            let width = STYLE.indent.len() + line.len() + 1 + name.len() + 1;
            if !line.is_empty() && width > USE_LIST_LINE_WIDTH {
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
        let holds_version = holds_version(definition);

        rust_doc(writer, &definition.docs);
        writer.line("#[derive(Clone, Copy, Debug)]");
        writer.open(format!("pub struct {name}<'a> {{"));
        writer.line("bytes: &'a [u8],");
        if holds_version {
            rust_doc(
                writer,
                &[
                    "The version that [`Self::new`] read, which tells the fields it holds"
                        .to_owned(),
                ],
            );
            writer.line("version: u32,");
        }
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
        if let Some(since) = layout.since {
            rust_doc(
                writer,
                &[
                    "The first version of the format that [`Self::new`] reads, whose tables lack"
                        .to_owned(),
                    "the fields that later versions added".to_owned(),
                ],
            );
            writer.line(format!(
                "pub const FIRST_VERSION: u32 = {};",
                integer(&since)
            ));
            writer.blank();
        }
        layout_constant(writer, layout, generics);
        writer.blank();

        if versioned {
            constructor(writer, name, layout, holds_version);
            writer.blank();

            let mut docs = vec![
                "Returns the format version in the header of `bytes`, without checking".to_owned(),
                "it, so that an application can choose the reader of that version".to_owned(),
            ];
            docs.extend(errors_doc("if the header doesn't fit in `bytes`"));
            rust_doc(writer, &docs);
            writer.open("pub fn version_of(bytes: &[u8]) -> Result<u32, DecodeError> {");
            writer.line("TableReader::new(bytes, Self::LAYOUT).version()");
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

        rust_doc(writer, &definition.docs);
        writer.line("#[derive(Clone, Copy, Debug, PartialEq)]");
        writer.open(format!("pub struct {name} {{"));
        for field in &definition.fields {
            rust_doc(writer, &field.docs);
            writer.line(format!(
                "pub {}: {},",
                identifier(&field.name),
                self.input_type(field)
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
        let mut start = 0;
        for field in &definition.fields {
            let value = format!("self.{}", identifier(&field.name));
            let (end, bytes) = match field.field_type {
                FieldType::Scalar(Scalar::Bool) => {
                    writer.line(format!("bytes[{}] = u8::from({value});", integer(&start)));
                    start += 1;
                    continue;
                }
                FieldType::Scalar(scalar) => (
                    start + usize::from(scalar.size()),
                    format!("{}.to_le_bytes()", stored(field, value)),
                ),
                FieldType::Struct(index) => (
                    start + self.schema.definition(index).struct_layout().size,
                    format!("{value}.to_bytes()"),
                ),
                _ => unreachable!("structs hold scalars and structs only"),
            };
            writer.line(format!(
                "bytes[{}..{}].copy_from_slice(&{bytes});",
                integer(&start),
                integer(&end)
            ));
            start = end;
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
                    let read = format!("{}::from_le_bytes", scalar_type(scalar));
                    let bytes = format!(
                        "bytes[{}..{}].try_into().unwrap_or_default()",
                        integer(&start),
                        integer(&end)
                    );
                    match self.scalar_enum(field) {
                        Some(enumeration) => {
                            enum_of_bytes(writer, &name, &enumeration.name, &read, &bytes);
                        }
                        None => writer.line(format!("{name}: {read}({bytes}),")),
                    }
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
    /// that write them. Its vectors and arrays of structs take items of any
    /// type whose reference converts into their item, each through a type
    /// parameter that defaults to the item, so that a caller writes the
    /// items it holds without collecting them into the item's type first.
    fn fields_struct(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
        let name = &definition.name;
        let borrows = self.borrows[index];
        let generic = self.generic_fields(index);
        let parameters: Vec<&GenericField> = generic.iter().flatten().collect();

        rust_doc(
            writer,
            &[format!("The fields of {}, to write", article(name))],
        );
        writer.line("#[derive(Clone, Debug)]");
        if parameters.is_empty() {
            let generics = if borrows { "<'a>" } else { "" };
            writer.open(format!("pub struct {name}Fields{generics} {{"));
        } else {
            let mut list = vec!["'a".to_owned()];
            list.extend(
                parameters
                    .iter()
                    .map(|field| format!("{} = {}", field.parameter, field.item)),
            );
            generic_list(writer, &format!("pub struct {name}Fields"), &list, " {");
            writer.indent();
        }
        for (field, generic) in definition.fields.iter().zip(&generic) {
            let (note, field_type) = match generic {
                Some(generic) => (
                    Some(self.generic_doc(field.field_type, generic)),
                    format!("&'a [{}]", generic.parameter),
                ),
                None => (self.integers_doc(field), self.input_type(field)),
            };
            rust_doc(writer, &field.docs);
            if let Some(note) = note {
                if !field.docs.is_empty() {
                    rust_doc(writer, &[String::new()]);
                }
                rust_doc(writer, &[note]);
            }
            // `None` leaves an optional field out
            let field_type = if field.optional {
                format!("Option<{field_type}>")
            } else {
                field_type
            };
            writer.line(format!("pub {}: {field_type},", identifier(&field.name)));
        }
        writer.close("}");
        writer.blank();

        if parameters.is_empty() {
            if borrows {
                writer.open(format!("impl {name}Fields<'_> {{"));
            } else {
                writer.open(format!("impl {name}Fields {{"));
            }
            match definition.kind {
                Kind::Message(layout) => self.serialize(writer, definition, layout, &generic),
                Kind::Table => {
                    self.alignment(writer, index);
                    writer.blank();
                    self.size_and_write(writer, index, &generic);
                }
                Kind::Struct(_) => unreachable!("structs are written by their own methods"),
            }
            writer.close("}");
            return;
        }

        // The alignment doesn't depend on the items' types, so it's the
        // default's, which `<Name>Fields::ALIGNMENT` finds without them
        if definition.kind == Kind::Table {
            writer.open(format!("impl {name}Fields<'_> {{"));
            self.alignment(writer, index);
            writer.close("}");
            writer.blank();
        }

        let mut names = vec!["'a".to_owned()];
        names.extend(parameters.iter().map(|field| field.parameter.clone()));
        let arguments = names.join(", ");
        generic_list(
            writer,
            "impl",
            &names,
            &format!(" {name}Fields<{arguments}>"),
        );
        writer.open("where");
        for field in &parameters {
            writer.line(format!("&'a {}: Into<{}>,", field.parameter, field.item));
        }
        writer.close("{");
        writer.indent();
        match definition.kind {
            Kind::Message(layout) => self.serialize(writer, definition, layout, &generic),
            Kind::Table => self.size_and_write(writer, index, &generic),
            Kind::Struct(_) => unreachable!("structs are written by their own methods"),
        }
        writer.close("}");
    }

    /// Which of a definition's fields take items of any type, by their
    /// position: every array of structs, and every vector but those whose
    /// table can hold the definition again through vectors, such as a tree's
    /// children, since their default type would name itself
    fn generic_fields(&self, index: usize) -> Vec<Option<GenericField>> {
        let mut names = ["T", "U", "V", "W"]
            .into_iter()
            .map(str::to_owned)
            .chain((5_usize..).map(|number| format!("T{number}")))
            .filter(|name| !self.declared.contains(name.as_str()));

        self.schema
            .definition(index)
            .fields
            .iter()
            .map(|field| {
                let item = match field.field_type {
                    FieldType::StructArray(structure) => {
                        self.schema.definition(structure).name.clone()
                    }
                    FieldType::Vector(table)
                        if self.vector_components[table] != self.vector_components[index] =>
                    {
                        self.fields_type(table, "'a")
                    }
                    _ => return None,
                };
                let parameter = names.next().expect("the names never run out");
                Some(GenericField { parameter, item })
            })
            .collect()
    }

    /// The documentation of a field that takes items of any type
    fn generic_doc(&self, field_type: FieldType, generic: &GenericField) -> String {
        let parameter = &generic.parameter;
        match field_type {
            FieldType::StructArray(index) => {
                let structure = &self.schema.definition(index).name;
                format!(
                    "[`{structure}`]s, or items of any type `{parameter}` whose reference \
                     converts into {}",
                    article(structure)
                )
            }
            FieldType::Vector(index) => {
                let table = &self.schema.definition(index).name;
                format!(
                    "[`{table}Fields`], or items of any type `{parameter}` whose reference \
                     converts into them"
                )
            }
            _ => unreachable!("only vectors and arrays of structs take items of any type"),
        }
    }

    /// The conversion of a reference to a struct or a table's fields into
    /// itself, with which a vector or an array of structs holds the default
    /// type of its items
    fn reference_conversion(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
        let (item, value) = if definition.is_struct() {
            (definition.name.clone(), "*value")
        } else {
            (self.fields_type(index, "'_"), "value.clone()")
        };

        writer.open(format!("impl From<&Self> for {item} {{"));
        writer.open("fn from(value: &Self) -> Self {");
        writer.line(value);
        writer.close("}");
        writer.close("}");
    }

    /// The name of the fields struct of the table at `index`, with `lifetime`
    /// if it borrows
    fn fields_type(&self, index: usize, lifetime: &str) -> String {
        let name = &self.schema.definition(index).name;
        if self.borrows[index] {
            format!("{name}Fields<{lifetime}>")
        } else {
            format!("{name}Fields")
        }
    }

    /// A message's `serialize()`, which sizes its fields and then writes them
    /// into bytes of that length
    fn serialize(
        &self,
        writer: &mut CodeWriter,
        definition: &Definition,
        layout: Layout,
        generic: &[Option<GenericField>],
    ) {
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
        if let Some(plan) = self.whole_message(definition, layout) {
            serialize_whole(writer, definition, layout, &plan);
            return;
        }

        writer.open("pub fn serialize(&self) -> Result<Vec<u8>, BuildError> {");
        let fields = text(integer(&definition.fields.len()));
        let layout_constant = text(format!("{name}::LAYOUT"));
        let sizer = call("TableSizer::new", vec![fields, layout_constant]);
        writer.statement(&STYLE, "let mut sizer = ", &sizer, "?;");
        self.sizes(writer, definition, generic, "&mut sizer");
        writer.blank();
        writer.line("let mut writer = sizer.writer()?;");
        self.writes(writer, definition, generic, "&mut writer");
        writer.blank();
        writer.line("writer.into_bytes()");
        writer.close("}");
    }

    /// How the generated code writes a message whole, at constant positions,
    /// when its fields' sizes are all fixed, but for a string after them,
    /// and they fit in its offset type
    fn whole_message(&self, definition: &Definition, layout: Layout) -> Option<WholeMessage> {
        // A field that may be left out has no position of its own
        if definition.fields.iter().any(|field| field.optional) {
            return None;
        }

        let width = fixed::offset_size(layout.offset_type);
        let header = (usize::from(layout.sized) + usize::from(layout.version.is_some())) * width;

        let (string, fixed) = match definition.fields.split_last() {
            Some((last, fixed)) if last.field_type == FieldType::String => (true, fixed),
            _ => (false, definition.fields.as_slice()),
        };
        let mut sizes = Vec::with_capacity(fixed.len());
        for field in fixed {
            sizes.push(self.fixed.field(self.schema, field.field_type)?);
        }

        // The offsets, the fields and the string's length, after the header
        let offsets = definition.fields.len() * width;
        let fields: usize = sizes.iter().sum();
        let length = offsets + fields + if string { width } else { 0 };
        let max = usize::try_from(layout.offset_type.max_value()).unwrap_or(usize::MAX);
        if length > MAX_LAID_OUT || header + length > max {
            return None;
        }

        Some(WholeMessage {
            header,
            width,
            sizes,
            string,
            length,
        })
    }

    /// The alignment of a nested table, which its parent pads it to
    fn alignment(&self, writer: &mut CodeWriter, index: usize) {
        let name = &self.schema.definition(index).name;
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
    }

    /// The methods with which a nested table's parent sizes and writes it
    fn size_and_write(
        &self,
        writer: &mut CodeWriter,
        index: usize,
        generic: &[Option<GenericField>],
    ) {
        let definition = self.schema.definition(index);
        if let Some(size) = self.fixed.table(index) {
            self.whole_table(writer, definition, size);
            return;
        }

        let name = &definition.name;
        let fields = integer(&definition.fields.len());
        // The length of a table of scalars and structs doesn't depend on
        // their values
        let sized_by_values = definition.fields.iter().any(|field| {
            field.optional
                || !matches!(
                    field.field_type,
                    FieldType::Scalar(_) | FieldType::Struct(_)
                )
        });

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
        self.sizes(writer, definition, generic, "sizer");
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
        self.writes(writer, definition, generic, "writer");
        writer.line("writer.end_table(parent);");
        writer.close("}");
    }

    /// The methods of a nested table whose fields' sizes the schema fixes,
    /// which its parent writes whole, as the `size` bytes of its offsets and
    /// its fields that `to_bytes()` returns
    fn whole_table(&self, writer: &mut CodeWriter, definition: &Definition, size: usize) {
        let name = &definition.name;

        rust_doc(
            writer,
            &[
                format!(
                    "The byte length of {}: its offsets and its fields, whose sizes the",
                    article(name)
                ),
                "schema fixes, so that its parent writes it whole".to_owned(),
            ],
        );
        writer.line(format!("pub const SIZE: usize = {};", integer(&size)));
        writer.blank();

        rust_doc(
            writer,
            &[format!(
                "Adds {} to its parent's sizer, after the field or vector that holds it",
                article(name)
            )],
        );
        writer.line("#[allow(clippy::unused_self)]");
        writer.open("pub fn size(&self, sizer: &mut TableSizer) {");
        writer.line("sizer.add_table_bytes(1, Self::SIZE);");
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
        writer.line("writer.copy_table(&self.to_bytes());");
        writer.close("}");
        writer.blank();

        self.table_to_bytes(writer, definition, size);
    }

    /// A whole table's `to_bytes()`, which writes its offsets, and then its
    /// fields at them, one after another, as a writer would
    fn table_to_bytes(&self, writer: &mut CodeWriter, definition: &Definition, size: usize) {
        let name = &definition.name;
        let size_of = |field: &Field| {
            self.fixed
                .field(self.schema, field.field_type)
                .expect("a whole table's fields have fixed sizes")
        };

        rust_doc(
            writer,
            &[
                format!(
                    "The bytes of {}, as its parent holds it: its offsets, and then its",
                    article(name)
                ),
                "fields at them".to_owned(),
            ],
        );
        writer.line("#[must_use]");
        writer.open(format!(
            "pub fn to_bytes(&self) -> [u8; {}] {{",
            integer(&size)
        ));
        writer.line("let mut bytes = [0; Self::SIZE];");

        let mut position = definition.fields.len() * NESTED_OFFSET_SIZE;
        let mut ranges = Vec::with_capacity(definition.fields.len());
        for (index, field) in definition.fields.iter().enumerate() {
            let slot = index * NESTED_OFFSET_SIZE;
            writer.line(format!(
                "bytes[{}..{}].copy_from_slice(&{}_u32.to_le_bytes());",
                integer(&slot),
                integer(&(slot + NESTED_OFFSET_SIZE)),
                integer(&position)
            ));
            ranges.push((field, position, position + size_of(field)));
            position += size_of(field);
        }

        for (field, start, end) in ranges {
            let value = format!("self.{}", identifier(&field.name));
            let bytes = match field.field_type {
                FieldType::Scalar(Scalar::Bool) => {
                    writer.line(format!("bytes[{}] = u8::from({value});", integer(&start)));
                    continue;
                }
                FieldType::Scalar(_) => format!("{}.to_le_bytes()", stored(field, value)),
                FieldType::Struct(_) | FieldType::Table(_) => format!("{value}.to_bytes()"),
                _ => unreachable!("whole tables hold scalars, structs and whole tables only"),
            };
            writer.line(format!(
                "bytes[{}..{}].copy_from_slice(&{bytes});",
                integer(&start),
                integer(&end)
            ));
        }
        writer.blank();
        writer.line("bytes");
        writer.close("}");
    }

    /// The calls that add a table's fields to its sizer, in order, with
    /// `sizer`, the expression of the sizer that nested tables take. The
    /// locals don't clash with fields, whose values are `self`'s, and an
    /// optional field's value is bound to `value` where the table holds it.
    fn sizes(
        &self,
        writer: &mut CodeWriter,
        definition: &Definition,
        generic: &[Option<GenericField>],
        sizer: &str,
    ) {
        for (field, generic) in definition.fields.iter().zip(generic) {
            let value = format!("self.{}", identifier(&field.name));
            if !field.optional {
                self.size(writer, field.field_type, generic.as_ref(), &value, sizer);
                continue;
            }

            // An absent field adds nothing: its offset is counted with the
            // table's others. A scalar's or a struct's size doesn't depend on
            // its value.
            if matches!(
                field.field_type,
                FieldType::Scalar(_) | FieldType::Struct(_)
            ) {
                writer.open(format!("if {value}.is_some() {{"));
            } else {
                writer.open(format!(
                    "if let Some(value) = {} {{",
                    option_of(field, &value)
                ));
            }
            self.size(writer, field.field_type, generic.as_ref(), "value", sizer);
            writer.close("}");
        }
    }

    /// The calls that add a field of `value` to its table's sizer
    fn size(
        &self,
        writer: &mut CodeWriter,
        field_type: FieldType,
        generic: Option<&GenericField>,
        value: &str,
        sizer: &str,
    ) {
        let (method, arguments) = match field_type {
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
                return;
            }
            FieldType::Vector(table) => {
                let count = text(format!("{value}.len()"));
                writer.statement(&STYLE, "", &call("sizer.add_vector", vec![count]), ";");
                // Items of the same length are added at once
                if self.fixed.table(table).is_some() {
                    let item = &self.schema.definition(table).name;
                    let arguments = vec![
                        text(format!("{value}.len()")),
                        text(format!("{item}Fields::SIZE")),
                    ];
                    writer.statement(&STYLE, "", &call("sizer.add_table_bytes", arguments), ";");
                    return;
                }
                if let Some(generic) = generic {
                    writer.open(format!("for item in {value} {{"));
                    writer.line(format!("let fields: {} = item.into();", generic.item));
                } else {
                    writer.open(format!("for fields in {value} {{"));
                }
                writer.line(format!("fields.size({sizer});"));
                writer.close("}");
                return;
            }
        };
        writer.statement(&STYLE, "", &call(format!("sizer.{method}"), arguments), ";");
    }

    /// The calls that write a table's fields, each with its index, in the
    /// order that `sizes()` added them, with `table_writer`, the expression of
    /// the writer that nested tables take. An optional field that the table
    /// doesn't hold is written absent.
    fn writes(
        &self,
        writer: &mut CodeWriter,
        definition: &Definition,
        generic: &[Option<GenericField>],
        table_writer: &str,
    ) {
        for ((index, field), generic) in definition.fields.iter().enumerate().zip(generic) {
            let value = format!("self.{}", identifier(&field.name));
            let generic = generic.as_ref();
            if !field.optional {
                self.write(writer, index, field, generic, &value, table_writer);
                continue;
            }

            writer.open(format!(
                "if let Some(value) = {} {{",
                option_of(field, &value)
            ));
            self.write(writer, index, field, generic, "value", table_writer);
            writer.close("} else {");
            writer.indent();
            writer.line(format!("writer.set_absent({});", integer(&index)));
            writer.close("}");
        }
    }

    /// The calls that write field `index` of `value`
    fn write(
        &self,
        writer: &mut CodeWriter,
        index: usize,
        field: &Field,
        generic: Option<&GenericField>,
        value: &str,
        table_writer: &str,
    ) {
        let position = text(integer(&index));
        let (method, arguments) = match field.field_type {
            FieldType::Scalar(Scalar::Bool) => (
                "set_u8".to_owned(),
                vec![position, text(format!("u8::from({value})"))],
            ),
            FieldType::Scalar(scalar) => (
                format!("set_{}", scalar_type(scalar)),
                vec![position, text(stored(field, value.to_owned()))],
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
            // The items and their conversion to bytes, which the writer
            // appends without collecting them first
            FieldType::StructArray(struct_index) => {
                let structure = &self.schema.definition(struct_index).name;
                (
                    "set_structs".to_owned(),
                    vec![
                        position,
                        text(value),
                        text(format!("|item| {structure}::to_bytes(item.into())")),
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
                return;
            }
            FieldType::Vector(_) => {
                let count = text(format!("{value}.len()"));
                writer.statement(
                    &STYLE,
                    "let vector = ",
                    &call("writer.set_vector", vec![position, count]),
                    ";",
                );
                if let Some(generic) = generic {
                    writer.open(format!(
                        "for (index, item) in {value}.iter().enumerate() {{"
                    ));
                    writer.line(format!("let fields: {} = item.into();", generic.item));
                    writer.line(format!("fields.write({table_writer});"));
                    writer.line("writer.set_item_end(vector, index);");
                } else {
                    writer.open(format!(
                        "for (item, fields) in {value}.iter().enumerate() {{"
                    ));
                    writer.line(format!("fields.write({table_writer});"));
                    writer.line("writer.set_item_end(vector, item);");
                }
                writer.close("}");
                return;
            }
        };
        writer.statement(
            &STYLE,
            "",
            &call(format!("writer.{method}"), arguments),
            ";",
        );
    }

    fn getters(&self, writer: &mut CodeWriter, definition: &Definition, is_root: bool) {
        // Each getter reads with a reader in the table's layout, a constant,
        // rather than one that the reader holds, so that its reads compile
        // for that layout's offset type wherever the reader is passed
        let reader = if is_root {
            "let reader = TableReader::new(self.bytes, Self::LAYOUT);"
        } else {
            "let reader = TableReader::new(self.bytes, TableLayout::DEFAULT);"
        };

        for (index, field) in definition.fields.iter().enumerate() {
            let offset = if is_root {
                format!("reader.root_offset({})?", integer(&index))
            } else {
                format!("reader.field_offset(self.position, {})?", integer(&index))
            };
            // A scalar that may be missing returns its default rather than
            // `None`, and `has_<field>()` tells whether the table holds it
            let default = match field.field_type {
                FieldType::Scalar(scalar) => field.default.map(|value| {
                    let literal = rust_literal(value, scalar);
                    match self.scalar_enum(field) {
                        Some(enumeration) => format!("{}({literal})", enumeration.name),
                        None => literal,
                    }
                }),
                _ => None,
            };
            let wrapped = field.may_be_missing() && default.is_none();

            let docs = self.getter_docs(field, default.as_deref());
            writer.blank();
            rust_doc(writer, &docs);
            let name = identifier(&field.name);
            let value = self.value_type(field);
            let result = if wrapped {
                format!("Result<Option<{value}>, DecodeError>")
            } else {
                format!("Result<{value}, DecodeError>")
            };
            open_method(writer, &format!("pub fn {name}"), &["&self"], &result);

            let checks = Checks::of(definition, index, field, is_root);
            let missing = format!("Ok({})", default.as_deref().unwrap_or("None"));
            checks.write(writer, reader, &missing);
            writer.line(format!("let position = {offset};"));
            if let Some(absent) = checks.absent {
                writer.open(format!("if position == {absent} {{"));
                writer.line(format!("return {missing};"));
                writer.close("}");
                writer.blank();
            }
            self.read(writer, field, wrapped);
            writer.close("}");

            if default.is_some() {
                presence_method(writer, &checks, field, reader, &offset);
            }
        }
    }

    /// The read of a field's value at `position`, as a getter's result, in
    /// `Some` when the getter is `wrapped` in an `Option`
    fn read(&self, writer: &mut CodeWriter, field: &Field, wrapped: bool) {
        // A result that may fail, and the parts around a value that can't
        let some = if wrapped { ".map(Some)" } else { "" };
        let (open, close) = if wrapped {
            ("Ok(Some(", "))")
        } else {
            ("Ok(", ")")
        };

        match field.field_type {
            FieldType::Scalar(Scalar::Bool) => writer.line("Ok(reader.get_u8(position)? != 0)"),
            FieldType::Scalar(scalar) => {
                let read = format!("get_{}(position)", scalar_type(scalar));
                match self.scalar_enum(field) {
                    Some(enumeration) => {
                        let wrap = format!("map({})", enumeration.name);
                        chain(writer, "reader", &[&read, &wrap]);
                    }
                    None => writer.line(format!("reader.{read}")),
                }
            }
            FieldType::Array(scalar) => {
                writer.line(format!(
                    "reader.get_{}_array(position){some}",
                    scalar_type(scalar)
                ));
            }
            FieldType::String => writer.line(format!("reader.get_str(position){some}")),
            FieldType::Table(table) => {
                let table = &self.schema.definition(table).name;
                let read = format!("{table}::new(self.bytes, position)");
                let argument = if wrapped {
                    format!("Some({read})")
                } else {
                    read.clone()
                };

                // rustfmt breaks a call wider than its `fn_call_width`
                // as `Ok`'s argument, with its arguments on a line each, or
                // on one line within `Some`
                if argument.len() <= FN_CALL_WIDTH {
                    writer.line(format!("{open}{read}{close}"));
                } else {
                    writer.line(format!("{open}{table}::new("));
                    writer.indent();
                    if wrapped {
                        writer.line("self.bytes, position,");
                    } else {
                        writer.line("self.bytes,");
                        writer.line("position,");
                    }
                    writer.dedent();
                    writer.line(format!("){close}"));
                }
            }
            FieldType::Struct(index) => {
                let structure = &self.schema.definition(index).name;
                writer.line(format!(
                    "let bytes = reader.get_struct(position, {structure}::SIZE)?;"
                ));
                writer.blank();
                writer.line(format!("{structure}::read(bytes){some}"));
            }
            FieldType::StructArray(index) => {
                let structure = &self.schema.definition(index).name;
                writer.statement(
                    &STYLE,
                    "let bytes = ",
                    &call(
                        "reader.get_struct_array",
                        vec![text("position"), text(format!("{structure}::SIZE"))],
                    ),
                    "?;",
                );
                // The bytes are a whole number of structs
                writer.line(format!(
                    "let (structs, _) = bytes.as_chunks::<{{ {structure}::SIZE }}>();"
                ));
                writer.blank();
                writer.line(format!("{open}{structure}Array {{ structs }}{close}"));
            }
            FieldType::Vector(table) => {
                let table = &self.schema.definition(table).name;
                writer.open(format!("{open}{table}Vector {{"));
                writer.line("vector: reader.get_vector(position)?,");
                writer.close(format!("}}{close}"));
            }
        }
    }

    /// The documentation of a field's getter: the field's own, the `default`
    /// of a scalar that may be missing, and for a vector or an array of
    /// structs, the view that it returns, and then its errors
    fn getter_docs(&self, field: &Field, default: Option<&str>) -> Vec<String> {
        let name = |index: usize| &self.schema.definition(index).name;
        let (view, errors) = match field.field_type {
            FieldType::Vector(index) => (
                vec![
                    format!(
                        "A view of the items, which reads each [`{}`] when it's asked for, and",
                        name(index)
                    ),
                    "fails then if its bytes are corrupt".to_owned(),
                ],
                "if the field, or the vector's count or the ends of its items, lie outside the bytes",
            ),
            FieldType::StructArray(index) => (
                vec![format!(
                    "A view of the structs, which reads each [`{}`] when it's asked for",
                    name(index)
                )],
                "if the field or its structs lie outside the bytes",
            ),
            _ => (
                self.integers_doc(field).into_iter().collect(),
                "if the field lies outside the bytes",
            ),
        };

        let mut docs = field.docs.clone();
        if let Some(default) = default {
            // A paragraph of its own, after the field's
            if !docs.is_empty() {
                docs.push(String::new());
            }
            docs.push(format!(
                "`{default}` when the table doesn't hold it, as [`Self::has_{}`] tells",
                field.name
            ));
        }
        if !docs.is_empty() && !view.is_empty() {
            docs.push(String::new());
        }
        docs.extend(view);
        docs.extend(errors_doc(errors));
        // The errors' heading follows a blank line only after a summary
        if docs.first().is_some_and(String::is_empty) {
            docs.remove(0);
        }
        docs
    }

    /// The enum that names the values of a scalar field, whose struct the
    /// field holds. An array of an enum holds its integers, so it has none.
    fn scalar_enum(&self, field: &Field) -> Option<&Enum> {
        match field.field_type {
            FieldType::Scalar(_) => field
                .enumeration
                .map(|index| self.schema.enumeration(index)),
            _ => None,
        }
    }

    /// The documentation of an array of an enum, which is an array of the
    /// enum's integers, and `None` for any other field
    fn integers_doc(&self, field: &Field) -> Option<String> {
        let FieldType::Array(scalar) = field.field_type else {
            return None;
        };
        let enumeration = self.schema.enumeration(field.enumeration?);

        Some(format!(
            "Each element is the `{}` of {}",
            scalar_type(scalar),
            article(&enumeration.name)
        ))
    }

    /// The type of a scalar field: its enum's struct, or the scalar's type
    fn scalar_field_type(&self, field: &Field, scalar: Scalar) -> String {
        match self.scalar_enum(field) {
            Some(enumeration) => enumeration.name.clone(),
            None => scalar_type(scalar).to_owned(),
        }
    }

    fn value_type(&self, field: &Field) -> String {
        match field.field_type {
            FieldType::Scalar(scalar) => self.scalar_field_type(field, scalar),
            FieldType::Array(Scalar::Uint8) => "&'a [u8]".to_owned(),
            FieldType::Array(scalar) => format!("ArrayView<'a, {}>", scalar_type(scalar)),
            FieldType::String => "&'a str".to_owned(),
            FieldType::Struct(index) => self.schema.definition(index).name.clone(),
            FieldType::StructArray(index) => {
                format!("{}Array<'a>", self.schema.definition(index).name)
            }
            FieldType::Table(index) => format!("{}<'a>", self.schema.definition(index).name),
            FieldType::Vector(index) => format!("{}Vector<'a>", self.schema.definition(index).name),
        }
    }

    fn input_type(&self, field: &Field) -> String {
        let fields = |index: usize| self.fields_type(index, "'a");

        match field.field_type {
            FieldType::Scalar(scalar) => self.scalar_field_type(field, scalar),
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

/// The expression of an optional field's `Option`, of the fields struct's
/// `value` of it, which `if let Some(value)` binds: the `Option` itself,
/// which is `Copy`, or a reference to that of a nested table's fields
fn option_of(field: &Field, value: &str) -> String {
    match field.field_type {
        FieldType::Table(_) => format!("&{value}"),
        _ => value.to_owned(),
    }
}

/// Whether a message reads more than one version and has fields that later
/// versions added, whose getters compare the version that it read with
/// theirs, which it then holds
fn holds_version(definition: &Definition) -> bool {
    let versioned = definition
        .layout()
        .is_some_and(|layout| layout.version.is_some());
    versioned && definition.fields.iter().any(|field| field.since.is_some())
}

/// The checks before the read of a field that may be missing, which find
/// whether the table holds it:
///
/// - A field added since a version of a message with a version is missing
///   from a table of an earlier version, which the message holds.
/// - A field added since a revision of any other table is missing when its
///   offset would end after the table's first value starts, which its first
///   field's offset gives, and when its offset lies in the padding before an
///   aligned first value, which reads as 0.
/// - An optional field is absent when its offset is 0, which a nested
///   table's reader finds at the table's own position.
///
/// A required field has none, and is read as before.
struct Checks {
    /// The version that added the field to a message with a version
    version: Option<u32>,
    /// The offset of the table's first value, from the table's start, and
    /// where the field's offset ends, after which the field is missing
    revision: Option<(String, usize)>,
    /// The field's position when it's absent, which the getter compares its
    /// position with: 0 at the root, or the nested table's position
    absent: Option<&'static str>,
}

impl Checks {
    fn of(definition: &Definition, index: usize, field: &Field, is_root: bool) -> Self {
        let versioned = definition
            .layout()
            .is_some_and(|layout| layout.version.is_some());

        let revision = field
            .since
            .filter(|_| !versioned)
            .map(|_| match definition.layout() {
                Some(layout) => {
                    let width = fixed::offset_size(layout.offset_type);
                    let header = usize::from(layout.sized) * width;
                    (
                        "reader.root_offset(0)?".to_owned(),
                        header + (index + 1) * width,
                    )
                }
                None => (
                    "reader.field_offset(self.position, 0)? - self.position".to_owned(),
                    (index + 1) * NESTED_OFFSET_SIZE,
                ),
            });
        let checks_offset = field.optional || revision.is_some();

        Self {
            version: field.since.filter(|_| versioned),
            revision,
            absent: checks_offset.then_some(if is_root { "0" } else { "self.position" }),
        }
    }

    /// Writes the checks of the version and the revision, which return
    /// `missing`, with the line that makes the reader that they and the
    /// read after them use
    fn write(&self, writer: &mut CodeWriter, reader: &str, missing: &str) {
        if let Some(since) = self.version {
            writer.open(format!("if self.version < {} {{", integer(&since)));
            writer.line(format!("return {missing};"));
            writer.close("}");
            writer.blank();
        }

        writer.line(reader);
        if let Some((first, end)) = &self.revision {
            writer.open(format!("if {first} < {} {{", integer(end)));
            writer.line(format!("return {missing};"));
            writer.close("}");
            writer.blank();
        }
    }
}

/// The method that tells whether the table holds a scalar that may be
/// missing, `has_<field>()`, whose getter returns its default otherwise.
/// One that only the version tells can't fail, but returns a `Result` as
/// every other does.
fn presence_method(
    writer: &mut CodeWriter,
    checks: &Checks,
    field: &Field,
    reader: &str,
    offset: &str,
) {
    let name = format!("has_{}", field.name);
    let mut docs = vec![format!("Whether the table holds `{}`", field.name)];

    writer.blank();
    let Some(absent) = checks.absent else {
        let since = checks
            .version
            .expect("a field that may be missing has a check");
        docs.extend(
            [
                "",
                "# Errors",
                "",
                "None: the version that [`Self::new`] read tells.",
            ]
            .map(str::to_owned),
        );
        rust_doc(writer, &docs);
        writer.line("#[allow(clippy::unnecessary_wraps)]");
        writer.open(format!(
            "pub const fn {name}(&self) -> Result<bool, DecodeError> {{"
        ));
        writer.line(format!("Ok(self.version >= {})", integer(&since)));
        writer.close("}");
        return;
    };

    docs.extend(errors_doc("if the field's offset lies outside the bytes"));
    rust_doc(writer, &docs);
    writer.open(format!(
        "pub fn {name}(&self) -> Result<bool, DecodeError> {{"
    ));
    checks.write(writer, reader, "Ok(false)");
    writer.line(format!("let position = {offset};"));
    writer.line(format!("Ok(position != {absent})"));
    writer.close("}");
}

/// A versioned message's `new()`, which checks its version: that it's the
/// message's own, or within the versions that it reads when it has a first
/// version, which it then holds when the getters of the fields that later
/// versions added need it
fn constructor(writer: &mut CodeWriter, name: &str, layout: Layout, holds_version: bool) {
    let mut docs = vec![format!("Reads {} from `bytes`", article(name))];
    if layout.since.is_some() {
        docs.extend(
            [
                "",
                "# Errors",
                "",
                "Returns a [`DecodeError`] if `bytes` are in a format version before",
                "[`Self::FIRST_VERSION`] or after [`Self::FORMAT_VERSION`].",
            ]
            .map(str::to_owned),
        );
    } else {
        docs.extend(errors_doc("if `bytes` are in another format version"));
    }
    rust_doc(writer, &docs);

    writer.open("pub fn new(bytes: &'a [u8]) -> Result<Self, DecodeError> {");
    writer.line("let reader = TableReader::new(bytes, Self::LAYOUT);");
    writer.blank();
    writer.line("let version = reader.version()?;");
    if layout.since.is_some() {
        writer.open("if !(Self::FIRST_VERSION..=Self::FORMAT_VERSION).contains(&version) {");
        writer.open("return Err(DecodeError::VersionOutsideRange {");
        writer.line("version,");
        writer.line("first: Self::FIRST_VERSION,");
        writer.line("last: Self::FORMAT_VERSION,");
    } else {
        writer.open("if version != Self::FORMAT_VERSION {");
        writer.open("return Err(DecodeError::UnsupportedVersion {");
        writer.line("version,");
        writer.line("supported: Self::FORMAT_VERSION,");
    }
    writer.close("});");
    writer.close("}");
    writer.blank();
    writer.line(if holds_version {
        "Ok(Self { bytes, version })"
    } else {
        "Ok(Self { bytes })"
    });
    writer.close("}");
}

/// A message's `serialize()` when it's written whole: into the bytes that
/// the runtime's `TableWriter::table_bytes()` returns, with its header,
/// its offsets and fields of fixed size at constant positions, and then
/// its string
fn serialize_whole(
    writer: &mut CodeWriter,
    definition: &Definition,
    layout: Layout,
    plan: &WholeMessage,
) {
    let name = &definition.name;
    let width_type = offset_type_name(layout.offset_type);
    let string = plan
        .string
        .then(|| identifier(&definition.fields[definition.fields.len() - 1].name));

    // The string's length is checked before it's written in its slot
    if string.is_some() {
        writer.line("#[allow(clippy::cast_possible_truncation)]");
    }
    writer.open("pub fn serialize(&self) -> Result<Vec<u8>, BuildError> {");
    let length = integer(&(plan.header + plan.length));
    let length = match &string {
        Some(string) => format!("{length} + self.{string}.len() as u64"),
        None => length,
    };
    let mut arguments = vec![
        text(integer(&definition.fields.len())),
        text(format!("{name}::LAYOUT")),
    ];
    arguments.push(text(length));
    let_call(
        writer,
        "let mut bytes =",
        "TableWriter::table_bytes",
        arguments,
        "?;",
    );
    writer.blank();

    // The positions in the bytes after the header
    writer.line(format!("let mut fixed = [0; {}];", integer(&plan.length)));
    let mut position = definition.fields.len() * plan.width;
    let mut offsets = Vec::with_capacity(definition.fields.len());
    for size in plan.sizes.iter().chain(plan.string.then_some(&plan.width)) {
        offsets.push(position);
        position += size;
    }
    for (index, offset) in offsets.iter().enumerate() {
        let value = format!("{}_{width_type}", integer(&(plan.header + offset)));
        slot(writer, index * plan.width, plan.width, &value, false);
    }
    for ((field, size), &start) in definition.fields.iter().zip(&plan.sizes).zip(&offsets) {
        let value = format!("self.{}", identifier(&field.name));
        let end = start + size;
        let bytes = match field.field_type {
            FieldType::Scalar(Scalar::Bool) => {
                writer.line(format!("fixed[{}] = u8::from({value});", integer(&start)));
                continue;
            }
            FieldType::Scalar(_) => format!("{}.to_le_bytes()", stored(field, value)),
            FieldType::Struct(_) | FieldType::Table(_) => format!("{value}.to_bytes()"),
            _ => unreachable!("a whole message's fields before its string have fixed sizes"),
        };
        writer.line(format!(
            "fixed[{}..{}].copy_from_slice(&{bytes});",
            integer(&start),
            integer(&end)
        ));
    }
    // The string's length is the last offset's field
    if let (Some(string), Some(&start)) = (&string, offsets.last()) {
        let value = format!("self.{string}.len() as {width_type}");
        slot(writer, start, plan.width, &value, true);
    }
    writer.line("bytes.extend_from_slice(&fixed);");
    if let Some(string) = &string {
        writer.line(format!(
            "bytes.extend_from_slice(self.{string}.as_bytes());"
        ));
    }
    writer.blank();
    writer.line("Ok(bytes)");
    writer.close("}");
}

/// A message that the generated code writes whole, at constant positions
struct WholeMessage {
    /// The byte length of its header's slots, which the runtime writes
    header: usize,
    /// The width of its offsets and of its string's length
    width: usize,
    /// The sizes of its fields of fixed size, which are all its fields but a
    /// string after them
    sizes: Vec<usize>,
    /// Whether a string follows those fields
    string: bool,
    /// The byte length of its offsets, of those fields, and of the string's
    /// length, after the header: all of it but the string's bytes
    length: usize,
}

/// Writes `value`, an expression of an offset type of `width` bytes, into
/// the slot at `position` of a whole message's fixed bytes. A `cast` is
/// parenthesized where it's a method's receiver.
fn slot(writer: &mut CodeWriter, position: usize, width: usize, value: &str, cast: bool) {
    if width == 1 {
        writer.line(format!("fixed[{}] = {value};", integer(&position)));
        return;
    }

    let receiver = if cast {
        format!("({value})")
    } else {
        value.to_owned()
    };
    writer.line(format!(
        "fixed[{}..{}].copy_from_slice(&{receiver}.to_le_bytes());",
        integer(&position),
        integer(&(position + width))
    ));
}

/// The unsigned integer type of an offset type's slots
const fn offset_type_name(offset_type: OffsetType) -> &'static str {
    match offset_type {
        OffsetType::Uint8 => "u8",
        OffsetType::Uint16 => "u16",
        OffsetType::Uint32 => "u32",
    }
}

/// An enum: a struct of its integer, since a field of the enum holds any
/// integer of its type, with a constant of each value that the schema names.
/// The constants are in `SCREAMING_SNAKE_CASE`, which no keyword is in, so
/// they need no escaping.
fn enum_struct(writer: &mut CodeWriter, enumeration: &Enum) {
    let name = &enumeration.name;
    let underlying = scalar_type(enumeration.underlying);

    let mut docs = enumeration.docs.clone();
    if !docs.is_empty() {
        docs.push(String::new());
    }
    docs.push(format!(
        "A value is any `{underlying}`: one that a constant of [`{name}`] names, or any other"
    ));
    rust_doc(writer, &docs);
    writer.line("#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]");
    writer.line(format!("pub struct {name}(pub {underlying});"));
    writer.blank();

    writer.open(format!("impl {name} {{"));
    for value in &enumeration.values {
        rust_doc(writer, &value.docs);
        writer.line(format!(
            "pub const {}: Self = Self({});",
            value.name.to_ascii_uppercase(),
            signed_integer(value.value)
        ));
    }
    writer.close("}");
}

/// A struct's field `name` of an enum, read from `bytes` by `read`, the
/// function of the enum's integer type, as rustfmt breaks it. The read is
/// always wider than rustfmt's `fn_call_width`, so it never stays on the
/// field's line as the enum's argument: rustfmt breaks the read's own
/// argument when both calls fit on the field's line with room for the `),`
/// that ends the enum's, and the enum's otherwise.
fn enum_of_bytes(writer: &mut CodeWriter, name: &str, enumeration: &str, read: &str, bytes: &str) {
    let calls = format!("{name}: {enumeration}({read}(");
    if writer.fits(&STYLE, &format!("{calls}),")) {
        writer.open(calls);
        writer.line(format!("{bytes},"));
        writer.close(")),");
        return;
    }

    let argument = text(format!("{read}({bytes})"));
    writer.statement(
        &STYLE,
        &format!("{name}: "),
        &call(enumeration, vec![argument]),
        ",",
    );
}

/// The expression of the integer that the `value` of a scalar field stores:
/// the integer of its enum's struct, or the value itself
fn stored(field: &Field, value: String) -> String {
    match field.enumeration {
        Some(_) => format!("{value}.0"),
        None => value,
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

/// The tables that a definition's vectors hold
fn vectors(definition: &Definition) -> impl Iterator<Item = usize> + '_ {
    definition
        .fields
        .iter()
        .filter_map(|field| match field.field_type {
            FieldType::Vector(index) => Some(index),
            _ => None,
        })
}

/// `prefix<generics>suffix`, with each of the generics on a line of its own
/// when it doesn't fit, as rustfmt breaks them
fn generic_list(writer: &mut CodeWriter, prefix: &str, generics: &[String], suffix: &str) {
    let line = format!("{prefix}<{}>{suffix}", generics.join(", "));
    if writer.fits(&STYLE, &line) {
        writer.line(line);
        return;
    }

    writer.open(format!("{prefix}<"));
    for generic in generics {
        writer.line(format!("{generic},"));
    }
    writer.close(format!(">{suffix}"));
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
        calls.push(".with_version(Self::FORMAT_VERSION)".to_owned());
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

/// Writes `binding` = a call of `callee` + `suffix` as rustfmt breaks a
/// `let` statement that doesn't fit: with the call on the next line when it
/// fits there, its arguments within rustfmt's `fn_call_width`, and with its
/// arguments broken otherwise
fn let_call(
    writer: &mut CodeWriter,
    binding: &str,
    callee: &str,
    arguments: Vec<Expr>,
    suffix: &str,
) {
    let flat_arguments: Vec<String> = arguments.iter().map(Expr::flat).collect();
    let flat_arguments = flat_arguments.join(", ");
    let flat = format!("{callee}({flat_arguments}){suffix}");
    let next_line = format!("{}{flat}", STYLE.indent);
    if !writer.fits(&STYLE, &format!("{binding} {flat}"))
        && flat_arguments.len() <= FN_CALL_WIDTH
        && writer.fits(&STYLE, &next_line)
    {
        writer.line(binding);
        writer.indent();
        writer.line(flat);
        writer.dedent();
        return;
    }

    writer.statement(
        &STYLE,
        &format!("{binding} "),
        &call(callee, arguments),
        suffix,
    );
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

/// A scalar's default as a Rust literal of its type: a float as the shortest
/// one that reads as the same value of its type, a `float32`'s as the
/// `f32` that it holds, and digits in groups of three as [`integer()`]
/// groups them
fn rust_literal(value: ScalarValue, scalar: Scalar) -> String {
    match value {
        ScalarValue::Bool(value) => value.to_string(),
        ScalarValue::Integer(value) => signed_integer(value),
        // The default is the float32 nearest to the schema's, so it's
        // exactly an f32
        #[allow(clippy::cast_possible_truncation)]
        ScalarValue::Float(value) if scalar == Scalar::Float32 => {
            float_literal(&format!("{:?}", value as f32))
        }
        ScalarValue::Float(value) => float_literal(&format!("{value:?}")),
    }
}

/// A float's `Debug` form, which has a fraction or an exponent, with the
/// digits of its whole part and of its fraction in groups of three when
/// either has more than four, as clippy's `unreadable_literal` asks
fn float_literal(debug: &str) -> String {
    let (sign, unsigned) = debug
        .strip_prefix('-')
        .map_or(("", debug), |unsigned| ("-", unsigned));
    let (mantissa, exponent) = unsigned
        .split_once('e')
        .map_or((unsigned, None), |(mantissa, exponent)| {
            (mantissa, Some(exponent))
        });
    let (whole, fraction) = mantissa
        .split_once('.')
        .map_or((mantissa, None), |(whole, fraction)| {
            (whole, Some(fraction))
        });

    let mut literal = format!("{sign}{}", integer(&whole));
    if let Some(fraction) = fraction {
        literal.push('.');
        for (index, digit) in fraction.chars().enumerate() {
            if index > 0 && fraction.len() > 4 && index.is_multiple_of(3) {
                literal.push('_');
            }
            literal.push(digit);
        }
    }
    if let Some(exponent) = exponent {
        literal.push('e');
        literal.push_str(exponent);
    }
    literal
}

/// The literal of an enum's value: its digits in groups, as [`integer()`]
/// groups them, after its sign
fn signed_integer(value: i128) -> String {
    let digits = integer(&value.unsigned_abs());
    if value < 0 {
        format!("-{digits}")
    } else {
        digits
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Defaults as literals of their types that read as the same values,
    /// with the digits that clippy's `unreadable_literal` groups grouped
    #[test]
    fn test_rust_literal_writes_a_default_as_its_type_reads_it() {
        let cases = [
            (ScalarValue::Bool(true), Scalar::Bool, "true"),
            (ScalarValue::Integer(7), Scalar::Uint16, "7"),
            (ScalarValue::Integer(-100_000), Scalar::Int32, "-100_000"),
            (
                ScalarValue::Integer(i128::from(i64::MIN)),
                Scalar::Int64,
                "-9_223_372_036_854_775_808",
            ),
            (
                ScalarValue::Integer(i128::from(u64::MAX)),
                Scalar::Uint64,
                "18_446_744_073_709_551_615",
            ),
            // The float32 nearest to 0.1, as the checker holds it
            (
                ScalarValue::Float(f64::from(0.1_f32)),
                Scalar::Float32,
                "0.1",
            ),
            (ScalarValue::Float(0.1), Scalar::Float64, "0.1"),
            (ScalarValue::Float(-0.0), Scalar::Float64, "-0.0"),
            (ScalarValue::Float(1.0), Scalar::Float32, "1.0"),
            (
                ScalarValue::Float(123_456.789_012),
                Scalar::Float64,
                "123_456.789_012",
            ),
            (ScalarValue::Float(1e-7), Scalar::Float64, "1e-7"),
            (ScalarValue::Float(1e16), Scalar::Float64, "1e16"),
            (
                ScalarValue::Float(f64::from(f32::MAX)),
                Scalar::Float32,
                "3.402_823_5e38",
            ),
        ];

        for (value, scalar, literal) in cases {
            assert_eq!(rust_literal(value, scalar), literal, "{value:?}");
        }
    }
}
