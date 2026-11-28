//! Resolves and checks parsed declarations into a [`Schema`].

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use crate::diagnostic::{Diagnostic, Span};
use crate::generate::reserved::{self, Declared};
use crate::model::{
    Definition, Enum, EnumValue, Field, FieldType, Kind, Layout, OffsetType, Scalar, Schema,
    StructLayout, strongly_connected_components,
};
use crate::naming::{camel_case, pascal_case};
use crate::parser::{
    Attribute, AttributeValue, Declaration, Declarations, EnumDeclaration, FieldDeclaration,
    Keyword, Name,
};

/// What a schema declares by each name: a message, table or struct, by its
/// index among them, or an enum, by its index among the enums
#[derive(Default)]
struct Names<'a> {
    definitions: HashMap<&'a str, usize>,
    /// With the enum's underlying type, when it names an integer type
    enums: HashMap<&'a str, (usize, Option<Scalar>)>,
    /// Where each name is declared, for an error that points at the
    /// declaration that takes a name of the generated code
    spans: HashMap<&'a str, Span>,
}

impl Names<'_> {
    /// Where `name` is declared, if the schema declares it
    fn span(&self, name: &str) -> Option<Span> {
        self.spans.get(name).copied()
    }
}

/// Checks every declaration and returns all the errors found, in source order
pub fn check(schema: &Declarations) -> Result<Schema, Vec<Diagnostic>> {
    let declarations = schema.declarations.as_slice();
    let mut errors = Vec::new();
    let names = index_names(schema, &mut errors);

    let enums: Vec<Enum> = schema
        .enums
        .iter()
        .map(|declaration| check_enum(declaration, &mut errors))
        .collect();
    let mut definitions: Vec<Definition> = declarations
        .iter()
        .map(|declaration| check_declaration(declaration, declarations, &names, &mut errors))
        .collect();

    check_fields_declarations(declarations, &names, &mut errors);
    check_view_declarations(&definitions, &names, &mut errors);
    check_go_functions(&definitions, &names, &mut errors);
    check_go_constants(&definitions, &schema.enums, &names, &mut errors);

    if errors.is_empty() {
        check_cycles(&definitions, declarations, &mut errors);
        check_nested_offsets(&definitions, declarations, &mut errors);
    }

    // Every struct has a size, and every table and struct a depth, once
    // none contains itself
    if errors.is_empty() {
        let order = nesting_order(&definitions);
        check_nesting_depth(&definitions, declarations, &order, &mut errors);
        check_scalar_paths(&definitions, declarations, &order, &mut errors);
        measure_structs(&mut definitions, declarations, &order, &mut errors);
    }

    if errors.is_empty() {
        Ok(Schema::new(definitions, enums))
    } else {
        errors.sort_by_key(|error| error.span.start);
        Err(errors)
    }
}

fn index_names<'a>(schema: &'a Declarations, errors: &mut Vec<Diagnostic>) -> Names<'a> {
    let mut names = Names::default();
    let mut enums = schema.enums.iter().enumerate().peekable();

    // The definitions and the enums are each in source order: taking the
    // enums declared before each definition first indexes a name at its
    // first declaration, so that the later one is the one declared twice
    for (index, declaration) in schema.declarations.iter().enumerate() {
        while let Some((index, enumeration)) = enums
            .next_if(|(_, enumeration)| enumeration.name.span.start < declaration.name.span.start)
        {
            index_enum(index, enumeration, &mut names, errors);
        }
        if check_declared_name(&declaration.name, Declared::Definition, &mut names, errors) {
            names.definitions.insert(&declaration.name.text, index);
        }
    }
    for (index, enumeration) in enums {
        index_enum(index, enumeration, &mut names, errors);
    }

    names
}

/// Indexes an enum by its name, with its underlying type, when it's the
/// first declaration of the name
fn index_enum<'a>(
    index: usize,
    declaration: &'a EnumDeclaration,
    names: &mut Names<'a>,
    errors: &mut Vec<Diagnostic>,
) {
    if check_declared_name(&declaration.name, Declared::Enum, names, errors) {
        let underlying = integer_type(&declaration.underlying.text);
        names
            .enums
            .insert(&declaration.name.text, (index, underlying));
    }
}

/// Checks the name of a message, table, struct or enum, which share their
/// names, and whether it's the first declaration of the name, which the name
/// then stands for. `declared` says which it names, since an enum can't take
/// some names that the others can.
fn check_declared_name<'a>(
    name: &'a Name,
    declared: Declared,
    names: &mut Names<'a>,
    errors: &mut Vec<Diagnostic>,
) -> bool {
    if !is_pascal_case(&name.text) {
        errors.push(Diagnostic::new(
            format!("`{}` should be PascalCase", name.text),
            name.span,
        ));
    }
    if Scalar::from_name(&name.text).is_some() || name.text == "string" {
        errors.push(Diagnostic::new(
            format!("`{}` is a built-in type", name.text),
            name.span,
        ));
    }
    check_reserved_name(name, declared, errors);

    match names.spans.entry(&name.text) {
        Entry::Vacant(entry) => {
            entry.insert(name.span);
            true
        }
        Entry::Occupied(_) => {
            errors.push(Diagnostic::new(
                format!("`{}` is declared twice", name.text),
                name.span,
            ));
            false
        }
    }
}

