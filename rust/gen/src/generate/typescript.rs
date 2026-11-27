//! TypeScript, and JavaScript with `JSDoc` types, for `tafla`. Messages
//! and tables are classes that read fields lazily from their bytes, and are
//! written from interfaces of their fields. Structs are read eagerly. The
//! fields whose sizes the schema fixes are written at positions that the
//! generator works out: nested tables of them whole, and messages of them,
//! and a string after them, without a sizer and a writer.

use super::fixed::{self, FixedSizes, NESTED_OFFSET_SIZE};
use super::prose::article;
use super::writer::{
    CodeWriter, Expr, Style, call, comment_text, doc_comment, escape_outside_code_spans, text,
};
use super::{GeneratedFile, Options};
use crate::model::{
    Definition, Enum, Field, FieldType, Kind, Layout, OffsetType, Scalar, ScalarValue, Schema,
    strongly_connected_components,
};
use crate::naming::{camel_case, pascal_case};

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

/// The longest nested table that the generated code writes whole, the
/// longest that `uint32` offsets address
const MAX_WHOLE_TABLE: usize = 0xffff_ffff;

/// The shortest run of structs, or of nested tables of fixed size, in bytes,
/// that the generated code writes through a `DataView` rather than with the
/// runtime's write functions. A `DataView` writes each scalar about 2 ns
/// faster, but takes about 56 ns to create over bytes outside V8's heap, and
/// 120 ns over bytes of up to 64 that V8 keeps on its heap until something
/// asks for their `ArrayBuffer`, which a table holding a run this long never
/// is. In Node 27, the two broke even at 8 structs of three float32s, 96
/// bytes, and between 4 and 8 tables of three float32s, 112 and 224 bytes.
const MIN_VIEW_RUN: usize = 128;

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
    let fixed = FixedSizes::new(schema, MAX_WHOLE_TABLE);
    let generator = Generator {
        schema,
        typed,
        reads: Reads::of(schema),
        runs: Runs::of(schema, &fixed),
        fixed,
    };
    let runtime = options.runtime.as_deref().unwrap_or(RUNTIME);

    let mut writer = CodeWriter::new(STYLE.indent);
    writer.line(format!("// {header}"));
    writer.blank();
    generator.imports(&mut writer, runtime);

    for enumeration in &schema.enums {
        writer.blank();
        generator.enumeration(&mut writer, enumeration);
    }

    for class in class_order(schema) {
        writer.blank();
        match class {
            Class::Definition(index) => {
                let definition = schema.definition(index);
                match definition.kind {
                    Kind::Message(layout) => generator.message(&mut writer, index, layout),
                    Kind::Table => generator.table(&mut writer, index),
                    Kind::Struct(_) => generator.structure(&mut writer, index),
                }
                writer.blank();
                generator.fields_type(&mut writer, definition);
            }
            Class::View(index) => {
                let definition = schema.definition(index);
                if matches!(definition.kind, Kind::Struct(_)) {
                    generator.struct_array_view(&mut writer, definition);
                } else {
                    generator.vector_view(&mut writer, index);
                }
            }
        }
    }

    let extension = if typed { "ts" } else { "js" };
    vec![GeneratedFile {
        path: format!("{}.{extension}", module_name(stem)),
        contents: writer.finish(),
    }]
}

/// The name of a schema's module, its file name in kebab-case, as TypeScript
/// and JavaScript name modules: with a `-` for each `_`, and its other
/// characters as they are, such as `point-cloud` of `point_cloud.v1.tafla`
fn module_name(stem: &str) -> String {
    stem.replace('_', "-")
}

/// A class of the generated module
#[derive(Clone, Copy)]
enum Class {
    /// A message's, table's or struct's, with the interface of its fields
    Definition(usize),
    /// The view of a vector of tables, or of an array of structs, that holds
    /// the definition
    View(usize),
}

impl Class {
    /// Its position among every possible class of the schema
    fn slot(self) -> usize {
        match self {
            Self::Definition(index) => 2 * index,
            Self::View(index) => 2 * index + 1,
        }
    }
}

/// The classes in the schema's order, except that each comes after the
/// classes that its code reads: a table after its nested tables and structs,
/// the views of its vectors and arrays of structs and the vectors' items, and
/// a view after its items.
///
/// A method that reads an exported class declared after its own, as
/// `new Pair()` in a getter of `Item` does, checks first that the class is
/// initialized. V8's optimizing compiler lets the receiver of a method
/// escape wherever the method can throw, as that check can, for the stack
/// trace's `getThis()`. So a getter of a nested table, even inlined into a
/// loop, kept its parent's reader on the heap, and summing a vector of 100
/// nested tables through `get(i).pair()` allocated 4 KB. V8 reads a class
/// declared before the method without a check.
///
/// Only vectors nest a table in itself, so a cycle of tables passes through
/// a view, which comes before its items. Its `get()` then checks its items'
/// class, and keeps the view, made once per vector, on the heap, rather than
/// the getters of the items that create it.
fn class_order(schema: &Schema) -> Vec<Class> {
    let definitions = &schema.definitions;
    let components = strongly_connected_components(definitions, tables);

    // The tables that a table of their own component holds in a vector,
    // whose views lie in a cycle
    let mut view_first = vec![false; definitions.len()];
    for (index, definition) in definitions.iter().enumerate() {
        for field in &definition.fields {
            if let FieldType::Vector(item) = field.field_type
                && components[item] == components[index]
            {
                view_first[item] = true;
            }
        }
    }

    // The classes that a class reads, which come before it. A table's static
    // `size()` and `write()` read the class of its vectors' items, which
    // comes before it unless both lie in a cycle, where only those static
    // methods check it. Without those edges in cycles, and the edges from
    // the views first to their items, the classes have no cycles.
    let read = |class: Class| -> std::vec::IntoIter<Class> {
        let mut classes = Vec::new();
        match class {
            Class::Definition(index) => {
                for field in &definitions[index].fields {
                    match field.field_type {
                        FieldType::Table(nested) | FieldType::Struct(nested) if nested != index => {
                            classes.push(Class::Definition(nested));
                        }
                        FieldType::Vector(item) => {
                            classes.push(Class::View(item));
                            if components[item] != components[index] {
                                classes.push(Class::Definition(item));
                            }
                        }
                        FieldType::StructArray(item) => classes.push(Class::View(item)),
                        _ => {}
                    }
                }
            }
            Class::View(index) if !view_first[index] => classes.push(Class::Definition(index)),
            Class::View(_) => {}
        }
        classes.into_iter()
    };

    // Each class is visited once, from a stack rather than by recursion, so
    // that long chains of them don't overflow it
    let mut order = Vec::with_capacity(2 * definitions.len());
    let mut visited = vec![false; 2 * definitions.len()];
    let mut stack = Vec::new();
    let roots = (0..definitions.len()).flat_map(|index| {
        let view = schema.is_item(index).then_some(Class::View(index));
        std::iter::once(Class::Definition(index)).chain(view)
    });

    for root in roots {
        if std::mem::replace(&mut visited[root.slot()], true) {
            continue;
        }
        stack.push((root, read(root)));

        while let Some((class, unvisited)) = stack.last_mut() {
            let class = *class;
            if let Some(next) = unvisited.find(|next| !visited[next.slot()]) {
                visited[next.slot()] = true;
                stack.push((next, read(next)));
            } else {
                order.push(class);
                stack.pop();
            }
        }
    }

    order
}

