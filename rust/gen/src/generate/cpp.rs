//! C++ for `tafla`'s header-only runtime, in a header of C++20. Messages and
//! tables are views that read fields lazily from bytes that the caller owns,
//! as `std::string_view`s and `tafla::ArrayView`s of strings and arrays;
//! they're written from structs of their fields, which a `TableSizer` sizes
//! and a `TableWriter` writes, nested tables in place. Structs are aggregates
//! read and written at fixed offsets.
//!
//! A header declares its classes before any of their members' definitions,
//! which follow them, so that tables can name each other in any order,
//! including through vectors that hold their own table. Structs, and the
//! structs of tables' fields, which hold others by value, come after those
//! that they hold.

use super::prose::indefinite_article;
use super::writer::{CodeWriter, comment_text, escape_outside_code_spans};
use super::{GeneratedFile, Options};
use crate::model::{
    Definition, Field, FieldType, Kind, Layout, OffsetType, Scalar, Schema,
    strongly_connected_components,
};

/// The runtime's header, unless [`Options::runtime`] names another
const RUNTIME: &str = "tafla/tafla.hpp";

const INDENT: &str = "    ";

/// The column limit of clang-format's style in `cpp/.clang-format`, within
/// which comments stay
const WIDTH: usize = 120;

/// C++20's keywords and alternative tokens, which names take a `_` after
const KEYWORDS: [&str; 96] = [
    "alignas",
    "alignof",
    "and",
    "and_eq",
    "asm",
    "auto",
    "bitand",
    "bitor",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "char16_t",
    "char32_t",
    "char8_t",
    "class",
    "co_await",
    "co_return",
    "co_yield",
    "compl",
    "concept",
    "const",
    "const_cast",
    "consteval",
    "constexpr",
    "constinit",
    "continue",
    "decltype",
    "default",
    "delete",
    "do",
    "double",
    "dynamic_cast",
    "else",
    "enum",
    "explicit",
    "export",
    "extern",
    "false",
    "final",
    "float",
    "for",
    "friend",
    "goto",
    "if",
    "import",
    "inline",
    "int",
    "long",
    "module",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "not",
    "not_eq",
    "nullptr",
    "operator",
    "or",
    "or_eq",
    "override",
    "private",
    "protected",
    "public",
    "register",
    "reinterpret_cast",
    "requires",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "static_assert",
    "static_cast",
    "struct",
    "switch",
    "template",
    "this",
    "thread_local",
    "throw",
    "true",
    "try",
    "typedef",
    "typeid",
    "typename",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "volatile",
    "wchar_t",
    "while",
    "xor",
    "xor_eq",
];

/// The namespaces that the generated code names, which its own namespace
/// can't take, since a name of it would hide them
const NAMESPACES: [&str; 2] = ["std", "tafla"];

/// The macros in `snake_case` that a header that the generated code
/// includes, or that it's included beside, may define, so names take a `_`
/// after them too: the C standard library's; those that compilers predefine
/// for their platforms outside strict ISO modes, such as GNU's `-std=gnu++20`,
/// which `CMAKE_CXX_EXTENSIONS` selects by default; `major()` and `minor()`
/// of glibc's `<sys/types.h>`; and
/// `min()`, `max()` and the empty or type macros of `<windows.h>`
const MACROS: [&str; 32] = [
    "assert",
    "cdecl",
    "errno",
    "far",
    "hyper",
    "i386",
    "interface",
    "linux",
    "m68k",
    "major",
    "makedev",
    "max",
    "mc68000",
    "mc68020",
    "min",
    "minor",
    "mips",
    "near",
    "offsetof",
    "pascal",
    "setjmp",
    "small",
    "sparc",
    "stderr",
    "stdin",
    "stdout",
    "sun",
    "unix",
    "va_arg",
    "va_copy",
    "va_end",
    "va_start",
];

/// The macros of the C standard library in `PascalCase`, which a header that
/// the generated code includes may define, so that types and namespaces take
/// a `_` after them. Platforms define more, such as POSIX's `EPERM` and
/// `SIGHUP`.
const TYPE_MACROS: [&str; 14] = [
    "BUFSIZ", "EDOM", "EILSEQ", "EOF", "ERANGE", "INFINITY", "NAN", "NULL", "SIGABRT", "SIGFPE",
    "SIGILL", "SIGINT", "SIGSEGV", "SIGTERM",
];

/// The most fields that a table's `size()` and `write()` write, with those
/// of the tables nested in it that they inline, for them to be inlined into
/// their parent's in turn. Forcing every table's inline would copy a table's
/// code into each field that holds it, so that tables that hold others in
/// pairs, a few levels deep, compiled for minutes.
const INLINE_FIELDS: usize = 64;