/// Checks an enum: that its underlying type is an integer type, and that its
/// values are `snake_case`, at least one, each of a name and an integer of
/// its own, which its underlying type holds
fn check_enum(declaration: &EnumDeclaration, errors: &mut Vec<Diagnostic>) -> Enum {
    let name = &declaration.name.text;
    let underlying = check_underlying_type(&declaration.underlying, errors);

    // An enum without values names nothing, and would be written as an
    // empty type in each language
    if declaration.values.is_empty() {
        errors.push(
            Diagnostic::new(format!("`{name}` has no values"), declaration.name.span)
                .with_help("enums name at least one value"),
        );
    }

    if let Some(scalar) = underlying {
        check_java_enum_size(declaration, scalar, errors);
    }
    check_value_names(declaration, errors);
    check_value_integers(declaration, underlying, errors);

    let values = declaration.values.iter().map(|value| EnumValue {
        name: value.name.text.clone(),
        docs: value.docs.clone(),
        value: value.value,
    });
    Enum {
        name: name.clone(),
        docs: declaration.docs.clone(),
        // An enum of another type has its error, and no schema
        underlying: underlying.unwrap_or(Scalar::Uint8),
        values: values.collect(),
    }
}

/// The most slots of a Java class's constant pool that an enum's constants
/// take, of the pool's 65,534. The rest hold the class's own entries, such as
/// its name and its constructor's: 17 of them from javac 26 with `-g`.
const MAX_JAVA_ENUM_SLOTS: usize = 65_500;

/// Checks that Java can declare a class of an enum's constants. Each takes a
/// slot of the constant pool for its name and one for its integer, or two
/// for a `long`, as a parameter does: a `uint32`, `int64` or `uint64`.
fn check_java_enum_size(
    declaration: &EnumDeclaration,
    scalar: Scalar,
    errors: &mut Vec<Diagnostic>,
) {
    let slots_per_value = 1 + java_parameter_slots(FieldType::Scalar(scalar));
    let limit = MAX_JAVA_ENUM_SLOTS / slots_per_value;

    if declaration.values.len() > limit {
        errors.push(
            Diagnostic::new(
                format!(
                    "`{}` has too many values for a Java class",
                    declaration.name.text
                ),
                declaration.name.span,
            )
            .with_help(format!(
                "an enum of `{}` can declare at most {limit} values to fit in a Java constant pool",
                scalar.name()
            )),
        );
    }
}

/// The integer type that an enum names after its colon, or `None`, with an
/// error, for any other name
fn check_underlying_type(name: &Name, errors: &mut Vec<Diagnostic>) -> Option<Scalar> {
    if let Some(scalar) = integer_type(&name.text) {
        return Some(scalar);
    }

    let message = if Scalar::from_name(&name.text).is_some() || name.text == "string" {
        format!("an enum can't be stored as `{}`", name.text)
    } else {
        format!("`{}` isn't an integer type", name.text)
    };
    errors.push(
        Diagnostic::new(message, name.span)
            .with_help("an enum is stored as an integer type, `int8` to `uint64`"),
    );
    None
}

/// The integer type of a name, or `None` for any other name
fn integer_type(name: &str) -> Option<Scalar> {
    Scalar::from_name(name).filter(|scalar| scalar.integer_range().is_some())
}

/// Checks that an enum's values are `snake_case` and declared once, and that
/// no two share a name without their underscores, as
/// [`check_field_names()`] checks fields
fn check_value_names(declaration: &EnumDeclaration, errors: &mut Vec<Diagnostic>) {
    let mut names = HashSet::new();
    // Each camelCase name, with the value that takes it first
    let mut camel_case_names = HashMap::new();

    for value in &declaration.values {
        let name = &value.name;
        let snake_case = is_snake_case(&name.text);
        if !snake_case {
            errors.push(Diagnostic::new(
                format!("`{}` should be snake_case", name.text),
                name.span,
            ));
        }
        if !names.insert(name.text.as_str()) {
            errors.push(Diagnostic::new(
                format!(
                    "`{}` is declared twice in `{}`",
                    name.text, declaration.name.text
                ),
                name.span,
            ));
            continue;
        }

        // A name that isn't snake_case has its error already
        if !snake_case {
            continue;
        }
        match camel_case_names.entry(camel_case(&name.text)) {
            Entry::Vacant(entry) => {
                entry.insert(name.text.as_str());
            }
            Entry::Occupied(entry) => errors.push(
                Diagnostic::new(
                    format!(
                        "`{}` and `{}` are both `{}` in camelCase",
                        name.text,
                        entry.get(),
                        entry.key()
                    ),
                    name.span,
                )
                .with_help(
                    "TypeScript, JavaScript, Go, Swift and C# name values without underscores",
                ),
            ),
        }
    }
}

/// Checks that each of an enum's values fits in its underlying type, when
/// that's an integer type, and that no two values are the same integer, so
/// that an integer has at most one name
fn check_value_integers(
    declaration: &EnumDeclaration,
    underlying: Option<Scalar>,
    errors: &mut Vec<Diagnostic>,
) {
    // Each integer, with the value that takes it first
    let mut integers = HashMap::new();

    for value in &declaration.values {
        if let Some(scalar) = underlying
            && let Some((least, greatest)) = scalar.integer_range()
            && !(least..=greatest).contains(&value.value)
        {
            errors.push(
                Diagnostic::new(
                    format!("{} doesn't fit in `{}`", value.value, scalar.name()),
                    value.span,
                )
                .with_help(format!("`{}` holds {least} to {greatest}", scalar.name())),
            );
        }

        match integers.entry(value.value) {
            Entry::Vacant(entry) => {
                entry.insert(value.name.text.as_str());
            }
            Entry::Occupied(entry) => errors.push(
                Diagnostic::new(
                    format!(
                        "`{}` and `{}` are both {}",
                        value.name.text,
                        entry.get(),
                        value.value
                    ),
                    value.span,
                )
                .with_help("each value of an enum is an integer of its own"),
            ),
        }
    }
}