/// The tables that a definition holds, nested or in vectors
fn tables(definition: &Definition) -> impl Iterator<Item = usize> + '_ {
    definition
        .fields
        .iter()
        .filter_map(|field| match field.field_type {
            FieldType::Table(index) | FieldType::Vector(index) => Some(index),
            _ => None,
        })
}

struct Generator<'a> {
    schema: &'a Schema,
    /// TypeScript, rather than JavaScript with `JSDoc`
    typed: bool,
    reads: Reads,
    /// The byte length of each nested table whose fields' sizes the schema
    /// fixes, which the generated code writes whole
    fixed: FixedSizes,
    runs: Runs,
}

/// Which structs and nested tables the generated code writes in runs, one
/// after another: those that arrays of structs and vectors of tables of
/// fixed size hold, and the structs and tables in them
struct Runs {
    /// Items of arrays and vectors, which get `writeItemsAt()`
    items: Vec<bool>,
    /// Items and what they hold, which get `writeView()`
    viewed: Vec<bool>,
}

impl Runs {
    fn of(schema: &Schema, fixed: &FixedSizes) -> Self {
        let mut items = vec![false; schema.definitions.len()];
        for field in schema
            .definitions
            .iter()
            .flat_map(|definition| &definition.fields)
        {
            match field.field_type {
                FieldType::StructArray(index) => items[index] = true,
                FieldType::Vector(index) if fixed.table(index).is_some() => items[index] = true,
                _ => {}
            }
        }

        // Structs and tables of fixed size hold neither themselves nor
        // anything but structs and such tables, and each is visited once
        let mut viewed = items.clone();
        let mut pending: Vec<usize> = (0..items.len()).filter(|&index| items[index]).collect();
        while let Some(index) = pending.pop() {
            for field in &schema.definition(index).fields {
                if let FieldType::Struct(nested) | FieldType::Table(nested) = field.field_type
                    && !viewed[nested]
                {
                    viewed[nested] = true;
                    pending.push(nested);
                }
            }
        }

        Self { items, viewed }
    }
}

/// How the generated code writes a message whole, at constant positions,
/// without a sizer and a writer: one of fields whose sizes the schema fixes,
/// and a string after them, which fits in its offset type
struct WholeMessage {
    /// The width of its header slots, offsets and string length
    width: usize,
    /// The position of its first offset, after its header
    header: usize,
    /// The position of each field, a string's length's included
    positions: Vec<usize>,
    /// Whether its last field is a string, whose bytes end the table
    string: bool,
    /// Its byte length, but for the string's bytes
    length: usize,
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

        // A field that may be missing is read after a check, by its getter
        let mut result = !schema.definition(index).has_missing_fields();
        if !result {
            fixed[index] = Some(false);
            return false;
        }

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

        let mut result = !schema.definition(index).has_missing_fields();
        if !result {
            whole[index] = Some(false);
            return false;
        }

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

        // Messages written field by field make sizers, messages written
        // whole make bytes with TableWriter's static functions, and tables
        // take sizers and writers
        let wholes = self.whole_messages();
        let has_whole = wholes.iter().any(Option::is_some);
        let has_sized = self
            .schema
            .definitions
            .iter()
            .zip(&wholes)
            .any(|(definition, whole)| {
                matches!(definition.kind, Kind::Message(_)) && whole.is_none()
            });
        let has_whole_string = wholes.iter().flatten().any(|whole| whole.string);
        let has_runs = self.runs.items.iter().any(|&item| item);

        let readers = self.read_functions();
        let writers = self.write_functions(&wholes);

        // The names are listed in the order that the TypeScript project's
        // oxlint configuration checks: functions, then classes, then types,
        // each in natural order, such as `writeUint8` before `writeUint16`
        let mut values: Vec<String> = readers;
        if has_whole_string {
            values.push("utf8Length".to_owned());
        }
        if has_runs {
            values.push("viewOf".to_owned());
        }
        values.extend(writers);
        if has_whole_string {
            values.push("writeUtf8".to_owned());
        }

        // (class, whether it's only a type here)
        let mut classes = Vec::new();
        if versioned {
            classes.push(("DecodeError", false));
        }
        if has_struct {
            classes.push(("StructWriter", false));
        }
        if has_message {
            classes.push(("TableLayout", false));
        }
        if has_table {
            classes.push(("TableReader", !has_message));
        }
        if has_sized || has_nested {
            classes.push(("TableSizer", !has_sized));
        }
        if has_whole || has_nested {
            classes.push(("TableWriter", !has_whole));
        }