/// The members that the generated code declares beside the fields, which a
/// field of the same name would clash with, so names take a `_` after them
/// too: a struct's `read()`, `from_bytes()`, `to_bytes()` and
/// `array_to_bytes()`, a fields struct's `serialize()`, `size()` and
/// `write()`, and a versioned message's `version_of()`
const MEMBERS: [&str; 8] = [
    "array_to_bytes",
    "from_bytes",
    "read",
    "serialize",
    "size",
    "to_bytes",
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
    let namespace = match &options.package {
        Some(package) => package
            .as_str()
            .split('.')
            .map(namespace_of)
            .collect::<Vec<_>>()
            .join("::"),
        None => namespace_of(stem),
    };
    let recursive = recursive_tables(schema);
    let generator = Generator {
        schema,
        inlined: inlined_tables(schema, &recursive),
    };

    let mut writer = CodeWriter::new(INDENT);
    writer.line(format!("// {header}"));
    // A schema without definitions includes nothing
    if schema.definitions.is_empty() {
        return vec![GeneratedFile {
            path: format!("{stem}.hpp"),
            contents: writer.finish(),
        }];
    }

    writer.blank();
    writer.line("#pragma once");
    writer.blank();
    for include in ["cstddef", "cstdint", "span", "string_view", "vector"] {
        writer.line(format!("#include <{include}>"));
    }
    writer.blank();
    writer.line(format!("#include <{runtime}>"));
    writer.blank();
    writer.line(format!("namespace {namespace} {{"));

    let tables: Vec<usize> = nesting_order(schema, |definition| !definition.is_struct());
    let structs: Vec<usize> = nesting_order(schema, Definition::is_struct);

    // Every table's reader and fields struct, so that they and the views can
    // name each other
    if !tables.is_empty() {
        writer.blank();
        for &index in &tables {
            writer.line(format!(
                "class {};",
                type_name(&schema.definition(index).name)
            ));
        }
        for &index in &tables {
            writer.line(format!(
                "struct {}Fields;",
                type_name(&schema.definition(index).name)
            ));
        }
    }

    for &index in &structs {
        writer.blank();
        generator.structure(&mut writer, index);
    }

    view_aliases(&mut writer, schema);

    for &index in &tables {
        writer.blank();
        let definition = schema.definition(index);
        match definition.kind {
            Kind::Message(layout) => generator.message(&mut writer, definition, layout),
            Kind::Table => generator.table(&mut writer, definition),
            Kind::Struct(_) => unreachable!("structs are written before tables"),
        }
        writer.blank();
        generator.fields_struct(&mut writer, index);
    }

    for &index in &tables {
        let definition = schema.definition(index);
        generator.getter_definitions(&mut writer, definition);
        match definition.kind {
            Kind::Message(layout) => generator.serialize(&mut writer, definition, layout),
            Kind::Table => generator.size_and_write(&mut writer, definition),
            Kind::Struct(_) => unreachable!("structs are written before tables"),
        }
    }

    writer.blank();
    writer.line(format!("}}  // namespace {namespace}"));

    vec![GeneratedFile {
        path: format!("{stem}.hpp"),
        contents: writer.finish(),
    }]
}

struct Generator<'a> {
    schema: &'a Schema,
    /// Whether each table's `size()` and `write()` are forced inline into
    /// their parent's, by its index
    inlined: Vec<bool>,
}