/// Checks that a declaration of `declared` doesn't take a name that a
/// target's generated code imports or relies on, such as `List` in Java,
/// which every target declares it beside
fn check_reserved_name(name: &Name, declared: Declared, errors: &mut Vec<Diagnostic>) {
    let targets = reserved::targets_using(&name.text, declared);
    let Some((last, others)) = targets.split_last() else {
        return;
    };

    let targets = if others.is_empty() {
        (*last).to_owned()
    } else {
        format!("{} and {last}", others.join(", "))
    };
    errors.push(
        Diagnostic::new(
            format!("`{}` clashes with a name of the generated code", name.text),
            name.span,
        )
        .with_help(format!(
            "the code generated for {targets} uses `{}`",
            name.text
        )),
    );
}

fn check_declaration(
    declaration: &Declaration,
    declarations: &[Declaration],
    names: &Names,
    errors: &mut Vec<Diagnostic>,
) -> Definition {
    let kind = match declaration.keyword {
        Keyword::Message => Kind::Message(check_layout(&declaration.attributes, errors)),
        Keyword::Table | Keyword::Struct => {
            if let Some(attribute) = declaration.attributes.first() {
                errors.push(Diagnostic::new(
                    "only messages have attributes",
                    attribute.name.span,
                ));
            }
            if declaration.keyword == Keyword::Table {
                Kind::Table
            } else {
                // Measured by measure_structs() once every struct is resolved
                Kind::Struct(StructLayout {
                    size: 0,
                    alignment: 1,
                })
            }
        }
    };

    // A definition without fields holds no data, and would be written as an
    // empty class or struct in each language
    if declaration.fields.is_empty() {
        errors.push(
            Diagnostic::new(
                format!("`{}` has no fields", declaration.name.text),
                declaration.name.span,
            )
            .with_help("messages, tables and structs hold at least one field"),
        );
    }

    check_field_names(declaration, errors);

    let mut fields = Vec::with_capacity(declaration.fields.len());

    for field in &declaration.fields {
        if let Some((field_type, enumeration)) = resolve_type(field, declarations, names, errors) {
            if matches!(kind, Kind::Struct(_))
                && !matches!(field_type, FieldType::Scalar(_) | FieldType::Struct(_))
            {
                errors.push(Diagnostic::new(
                    "struct fields must be scalars or structs",
                    field.type_name.span,
                ));
            }

            fields.push(Field {
                name: field.name.text.clone(),
                docs: field.docs.clone(),
                field_type,
                enumeration,
            });
        }
    }

    check_view_names(declaration, &fields, errors);
    check_java_parameters(declaration, &fields, errors);

    Definition {
        name: declaration.name.text.clone(),
        docs: declaration.docs.clone(),
        kind,
        fields,
    }
}

/// Checks that a definition's fields are `snake_case` and declared once, and
/// that no two share a name in `camelCase`, which TypeScript, JavaScript,
/// Java and Swift name them in
fn check_field_names(declaration: &Declaration, errors: &mut Vec<Diagnostic>) {
    let mut names = HashSet::new();
    // Each camelCase name, with the field that takes it first
    let mut camel_case_names = HashMap::new();

    for field in &declaration.fields {
        let name = &field.name;
        let snake_case = is_snake_case(&name.text);
        if !snake_case {
            errors.push(Diagnostic::new(
                format!("`{}` should be snake_case", name.text),
                name.span,
            ));
        }
        if !names.insert(name.text.as_str()) {
            errors.push(Diagnostic::new(
                format!(
                    "`{}` is declared twice in `{}`",
                    name.text, declaration.name.text
                ),
                name.span,
            ));
            continue;
        }

        // A name that isn't snake_case has its error already
        if !snake_case {
            continue;
        }
        match camel_case_names.entry(camel_case(&name.text)) {
            Entry::Vacant(entry) => {
                entry.insert(name.text.as_str());
            }
            Entry::Occupied(entry) => errors.push(
                Diagnostic::new(
                    format!(
                        "`{}` and `{}` are both `{}` in camelCase",
                        name.text,
                        entry.get(),
                        entry.key()
                    ),
                    name.span,
                )
                .with_help("TypeScript, JavaScript, Java and Swift name fields in camelCase"),
            ),
        }
    }
}

/// Checks that no field takes the name of the view of a vector or an array
/// of structs, `<name>_view`, which the generated code reads it lazily with,
/// in `camelCase` too, as [`check_field_names()`] compares names, nor that of
/// the view of an array of scalars, which Java reads it in place with
fn check_view_names(declaration: &Declaration, fields: &[Field], errors: &mut Vec<Diagnostic>) {
    for field in fields {
        let view = format!("{}View", camel_case(&field.name));
        let help = match field.field_type {
            FieldType::Vector(_) | FieldType::StructArray(_) => format!(
                "the generated code reads `{}` lazily with `{}_view()`, or `{view}()` in camelCase",
                field.name, field.name
            ),
            FieldType::Array(_) => format!(
                "Java's generated code reads `{}` in place with `{view}()`",
                field.name
            ),
            _ => continue,
        };

        let clashes = declaration.fields.iter().filter(|other| {
            is_snake_case(&other.name.text) && camel_case(&other.name.text) == view
        });
        for other in clashes {
            errors.push(
                Diagnostic::new(
                    format!(
                        "`{}` clashes with the view of `{}`",
                        other.name.text, field.name
                    ),
                    other.name.span,
                )
                .with_help(help.clone()),
            );
        }
    }
}

/// The most parameter slots that a Java constructor takes: the JVM's 255,
/// less the one that `this` takes
pub const MAX_JAVA_PARAMETER_SLOTS: usize = 254;