        let (types, classes): (Vec<_>, Vec<_>) =
            classes.into_iter().partition(|&(_, type_only)| type_only);
        values.extend(classes.into_iter().map(|(name, _)| name.to_owned()));
        let mut types: Vec<&str> = types.into_iter().map(|(name, _)| name).collect();
        // TypeScript imports types with the values, and JavaScript in JSDoc
        if self.typed {
            values.extend(types.drain(..).map(|name| format!("type {name}")));
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

    /// The functions that read the scalars of structs, such as
    /// `readUint32`, in the order that the runtime exports them
    fn read_functions(&self) -> Vec<String> {
        // A bool is read and written as the uint8 that stores it
        let stored: Vec<Scalar> = self
            .schema
            .definitions
            .iter()
            .filter(|definition| definition.is_struct())
            .flat_map(|definition| &definition.fields)
            .filter_map(|field| match field.field_type {
                FieldType::Scalar(scalar) => Some(scalar.stored()),
                _ => None,
            })
            .collect();

        scalar_functions("read", &stored)
    }

    /// The write functions of the scalars that structs, nested tables of
    /// fixed size and messages written whole hold, such as `writeUint32`, in
    /// the order that the runtime exports them. Their offsets are written as
    /// bytes.
    fn write_functions(&self, wholes: &[Option<WholeMessage>]) -> Vec<String> {
        let stored: Vec<Scalar> = self
            .schema
            .definitions
            .iter()
            .enumerate()
            .filter(|&(index, definition)| match definition.kind {
                Kind::Struct(_) => true,
                Kind::Table => self.fixed.table(index).is_some(),
                Kind::Message(_) => wholes[index].is_some(),
            })
            .flat_map(|(_, definition)| &definition.fields)
            .filter_map(|field| match field.field_type {
                FieldType::Scalar(scalar) => Some(scalar.stored()),
                _ => None,
            })
            .collect();

        scalar_functions("write", &stored)
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
        if let Some(since) = layout.since {
            writer.lines(&doc_comment(&[
                "The first version of the format that the constructor reads, which".to_owned(),
                "returns `undefined` for the fields that later versions added".to_owned(),
            ]));
            writer.line(format!(
                "{}FIRST_VERSION = {since};",
                self.static_readonly()
            ));
            writer.blank();
        }

        writer.statement(
            &STYLE,
            &format!("{}#LAYOUT = ", self.static_readonly()),
            &layout_expression(layout, name),
            ";",
        );
        writer.blank();
        self.private_field(writer, "reader", "TableReader");
        if layout.since.is_some() {
            self.private_field(writer, "version", "number");
        }
        writer.blank();

        self.constructor(writer, name, layout);
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

    /// A message's constructor, which checks a versioned message's version:
    /// that it's the message's own, or within the versions that it reads
    /// when it has a first version, which it then holds for the getters of
    /// the fields that later versions added
    fn constructor(&self, writer: &mut CodeWriter, name: &str, layout: Layout) {
        let mut tags = vec!["@param {Uint8Array} bytes".to_owned()];
        if layout.since.is_some() {
            tags.push("@throws {DecodeError} If `bytes` are in a format version before".to_owned());
            tags.push("`FIRST_VERSION` or after `FORMAT_VERSION`.".to_owned());
        } else if layout.version.is_some() {
            tags.push("@throws {DecodeError} If `bytes` are in another format version.".to_owned());
        }
        self.doc(writer, &[], &tags);
        writer.open(format!(
            "constructor(bytes{}) {{",
            self.annotation("Uint8Array")
        ));
        writer.statement(
            &STYLE,
            "this._reader = ",
            &call(
                "new TableReader",
                vec![text("bytes"), text(format!("{name}.#LAYOUT"))],
            ),
            ";",
        );
        if layout.since.is_some() {
            writer.blank();
            writer.line("const version = this._reader.getVersion();");
            writer.open(format!(
                "if (version < {name}.FIRST_VERSION || version > {name}.FORMAT_VERSION) {{"
            ));
            let message = format!(
                "`Expected format versions ${{{name}.FIRST_VERSION}} to ${{{name}.FORMAT_VERSION}} of {name}, got ${{version}}`"
            );
            writer.statement(
                &STYLE,
                "throw ",
                &call("new DecodeError", vec![text(message)]),
                ";",
            );
            writer.close("}");
            writer.line("this._version = version;");
        } else if layout.version.is_some() {
            writer.blank();
            writer.line("const version = this._reader.getVersion();");
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

        if let Some(whole) = self.whole_message(definition, layout) {
            self.whole_message_writes(writer, definition, &whole, output);
            writer.close("}");
            return;
        }

        let arguments = vec![
            text(definition.fields.len().to_string()),
            text(format!("{name}.#LAYOUT")),
        ];
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

    /// How the generated code writes each message whole, by each
    /// definition's index, and `None` for every other definition
    fn whole_messages(&self) -> Vec<Option<WholeMessage>> {
        self.schema
            .definitions
            .iter()
            .map(|definition| match definition.kind {
                Kind::Message(layout) => self.whole_message(definition, layout),
                _ => None,
            })
            .collect()
    }

    /// How the generated code writes a message whole, when its fields'
    /// sizes are all fixed, but for a string after them, and they fit in
    /// its offset type
    fn whole_message(&self, definition: &Definition, layout: Layout) -> Option<WholeMessage> {
        // A field that may be left out has no position of its own
        if definition.fields.iter().any(|field| field.optional) {
            return None;
        }

        let width = fixed::offset_size(layout.offset_type);
        let header = (usize::from(layout.sized) + usize::from(layout.version.is_some())) * width;
        let (string, fixed_fields) = match definition.fields.split_last() {
            Some((last, fixed_fields)) if last.field_type == FieldType::String => {
                (true, fixed_fields)
            }
            _ => (false, definition.fields.as_slice()),
        };

        let mut positions = Vec::with_capacity(definition.fields.len());
        let mut position = header + definition.fields.len() * width;
        for field in fixed_fields {
            positions.push(position);
            position += self.fixed.field(self.schema, field.field_type)?;
        }
        if string {
            positions.push(position);
            position += width;
        }

        let max = usize::try_from(layout.offset_type.max_value()).unwrap_or(usize::MAX);
        (position <= max).then_some(WholeMessage {
            width,
            header,
            positions,
            string,
            length: position,
        })
    }

    /// The body of a message's serializer that writes it whole: into the
    /// bytes that the runtime's `TableWriter.tableBytes()` returns with the
    /// header, its offsets and fields at constant positions, and then the
    /// string after them. The string is read once, as its length must be
    /// the length of what's written.
    fn whole_message_writes(
        &self,
        writer: &mut CodeWriter,
        definition: &Definition,
        whole: &WholeMessage,
        output: Output,
    ) {
        let name = &definition.name;
        // Named after the field's index, since its name may be a reserved
        // word
        let string = whole.string.then(|| {
            let index = definition.fields.len() - 1;
            let field = &definition.fields[index];
            let local = format!("string{index}");
            writer.line(format!(
                "const {local} = fields.{};",
                identifier(&field.name)
            ));
            local
        });

        let mut length = whole.length.to_string();
        if let Some(string) = &string {
            length = format!("{length} + utf8Length({string})");
        }
        let mut arguments = vec![
            text(definition.fields.len().to_string()),
            text(format!("{name}.#LAYOUT")),
        ];
        arguments.push(text(length));
        if let Output::Into = output {
            arguments.push(text("bytes"));
        }
        writer.statement(
            &STYLE,
            "const table = ",
            &call("TableWriter.tableBytes", arguments),
            ";",
        );
        writer.blank();

        for (index, &position) in whole.positions.iter().enumerate() {
            let slot = whole.header + index * whole.width;
            slot_stores(
                writer,
                "table",
                None,
                slot,
                whole.width,
                &Slot::Constant(position),
                output,
            );
        }
        writer.blank();
        for (index, (field, &position)) in
            definition.fields.iter().zip(&whole.positions).enumerate()
        {
            match &string {
                Some(string) if field.field_type == FieldType::String => {
                    // The string's length is what's written of it
                    let length = format!("length{index}");
                    let bytes = text((position + whole.width).to_string());
                    writer.statement(
                        &STYLE,
                        &format!("const {length} = "),
                        &call("writeUtf8", vec![text("table"), bytes, text(string)]),
                        ";",
                    );
                    slot_stores(
                        writer,
                        "table",
                        None,
                        position,
                        whole.width,
                        &Slot::Local(&length),
                        output,
                    );
                }
                _ => {
                    let write = self.fixed_write(field, "table", text(position.to_string()));
                    writer.statement(&STYLE, "", &write, ";");
                }
            }
        }

        writer.blank();
        writer.line("return table;");
    }

    /// The write of a field whose size the schema fixes at `position` in
    /// `bytes`: a scalar's with the runtime's write function, and a struct's
    /// or a nested table's with its `writeAt()`
    fn fixed_write(&self, field: &Field, bytes: &str, position: Expr) -> Expr {
        let value = format!("fields.{}", identifier(&field.name));
        match field.field_type {
            FieldType::Scalar(scalar) => call(
                format!("write{}", scalar_method(scalar)),
                vec![text(bytes), position, stored_value(scalar, &value)],
            ),
            FieldType::Struct(index) | FieldType::Table(index) => call(
                format!("{}.writeAt", self.schema.definition(index).name),
                vec![text(bytes), position, text(value)],
            ),
            _ => unreachable!("fields of fixed size are scalars, structs and such tables"),
        }
    }

    /// The write of a field whose size the schema fixes at `position` in a
    /// `DataView`, `view`, as [`Generator::fixed_write`] writes it into bytes
    fn fixed_view_write(&self, field: &Field, position: Expr) -> Expr {
        let value = format!("fields.{}", identifier(&field.name));
        match field.field_type {
            FieldType::Scalar(scalar) => {
                let mut arguments = vec![position, stored_value(scalar, &value)];
                // A byte has no byte order
                if scalar.size() > 1 {
                    arguments.push(text("true"));
                }
                call(format!("view.set{}", data_view_method(scalar)), arguments)
            }
            FieldType::Struct(index) | FieldType::Table(index) => call(
                format!("{}.writeView", self.schema.definition(index).name),
                vec![text("view"), position, text(value)],
            ),
            _ => unreachable!("fields of fixed size are scalars, structs and such tables"),
        }
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
        if let Some(size) = self.fixed.table(index) {
            self.doc(
                writer,
                &[
                    format!(
                        "The byte length of {}: its offsets and its fields, whose sizes",
                        article(name)
                    ),
                    "the schema fixes, so that its parent writes it whole".to_owned(),
                ],
                &[],
            );
            writer.line(format!("{}SIZE = {size};", self.static_readonly()));
            writer.blank();
        }
        self.private_field(writer, "reader", "TableReader");
        self.private_field(writer, "position", "number");
        writer.blank();

        self.doc(
            writer,
            &["Reads the table at `position`, as its parent's getter finds it".to_owned()],
            &[
                "@param {TableReader} reader".to_owned(),
                "@param {number} position".to_owned(),
            ],
        );
        writer.open(format!(
            "constructor(reader{}, position{}) {{",
            self.annotation("TableReader"),
            self.annotation("number")
        ));
        writer.line("this._reader = reader;");
        writer.line("this._position = position;");
        writer.close("}");
        writer.blank();

        let whole = self.reads.whole[index];
        if whole {
            self.static_read(writer, definition);
            writer.blank();
        }
        if self.fixed.table(index).is_some() {
            self.whole_table(writer, index);
        } else {
            self.nested_size_and_write(writer, definition);
        }
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
            writer.line(format!("return {name}.read(this._reader, this._position);"));
        }
        writer.close("}");
    }

    /// A nested table's static `read()`, which reads every field at once
    /// without creating an instance of it, for its parent's `fields()` and a
    /// view's
    fn static_read(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;

        self.doc(
            writer,
            &[
                "Reads every field of the table at `position`, as `fields()` does,".to_owned(),
                format!("without creating {}", article(name)),
            ],
            &[
                "@param {TableReader} reader".to_owned(),
                "@param {number} position".to_owned(),
                format!("@returns {{{name}Fields}}"),
            ],
        );
        self.signature(
            writer,
            "static read",
            &[
                ("reader", "TableReader".to_owned()),
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
        let reader = match at {
            TableAt::Root => "this._reader",
            TableAt::Position => "reader",
        };

        definition
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let offset = match at {
                    TableAt::Root => call(
                        format!("{reader}.rootOffset"),
                        vec![text(index.to_string())],
                    ),
                    TableAt::Position => call(
                        format!("{reader}.fieldOffset"),
                        vec![text("position"), text(index.to_string())],
                    ),
                };
                let mut suffix = "";
                let read = match field.field_type {
                    FieldType::Scalar(scalar) => call(
                        format!("{reader}.get{}", scalar_method(scalar)),
                        vec![offset],
                    ),
                    FieldType::Array(scalar) => call(
                        format!("{reader}.get{}Array", scalar_method(scalar)),
                        vec![offset],
                    ),
                    FieldType::String => call(format!("{reader}.getString"), vec![offset]),
                    FieldType::Struct(index) => call(
                        format!("{}.read", self.schema.definition(index).name),
                        vec![text(format!("{reader}.bytes()")), offset],
                    ),
                    FieldType::Table(index) => call(
                        format!("{}.read", self.schema.definition(index).name),
                        vec![text(reader), offset],
                    ),
                    // The view's fields(), which reads every item
                    FieldType::StructArray(_) | FieldType::Vector(_) => {
                        suffix = ".fields()";
                        let view = value_type(field, self.schema);
                        call(format!("new {view}"), vec![text(reader), offset])
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
            field.optional
                || !matches!(
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

    /// The static functions with which a parent sizes and writes a nested
    /// table whose fields' sizes the schema fixes, as the `SIZE` bytes of its
    /// offsets and its fields, which `writeAt()` writes whole
    fn whole_table(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
        let name = &definition.name;

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
                format!("@param {{{name}Fields}} _fields"),
                "@returns {void}".to_owned(),
            ],
        );
        writer.open(format!(
            "static size(sizer{}, _fields{}){} {{",
            self.annotation("TableSizer"),
            self.annotation(&format!("{name}Fields")),
            self.annotation("void")
        ));
        writer.line(format!("sizer.addTableBytes(1, {name}.SIZE);"));
        writer.close("}");
        writer.blank();

        self.doc(
            writer,
            &[
                format!(
                    "Writes {} where its parent's writer is, after the",
                    article(name)
                ),
                "field that holds it".to_owned(),
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
        let reserve = call(
            "writer.reserveTableBytes",
            vec![text(format!("{name}.SIZE"))],
        );
        writer.statement(
            &STYLE,
            "",
            &call(
                format!("{name}.writeAt"),
                vec![text("writer.target()"), reserve, text("fields")],
            ),
            ";",
        );
        writer.close("}");
        writer.blank();

        self.whole_table_write_at(writer, definition);
        if self.runs.viewed[index] {
            writer.blank();
            self.write_view(writer, definition);
        }
        if self.runs.items[index] {
            writer.blank();
            self.write_items_at(writer, definition);
        }
    }

    /// A nested table's `writeAt()`, which writes its offsets, and then its
    /// fields at them, one after another, as a writer would
    fn whole_table_write_at(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;

        self.doc(
            writer,
            &[
                format!(
                    "Writes {} at `offset` in `bytes`, as its parent holds it: its",
                    article(name)
                ),
                "offsets, and then its fields at them".to_owned(),
            ],
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
        for (slot, position) in self.whole_table_offsets(definition) {
            slot_stores(
                writer,
                "bytes",
                Some("offset"),
                slot,
                NESTED_OFFSET_SIZE,
                &Slot::Constant(position),
                Output::Into,
            );
        }
        writer.blank();
        for (field, (_, position)) in definition
            .fields
            .iter()
            .zip(self.whole_table_offsets(definition))
        {
            let write = self.fixed_write(field, "bytes", struct_position("offset", position));
            writer.statement(&STYLE, "", &write, ";");
        }
        writer.close("}");
    }

    /// The position of each offset of a nested table of fixed size, and the
    /// offset that it holds, from the table's start
    fn whole_table_offsets(&self, definition: &Definition) -> Vec<(usize, usize)> {
        let mut position = definition.fields.len() * NESTED_OFFSET_SIZE;
        let mut offsets = Vec::with_capacity(definition.fields.len());
        for (index, field) in definition.fields.iter().enumerate() {
            offsets.push((index * NESTED_OFFSET_SIZE, position));
            position += self
                .fixed
                .field(self.schema, field.field_type)
                .expect("a whole table's fields have fixed sizes");
        }

        offsets
    }

    /// The `writeView()` of a struct or a nested table of fixed size, which
    /// writes it through a `DataView` as `writeAt()` writes it into bytes, so
    /// that `writeItemsAt()` can write a long run of them through one
    fn write_view(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;

        self.doc(
            writer,
            &[
                format!(
                    "Writes {} at `offset` in `view`, as `writeAt()` writes it",
                    article(name)
                ),
                "into bytes".to_owned(),
            ],
            &[
                "@param {DataView} view".to_owned(),
                "@param {number} offset".to_owned(),
                format!("@param {{{name}Fields}} fields"),
                "@returns {void}".to_owned(),
                format!("@throws {{RangeError}} If `view` ends before the `{name}` does."),
            ],
        );
        self.signature(
            writer,
            "static writeView",
            &[
                ("view", "DataView".to_owned()),
                ("offset", "number".to_owned()),
                ("fields", format!("{name}Fields")),
            ],
            "void",
        );
        let mut statements = Vec::new();
        if definition.is_struct() {
            let mut offset = 0;
            for field in &definition.fields {
                statements.push(self.fixed_view_write(field, struct_position("offset", offset)));
                offset += self
                    .fixed
                    .field(self.schema, field.field_type)
                    .expect("structs hold scalars and structs only");
            }
        } else {
            let offsets = self.whole_table_offsets(definition);
            for &(slot, position) in &offsets {
                statements.push(call(
                    "view.setUint32",
                    vec![
                        struct_position("offset", slot),
                        text(position.to_string()),
                        text("true"),
                    ],
                ));
            }
            for (field, (_, position)) in definition.fields.iter().zip(offsets) {
                statements.push(self.fixed_view_write(field, struct_position("offset", position)));
            }
        }
        for statement in statements {
            writer.statement(&STYLE, "", &statement, ";");
        }
        writer.close("}");
    }

    /// The `writeItemsAt()` of a struct or a nested table of fixed size that
    /// an array or a vector holds, which writes the items one after another:
    /// a short run with `writeAt()`, and a long one through a `DataView` with
    /// `writeView()`
    fn write_items_at(&self, writer: &mut CodeWriter, definition: &Definition) {
        let name = &definition.name;
        let holder = if definition.is_struct() {
            "an array"
        } else {
            "a vector"
        };

        self.doc(
            writer,
            &[
                format!("Writes `{name}`s one after another at `offset` in `bytes`, as {holder}"),
                format!("of them holds them. A run shorter than {MIN_VIEW_RUN} bytes is written with the"),
                "runtime's write functions, and a longer one through a `DataView`, which"
                    .to_owned(),
                "writes each scalar faster but takes longer to create.".to_owned(),
            ],
            &[
                "@param {Uint8Array} bytes".to_owned(),
                "@param {number} offset".to_owned(),
                format!("@param {{readonly {name}Fields[]}} items"),
                "@returns {void}".to_owned(),
                format!("@throws {{RangeError}} If `bytes` end before the last `{name}` does."),
            ],
        );
        self.signature(
            writer,
            "static writeItemsAt",
            &[
                ("bytes", "Uint8Array".to_owned()),
                ("offset", "number".to_owned()),
                ("items", format!("readonly {name}Fields[]")),
            ],
            "void",
        );
        writer.line("const count = items.length;");
        writer.line(format!("const length = count * {name}.SIZE;"));
        writer.open(format!("if (length < {MIN_VIEW_RUN}) {{"));
        writer.open("for (let i = 0; i < count; i++) {");
        writer.statement(
            &STYLE,
            "",
            &call(
                format!("{name}.writeAt"),
                vec![
                    text("bytes"),
                    text(format!("offset + i * {name}.SIZE")),
                    text("items[i]"),
                ],
            ),
            ";",
        );
        writer.close("}");
        writer.blank();
        writer.line("return;");
        writer.close("}");
        writer.blank();
        writer.line("const view = viewOf(bytes, offset, length);");
        writer.open("for (let i = 0; i < count; i++) {");
        writer.statement(
            &STYLE,
            "",
            &call(
                format!("{name}.writeView"),
                vec![
                    text("view"),
                    text(format!("i * {name}.SIZE")),
                    text("items[i]"),
                ],
            ),
            ";",
        );
        writer.close("}");
        writer.close("}");
    }

    #[allow(clippy::too_many_lines)]
    fn structure(&self, writer: &mut CodeWriter, index: usize) {
        let definition = self.schema.definition(index);
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
                &value_type(field, self.schema),
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
        // TableReader and its DataView, which took five times as long
        for Read {
            field,
            expression: read,
            ..
        } in self.struct_reads(definition, StructRead::Reader, "bytes", "offset")
        {
            let target = format!("this._{} = ", identifier(&field.name));
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
            let write = self.fixed_write(field, "bytes", struct_position("offset", offset));
            writer.statement(&STYLE, "", &write, ";");
            offset += self
                .fixed
                .field(self.schema, field.field_type)
                .expect("structs hold scalars and structs only");
        }
        writer.close("}");

        if self.runs.viewed[index] {
            writer.blank();
            self.write_view(writer, definition);
        }
        if self.runs.items[index] {
            writer.blank();
            self.write_items_at(writer, definition);
        }

        for field in &definition.fields {
            let field_name = identifier(&field.name);
            writer.blank();
            self.getter_doc(
                writer,
                &schema_docs(&field.docs),
                &value_type(field, self.schema),
            );
            writer.open(format!(
                "{field_name}(){} {{",
                self.annotation(&value_type(field, self.schema))
            ));
            writer.line(format!("return this._{field_name};"));
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
        self.private_field(writer, "reader", "TableReader");
        self.private_field(writer, "position", "number");
        self.private_field(writer, "length", "number");
        writer.blank();

        self.doc(
            writer,
            &["Views the vector at `position`, as its parent's getter finds it".to_owned()],
            &[
                "@param {TableReader} reader".to_owned(),
                "@param {number} position".to_owned(),
                "@throws {DecodeError} If the vector's count or the ends of its items lie"
                    .to_owned(),
                "outside the bytes.".to_owned(),
            ],
        );
        writer.open(format!(
            "constructor(reader{}, position{}) {{",
            self.annotation("TableReader"),
            self.annotation("number")
        ));
        writer.line("this._reader = reader;");
        writer.line("this._position = position;");
        writer.line("this._length = reader.getVectorLength(position);");
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
            "this._reader.getVectorItemOffset",
            vec![text("this._position"), text("this._length"), text("index")],
        );
        writer.statement(
            &STYLE,
            "return ",
            &call(format!("new {table}"), vec![text("this._reader"), offset]),
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
                "this._reader.getVectorOffsets",
                vec![text("this._position")],
            ),
            ";",
        );
        writer.statement(
            &STYLE,
            "const items = ",
            &self.new_array(table, "offsets.length"),
            ";",
        );
        writer.blank();
        writer.open("for (let i = 0; i < offsets.length; i++) {");
        writer.statement(
            &STYLE,
            "items[i] = ",
            &call(
                format!("{table}.read"),
                vec![text("this._reader"), text("offsets[i]")],
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
    fn new_array(&self, item: &str, length: &str) -> Expr {
        if self.typed {
            text(format!("new Array<{item}Fields>({length})"))
        } else {
            // A JSDoc cast types the array, which would otherwise be `any[]`.
            // Its expression takes the parentheses that the cast needs, and
            // breaks within them as a call's lone argument does.
            call(
                format!("/** @type {{{item}Fields[]}} */ "),
                vec![text(format!("new Array({length})"))],
            )
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
                "@param {TableReader} reader".to_owned(),
                "@param {number} position".to_owned(),
                "@throws {DecodeError} If the array lies outside the bytes.".to_owned(),
            ],
        );
        writer.open(format!(
            "constructor(reader{}, position{}) {{",
            self.annotation("TableReader"),
            self.annotation("number")
        ));
        writer.statement(
            &STYLE,
            "this._bytes = ",
            &call(
                "reader.getStructArray",
                vec![text("position"), text(format!("{structure}.SIZE"))],
            ),
            ";",
        );
        writer.line(format!(
            "this._length = this._bytes.length / {structure}.SIZE;"
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
                vec![text("this._bytes"), text(&offset)],
            ),
            ";",
        );
        writer.close("}");

        let reads = self.struct_reads(definition, StructRead::Reader, "this._bytes", &offset);
        for Read {
            field,
            expression: read,
            ..
        } in reads
        {
            let return_type = value_type(field, self.schema);
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
        // than its read() of this._bytes, which took a quarter longer
        writer.line("const bytes = this._bytes;");
        writer.statement(
            &STYLE,
            "const items = ",
            &self.new_array(structure, "this._length"),
            ";",
        );
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
        writer.line("return this._length;");
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
                // `undefined` too, so that a getter's result passes through
                // where `exactOptionalPropertyTypes` holds
                let input = input_type(field, self.schema);
                writer.line(if field.optional {
                    format!("{}?: {input} | undefined;", identifier(&field.name))
                } else {
                    format!("{}: {input};", identifier(&field.name))
                });
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
                let name = identifier(&field.name);
                let property = format!(
                    "@property {{{}}} {} {description}",
                    input_type(field, self.schema),
                    if field.optional {
                        format!("[{name}]")
                    } else {
                        name
                    }
                );
                lines.push(property.trim_end().to_owned());
            }
            writer.lines(&doc_comment(&lines));
        }
    }

    /// An enum: an object of its values by their names in `PascalCase`, and
    /// a type of the same name, an alias of its integer type, since a field
    /// of the enum holds any integer of that type. The object is frozen, so
    /// that its values are constants, of literal types.
    fn enumeration(&self, writer: &mut CodeWriter, enumeration: &Enum) {
        let name = &enumeration.name;
        let underlying = enumeration.underlying;
        let is_big = scalar_type(underlying) == "bigint";

        writer.lines(&doc_comment(&schema_docs(&enumeration.docs)));
        writer.open(format!("export const {name} = globalThis.Object.freeze({{"));
        for (index, value) in enumeration.values.iter().enumerate() {
            let last = index + 1 == enumeration.values.len();
            writer.lines(&doc_comment(&schema_docs(&value.docs)));
            writer.line(format!(
                "{}: {}{}{}",
                pascal_case(&value.name),
                value.value,
                if is_big { "n" } else { "" },
                if last { "" } else { "," }
            ));
        }
        writer.close("});");

        let summary = format!(
            "A value of {{@link {name}}}: one that it names, or any other `{}`",
            underlying.name()
        );
        writer.blank();
        if self.typed {
            writer.lines(&doc_comment(&[summary]));
            writer.line(format!("export type {name} = {};", scalar_type(underlying)));
        } else {
            let typedef = format!("@typedef {{{}}} {name}", scalar_type(underlying));
            writer.lines(&doc_comment(&[summary, String::new(), typedef]));
        }
    }

    /// The calls that add a table's fields to its sizer, in order
    fn sizes(&self, writer: &mut CodeWriter, definition: &Definition) {
        for field in &definition.fields {
            if !field.optional {
                self.size(writer, field);
                continue;
            }

            // An absent field adds nothing: its offset is counted with the
            // table's others
            writer.open(format!(
                "if (fields.{} != null) {{",
                identifier(&field.name)
            ));
            self.size(writer, field);
            writer.close("}");
        }
    }

    /// The calls that add a field to its table's sizer
    fn size(&self, writer: &mut CodeWriter, field: &Field) {
        {
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
                    return;
                }
                FieldType::Vector(index) => {
                    let table = &self.schema.definition(index).name;
                    writer.line(format!("sizer.addVector({value}.length);"));
                    // Items of the same length are added at once
                    if self.fixed.table(index).is_some() {
                        let arguments = vec![
                            text(format!("{value}.length")),
                            text(format!("{table}.SIZE")),
                        ];
                        writer.statement(&STYLE, "", &call("sizer.addTableBytes", arguments), ";");
                        return;
                    }
                    writer.open(format!("for (const item of {value}) {{"));
                    writer.line(format!("{table}.size(sizer, item);"));
                    writer.close("}");
                    return;
                }
            };
            writer.statement(&STYLE, "", &call(format!("sizer.{method}"), arguments), ";");
        }
    }

    /// The calls that write a table's fields, in order
    fn writes(&self, writer: &mut CodeWriter, definition: &Definition) {
        for (index, field) in definition.fields.iter().enumerate() {
            if !field.optional {
                self.write(writer, index, field);
                continue;
            }

            writer.open(format!(
                "if (fields.{} == null) {{",
                identifier(&field.name)
            ));
            writer.line(format!("writer.setAbsent({index});"));
            writer.close("} else {");
            writer.indent();
            self.write(writer, index, field);
            writer.close("}");
        }
    }

    /// The calls that write field `index` of a table
    fn write(&self, writer: &mut CodeWriter, index: usize, field: &Field) {
        {
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
                    // Written into the table's bytes where the writer
                    // reserves them, rather than into bytes of their own
                    let structure = &self.schema.definition(struct_index).name;
                    let reserve = call(
                        "writer.reserveStructArray",
                        vec![
                            position,
                            text(format!("{value}.length")),
                            text(format!("{structure}.SIZE")),
                            text(format!("{structure}.ALIGNMENT")),
                        ],
                    );
                    items_writes(writer, index, structure, &reserve, &value);
                    return;
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
                    return;
                }
                FieldType::Vector(table_index) if self.fixed.table(table_index).is_some() => {
                    let table = &self.schema.definition(table_index).name;
                    let reserve = call(
                        "writer.reserveTableVector",
                        vec![
                            position,
                            text(format!("{value}.length")),
                            text(format!("{table}.SIZE")),
                        ],
                    );
                    items_writes(writer, index, table, &reserve, &value);
                    return;
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
    }

    fn getters(&self, writer: &mut CodeWriter, definition: &Definition, root: bool) {
        for (index, field) in definition.fields.iter().enumerate() {
            let offset = if root {
                call("this._reader.rootOffset", vec![text(index.to_string())])
            } else {
                call(
                    "this._reader.fieldOffset",
                    vec![text("this._position"), text(index.to_string())],
                )
            };
            let return_type = value_type(field, self.schema);
            // A scalar that may be missing returns its default rather than
            // undefined, since V8 boxes a number that it merges with
            // another type, which took 1.5 times as long to read
            let default = match field.field_type {
                FieldType::Scalar(scalar) => field.default.map(|value| ts_literal(value, scalar)),
                _ => None,
            };
            let returned = if field.may_be_missing() && default.is_none() {
                format!("{return_type} | undefined")
            } else {
                return_type.clone()
            };

            let mut docs = schema_docs(&field.docs);
            if let Some(default) = &default {
                let presence = identifier(&format!("has_{}", field.name));
                docs.push(format!(
                    "`{default}` when the table doesn't hold it, as `{presence}()` tells"
                ));
            }
            writer.blank();
            self.getter_doc(writer, &docs, &returned);
            writer.open(format!(
                "{}(){} {{",
                identifier(&field.name),
                self.annotation(&returned)
            ));

            // A field that may be missing is read from its offset once the
            // checks find it in the table; every other field as before
            let missing = default.as_deref().unwrap_or("undefined");
            let read_offset = offset.clone();
            let offset = if field.may_be_missing() {
                missing_checks(writer, definition, index, field, root, offset, missing)
            } else {
                offset
            };

            match field.field_type {
                FieldType::Scalar(scalar) => {
                    let get = call(
                        format!("this._reader.get{}", scalar_method(scalar)),
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
                        format!("this._reader.get{}Array", scalar_method(scalar)),
                        vec![offset],
                    );
                    writer.statement(&STYLE, "return ", &get, ";");
                }
                FieldType::String => {
                    writer.statement(
                        &STYLE,
                        "return ",
                        &call("this._reader.getString", vec![offset]),
                        ";",
                    );
                }
                FieldType::Struct(index) => {
                    let structure = &self.schema.definition(index).name;
                    let read = call(
                        format!("new {structure}"),
                        vec![text("this._reader.bytes()"), offset],
                    );
                    writer.statement(&STYLE, "return ", &read, ";");
                }
                FieldType::Table(table) => {
                    let table = &self.schema.definition(table).name;
                    let read = call(format!("new {table}"), vec![text("this._reader"), offset]);
                    writer.statement(&STYLE, "return ", &read, ";");
                }
                FieldType::StructArray(_) | FieldType::Vector(_) => {
                    // A view, which reads each item when it's asked for
                    let read = call(
                        format!("new {return_type}"),
                        vec![text("this._reader"), offset],
                    );
                    writer.statement(&STYLE, "return ", &read, ";");
                }
            }

            writer.close("}");

            if default.is_some() {
                self.presence_method(writer, definition, index, field, root, &read_offset);
            }
        }
    }

    /// The method that tells whether the table holds a scalar that may be
    /// missing, `has<Field>()`, whose getter returns the default otherwise
    fn presence_method(
        &self,
        writer: &mut CodeWriter,
        definition: &Definition,
        index: usize,
        field: &Field,
        root: bool,
        offset: &Expr,
    ) {
        writer.blank();
        self.doc(
            writer,
            &[format!(
                "Whether the table holds `{}`",
                camel_case(&field.name)
            )],
            &["@returns {boolean}".to_owned()],
        );
        writer.open(format!(
            "{}(){} {{",
            identifier(&format!("has_{}", field.name)),
            self.annotation("boolean")
        ));

        let since = since_condition(definition, index, field);
        let checks_offset = checks_offset(definition, field);
        match (since, checks_offset) {
            (Some(since), false) => writer.line(format!("return {};", since.present)),
            (since, true) => {
                if let Some(since) = since {
                    writer.open(format!("if ({}) {{", since.missing));
                    writer.line("return false;");
                    writer.close("}");
                    writer.blank();
                }
                let absent = if root { "0" } else { "this._position" };
                writer.statement(&STYLE, "return ", offset, &format!(" !== {absent};"));
            }
            (None, false) => unreachable!("a field that may be missing has a check"),
        }
        writer.close("}");
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
                "new TableReader",
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

    /// Declares a field that the constructor assigns, without defining it.
    /// A `#name` field, or any field that the class defines, is first
    /// `undefined` and then assigned, so V8 holds it as mutable and of any
    /// type, and reads through it took up to 17% longer. TypeScript's
    /// `private declare` emits nothing, and JavaScript declares nothing: its
    /// types, which its doc comments give, follow from the constructor's
    /// assignment.
    fn private_field(&self, writer: &mut CodeWriter, name: &str, field_type: &str) {
        if self.typed {
            writer.line(format!("declare private readonly _{name}: {field_type};"));
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
    /// lines when it's too long, as oxfmt breaks it
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

/// The checks before the read of a field that may be missing, which
/// return `missing` when the table doesn't hold it, and leave its offset
/// in `offset`:
///
/// - A field added since a version of a message with a version is
///   missing from a table of an earlier version, which the reader
///   holds.
/// - A field added since a revision of any other table is missing when
///   its offset would end after the table's first value starts, which
///   its first field's offset gives, and when its offset lies in the
///   padding before an aligned first value, which reads as 0.
/// - An optional field is absent when its offset is 0, which a nested
///   table's reader finds at the table's own position.
///
/// Returns the expression of the field's offset after them: `offset`,
/// once a check reads it, or the read itself.
fn missing_checks(
    writer: &mut CodeWriter,
    definition: &Definition,
    index: usize,
    field: &Field,
    root: bool,
    offset: Expr,
    missing: &str,
) -> Expr {
    if let Some(since) = since_condition(definition, index, field) {
        writer.open(format!("if ({}) {{", since.missing));
        writer.line(format!("return {missing};"));
        writer.close("}");
        writer.blank();
    }

    if !checks_offset(definition, field) {
        return offset;
    }

    writer.statement(&STYLE, "const offset = ", &offset, ";");
    let absent = if root { "0" } else { "this._position" };
    writer.open(format!("if (offset === {absent}) {{"));
    writer.line(format!("return {missing};"));
    writer.close("}");
    writer.blank();

    text("offset")
}

/// The conditions on which a field that a later version or revision added
/// is missing from a table, and present, as `missing_checks()` describes
struct SinceCondition {
    missing: String,
    present: String,
}

fn since_condition(definition: &Definition, index: usize, field: &Field) -> Option<SinceCondition> {
    let since = field.since?;
    let layout = definition.layout();

    let (first, end) = match layout {
        Some(layout) if layout.version.is_some() => {
            return Some(SinceCondition {
                missing: format!("this._version < {since}"),
                present: format!("this._version >= {since}"),
            });
        }
        Some(layout) => {
            let width = fixed::offset_size(layout.offset_type);
            let header = usize::from(layout.sized) * width;
            ("this._reader.rootOffset(0)", header + (index + 1) * width)
        }
        None => (
            "this._reader.fieldOffset(this._position, 0) - this._position",
            (index + 1) * fixed::NESTED_OFFSET_SIZE,
        ),
    };

    Some(SinceCondition {
        missing: format!("{first} < {end}"),
        present: format!("{first} >= {end}"),
    })
}

/// Whether a field that may be missing is read after comparing its offset
/// with 0: an optional field, and a field that a later revision of a table
/// without a version added, whose offset may lie in padding
fn checks_offset(definition: &Definition, field: &Field) -> bool {
    let versioned = definition
        .layout()
        .is_some_and(|layout| layout.version.is_some());
    field.optional || (field.since.is_some() && !versioned)
}

/// A scalar's default as a literal of TypeScript and JavaScript: a 64-bit
/// integer's as a `bigint`, and a float's without a fraction of zeros or an
/// exponent where it has none
fn ts_literal(value: ScalarValue, scalar: Scalar) -> String {
    match value {
        ScalarValue::Bool(value) => value.to_string(),
        ScalarValue::Integer(value) if matches!(scalar, Scalar::Int64 | Scalar::Uint64) => {
            format!("{value}n")
        }
        ScalarValue::Integer(value) => value.to_string(),
        ScalarValue::Float(value) => number_literal(value),
    }
}

fn number_literal(value: f64) -> String {
    if value == 0.0 && value.is_sign_negative() {
        "-0".to_owned()
    } else if value.fract() == 0.0 && value.abs() < 1e16 {
        format!("{value}")
    } else {
        format!("{value:?}")
    }
}

fn writer_fits(_writer: &CodeWriter, line: &str) -> bool {
    // Getters are indented once, in a class
    STYLE.indent.len() + line.len() <= STYLE.width
}

/// The writes of field `index`, an array of structs or a vector of tables of
/// fixed size, `item`s, in `value`: the writer's `reserve` of them, whose
/// position the local takes, named after the field's index, since its name
/// may be a reserved word, and then the items' writes
fn items_writes(writer: &mut CodeWriter, index: usize, item: &str, reserve: &Expr, value: &str) {
    let items = format!("items{index}");
    writer.statement(&STYLE, &format!("const {items} = "), reserve, ";");
    writer.statement(
        &STYLE,
        "",
        &call(
            format!("{item}.writeItemsAt"),
            vec![text("writer.target()"), text(items), text(value)],
        ),
        ";",
    );
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

/// The layout of message `name`, whose version is the class's
/// `FORMAT_VERSION`
fn layout_expression(layout: Layout, name: &str) -> Expr {
    let mut entries = Vec::new();
    if layout.sized {
        entries.push(("sized".to_owned(), "true".to_owned()));
    }
    if layout.version.is_some() {
        entries.push(("version".to_owned(), format!("{name}.FORMAT_VERSION")));
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

/// The functions of `verb` for each of the `stored` scalars once, such as
/// `readUint32`, in the order that the runtime exports them
fn scalar_functions(verb: &str, stored: &[Scalar]) -> Vec<String> {
    let mut scalars: Vec<Scalar> = Scalar::ALL
        .into_iter()
        .filter(|scalar| stored.contains(scalar))
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

/// The value of an offset or a length that generated code writes as bytes
enum Slot<'a> {
    /// An offset that the generator works out
    Constant(usize),
    /// A local that holds a length
    Local(&'a str),
}

/// Writes `value`, an offset or a length of `width` bytes, little-endian at
/// `slot` from `base`, or from the start, in `bytes`, a byte at a time. The
/// runtime's write functions convert a value through a `DataView` and check
/// its range, which made writing a record of 60 bytes take 0.120 µs rather
/// than 0.100 µs in Node 27. The slots lie within the bytes, which the table's
/// writes of its fields check.
///
/// New bytes, `Output::New`, are zero, so a constant's zero bytes are left
/// out of them: a record's offsets of 4 bytes were 12 of its 20 stores, and
/// 96 of the 317 bytes of bytecode of its `serialize()`, which V8 counts
/// against the bytecode that it inlines into the function that calls it.
fn slot_stores(
    writer: &mut CodeWriter,
    bytes: &str,
    base: Option<&str>,
    slot: usize,
    width: usize,
    value: &Slot,
    output: Output,
) {
    for byte in 0..width {
        let position = match base {
            Some(base) => struct_position(base, slot + byte).flat(),
            None => (slot + byte).to_string(),
        };
        let stored = match value {
            Slot::Constant(constant) => {
                let stored = (constant >> (8 * byte)) & 0xff;
                if stored == 0 && matches!(output, Output::New) {
                    continue;
                }
                stored.to_string()
            }
            Slot::Local(local) if byte == 0 => (*local).to_owned(),
            Slot::Local(local) => format!("{local} >>> {}", 8 * byte),
        };
        writer.line(format!("{bytes}[{position}] = {stored};"));
    }
}

/// `Float32` in `setFloat32()` of a `DataView`, `BigInt64` for an `int64`,
/// and `Uint8` for a bool
fn data_view_method(scalar: Scalar) -> String {
    match scalar.stored() {
        Scalar::Int64 => "BigInt64".to_owned(),
        Scalar::Uint64 => "BigUint64".to_owned(),
        stored => scalar_method(stored),
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
/// parentheses when it doesn't fit on a line, as oxfmt breaks a comparison
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
/// compared with 0, broken over a line for each, as oxfmt keeps an object
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
/// the `=` or `:` when it doesn't fit on a line, as oxfmt breaks a comparison,
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

/// The type that a getter returns. An array of enums is the typed array of
/// their integer type, which has no type of its elements to name the enum.
fn value_type(field: &Field, schema: &Schema) -> String {
    match field.field_type {
        FieldType::Scalar(scalar) => element_type(field, scalar, schema).to_owned(),
        FieldType::Array(scalar) => typed_array(scalar).to_owned(),
        FieldType::String => "string".to_owned(),
        FieldType::Struct(index) | FieldType::Table(index) => schema.definition(index).name.clone(),
        // A view of the items, which reads each one when it's asked for
        FieldType::Vector(index) => format!("{}Vector", schema.definition(index).name),
        FieldType::StructArray(index) => format!("{}Array", schema.definition(index).name),
    }
}

/// The type that a field's setter takes
fn input_type(field: &Field, schema: &Schema) -> String {
    match field.field_type {
        FieldType::Scalar(scalar) => element_type(field, scalar, schema).to_owned(),
        FieldType::Array(scalar) => {
            format!(
                "ArrayLike<{}> | {}",
                element_type(field, scalar, schema),
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

/// The type of a scalar field or of an array's elements: the field's enum,
/// which is an alias of its integer type, or the scalar's type
fn element_type<'a>(field: &Field, scalar: Scalar, schema: &'a Schema) -> &'a str {
    match field.enumeration {
        Some(index) => &schema.enumeration(index).name,
        None => scalar_type(scalar),
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
