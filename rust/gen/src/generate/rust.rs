//! Rust for `tafla`. Messages and tables are `Copy` views that read fields
//! lazily, borrowing strings and arrays from the bytes, and vectors and
//! arrays of structs as views that read each item when it's asked for;
//! they're written from structs of their fields, which a `TableSizer` sizes
//! and a `TableWriter` writes, nested tables in place. Structs are plain
//! structs read and written at fixed offsets.

use std::collections::HashSet;
use std::fmt::Display;

use super::fixed::{self, FixedSizes, NESTED_OFFSET_SIZE};
use super::prose::indefinite_article;
use super::writer::{CodeWriter, Expr, Style, call, text};
use super::{GeneratedFile, Options};
use crate::model::{
    Definition, Field, FieldType, Kind, Layout, OffsetType, Scalar, Schema,
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
    let generator = Generator {
        schema,
        fixed: FixedSizes::new(schema, MAX_LAID_OUT),
        borrows: borrowing(schema),
        vector_components: strongly_connected_components(&schema.definitions, vectors),
        declared: schema
            .definitions
            .iter()
            .map(|definition| definition.name.as_str())
            .collect(),
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
        if has_tables {
            names.push("ByteBuffer");
        }
        names.push("DecodeError");
        if has_offset_type {
            names.push("OffsetType");
        }
        // Messages make sizers, or the bytes of a whole one, and tables take
        // sizers and writers
        if has_tables {
            names.push("TableLayout");
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
            rust_doc(writer, &field.docs);
            let field_type = match generic {
                Some(generic) => {
                    if !field.docs.is_empty() {
                        rust_doc(writer, &[String::new()]);
                    }
                    rust_doc(writer, &[self.generic_doc(field.field_type, generic)]);
                    format!("&'a [{}]", generic.parameter)
                }
                None => self.input_type(field.field_type),
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
            !matches!(
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
                FieldType::Scalar(_) => format!("{value}.to_le_bytes()"),
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
    /// locals don't clash with fields, whose values are `self`'s.
    fn sizes(
        &self,
        writer: &mut CodeWriter,
        definition: &Definition,
        generic: &[Option<GenericField>],
        sizer: &str,
    ) {
        for (field, generic) in definition.fields.iter().zip(generic) {
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
                        writer.statement(
                            &STYLE,
                            "",
                            &call("sizer.add_table_bytes", arguments),
                            ";",
                        );
                        continue;
                    }
                    if let Some(generic) = generic {
                        writer.open(format!("for item in {value} {{"));
                        writer.line(format!("let fields: {} = item.into();", generic.item));
                    } else {
                        writer.open(format!("for fields in {value} {{"));
                    }
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
    fn writes(
        &self,
        writer: &mut CodeWriter,
        definition: &Definition,
        generic: &[Option<GenericField>],
        table_writer: &str,
    ) {
        for ((index, field), generic) in definition.fields.iter().enumerate().zip(generic) {
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

            let docs = self.getter_docs(field);
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
                    writer.line(format!("Ok({structure}Array {{ structs }})"));
                }
                FieldType::Vector(table) => {
                    let table = &self.schema.definition(table).name;
                    writer.open(format!("Ok({table}Vector {{"));
                    writer.line("vector: buffer.get_vector(position)?,");
                    writer.close("})");
                }
            }
            writer.close("}");
        }
    }

    /// The documentation of a field's getter: the field's own, and for a
    /// vector or an array of structs, the view that it returns, and then its
    /// errors
    fn getter_docs(&self, field: &Field) -> Vec<String> {
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
            _ => (vec![], "if the field lies outside the bytes"),
        };

        let mut docs = field.docs.clone();
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

    fn value_type(&self, field_type: FieldType) -> String {
        match field_type {
            FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
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

    fn input_type(&self, field_type: FieldType) -> String {
        let fields = |index: usize| self.fields_type(index, "'a");

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
    let function = if layout.version.is_some() {
        arguments.push(text(format!("{name}::FORMAT_VERSION")));
        "TableWriter::versioned_table_bytes"
    } else {
        "TableWriter::table_bytes"
    };
    arguments.push(text(length));
    let_call(writer, "let mut bytes =", function, arguments, "?;");
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
            FieldType::Scalar(_) => format!("{value}.to_le_bytes()"),
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