/// Checks that Java can declare a record of a definition's fields: it reads a
/// struct into a record of its fields, and writes a message or table from a
/// `Fields` record of them, whose constructors take every field
fn check_java_parameters(
    declaration: &Declaration,
    fields: &[Field],
    errors: &mut Vec<Diagnostic>,
) {
    let slots: usize = fields
        .iter()
        .map(|field| java_parameter_slots(field.field_type))
        .sum();

    if slots > MAX_JAVA_PARAMETER_SLOTS {
        errors.push(
            Diagnostic::new(
                format!(
                    "`{}` has too many fields for a Java record",
                    declaration.name.text
                ),
                declaration.name.span,
            )
            .with_help(format!(
                "its fields take {slots} of a Java constructor's {MAX_JAVA_PARAMETER_SLOTS} parameter slots"
            )),
        );
    }
}

/// The parameter slots of a field in Java, where a `long` or a `double` takes
/// two: 64-bit scalars, and `uint32`, which Java widens to a `long`
pub const fn java_parameter_slots(field_type: FieldType) -> usize {
    match field_type {
        FieldType::Scalar(Scalar::Uint32 | Scalar::Int64 | Scalar::Uint64 | Scalar::Float64) => 2,
        _ => 1,
    }
}

/// Checks that no declaration takes the name of another's fields,
/// `<Name>Fields`, which the generated code declares beside it to write it
/// from: TypeScript, JavaScript and Python for every declaration, and Rust
/// for messages and tables, nor that of a message's or a table's
/// `<Name>FieldValues`, which Python declares as the tuple of its values
fn check_fields_declarations(
    declarations: &[Declaration],
    names: &Names,
    errors: &mut Vec<Diagnostic>,
) {
    for declaration in declarations {
        let name = &declaration.name.text;
        let fields = format!("{name}Fields");
        if let Some(clash) = names.span(&fields) {
            errors.push(
                Diagnostic::new(
                    format!("`{fields}` clashes with the fields of `{name}`"),
                    clash,
                )
                .with_help(format!(
                    "the generated code declares `{fields}` to write `{name}` from"
                )),
            );
        }

        // Python's tuple of a message's or a table's values
        let values = format!("{name}FieldValues");
        if declaration.keyword != Keyword::Struct
            && let Some(clash) = names.span(&values)
        {
            errors.push(
                Diagnostic::new(
                    format!("`{values}` clashes with the field values of `{name}`"),
                    clash,
                )
                .with_help(format!(
                    "Python's generated code declares `{values}` as the tuple of `{name}`'s values"
                )),
            );
        }
    }
}

/// Checks that no declaration takes the name of the view of a vector or an
/// array of structs, `<Table>Vector` or `<Struct>Array`, which the generated
/// code declares beside the tables and structs that they hold
fn check_view_declarations(
    definitions: &[Definition],
    names: &Names,
    errors: &mut Vec<Diagnostic>,
) {
    let mut views = HashSet::new();
    let items = definitions
        .iter()
        .flat_map(|definition| &definition.fields)
        .filter_map(|field| match field.field_type {
            FieldType::Vector(index) => Some(("Vector", "a vector", index)),
            FieldType::StructArray(index) => Some(("Array", "an array", index)),
            _ => None,
        });

    for (suffix, kind, index) in items {
        let item = &definitions[index].name;
        let view = format!("{item}{suffix}");
        let Some(clash) = names.span(&view) else {
            continue;
        };
        if !views.insert(clash.start) {
            continue;
        }

        errors.push(
            Diagnostic::new(
                format!("`{view}` clashes with the view of {kind} of `{item}`"),
                clash,
            )
            .with_help(format!(
                "the generated code declares `{view}` to read {kind} of `{item}` lazily"
            )),
        );
    }
}

/// Checks that no declaration takes the name of a function or constant that
/// the code generated for Go declares beside a message or a struct, in the
/// package of the schema's declarations: `New<Message>`, which reads a
/// message, a versioned message's `<Message>VersionOf` and
/// `<Message>FormatVersion`, and `Read<Struct>`, which reads a struct
fn check_go_functions(definitions: &[Definition], names: &Names, errors: &mut Vec<Diagnostic>) {
    for definition in definitions {
        let name = &definition.name;

        for (function, purpose) in go_functions(definition) {
            let Some(clash) = names.span(&function) else {
                continue;
            };

            errors.push(
                Diagnostic::new(
                    format!("`{function}` clashes with a name of the code generated for `{name}`"),
                    clash,
                )
                .with_help(format!(
                    "the code generated for Go declares `{function}` to {purpose} `{name}`"
                )),
            );
        }
    }
}

/// The functions and constants that the code generated for Go declares
/// beside a definition, each with what it does to the definition
fn go_functions(definition: &Definition) -> Vec<(String, &'static str)> {
    let name = &definition.name;
    match definition.kind {
        Kind::Message(layout) => {
            let mut functions = vec![(format!("New{name}"), "read")];
            if layout.version.is_some() {
                functions.push((format!("{name}VersionOf"), "read the version of"));
                functions.push((format!("{name}FormatVersion"), "hold the version of"));
            }
            functions
        }
        Kind::Struct(_) => vec![(format!("Read{name}"), "read")],
        Kind::Table => Vec::new(),
    }
}