impl Generator<'_> {
    fn message(&self, writer: &mut CodeWriter, definition: &Definition, layout: Layout) {
        let name = &type_name(&definition.name);

        cpp_doc(writer, &definition.docs);
        writer.line(format!("class {name} {{"));
        writer.line("public:");
        writer.indent();
        if let Some(version) = layout.version {
            cpp_doc(
                writer,
                &["The version of the format, which the header carries and the constructor checks"
                    .to_owned()],
            );
            writer.line(format!(
                "static constexpr std::uint32_t FORMAT_VERSION = {version};"
            ));
            writer.blank();
        }

        cpp_doc(
            writer,
            &["The layout of the table, which its writer writes and its reader reads".to_owned()],
        );
        assignment(
            writer,
            "static constexpr auto LAYOUT",
            &layout_expression(layout),
        );
        writer.blank();

        if layout.version.is_some() {
            cpp_doc(
                writer,
                &[format!(
                    "Reads {} from `bytes`, which it views, or throws a `tafla::DecodeError` if \
                     they're in another format version",
                    article(name)
                )],
            );
            writer.line(format!(
                "explicit {name}(std::span<const std::uint8_t> bytes);"
            ));
            writer.blank();
            cpp_doc(
                writer,
                &[
                    "Returns the format version in the header of `bytes`, without checking it, so \
                     that an application can choose the reader of that version, or throws a \
                     `tafla::DecodeError` if the header doesn't fit in `bytes`"
                        .to_owned(),
                ],
            );
            writer.line(
                "[[nodiscard]] static std::uint32_t version_of(std::span<const std::uint8_t> bytes);",
            );
        } else {
            cpp_doc(
                writer,
                &[format!(
                    "Reads {} from `bytes`, which it views",
                    article(name)
                )],
            );
            writer.line(format!(
                "explicit constexpr {name}(std::span<const std::uint8_t> bytes) noexcept : bytes_(bytes) {{}}"
            ));
        }

        self.getter_declarations(writer, definition);
        writer.dedent();
        writer.blank();
        writer.line("private:");
        writer.indent();
        writer.line("std::span<const std::uint8_t> bytes_;");
        writer.dedent();
        writer.line("};");
    }

    fn table(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &type_name(&definition.name);

        cpp_doc(writer, &definition.docs);
        writer.line(format!("class {name} {{"));
        writer.line("public:");
        writer.indent();
        cpp_doc(
            writer,
            &[
                "Reads the table at `position` in `bytes`, as its parent's getter finds it"
                    .to_owned(),
            ],
        );
        writer.line(format!(
            "constexpr {name}(std::span<const std::uint8_t> bytes, std::size_t position) noexcept"
        ));
        writer.line(format!("{INDENT}: bytes_(bytes), position_(position) {{}}"));
        writer.blank();
        cpp_doc(
            writer,
            &[
                "Reads an empty table, whose getters throw a `tafla::DecodeError`, so that a \
                 `std::vector` of them can be sized before they're read"
                    .to_owned(),
            ],
        );
        writer.line(format!("constexpr {name}() noexcept = default;"));

        self.getter_declarations(writer, definition);
        writer.dedent();
        writer.blank();
        writer.line("private:");
        writer.indent();
        writer.line("std::span<const std::uint8_t> bytes_;");
        writer.line("std::size_t position_ = 0;");
        writer.dedent();
        writer.line("};");
    }

    fn getter_declarations(&self, writer: &mut CodeWriter, definition: &Definition) {
        for field in &definition.fields {
            let name = identifier(&field.name);
            let mut docs = field.docs.clone();
            if !docs.is_empty() {
                docs.push(String::new());
            }
            docs.push(
                "Throws a `tafla::DecodeError` if the field lies outside the bytes".to_owned(),
            );

            writer.blank();
            cpp_doc(writer, &docs);
            writer.line(format!(
                "[[nodiscard]] {} {name}() const;",
                self.value_type(field.field_type)
            ));

            if let Some((view, errors)) = self.view(field.field_type) {
                let mut docs = field.docs.clone();
                if !docs.is_empty() {
                    docs.push(String::new());
                }
                docs.push(format!(
                    "Views the items of `{name}()`, reading each one when it's asked for, or \
                     throws a `tafla::DecodeError` {errors}"
                ));

                writer.blank();
                cpp_doc(writer, &docs);
                writer.line(format!("[[nodiscard]] {view} {}_view() const;", field.name));
            }
        }
    }

    /// The definitions of a message's or table's getters, after every
    /// declaration. Each reads with a buffer in the table's layout, a
    /// constant, so that its reads compile for that layout's offset type.
    fn getter_definitions(&self, writer: &mut CodeWriter, definition: &Definition) {
        let class = &type_name(&definition.name);
        let is_root = matches!(definition.kind, Kind::Message(_));
        let buffer = if is_root {
            "const ::tafla::ByteBuffer buffer(bytes_, LAYOUT);"
        } else {
            "const ::tafla::ByteBuffer buffer(bytes_, ::tafla::TableLayout());"
        };

        if let Kind::Message(Layout {
            version: Some(_), ..
        }) = definition.kind
        {
            version_definitions(writer, class);
        }

        for (index, field) in definition.fields.iter().enumerate() {
            let name = identifier(&field.name);
            let offset = if is_root {
                format!("buffer.root_offset({index})")
            } else {
                format!("buffer.field_offset(position_, {index})")
            };

            // The getter of a vector or an array of structs reads every item
            // through its view
            if let Some((view, _)) = self.view(field.field_type) {
                self.view_definitions(writer, class, field, &view, buffer, &offset);
                continue;
            }

            writer.blank();
            writer.line(format!(
                "inline {} {class}::{name}() const {{",
                self.value_type(field.field_type)
            ));
            writer.indent();
            writer.line(buffer);
            writer.line(format!("const auto position = {offset};"));
            let read = match field.field_type {
                FieldType::Scalar(Scalar::Bool) => "buffer.get_uint8(position) != 0".to_owned(),
                FieldType::Scalar(scalar) => format!("buffer.get_{}(position)", scalar.name()),
                FieldType::Array(scalar) => format!("buffer.get_{}_array(position)", scalar.name()),
                FieldType::String => "buffer.get_string(position)".to_owned(),
                FieldType::Struct(structure) => {
                    let structure = &type_name(&self.schema.definition(structure).name);
                    format!(
                        "{structure}::from_bytes(buffer.get_struct(position, {structure}::SIZE).data())"
                    )
                }
                FieldType::Table(table) => {
                    let table = &type_name(&self.schema.definition(table).name);
                    format!("{table}(bytes_, position)")
                }
                FieldType::StructArray(_) | FieldType::Vector(_) => {
                    unreachable!("vectors and arrays of structs are read through their views")
                }
            };
            writer.line(format!("return {read};"));
            writer.dedent();
            writer.line("}");
        }
    }

    /// The definitions of the getter of a vector or an array of structs,
    /// which reads every item through its view, and of the view's getter,
    /// which finds the field as the other getters do, with `buffer`'s line
    /// and the `offset` it reads
    fn view_definitions(
        &self,
        writer: &mut CodeWriter,
        class: &str,
        field: &Field,
        view: &str,
        buffer: &str,
        offset: &str,
    ) {
        let name = identifier(&field.name);

        writer.blank();
        writer.line(format!(
            "inline {} {class}::{name}() const {{",
            self.value_type(field.field_type)
        ));
        writer.line(format!("{INDENT}return {}_view().to_vector();", field.name));
        writer.line("}");

        writer.blank();
        writer.line(format!(
            "inline {view} {class}::{}_view() const {{",
            field.name
        ));
        writer.indent();
        writer.line(buffer);
        writer.line(format!("const auto position = {offset};"));
        match field.field_type {
            FieldType::Vector(_) => {
                writer.line(format!("return {view}(buffer.get_vector(position));"));
            }
            FieldType::StructArray(structure) => {
                let structure = &type_name(&self.schema.definition(structure).name);
                writer.line(format!(
                    "return {view}(buffer.get_struct_array(position, {structure}::SIZE));"
                ));
            }
            _ => unreachable!("only vectors and arrays of structs have views"),
        }
        writer.dedent();
        writer.line("}");
    }

    /// A struct: its fields, its size and alignment, and the functions that
    /// read and write it at fixed offsets
    fn structure(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
        let name = &type_name(&definition.name);
        let layout = definition.struct_layout();

        cpp_doc(writer, &definition.docs);
        writer.line(format!("struct {name} {{"));
        writer.indent();
        for field in &definition.fields {
            cpp_doc(writer, &field.docs);
            writer.line(format!(
                "{} {}{{}};",
                self.input_type(field.field_type),
                identifier(&field.name)
            ));
        }
        writer.blank();

        cpp_doc(writer, &[format!("The byte length of {}", article(name))]);
        writer.line(format!(
            "static constexpr std::size_t SIZE = {};",
            layout.size
        ));
        writer.blank();
        cpp_doc(
            writer,
            &[format!(
                "The alignment of an array of {}: its largest scalar's size",
                article(name)
            )],
        );
        writer.line(format!(
            "static constexpr ::tafla::Alignment ALIGNMENT = {};",
            alignment(layout.alignment)
        ));
        writer.blank();

        struct_read(writer, name);
        self.struct_from_bytes(writer, definition);
        self.struct_to_bytes(writer, definition);
        struct_array_to_bytes(writer, name);

        writer.line(format!(
            "friend bool operator==(const {name}&, const {name}&) = default;"
        ));
        writer.dedent();
        writer.line("};");
    }

    /// A struct's `from_bytes()`, which reads its fields at their offsets
    fn struct_from_bytes(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &type_name(&definition.name);

        cpp_doc(
            writer,
            &[format!(
                "Reads {} from its `SIZE` bytes at `bytes`, such as those of an array of them",
                article(name)
            )],
        );
        writer.line(format!(
            "[[nodiscard]] static {name} from_bytes(const std::uint8_t* bytes) noexcept {{"
        ));
        writer.indent();
        writer.line("return {");
        writer.indent();
        let mut offset = 0;
        for field in &definition.fields {
            let at = at_offset("bytes", offset);
            let read = match field.field_type {
                FieldType::Scalar(Scalar::Bool) => format!("bytes[{offset}] != 0"),
                FieldType::Scalar(scalar) => {
                    format!("::tafla::load_le<{}>({at})", scalar_type(scalar))
                }
                FieldType::Struct(inner) => {
                    format!(
                        "{}::from_bytes({at})",
                        type_name(&self.schema.definition(inner).name)
                    )
                }
                _ => unreachable!("structs hold scalars and structs only"),
            };
            writer.line(format!(".{} = {read},", identifier(&field.name)));
            offset += self.field_size(field.field_type);
        }
        writer.dedent();
        writer.line("};");
        writer.dedent();
        writer.line("}");
        writer.blank();
    }

    /// A struct's `to_bytes()`, which writes its fields at their offsets
    fn struct_to_bytes(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &type_name(&definition.name);

        cpp_doc(
            writer,
            &[format!(
                "Writes the `SIZE` bytes of {} at `bytes_`",
                article(name)
            )],
        );
        writer.line("void to_bytes(std::uint8_t* bytes_) const noexcept {");
        writer.indent();
        let mut offset = 0;
        for field in &definition.fields {
            let name = identifier(&field.name);
            let at = at_offset("bytes_", offset);
            match field.field_type {
                FieldType::Scalar(Scalar::Bool) => {
                    writer.line(format!(
                        "bytes_[{offset}] = static_cast<std::uint8_t>({name});"
                    ));
                }
                FieldType::Scalar(_) => writer.line(format!("::tafla::store_le({at}, {name});")),
                FieldType::Struct(_) => writer.line(format!("{name}.to_bytes({at});")),
                _ => unreachable!("structs hold scalars and structs only"),
            }
            offset += self.field_size(field.field_type);
        }
        writer.dedent();
        writer.line("}");
        writer.blank();
    }

    /// The struct of the values that write a message or table, and the
    /// declarations of the functions that write it
    fn fields_struct(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
        let name = &type_name(&definition.name);

        cpp_doc(
            writer,
            &[format!("The fields of {}, to write", article(name))],
        );
        writer.line(format!("struct {name}Fields {{"));
        writer.indent();
        for field in &definition.fields {
            // A nested table's fields struct initializes its own fields, so
            // it takes no `{}`, which Clang checks again for each struct
            // that holds it, in time that doubled with each level of nesting
            let initializer = if matches!(field.field_type, FieldType::Table(_)) {
                ""
            } else {
                "{}"
            };
            cpp_doc(writer, &field.docs);
            writer.line(format!(
                "{} {}{initializer};",
                self.input_type(field.field_type),
                identifier(&field.name)
            ));
        }
        writer.blank();

        match definition.kind {
            Kind::Message(layout) => {
                cpp_doc(
                    writer,
                    &[format!(
                        "Writes {} with every field, or throws a `tafla::BuildError` if it's longer \
                         than its offset type, `{}`, addresses",
                        article(name),
                        layout.offset_type.name()
                    )],
                );
                writer.line("[[nodiscard]] ::tafla::Bytes serialize() const;");
            }
            Kind::Table => {
                cpp_doc(
                    writer,
                    &[format!(
                        "The alignment that a parent pads {} to: the largest alignment of its \
                         arrays and nested tables, or 1",
                        article(name)
                    )],
                );
                writer.line(format!(
                    "static constexpr ::tafla::Alignment ALIGNMENT = {};",
                    alignment(self.schema.table_alignment(index))
                ));
                writer.blank();
                cpp_doc(
                    writer,
                    &[format!(
                        "Adds {} to its parent's sizer, after the field or vector that holds it",
                        article(name)
                    )],
                );
                // Inlined into their parent's, so that the parent's sizer
                // and writer stay in registers, but for a table that holds
                // itself, which they'd inline without end, and a table whose
                // code inlined would be long
                let inline = if self.inlined[index] {
                    "TAFLA_ALWAYS_INLINE "
                } else {
                    ""
                };
                writer.line(format!(
                    "{inline}void size(::tafla::TableSizer& sizer_) const;"
                ));
                writer.blank();
                cpp_doc(
                    writer,
                    &[format!(
                        "Writes {} where its parent's writer is, after the field or vector that \
                         holds it",
                        article(name)
                    )],
                );
                writer.line(format!(
                    "{inline}void write(::tafla::TableWriter& writer_) const;"
                ));
            }
            Kind::Struct(_) => unreachable!("structs are written by their own functions"),
        }
        writer.dedent();
        writer.line("};");
    }

    /// A message's `serialize()`, which sizes its fields and then writes them
    /// into bytes of that length
    fn serialize(&self, writer: &mut CodeWriter, definition: &Definition, layout: Layout) {
        let name = &type_name(&definition.name);
        let fields = definition.fields.len();

        writer.blank();
        writer.line(format!(
            "inline ::tafla::Bytes {name}Fields::serialize() const {{"
        ));
        writer.indent();
        if layout.version.is_some() {
            writer.line(format!(
                "::tafla::TableSizer sizer_({fields}, {name}::LAYOUT, {name}::FORMAT_VERSION);"
            ));
        } else {
            writer.line(format!(
                "::tafla::TableSizer sizer_({fields}, {name}::LAYOUT);"
            ));
        }
        self.sizes(writer, definition);
        writer.blank();
        writer.line("auto writer_ = sizer_.writer();");
        self.writes(writer, definition);
        writer.blank();
        writer.line("return writer_.finish();");
        writer.dedent();
        writer.line("}");
    }

    /// The functions with which a nested table's parent sizes and writes it
    fn size_and_write(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &type_name(&definition.name);
        let fields = definition.fields.len();

        writer.blank();
        writer.line(format!(
            "inline void {name}Fields::size(::tafla::TableSizer& sizer_) const {{"
        ));
        writer.indent();
        writer.line(format!(
            "const auto parent_ = sizer_.start_table({fields});"
        ));
        self.sizes(writer, definition);
        writer.line("sizer_.end_table(parent_);");
        writer.dedent();
        writer.line("}");

        writer.blank();
        writer.line(format!(
            "inline void {name}Fields::write(::tafla::TableWriter& writer_) const {{"
        ));
        writer.indent();
        writer.line(format!(
            "const auto parent_ = writer_.start_table({fields});"
        ));
        self.writes(writer, definition);
        writer.line("writer_.end_table(parent_);");
        writer.dedent();
        writer.line("}");
    }

    /// The calls that add a table's fields to `sizer_`, in order. The locals
    /// and parameters end with a `_`, which no field's name does.
    fn sizes(&self, writer: &mut CodeWriter, definition: &Definition) {
        for field in &definition.fields {
            let value = identifier(&field.name);
            match field.field_type {
                FieldType::Scalar(scalar) => {
                    writer.line(format!("sizer_.add_{}();", scalar.stored().name()));
                }
                FieldType::Array(scalar) => {
                    writer.line(format!(
                        "sizer_.add_{}_array({value}.size());",
                        scalar.name()
                    ));
                }
                FieldType::String => writer.line(format!("sizer_.add_string({value});")),
                FieldType::Struct(index) => {
                    let structure = &type_name(&self.schema.definition(index).name);
                    writer.line(format!("sizer_.add_struct({structure}::SIZE);"));
                }
                FieldType::StructArray(index) => {
                    let structure = &type_name(&self.schema.definition(index).name);
                    writer.line(format!(
                        "sizer_.add_struct_array({value}.size(), {structure}::SIZE, {structure}::ALIGNMENT);"
                    ));
                }
                FieldType::Table(index) => {
                    let table = &type_name(&self.schema.definition(index).name);
                    writer.line(format!("sizer_.add_table({table}Fields::ALIGNMENT);"));
                    writer.line(format!("{value}.size(sizer_);"));
                }
                FieldType::Vector(_) => {
                    writer.line(format!("sizer_.add_vector({value}.size());"));
                    writer.line(format!("for (const auto& item_ : {value}) {{"));
                    writer.line(format!("{INDENT}item_.size(sizer_);"));
                    writer.line("}");
                }
            }
        }
    }

    /// The calls that write a table's fields with `writer_`, each with its
    /// index, in the order that `sizes()` added them
    fn writes(&self, writer: &mut CodeWriter, definition: &Definition) {
        for (index, field) in definition.fields.iter().enumerate() {
            let value = identifier(&field.name);
            match field.field_type {
                FieldType::Scalar(Scalar::Bool) => writer.line(format!(
                    "writer_.set_uint8({index}, static_cast<std::uint8_t>({value}));"
                )),
                FieldType::Scalar(scalar) => {
                    writer.line(format!("writer_.set_{}({index}, {value});", scalar.name()));
                }
                FieldType::Array(scalar) => writer.line(format!(
                    "writer_.set_{}_array({index}, {value});",
                    scalar.name()
                )),
                FieldType::String => writer.line(format!("writer_.set_string({index}, {value});")),
                FieldType::Struct(_) => {
                    writer.line(format!("writer_.set_struct({index}, {value});"));
                }
                FieldType::StructArray(_) => {
                    writer.line(format!("writer_.set_structs({index}, {value});"));
                }
                FieldType::Table(table) => {
                    let table = &type_name(&self.schema.definition(table).name);
                    writer.line(format!(
                        "writer_.set_table({index}, {table}Fields::ALIGNMENT);"
                    ));
                    writer.line(format!("{value}.write(writer_);"));
                }
                FieldType::Vector(_) => {
                    // Named by the field's index, so that the vectors of a
                    // table don't clash
                    let vector = format!("vector{index}_");
                    writer.line(format!(
                        "const auto {vector} = writer_.set_vector({index}, {value}.size());"
                    ));
                    writer.line(format!(
                        "for (std::size_t index_ = 0; index_ < {value}.size(); ++index_) {{"
                    ));
                    writer.indent();
                    writer.line(format!("{value}[index_].write(writer_);"));
                    writer.line(format!("writer_.set_item_end({vector}, index_);"));
                    writer.dedent();
                    writer.line("}");
                }
            }
        }
    }

    /// The type and the condition of the errors of the view of a vector or
    /// an array of structs, or nothing for any other field
    fn view(&self, field_type: FieldType) -> Option<(String, &'static str)> {
        match field_type {
            FieldType::Vector(index) => Some((
                format!("{}Vector", type_name(&self.schema.definition(index).name)),
                "if the field, or the vector's count or the ends of its items, lie outside the bytes",
            )),
            FieldType::StructArray(index) => Some((
                format!("{}Array", type_name(&self.schema.definition(index).name)),
                "if the field or its structs lie outside the bytes",
            )),
            _ => None,
        }
    }

    fn field_size(&self, field_type: FieldType) -> usize {
        match field_type {
            FieldType::Scalar(scalar) => usize::from(scalar.size()),
            FieldType::Struct(index) => self.schema.definition(index).struct_layout().size,
            _ => unreachable!("structs hold scalars and structs only"),
        }
    }

    /// The type that a getter returns
    fn value_type(&self, field_type: FieldType) -> String {
        let name = |index: usize| type_name(&self.schema.definition(index).name);

        match field_type {
            FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
            FieldType::Array(Scalar::Uint8) => "std::span<const std::uint8_t>".to_owned(),
            FieldType::Array(scalar) => format!("::tafla::ArrayView<{}>", scalar_type(scalar)),
            FieldType::String => "std::string_view".to_owned(),
            FieldType::Struct(index) | FieldType::Table(index) => name(index),
            FieldType::StructArray(index) | FieldType::Vector(index) => {
                format!("std::vector<{}>", name(index))
            }
        }
    }

    /// The type of a field of a struct or of a fields struct
    fn input_type(&self, field_type: FieldType) -> String {
        let name = |index: usize| type_name(&self.schema.definition(index).name);

        match field_type {
            FieldType::Scalar(scalar) => scalar_type(scalar).to_owned(),
            FieldType::Array(scalar) => format!("std::span<const {}>", scalar_type(scalar)),
            FieldType::String => "std::string_view".to_owned(),
            FieldType::Struct(index) => name(index),
            FieldType::StructArray(index) => format!("std::span<const {}>", name(index)),
            FieldType::Table(index) => format!("{}Fields", name(index)),
            FieldType::Vector(index) => format!("std::span<const {}Fields>", name(index)),
        }
    }
}

