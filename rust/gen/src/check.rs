//! Resolves and checks parsed declarations into a [`Schema`].

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use crate::diagnostic::{Diagnostic, Span};
use crate::generate::reserved;
use crate::model::{
    Definition, Field, FieldType, Kind, Layout, OffsetType, Scalar, ScalarValue, Schema,
    StructLayout, strongly_connected_components,
};
use crate::naming::camel_case;
use crate::parser::{
    Attribute, AttributeValue, Declaration, FieldDeclaration, Keyword, Literal, LiteralKind, Name,
};

/// Checks every declaration and returns all the errors found, in source order
pub fn check(declarations: &[Declaration]) -> Result<Schema, Vec<Diagnostic>> {
    let mut errors = Vec::new();
    let indices = index_names(declarations, &mut errors);

    let mut definitions: Vec<Definition> = declarations
        .iter()
        .map(|declaration| check_declaration(declaration, declarations, &indices, &mut errors))
        .collect();

    check_fields_declarations(declarations, &indices, &mut errors);
    check_view_declarations(&definitions, declarations, &indices, &mut errors);
    check_go_functions(&definitions, declarations, &indices, &mut errors);

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
        Ok(Schema::new(definitions, first_presence(declarations)))
    } else {
        errors.sort_by_key(|error| error.span.start);
        Err(errors)
    }
}

fn index_names<'a>(
    declarations: &'a [Declaration],
    errors: &mut Vec<Diagnostic>,
) -> HashMap<&'a str, usize> {
    let mut indices = HashMap::new();

    for (index, declaration) in declarations.iter().enumerate() {
        let name = &declaration.name;
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
        check_reserved_name(name, errors);
        if indices.insert(name.text.as_str(), index).is_some() {
            errors.push(Diagnostic::new(
                format!("`{}` is declared twice", name.text),
                name.span,
            ));
        }
    }

    indices
}