/// The types that the code generated for Go declares beside the definitions,
/// each with the name of the definition that it's declared for:
/// `<Table>Fields`, which writes a message or a table, and the view of the
/// items of a vector or of an array of structs, `<Table>Vector` or
/// `<Struct>Array`
fn go_types(definitions: &[Definition]) -> Vec<(String, &str)> {
    let fields = definitions
        .iter()
        .filter(|definition| !definition.is_struct())
        .map(|definition| (format!("{}Fields", definition.name), &definition.name));
    let views = definitions
        .iter()
        .flat_map(|definition| &definition.fields)
        .filter_map(|field| match field.field_type {
            FieldType::Vector(index) => Some(("Vector", index)),
            FieldType::StructArray(index) => Some(("Array", index)),
            _ => None,
        })
        .map(|(suffix, index)| {
            let item = &definitions[index].name;
            (format!("{item}{suffix}"), item)
        });

    fields
        .chain(views)
        .map(|(name, definition)| (name, definition.as_str()))
        .collect()
}

/// Checks that no value of an enum takes a name of Go's package, where the
/// code generated for Go declares each value as the constant
/// `<Enum><Value>`, such as `ColorDarkRed` for `dark_red` of `Color`: a
/// declaration's, a function's, constant's or type's that Go declares beside
/// a definition, or another value's, as `b_c` of `A` and `c` of `AB`
fn check_go_constants(
    definitions: &[Definition],
    enums: &[EnumDeclaration],
    names: &Names,
    errors: &mut Vec<Diagnostic>,
) {
    // Each name that Go declares beside the declarations, with what it is
    let mut declared: HashMap<String, String> = definitions
        .iter()
        .flat_map(|definition| {
            go_functions(definition)
                .into_iter()
                .map(|(function, _)| (function, definition.name.as_str()))
        })
        .chain(go_types(definitions))
        .map(|(name, definition)| (name, format!("of the code generated for `{definition}`")))
        .collect();

    for declaration in enums {
        let name = &declaration.name.text;

        for value in &declaration.values {
            // A name that isn't snake_case has its error already
            if !is_snake_case(&value.name.text) {
                continue;
            }
            let constant = format!("{name}{}", pascal_case(&value.name.text));
            let help = format!(
                "the code generated for Go declares `{constant}` as `{}` of `{name}`",
                value.name.text
            );

            let of_enum = format!("of the enum `{name}`");
            let clash = if names.span(&constant).is_some() {
                format!("clashes with the declaration `{constant}`")
            } else {
                match declared.entry(constant) {
                    Entry::Vacant(entry) => {
                        entry.insert(of_enum);
                        continue;
                    }
                    // Two values of an enum that share a name without their
                    // underscores have their error already
                    Entry::Occupied(entry) if *entry.get() == of_enum => continue,
                    Entry::Occupied(entry) => {
                        format!("clashes with `{}` {}", entry.key(), entry.get())
                    }
                }
            };
            errors.push(
                Diagnostic::new(
                    format!("`{}` of `{name}` {clash}", value.name.text),
                    value.name.span,
                )
                .with_help(help),
            );
        }
    }
}

fn check_layout(attributes: &[Attribute], errors: &mut Vec<Diagnostic>) -> Layout {
    let mut layout = Layout {
        sized: false,
        version: None,
        offset_type: OffsetType::Uint32,
    };
    let mut version = None;
    let mut given = HashSet::new();

    for attribute in attributes {
        let name = attribute.name.text.as_str();
        if !given.insert(name) {
            errors.push(Diagnostic::new(
                format!("`{name}` is given twice"),
                attribute.name.span,
            ));
            continue;
        }

        match (name, &attribute.value) {
            ("sized", None) => layout.sized = true,
            ("version", Some(AttributeValue::Integer(value, span))) => {
                version = Some((*value, *span));
            }
            ("offset_type", Some(value)) => {
                layout.offset_type = offset_type(value).unwrap_or_else(|error| {
                    errors.push(error);
                    OffsetType::Uint32
                });
            }
            ("sized", Some(value)) => {
                errors.push(Diagnostic::new("`sized` takes no value", value.span()));
            }
            ("version", Some(AttributeValue::Name(value))) => errors.push(
                Diagnostic::new("`version` takes an integer", value.span)
                    .with_help("such as `version: 1`"),
            ),
            ("version", None) => errors.push(
                Diagnostic::new("`version` needs a value", attribute.name.span)
                    .with_help("such as `version: 1`"),
            ),
            ("offset_type", None) => errors.push(
                Diagnostic::new("`offset_type` needs a value", attribute.name.span)
                    .with_help("such as `offset_type: uint16`"),
            ),
            ("offset_size", value) => errors.push(
                Diagnostic::new("unknown attribute `offset_size`", attribute.name.span)
                    .with_help(offset_type_help(value.as_ref())),
            ),
            (other, _) => errors.push(
                Diagnostic::new(format!("unknown attribute `{other}`"), attribute.name.span)
                    .with_help("expected `sized`, `version` or `offset_type`"),
            ),
        }
    }

    // Checked once the offset type is known, whichever attribute comes first
    if let Some((value, span)) = version {
        let max = layout.offset_type.max_value();
        match u32::try_from(value) {
            Ok(value) if u64::from(value) <= max => layout.version = Some(value),
            _ => errors.push(
                Diagnostic::new(
                    format!(
                        "version {value} doesn't fit in the offset type `{}`",
                        layout.offset_type.name()
                    ),
                    span,
                )
                .with_help(format!("the largest is {max}")),
            ),
        }
    }

    layout
}

/// The offset type that an `offset_type` attribute's value names
fn offset_type(value: &AttributeValue) -> Result<OffsetType, Diagnostic> {
    let found = match value {
        AttributeValue::Name(name) => {
            if let Some(offset_type) = OffsetType::from_name(&name.text) {
                return Ok(offset_type);
            }
            name.text.clone()
        }
        AttributeValue::Integer(value, _) => value.to_string(),
    };

    Err(
        Diagnostic::new(format!("`{found}` isn't an offset type"), value.span())
            .with_help(offset_type_help(Some(value))),
    )
}