/// The definitions of a versioned message's constructor, which checks the
/// version, and of `version_of()`
fn version_definitions(writer: &mut CodeWriter, class: &str) {
    writer.blank();
    writer.line(format!(
        "inline {class}::{class}(std::span<const std::uint8_t> bytes) : bytes_(bytes) {{"
    ));
    writer.indent();
    writer.line("const auto version = version_of(bytes);");
    writer.line("if (version != FORMAT_VERSION) {");
    writer.line(format!(
        "{INDENT}throw ::tafla::DecodeError::unsupported_version(version, FORMAT_VERSION);"
    ));
    writer.line("}");
    writer.dedent();
    writer.line("}");
    writer.blank();
    writer.line(format!(
        "inline std::uint32_t {class}::version_of(std::span<const std::uint8_t> bytes) {{"
    ));
    writer.line(format!(
        "{INDENT}return ::tafla::ByteBuffer(bytes, LAYOUT).version();"
    ));
    writer.line("}");
}

/// A struct's `read()`, which checks that the bytes hold it
fn struct_read(writer: &mut CodeWriter, name: &str) {
    cpp_doc(
        writer,
        &[format!(
            "Reads {} from the start of `bytes`, or throws a `tafla::DecodeError` if they're \
             shorter than `SIZE`",
            article(name)
        )],
    );
    writer.line(format!(
        "[[nodiscard]] static {name} read(std::span<const std::uint8_t> bytes) {{"
    ));
    writer.indent();
    writer.line("if (bytes.size() < SIZE) {");
    writer.line(format!(
        "{INDENT}throw ::tafla::DecodeError::out_of_range(0, SIZE, bytes.size());"
    ));
    writer.line("}");
    writer.line("return from_bytes(bytes.data());");
    writer.dedent();
    writer.line("}");
    writer.blank();
}