/// Checks that a declaration doesn't take a name that a target's generated
/// code imports or relies on, such as `List` in Java, which every target
/// declares it beside
fn check_reserved_name(name: &Name, errors: &mut Vec<Diagnostic>) {
    let targets = reserved::targets_using(&name.text);
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
    indices: &HashMap<&str, usize>,
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
    let mut presence = Presence::of(declaration, kind);
    presence.check_optional(errors);

    for field in &declaration.fields {
        if let Some(field_type) = resolve_type(field, declarations, indices, errors) {
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
                optional: field.optional.is_some(),
                since: presence.since(field, errors),
                default: check_default(field, field_type, errors),
            });
        }
    }

    check_presence_names(declaration, &fields, errors);
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
/// `Fields` record of them, whose constructors take every field. An optional
/// scalar is boxed there, so that `null` leaves it out, which takes one slot,
/// as every reference does.
fn check_java_parameters(
    declaration: &Declaration,
    fields: &[Field],
    errors: &mut Vec<Diagnostic>,
) {
    let slots: usize = fields
        .iter()
        .map(|field| {
            if field.optional {
                1
            } else {
                java_parameter_slots(field.field_type)
            }
        })
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
    indices: &HashMap<&str, usize>,
    errors: &mut Vec<Diagnostic>,
) {
    for declaration in declarations {
        let name = &declaration.name.text;
        let fields = format!("{name}Fields");
        if let Some(&clash) = indices.get(fields.as_str()) {
            errors.push(
                Diagnostic::new(
                    format!("`{fields}` clashes with the fields of `{name}`"),
                    declarations[clash].name.span,
                )
                .with_help(format!(
                    "the generated code declares `{fields}` to write `{name}` from"
                )),
            );
        }

        // Python's tuple of a message's or a table's values
        let values = format!("{name}FieldValues");
        if declaration.keyword != Keyword::Struct
            && let Some(&clash) = indices.get(values.as_str())
        {
            errors.push(
                Diagnostic::new(
                    format!("`{values}` clashes with the field values of `{name}`"),
                    declarations[clash].name.span,
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
    declarations: &[Declaration],
    indices: &HashMap<&str, usize>,
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
        let Some(&clash) = indices.get(view.as_str()) else {
            continue;
        };
        if !views.insert(clash) {
            continue;
        }

        errors.push(
            Diagnostic::new(
                format!("`{view}` clashes with the view of {kind} of `{item}`"),
                declarations[clash].name.span,
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
/// `<Message>FormatVersion`, the `<Message>FirstVersion` of a message that
/// reads versions since another, and `Read<Struct>`, which reads a struct
fn check_go_functions(
    definitions: &[Definition],
    declarations: &[Declaration],
    indices: &HashMap<&str, usize>,
    errors: &mut Vec<Diagnostic>,
) {
    for definition in definitions {
        let name = &definition.name;
        let functions = match definition.kind {
            Kind::Message(layout) => {
                let mut functions = vec![(format!("New{name}"), "read")];
                if layout.version.is_some() {
                    functions.push((format!("{name}VersionOf"), "read the version of"));
                    functions.push((format!("{name}FormatVersion"), "hold the version of"));
                }
                if layout.since.is_some() {
                    functions.push((
                        format!("{name}FirstVersion"),
                        "hold the first readable version of",
                    ));
                }
                functions
            }
            Kind::Struct(_) => vec![(format!("Read{name}"), "read")],
            Kind::Table => continue,
        };

        for (function, purpose) in functions {
            let Some(&clash) = indices.get(function.as_str()) else {
                continue;
            };

            errors.push(
                Diagnostic::new(
                    format!("`{function}` clashes with a name of the code generated for `{name}`"),
                    declarations[clash].name.span,
                )
                .with_help(format!(
                    "the code generated for Go declares `{function}` to {purpose} `{name}`"
                )),
            );
        }
    }
}

fn check_layout(attributes: &[Attribute], errors: &mut Vec<Diagnostic>) -> Layout {
    let mut layout = Layout {
        sized: false,
        version: None,
        since: None,
        offset_type: OffsetType::Uint32,
    };
    let mut version = None;
    let mut since = None;
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
            ("since", Some(AttributeValue::Integer(value, span))) => {
                since = Some((*value, *span));
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
            ("since", Some(AttributeValue::Name(value))) => errors.push(
                Diagnostic::new("`since` takes an integer", value.span)
                    .with_help("such as `since: 1`"),
            ),
            ("since", None) => errors.push(
                Diagnostic::new("`since` needs a value", attribute.name.span)
                    .with_help("such as `since: 1`"),
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
                    .with_help("expected `sized`, `version`, `since` or `offset_type`"),
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

    // Checked once the version is known, whichever attribute comes first
    if let Some((value, span)) = since {
        layout.since = check_since(value, span, version.is_some(), layout.version, errors);
    }

    layout
}

/// The first version that a message's reader reads, from its `since`
/// attribute, which needs a `version` that's no earlier. `given` is whether
/// the message has a `version` attribute, and `version` its value, if it has
/// no error of its own.
fn check_since(
    value: u64,
    span: Span,
    given: bool,
    version: Option<u32>,
    errors: &mut Vec<Diagnostic>,
) -> Option<u32> {
    match (given, version) {
        (false, _) => {
            errors.push(
                Diagnostic::new("`since` needs a `version`", span).with_help(
                    "it's the first version that the message's reader reads, such as \
                     `version: 2, since: 1`",
                ),
            );
            None
        }
        (true, Some(version)) if value > u64::from(version) => {
            errors.push(
                Diagnostic::new(
                    format!("`since: {value}` is after the message's version, {version}"),
                    span,
                )
                .with_help("it's the first version that the message's reader reads"),
            );
            None
        }
        // No larger than the version, which is a u32
        (true, Some(_)) => u32::try_from(value).ok(),
        // The version has an error of its own
        (true, None) => None,
    }
}

/// Where the declarations first declare a field optional or added since a
/// version, if they do
fn first_presence(declarations: &[Declaration]) -> Option<Span> {
    declarations
        .iter()
        .flat_map(|declaration| &declaration.fields)
        .filter_map(|field| {
            let attribute = field
                .attributes
                .first()
                .map(|attribute| attribute.name.span);
            field.optional.or(attribute)
        })
        .min_by_key(|span| span.start)
}

/// The rules of a declaration's optional fields and of the fields that its
/// versions or revisions added, which are checked field by field, in order
struct Presence<'a> {
    declaration: &'a Declaration,
    kind: Kind,
    /// The latest `since` of the fields so far
    latest: Option<u64>,
}

impl<'a> Presence<'a> {
    const fn of(declaration: &'a Declaration, kind: Kind) -> Self {
        Self {
            declaration,
            kind,
            latest: None,
        }
    }

    /// Checks the fields' `?`s: a struct's fields are never optional, and
    /// neither is the first field of a table whose readers find the fields
    /// that it holds from that field's offset
    fn check_optional(&self, errors: &mut Vec<Diagnostic>) {
        let fields = &self.declaration.fields;

        if matches!(self.kind, Kind::Struct(_)) {
            for span in fields.iter().filter_map(|field| field.optional) {
                errors.push(
                    Diagnostic::new("a struct's fields can't be optional", span)
                        .with_help("a struct holds each of its scalars at a fixed offset"),
                );
            }
            return;
        }

        if let Some(attribute) = fields.first().and_then(|field| field.attributes.first()) {
            errors.push(
                Diagnostic::new(
                    "the first field can't be added `since` a later version",
                    attribute.name.span,
                )
                .with_help("every version or revision of a table holds its first field"),
            );
        }

        let versioned = matches!(self.kind, Kind::Message(layout) if layout.version.is_some());
        let adds_fields = fields.iter().any(|field| !field.attributes.is_empty());
        if let Some(span) = fields.first().and_then(|field| field.optional)
            && adds_fields
            && !versioned
        {
            errors.push(
                Diagnostic::new(
                    "the first field can't be optional in a table with fields added `since` a \
                     revision",
                    span,
                )
                .with_help(
                    "a reader finds which fields such a table holds from its first field's \
                     offset",
                ),
            );
        }
    }

    /// The version or revision that added a field, from its `since`
    /// attribute, which is the field's only one. A message with a version
    /// adds fields after the first version that it reads, up to its own,
    /// and any other table from its second revision, and each appends them,
    /// so that no field comes after one that a later version added.
    fn since(&mut self, field: &FieldDeclaration, errors: &mut Vec<Diagnostic>) -> Option<u32> {
        let mut since = None;

        for attribute in &field.attributes {
            let name = attribute.name.text.as_str();
            match (name, &attribute.value) {
                _ if matches!(self.kind, Kind::Struct(_)) => errors.push(Diagnostic::new(
                    "a struct's fields have no attributes",
                    attribute.name.span,
                )),
                ("since", Some(AttributeValue::Integer(_, span))) if since.is_some() => {
                    errors.push(Diagnostic::new("`since` is given twice", *span));
                }
                ("since", Some(AttributeValue::Integer(value, span))) => {
                    since = Some((*value, *span));
                }
                ("since", Some(AttributeValue::Name(value))) => errors.push(
                    Diagnostic::new("`since` takes an integer", value.span)
                        .with_help("such as `since: 2`"),
                ),
                ("since", None) => errors.push(
                    Diagnostic::new("`since` needs a value", attribute.name.span)
                        .with_help("such as `since: 2`"),
                ),
                (other, _) => errors.push(
                    Diagnostic::new(format!("unknown attribute `{other}`"), attribute.name.span)
                        .with_help("a field's only attribute is `since`, such as `since: 2`"),
                ),
            }
        }

        let Some((value, span)) = since else {
            if let Some(latest) = self.latest {
                errors.push(
                    Diagnostic::new(
                        format!(
                            "`{}` comes after a field added since {latest}",
                            field.name.text
                        ),
                        field.name.span,
                    )
                    .with_help(
                        "a version only appends fields, so every field after one with `since` \
                         has a `since` too",
                    ),
                );
            }
            return None;
        };

        if let Some(error) = self.range_error(value, span) {
            errors.push(error);
            return None;
        }
        if let Some(latest) = self.latest.filter(|&latest| value < latest) {
            errors.push(
                Diagnostic::new(
                    format!("`since: {value}` comes after a field added since {latest}"),
                    span,
                )
                .with_help("a version appends its fields to those of the versions before it"),
            );
            return None;
        }

        self.latest = Some(value);
        u32::try_from(value).ok()
    }

    /// The error of a `since` outside the versions or revisions that can add
    /// a field
    fn range_error(&self, value: u64, span: Span) -> Option<Diagnostic> {
        let name = &self.declaration.name.text;

        let Some((first, version)) = self.layout().and_then(|layout| layout.versions()) else {
            return (value < 2).then(|| {
                Diagnostic::new(
                    format!("`since: {value}` isn't a later revision of `{name}`"),
                    span,
                )
                .with_help(
                    "a table without a version counts its revisions from 1, so the fields \
                     that it adds are `since: 2` or later",
                )
            });
        };

        if value > u64::from(version) {
            return Some(
                Diagnostic::new(
                    format!("`since: {value}` is after the version of `{name}`, {version}"),
                    span,
                )
                .with_help("a field is added by the message's version or one before it"),
            );
        }
        if value <= u64::from(first) {
            let error = Diagnostic::new(
                format!(
                    "`since: {value}` isn't after the first version that `{name}` reads, {first}"
                ),
                span,
            );
            return Some(if value > 0 && first == version {
                error.with_help(format!(
                    "name the first version with the message's `since`, such as \
                     `version: {version}, since: {}`",
                    value - 1
                ))
            } else {
                error.with_help("every version that the message reads has the field")
            });
        }

        None
    }

    const fn layout(&self) -> Option<Layout> {
        match self.kind {
            Kind::Message(layout) => Some(layout),
            Kind::Table | Kind::Struct(_) => None,
        }
    }
}

/// The default of a scalar field that may be missing, which its getter
/// returns when the table lacks it: the value after `=`, which must be of the
/// field's type, or zero or false. Only such a field has one: a required
/// field is always there, and a missing string, array, table or struct reads
/// as the language's missing value, which costs nothing more, while a number
/// merged with one is boxed in JavaScript.
fn check_default(
    field: &FieldDeclaration,
    field_type: FieldType,
    errors: &mut Vec<Diagnostic>,
) -> Option<ScalarValue> {
    // Syntactically, so that an error in `since` doesn't add one here
    let may_be_missing = field.optional.is_some() || !field.attributes.is_empty();

    let FieldType::Scalar(scalar) = field_type else {
        if let Some(literal) = &field.default {
            errors.push(
                Diagnostic::new("only a scalar has a default", literal.span).with_help(
                    "a missing string, array, table or struct reads as the language's \
                     missing value",
                ),
            );
        }
        return None;
    };

    let Some(literal) = &field.default else {
        return may_be_missing.then_some(zero_of(scalar));
    };
    if !may_be_missing {
        errors.push(
            Diagnostic::new(
                "only a field that may be missing has a default",
                literal.span,
            )
            .with_help("declare it optional with `?`, or added `since` a version"),
        );
        return None;
    }

    match scalar_value(scalar, literal) {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(error);
            None
        }
    }
}

const fn zero_of(scalar: Scalar) -> ScalarValue {
    match scalar {
        Scalar::Bool => ScalarValue::Bool(false),
        Scalar::Float32 | Scalar::Float64 => ScalarValue::Float(0.0),
        _ => ScalarValue::Integer(0),
    }
}

/// The value of a default of a scalar type: `true` or `false` for a bool,
/// an integer within an integer type's range, or a finite float, of which a
/// `float32` takes the nearest `float32`
fn scalar_value(scalar: Scalar, literal: &Literal) -> Result<ScalarValue, Diagnostic> {
    let text = literal.text.as_str();
    let span = literal.span;
    let name = scalar.name();

    match (scalar, literal.kind) {
        (Scalar::Bool, LiteralKind::Name) if text == "true" || text == "false" => {
            Ok(ScalarValue::Bool(text == "true"))
        }
        (Scalar::Bool, _) => Err(Diagnostic::new(format!("`{text}` isn't a `bool`"), span)
            .with_help("a bool's default is `true` or `false`")),
        (Scalar::Float32 | Scalar::Float64, LiteralKind::Integer | LiteralKind::Decimal) => {
            let value: f64 = text
                .parse()
                .map_err(|_| Diagnostic::new(format!("`{text}` isn't a number"), span))?;
            #[allow(clippy::cast_possible_truncation)]
            let value = if scalar == Scalar::Float32 {
                f64::from(value as f32)
            } else {
                value
            };
            if value.is_finite() {
                Ok(ScalarValue::Float(value))
            } else {
                Err(Diagnostic::new(
                    format!("`{text}` isn't a finite `{name}`"),
                    span,
                ))
            }
        }
        (_, LiteralKind::Integer) if scalar != Scalar::Float32 && scalar != Scalar::Float64 => {
            let (min, max) = integer_range(scalar);
            match text.parse::<i128>() {
                Ok(value) if (min..=max).contains(&value) => Ok(ScalarValue::Integer(value)),
                _ => Err(
                    Diagnostic::new(format!("`{text}` doesn't fit in `{name}`"), span)
                        .with_help(format!("from {min} to {max}")),
                ),
            }
        }
        _ => Err(
            Diagnostic::new(format!("`{text}` isn't a `{name}`"), span).with_help(
                if matches!(scalar, Scalar::Float32 | Scalar::Float64) {
                    "a float's default is a number, such as `0.5` or `-1e3`"
                } else {
                    "an integer's default is an integer, such as `0` or `-1`"
                },
            ),
        ),
    }
}

/// The smallest and largest value of an integer type
fn integer_range(scalar: Scalar) -> (i128, i128) {
    let bits = u32::from(scalar.size()) * 8;
    match scalar {
        Scalar::Int8 | Scalar::Int16 | Scalar::Int32 | Scalar::Int64 => {
            (-(1 << (bits - 1)), (1 << (bits - 1)) - 1)
        }
        _ => (0, (1 << bits) - 1),
    }
}

/// Checks that no field takes the name of the method that tells whether a
/// scalar that may be missing is there, `has_<field>`, in `camelCase` too,
/// which every target declares beside its getter
fn check_presence_names(declaration: &Declaration, fields: &[Field], errors: &mut Vec<Diagnostic>) {
    let methods: HashMap<String, &str> = fields
        .iter()
        .filter(|field| field.default.is_some())
        .map(|field| {
            (
                camel_case(&format!("has_{}", field.name)),
                field.name.as_str(),
            )
        })
        .collect();

    for field in &declaration.fields {
        if let Some(scalar) = methods.get(&camel_case(&field.name.text)) {
            errors.push(
                Diagnostic::new(
                    format!(
                        "`{}` clashes with the method that tells whether `{scalar}` is there",
                        field.name.text
                    ),
                    field.name.span,
                )
                .with_help(format!("rename it, or `{scalar}`")),
            );
        }
    }
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
    indices: &HashMap<&str, usize>,
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
        if field.is_array {
            errors.push(Diagnostic::new(
                "arrays of strings aren't supported",
                name.span,
            ));
            return None;
        }
        return Some(FieldType::String);
    }

    let Some(&index) = indices.get(name.text.as_str()) else {
        errors.push(unknown_type(name, indices));
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

fn unknown_type(name: &Name, indices: &HashMap<&str, usize>) -> Diagnostic {
    let builtins = Scalar::ALL
        .iter()
        .map(|scalar| scalar.name())
        .chain(["string"]);
    let suggestion = indices
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