/// Points a value of `offset_size`, or one of `offset_type` that isn't an
/// offset type, to the offset types. A width in bits, which `offset_size`
/// took, gets the type of that width, such as `uint16` for 16.
fn offset_type_help(value: Option<&AttributeValue>) -> String {
    let of_width = match value {
        Some(AttributeValue::Integer(8, _)) => Some(OffsetType::Uint8),
        Some(AttributeValue::Integer(16, _)) => Some(OffsetType::Uint16),
        Some(AttributeValue::Integer(32, _)) => Some(OffsetType::Uint32),
        _ => None,
    };

    match of_width {
        Some(offset_type) => format!(
            "`offset_type` names the offset type, such as `offset_type: {}`",
            offset_type.name()
        ),
        None => "`offset_type` names the offset type: `uint8`, `uint16` or `uint32`".to_owned(),
    }
}

fn resolve_type(
    field: &FieldDeclaration,
    declarations: &[Declaration],
    names: &Names,
    errors: &mut Vec<Diagnostic>,
) -> Option<(FieldType, Option<usize>)> {
    if let Some(&(index, underlying)) = names.enums.get(field.type_name.text.as_str()) {
        // An enum whose underlying type isn't an integer type has its error
        // already
        let underlying = underlying?;
        let field_type = if field.is_array {
            FieldType::Array(underlying)
        } else {
            FieldType::Scalar(underlying)
        };
        return Some((field_type, Some(index)));
    }

    let field_type = resolve_definition(field, declarations, names, errors)?;
    Some((field_type, None))
}

/// The type of a field of a built-in type, or of a message, table or struct
fn resolve_definition(
    field: &FieldDeclaration,
    declarations: &[Declaration],
    names: &Names,
    errors: &mut Vec<Diagnostic>,
) -> Option<FieldType> {
    let name = &field.type_name;

    if let Some(scalar) = Scalar::from_name(&name.text) {
        if scalar == Scalar::Bool && field.is_array {
            errors.push(
                Diagnostic::new("arrays of bools aren't supported", name.span)
                    .with_help("use `[uint8]` of 0 and 1"),
            );
            return None;
        }
        return Some(if field.is_array {
            FieldType::Array(scalar)
        } else {
            FieldType::Scalar(scalar)
        });
    }

    if name.text == "string" {
        return Some(if field.is_array {
            FieldType::StringVector
        } else {
            FieldType::String
        });
    }

    let Some(&index) = names.definitions.get(name.text.as_str()) else {
        errors.push(unknown_type(name, names));
        return None;
    };

    match declarations[index].keyword {
        Keyword::Table => Some(if field.is_array {
            FieldType::Vector(index)
        } else {
            FieldType::Table(index)
        }),
        Keyword::Message => {
            errors.push(
                Diagnostic::new(
                    format!("`{}` is a message, which can't be nested", name.text),
                    name.span,
                )
                .with_help("declare it as a table to nest it"),
            );
            None
        }
        Keyword::Struct => Some(if field.is_array {
            FieldType::StructArray(index)
        } else {
            FieldType::Struct(index)
        }),
    }
}

/// The most edits that a misspelled type is suggested across
const MAX_SUGGESTION_DISTANCE: usize = 2;

fn unknown_type(name: &Name, names: &Names) -> Diagnostic {
    let builtins = Scalar::ALL
        .iter()
        .map(|scalar| scalar.name())
        .chain(["string"]);
    let suggestion = names
        .spans
        .keys()
        .copied()
        .chain(builtins)
        // At least as far apart as their lengths, which spares comparing long
        // names character by character
        .filter(|candidate| candidate.len().abs_diff(name.text.len()) <= MAX_SUGGESTION_DISTANCE)
        .map(|candidate| (edit_distance(&name.text, candidate), candidate))
        .filter(|(distance, _)| *distance <= MAX_SUGGESTION_DISTANCE)
        .min();

    let error = Diagnostic::new(format!("unknown type `{}`", name.text), name.span);
    match suggestion {
        Some((_, candidate)) => error.with_help(format!("did you mean `{candidate}`?")),
        None => error,
    }
}

/// Rejects tables that nest themselves without a vector in between, since
/// every field is required and such a table would never end, and structs that
/// contain themselves, which would have no size
fn check_cycles(
    definitions: &[Definition],
    declarations: &[Declaration],
    errors: &mut Vec<Diagnostic>,
) {
    let contains_itself = find_definitions_that_contain_themselves(definitions);

    for (index, definition) in definitions.iter().enumerate() {
        if !contains_itself[index] {
            continue;
        }

        let help = if definition.is_struct() {
            "a struct can't contain itself, even within another struct"
        } else {
            "only a vector can nest a table in itself"
        };
        errors.push(
            Diagnostic::new(
                format!("`{}` contains itself", definition.name),
                declarations[index].name.span,
            )
            .with_help(help),
        );
    }
}

/// Whether each definition holds itself in place, through the tables and
/// structs nested in it. A definition does so exactly when it nests itself
/// directly, or when it lies in a strongly connected component of more than
/// one definition.
fn find_definitions_that_contain_themselves(definitions: &[Definition]) -> Vec<bool> {
    let components = strongly_connected_components(definitions, nested_in_place);
    let mut sizes = vec![0_usize; definitions.len()];
    for &component in &components {
        sizes[component] += 1;
    }

    definitions
        .iter()
        .enumerate()
        .map(|(index, definition)| {
            sizes[components[index]] > 1 || nested_in_place(definition).any(|inner| inner == index)
        })
        .collect()
}