/// A struct's `array_to_bytes()`, which writes an array's structs
fn struct_array_to_bytes(writer: &mut CodeWriter, name: &str) {
    cpp_doc(
        writer,
        &[format!(
            "Writes `{name}`s one after another, as an array of them holds them"
        )],
    );
    writer.line(format!(
        "[[nodiscard]] static std::vector<std::uint8_t> array_to_bytes(std::span<const {name}> items) {{"
    ));
    writer.indent();
    writer.line("std::vector<std::uint8_t> bytes(items.size() * SIZE);");
    writer.line("for (std::size_t index = 0; index < items.size(); ++index) {");
    writer.line(format!(
        "{INDENT}items[index].to_bytes(bytes.data() + index * SIZE);"
    ));
    writer.line("}");
    writer.line("return bytes;");
    writer.dedent();
    writer.line("}");
    writer.blank();
}

/// The views of the items of vectors and arrays of structs, as aliases of the
/// runtime's views, which read the items when they're asked for, when the
/// items are complete
fn view_aliases(writer: &mut CodeWriter, schema: &Schema) {
    for (index, definition) in schema.definitions.iter().enumerate() {
        if !schema.is_item(index) {
            continue;
        }

        writer.blank();
        let name = &type_name(&definition.name);
        if definition.is_struct() {
            cpp_doc(
                writer,
                &[format!(
                    "An array of `{name}`s, which reads each struct when it's asked for"
                )],
            );
            writer.line(format!("using {name}Array = ::tafla::StructArray<{name}>;"));
        } else {
            cpp_doc(
                writer,
                &[format!(
                    "A vector of `{name}`s, which reads each item when it's asked for"
                )],
            );
            writer.line(format!(
                "using {name}Vector = ::tafla::TableVector<{name}>;"
            ));
        }
    }
}