/// The definitions in an order where each comes after the tables and structs
/// nested in it in place, so that a pass over them in this order finds those
/// nested in each one done. Each definition is visited once, from a stack
/// rather than by recursion, so that long chains of them don't overflow it.
/// The checker has rejected cycles of them.
fn nesting_order(definitions: &[Definition]) -> Vec<usize> {
    let mut order = Vec::with_capacity(definitions.len());
    let mut visited = vec![false; definitions.len()];
    // The definitions from the root to the one being visited, each with the
    // definitions nested in it that are left to visit
    let mut stack = Vec::new();

    for root in 0..definitions.len() {
        if std::mem::replace(&mut visited[root], true) {
            continue;
        }
        stack.push((root, nested_in_place(&definitions[root])));

        while let Some((index, nested)) = stack.last_mut() {
            let index = *index;
            if let Some(next) = nested.find(|&nested| !visited[nested]) {
                visited[next] = true;
                stack.push((next, nested_in_place(&definitions[next])));
            } else {
                order.push(index);
                stack.pop();
            }
        }
    }

    order
}

/// The most messages and tables that nest in place one within another, and
/// the most structs, the outermost included. The generated code nests the
/// reads and writes of a struct in those of the structs that hold it, which
/// Python's parser takes at most 200 parentheses deep, and repeats a struct's
/// scalars in each struct that holds it. It writes a table's nested tables by
/// calling their writers, which Python's stack takes at most 1,000 deep.
/// The verifier of `FlatBuffers` stops at 64 by default too. Vectors nest
/// tables without a limit, since the bytes, not the schema, set how deep
/// they go.
const MAX_NESTING_DEPTH: usize = 64;

/// Rejects tables and structs that nest more than [`MAX_NESTING_DEPTH`]
/// deep, from `order`, where each definition comes after those nested in it.
/// A struct within a table doesn't add to the table's depth, nor the table to
/// the struct's, since the generated code reads and writes a struct whole.
fn check_nesting_depth(
    definitions: &[Definition],
    declarations: &[Declaration],
    order: &[usize],
    errors: &mut Vec<Diagnostic>,
) {
    let mut depths = vec![0; definitions.len()];
    for &index in order {
        let deepest = nested_alike(&definitions[index])
            .map(|nested| depths[nested])
            .max();
        depths[index] = deepest.unwrap_or(0) + 1;
    }

    let too_deep = |index: usize| depths[index] > MAX_NESTING_DEPTH;
    for index in first_to_exceed(definitions, too_deep) {
        let definition = &definitions[index];
        let help = if definition.is_struct() {
            format!("structs nest at most {MAX_NESTING_DEPTH} deep, one within another")
        } else {
            format!("tables nest in place at most {MAX_NESTING_DEPTH} deep, one within another")
        };
        errors.push(
            Diagnostic::new(
                format!("`{}` nests too deeply", definition.name),
                declarations[index].name.span,
            )
            .with_help(help),
        );
    }
}

/// The most fields on the paths from a struct to its scalars, in all: a
/// struct whose field `start` holds a struct of `x` and `y` reaches them as
/// `start.x` and `start.y`, through 4 fields. The generated Python and Rust
/// read and write each of a struct's scalars through its path, indented as
/// deep as it lies, so a few lines of structs that hold others in pairs, or
/// wrap them one within another, would otherwise generate code of any size:
/// 17 lines, of structs up to 65,536 bytes, generated 70 MB of Python, and
/// 64 lines that wrap a struct of 1,024 scalars, 66 MB. A struct at the
/// limit, such as 32 structs of 16 scalars, generates about 75 KB of Python
/// and less Rust. It holds at most 1,024 scalars, of at most 8,192 bytes,
/// and a struct that nests others 64 deep holds its scalars in the innermost.
const MAX_SCALAR_PATHS: usize = 1_024;

/// Rejects structs whose paths to their scalars take more than
/// [`MAX_SCALAR_PATHS`] fields in all, from `order`, where each definition
/// comes after those nested in it. Counts saturate rather than overflowing,
/// since a few lines can double a struct's scalars many times over.
fn check_scalar_paths(
    definitions: &[Definition],
    declarations: &[Declaration],
    order: &[usize],
    errors: &mut Vec<Diagnostic>,
) {
    // A field of a struct adds itself to the path to each of the scalars
    // within it
    let mut scalars = vec![0_usize; definitions.len()];
    let mut paths = vec![0_usize; definitions.len()];
    for &index in order {
        let definition = &definitions[index];
        if !definition.is_struct() {
            continue;
        }

        for field in &definition.fields {
            let (field_scalars, field_paths) = match field.field_type {
                FieldType::Scalar(_) => (1, 1),
                FieldType::Struct(inner) => {
                    (scalars[inner], paths[inner].saturating_add(scalars[inner]))
                }
                _ => unreachable!("structs hold scalars and structs only"),
            };
            scalars[index] = scalars[index].saturating_add(field_scalars);
            paths[index] = paths[index].saturating_add(field_paths);
        }
    }

    // The structs that an error points at hold structs within the limit
    // only, so their counts haven't saturated
    let too_many = |index: usize| paths[index] > MAX_SCALAR_PATHS;
    for index in first_to_exceed(definitions, too_many) {
        let name = &definitions[index].name;
        let found = paths[index];
        errors.push(
            Diagnostic::new(
                format!("`{name}` has too many scalars for a struct"),
                declarations[index].name.span,
            )
            .with_help(format!(
                "the paths to a struct's scalars, such as `start.x`, take at most {MAX_SCALAR_PATHS} fields in all, and those of `{name}` take {found}"
            )),
        );
    }
}

/// The most bytes that a struct takes. The generated code reads and writes
/// each of a struct's scalars in place, and a schema of a few lines could
/// otherwise declare a struct of any size, by nesting structs in pairs.
const MAX_STRUCT_SIZE: usize = 65_536;

/// Measures each struct's size and alignment once, from those of the structs
/// in it, which come before it in `order`, and rejects structs larger than
/// [`MAX_STRUCT_SIZE`]. Sizes saturate rather than overflowing, since a few
/// lines can double a struct's size many times over.
fn measure_structs(
    definitions: &mut [Definition],
    declarations: &[Declaration],
    order: &[usize],
    errors: &mut Vec<Diagnostic>,
) {
    let mut layouts: Vec<Option<StructLayout>> = vec![None; definitions.len()];
    for &index in order {
        if definitions[index].is_struct() {
            layouts[index] = Some(measure_struct(&definitions[index], &layouts));
        }
    }

    let too_large = |index: usize| {
        layouts[index].is_some_and(|layout: StructLayout| layout.size > MAX_STRUCT_SIZE)
    };
    for index in first_to_exceed(definitions, too_large) {
        errors.push(
            Diagnostic::new(
                format!("`{}` is too large for a struct", definitions[index].name),
                declarations[index].name.span,
            )
            .with_help(format!("structs take at most {MAX_STRUCT_SIZE} bytes")),
        );
    }

    for (definition, layout) in definitions.iter_mut().zip(layouts) {
        if let Some(layout) = layout {
            definition.kind = Kind::Struct(layout);
        }
    }
}

/// The layout of a struct, from `layouts` of the structs in it, which are
/// measured before it
fn measure_struct(definition: &Definition, layouts: &[Option<StructLayout>]) -> StructLayout {
    let mut layout = StructLayout {
        size: 0,
        alignment: 1,
    };
    for field in &definition.fields {
        let (size, alignment) = match field.field_type {
            FieldType::Scalar(scalar) => (usize::from(scalar.size()), scalar.size()),
            FieldType::Struct(inner) => {
                let inner = layouts[inner].expect("structs are measured after those in them");
                (inner.size, inner.alignment)
            }
            _ => unreachable!("structs hold scalars and structs only"),
        };
        layout.size = layout.size.saturating_add(size);
        layout.alignment = layout.alignment.max(alignment);
    }

    layout
}

/// Rejects tables nested in messages of a narrower offset type, since readers
/// read a nested table's offsets as of its message's offset type, while
/// tables are written with `uint32` offsets
fn check_nested_offsets(
    definitions: &[Definition],
    declarations: &[Declaration],
    errors: &mut Vec<Diagnostic>,
) {
    for (definition, declaration) in definitions.iter().zip(declarations) {
        let Kind::Message(layout) = definition.kind else {
            continue;
        };
        if layout.offset_type == OffsetType::Uint32 {
            continue;
        }

        for (field, field_declaration) in definition.fields.iter().zip(&declaration.fields) {
            if matches!(field.field_type, FieldType::Table(_) | FieldType::Vector(_)) {
                errors.push(
                    Diagnostic::new(
                        format!(
                            "`{}` can't nest tables, since its offset type is `{}`",
                            definition.name,
                            layout.offset_type.name()
                        ),
                        field_declaration.type_name.span,
                    )
                    .with_help("nested tables need the offset type `uint32`"),
                );
            }
        }
    }
}

/// The tables and structs that a definition holds in place, rather than in a
/// vector or an array, which can be empty
fn nested_in_place(definition: &Definition) -> impl Iterator<Item = usize> + '_ {
    definition
        .fields
        .iter()
        .filter_map(|field| match field.field_type {
            FieldType::Table(index) | FieldType::Struct(index) => Some(index),
            _ => None,
        })
}

/// The definitions nested in place in a definition of the same kind: the
/// tables in a message or table, and the structs in a struct
fn nested_alike(definition: &Definition) -> impl Iterator<Item = usize> + '_ {
    let is_struct = definition.is_struct();
    definition
        .fields
        .iter()
        .filter_map(move |field| match field.field_type {
            FieldType::Table(index) if !is_struct => Some(index),
            FieldType::Struct(index) if is_struct => Some(index),
            _ => None,
        })
}

/// The definitions that exceed a limit of their own, rather than by holding
/// one of the same kind that does, which an error points at rather than at
/// every definition that holds one of them
fn first_to_exceed<'a>(
    definitions: &'a [Definition],
    exceeds: impl Fn(usize) -> bool + Copy + 'a,
) -> impl Iterator<Item = usize> + 'a {
    definitions
        .iter()
        .enumerate()
        .filter(move |&(index, definition)| {
            exceeds(index) && !nested_alike(definition).any(exceeds)
        })
        .map(|(index, _)| index)
}

fn is_pascal_case(name: &str) -> bool {
    name.starts_with(|character: char| character.is_ascii_uppercase())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

fn is_snake_case(name: &str) -> bool {
    name.starts_with(|character: char| character.is_ascii_lowercase())
        && !name.ends_with('_')
        && !name.contains("__")
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

/// The optimal string alignment distance: how many characters to insert,
/// delete, replace or swap with their neighbour to turn `a` into `b`, for
/// suggesting a type that was misspelled
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    // The distances from `a`'s prefixes of two characters ago, one ago and
    // this one to each of `b`'s prefixes
    let mut before = vec![0; b.len() + 1];
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];

    for i in 1..=a.len() {
        current[0] = i;
        for j in 1..=b.len() {
            let replacement = previous[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            let mut distance = replacement.min(previous[j] + 1).min(current[j - 1] + 1);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                distance = distance.min(before[j - 2] + 1);
            }
            current[j] = distance;
        }

        std::mem::swap(&mut before, &mut previous);
        std::mem::swap(&mut previous, &mut current);
    }

    previous[b.len()]
}