/// Whether each definition is a table that can hold itself, through vectors
/// and nested tables, which a vector of its own type, or of a table that
/// nests it, makes it
fn recursive_tables(schema: &Schema) -> Vec<bool> {
    let tables = |definition: &Definition| {
        definition
            .fields
            .iter()
            .filter_map(|field| match field.field_type {
                FieldType::Table(index) | FieldType::Vector(index) => Some(index),
                _ => None,
            })
            .collect::<Vec<_>>()
            .into_iter()
    };
    let components = strongly_connected_components(&schema.definitions, tables);

    // Counted once, so that a schema of many definitions takes linear time
    let mut sizes = vec![0_usize; schema.definitions.len()];
    for &component in &components {
        sizes[component] += 1;
    }

    (0..schema.definitions.len())
        .map(|index| {
            sizes[components[index]] > 1
                || tables(schema.definition(index)).any(|nested| nested == index)
        })
        .collect()
}

/// Whether each table's `size()` and `write()` are forced inline into their
/// parent's: those of a table that doesn't hold itself, whose code, with
/// that of the tables that it inlines in turn, writes at most
/// [`INLINE_FIELDS`] fields. The code of a function that's forced inline is
/// then at most that long, wherever it's inlined, and the code of the
/// others is that of their own fields, with that of the tables they inline,
/// so that the code of a schema grows linearly with its definitions.
fn inlined_tables(schema: &Schema, recursive: &[bool]) -> Vec<bool> {
    let nested = |index: usize| {
        schema
            .definition(index)
            .fields
            .iter()
            .filter_map(|field| match field.field_type {
                FieldType::Table(nested) | FieldType::Vector(nested) => Some(nested),
                _ => None,
            })
    };

    // The fields that each table's functions write, counted after those of
    // the tables nested in it, which, but for tables that hold themselves,
    // nest without cycles. A stack, rather than recursion, visits them,
    // since vectors may nest tables arbitrarily deep.
    let mut fields: Vec<Option<usize>> = vec![None; schema.definitions.len()];
    for root in 0..schema.definitions.len() {
        let mut stack = vec![(root, false)];
        while let Some((index, counted_nested)) = stack.pop() {
            if fields[index].is_some() || recursive[index] {
                continue;
            }
            if !counted_nested {
                stack.push((index, true));
                stack.extend(nested(index).map(|table| (table, false)));
                continue;
            }

            let written = schema
                .definition(index)
                .fields
                .iter()
                .map(|field| match field.field_type {
                    FieldType::Table(table) | FieldType::Vector(table) => match fields[table] {
                        Some(inlined) if inlined <= INLINE_FIELDS => inlined.saturating_add(1),
                        _ => 1,
                    },
                    _ => 1,
                })
                .fold(0, usize::saturating_add);
            fields[index] = Some(written);
        }
    }

    fields
        .iter()
        .map(|written| written.is_some_and(|written| written <= INLINE_FIELDS))
        .collect()
}

/// The definitions that `selected` selects, each after those that it holds
/// in place: the tables nested in a table, whose fields structs hold theirs,
/// and the structs in a struct. Otherwise they keep the schema's order.
/// Neither nests itself, and both nest at most 64 deep, which the checker
/// ensures, so the search ends soon.
fn nesting_order(schema: &Schema, selected: impl Fn(&Definition) -> bool) -> Vec<usize> {
    fn visit(schema: &Schema, index: usize, visited: &mut [bool], order: &mut Vec<usize>) {
        if visited[index] {
            return;
        }
        visited[index] = true;

        for field in &schema.definition(index).fields {
            if let FieldType::Table(nested) | FieldType::Struct(nested) = field.field_type {
                visit(schema, nested, visited, order);
            }
        }
        order.push(index);
    }

    let mut visited = vec![false; schema.definitions.len()];
    let mut order = Vec::new();
    for (index, definition) in schema.definitions.iter().enumerate() {
        if selected(definition) {
            visit(schema, index, &mut visited, &mut order);
        }
    }

    order.retain(|&index| selected(schema.definition(index)));
    order
}

/// The namespace of a schema's file name, or of one of `--package`'s names,
/// which takes a `_` for each run of characters other than ASCII letters and
/// digits, and starts with `schema_` when it doesn't start with a letter,
/// since C++ reserves names that start with `_` and have `__`
fn namespace_of(stem: &str) -> String {
    let mut name = String::with_capacity(stem.len());
    for character in stem.chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character);
        } else if !name.ends_with('_') {
            name.push('_');
        }
    }

    if !name.starts_with(|character: char| character.is_ascii_alphabetic()) {
        name = format!("schema_{}", name.trim_start_matches('_'));
    }
    namespace_name(&name)
}

/// One of a namespace's names, with a `_` after a keyword, a macro or a
/// namespace that the generated code names
fn namespace_name(name: &str) -> String {
    if NAMESPACES.contains(&name) {
        format!("{name}_")
    } else {
        type_name(&identifier(name))
    }
}

/// `declaration = value;`, broken after the `=` when it doesn't fit, as
/// clang-format breaks it
fn assignment(writer: &mut CodeWriter, declaration: &str, value: &str) {
    let line = format!("{declaration} = {value};");
    if writer.indentation() + line.len() <= WIDTH {
        writer.line(line);
        return;
    }

    writer.line(format!("{declaration} ="));
    writer.line(format!("{INDENT}{value};"));
}

/// A C++ expression of the message's layout
fn layout_expression(layout: Layout) -> String {
    let mut expression = "::tafla::TableLayout()".to_owned();
    if layout.sized {
        expression.push_str(".sized()");
    }
    if layout.version.is_some() {
        expression.push_str(".versioned()");
    }
    match layout.offset_type {
        OffsetType::Uint8 => expression.push_str(".with_offset_type(::tafla::OffsetType::uint8)"),
        OffsetType::Uint16 => expression.push_str(".with_offset_type(::tafla::OffsetType::uint16)"),
        OffsetType::Uint32 => {}
    }
    expression
}

/// `bytes` at `offset` from their start
fn at_offset(bytes: &str, offset: usize) -> String {
    if offset == 0 {
        bytes.to_owned()
    } else {
        format!("{bytes} + {offset}")
    }
}

/// A Doxygen comment of a declaration's documentation. Its text outside
/// Markdown's code spans escapes the characters that start Doxygen's commands
/// and HTML, and `*/` is written as an HTML entity so that it doesn't end the
/// comment. A block comment ends where it says, whatever its lines end with,
/// where a line comment that ends with `\` would continue on the next line.
fn cpp_doc(writer: &mut CodeWriter, docs: &[String]) {
    let lines: Vec<String> = docs
        .iter()
        .map(|line| comment_text(&escape_outside_code_spans(line, doxygen_text)))
        .collect();

    match lines.as_slice() {
        [] => {}
        [line] if writer.indentation() + line.chars().count() + 7 <= WIDTH => {
            writer.line(format!("/** {line} */"));
        }
        lines => {
            writer.line("/**");
            for line in lines {
                for wrapped in wrap(line, WIDTH - 3 - writer.indentation()) {
                    writer.line(format!(" * {wrapped}").trim_end());
                }
            }
            writer.line(" */");
        }
    }
}

/// Breaks a line of a comment at spaces into lines of at most `width`
/// characters, as clang-format reflows comments, but for words longer than
/// that
fn wrap(line: &str, width: usize) -> Vec<String> {
    if line.chars().count() <= width {
        return vec![line.to_owned()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    for word in line.split(' ') {
        let length = current.chars().count() + 1 + word.chars().count();
        if !current.is_empty() && length > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    lines.push(current);
    lines
}

/// Text that Doxygen reads as it's written: `\` and `@` start its commands,
/// and `<` and `&` its HTML
fn doxygen_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '\\' | '@' | '<' | '>' | '&') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn identifier(name: &str) -> String {
    if KEYWORDS.contains(&name) || MACROS.contains(&name) || MEMBERS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_owned()
    }
}

/// A definition's name, with a `_` after a macro's, which the names of its
/// fields struct and views start with too
fn type_name(name: &str) -> String {
    if TYPE_MACROS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_owned()
    }
}

/// "a `Name`" or "an `Name`"
fn article(name: &str) -> String {
    format!("{} `{name}`", indefinite_article(name))
}

const fn scalar_type(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::Bool => "bool",
        Scalar::Int8 => "std::int8_t",
        Scalar::Uint8 => "std::uint8_t",
        Scalar::Int16 => "std::int16_t",
        Scalar::Uint16 => "std::uint16_t",
        Scalar::Int32 => "std::int32_t",
        Scalar::Uint32 => "std::uint32_t",
        Scalar::Int64 => "std::int64_t",
        Scalar::Uint64 => "std::uint64_t",
        Scalar::Float32 => "float",
        Scalar::Float64 => "double",
    }
}

/// The runtime's `Alignment` of `bytes`
fn alignment(bytes: u8) -> &'static str {
    match bytes {
        1 => "::tafla::Alignment::one",
        2 => "::tafla::Alignment::two",
        4 => "::tafla::Alignment::four",
        _ => "::tafla::Alignment::eight",
    }
}
