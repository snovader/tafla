//! Compares a new version of a schema with an old one under the format's
//! evolution rules, which `SPEC.md` gives and README.md's "Compatibility"
//! lists: where a reader generated from one schema would misread the tables
//! written with the other.
//!
//! Declarations are matched by name. Each pair of tables has a
//! [`Relation`], which follows from its own fields and from the tables and
//! structs that they hold, and the diagnostics point at the declaration
//! that causes each difference, in the new schema's source.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Span};
use crate::model::{Definition, FieldType, Kind, Layout, OffsetType, Scalar, Schema};
use crate::parser::{Attribute, Declaration};

/// What [`compatibility()`](crate::compatibility) found
#[derive(Debug)]
pub struct Comparison {
    /// The errors and notes, which point into the new schema's source, in
    /// its order
    pub diagnostics: Vec<Diagnostic>,
    /// The old schema's declarations that the new schema doesn't declare, in
    /// the old schema's order, which nothing was compared with: removed, or
    /// renamed
    pub undeclared: Vec<String>,
}

impl Comparison {
    /// Whether the new schema breaks a rule, rather than only having notes
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    /// Renders the errors and notes as the command prints them, and then a
    /// note of each declaration that the new schema doesn't declare
    pub fn render(&self, file_name: &str, source: &str) -> String {
        let mut rendered: Vec<String> = self
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.render(file_name, source))
            .collect();
        rendered.extend(self.undeclared.iter().map(|name| {
            format!(
                "note: `{name}` isn't declared in {file_name}, so nothing is compared with it\n"
            )
        }));
        rendered.join("\n")
    }
}

/// How the bytes of a new table or struct relate to those of an old one
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Relation {
    /// A reader of either misreads the other's bytes
    Different,
    /// The new one appends to the old: a reader of the old fields reads
    /// them from the new bytes alike, and a reader of the new fields
    /// misreads the old bytes
    Extended,
    /// The same bytes, whatever the names
    Same,
}

/// The layout of every nested table: no header, and `uint32` offsets
const TABLE_LAYOUT: Layout = Layout {
    sized: false,
    version: None,
    offset_type: OffsetType::Uint32,
};

/// The layout of a message or a table, or `None` for a struct
const fn table_layout(definition: &Definition) -> Option<Layout> {
    match definition.kind {
        Kind::Message(layout) => Some(layout),
        Kind::Table => Some(TABLE_LAYOUT),
        Kind::Struct(_) => None,
    }
}

/// Compares the checked schemas. `declarations` are the new schema's, which
/// its definitions were checked from, in the same order.
pub fn compare(old: &Schema, new: &Schema, declarations: &[Declaration]) -> Comparison {
    let old_indices = index_names(old);
    let new_indices = index_names(new);

    let mut comparer = Comparer {
        old,
        new,
        declarations,
        relations: Relations::new(old, new),
        held: held_by_messages(new),
        diagnostics: Vec::new(),
    };

    // Every pair of tables that a diagnostic asks the relation of is known
    // before any relation is, since tables hold each other through vectors
    let pairs: Vec<(usize, usize)> = new
        .definitions
        .iter()
        .enumerate()
        .filter_map(|(new_index, definition)| {
            let old_index = *old_indices.get(definition.name.as_str())?;
            Some((old_index, new_index))
        })
        .collect();
    for &(old_index, new_index) in &pairs {
        comparer.relations.add(old_index, new_index);
        for reader in comparer.readers_of_old(old_index, new_index) {
            comparer.relations.add(old_index, reader);
        }
    }
    comparer.relations.settle();

    for (old_index, new_index) in pairs {
        comparer.compare(old_index, new_index);
    }

    let undeclared = old
        .definitions
        .iter()
        .filter(|definition| !new_indices.contains_key(definition.name.as_str()))
        .map(|definition| definition.name.clone())
        .collect();
    let mut diagnostics = comparer.diagnostics;
    diagnostics.sort_by_key(|diagnostic| diagnostic.span.start);
    Comparison {
        diagnostics,
        undeclared,
    }
}

fn index_names(schema: &Schema) -> HashMap<&str, usize> {
    schema
        .definitions
        .iter()
        .enumerate()
        .map(|(index, definition)| (definition.name.as_str(), index))
        .collect()
}

/// Whether a message holds each definition, by its index: in a field, an
/// array or a vector, or in a table or struct that it holds. A message
/// holds itself.
fn held_by_messages(schema: &Schema) -> Vec<bool> {
    let mut held = vec![false; schema.definitions.len()];
    let mut reached: Vec<usize> = (0..schema.definitions.len())
        .filter(|&index| matches!(schema.definition(index).kind, Kind::Message(_)))
        .collect();

    while let Some(index) = reached.pop() {
        if std::mem::replace(&mut held[index], true) {
            continue;
        }
        reached.extend(
            schema
                .definition(index)
                .fields
                .iter()
                .filter_map(|field| definition_of(field.field_type)),
        );
    }
    held
}

/// The definition that a field's type names, by its index
const fn definition_of(field_type: FieldType) -> Option<usize> {
    match field_type {
        FieldType::Struct(index)
        | FieldType::StructArray(index)
        | FieldType::Table(index)
        | FieldType::Vector(index) => Some(index),
        FieldType::Scalar(_) | FieldType::Array(_) | FieldType::String => None,
    }
}

/// The relation of each pair of an old table and a new one, messages
/// included, and of the structs that they hold
struct Relations<'a> {
    old: &'a Schema,
    new: &'a Schema,
    /// Each pair's place in `tables`, by its old and new index
    places: HashMap<(usize, usize), usize>,
    tables: Vec<TablePair>,
    /// The scalars of each struct, by its index, found once
    old_scalars: Vec<Option<Vec<Scalar>>>,
    new_scalars: Vec<Option<Vec<Scalar>>>,
}

struct TablePair {
    relation: Relation,
    /// The places of the pairs whose tables hold this pair's, each of which
    /// relates no closer than this one
    holders: Vec<usize>,
}

impl<'a> Relations<'a> {
    fn new(old: &'a Schema, new: &'a Schema) -> Self {
        Self {
            old,
            new,
            places: HashMap::new(),
            tables: Vec::new(),
            old_scalars: vec![None; old.definitions.len()],
            new_scalars: vec![None; new.definitions.len()],
        }
    }

    /// Adds the pair of an old and a new message or table, and the pairs of
    /// the tables that their fields hold at the same index, each with the
    /// relation of its own fields. A pair of other definitions isn't added.
    fn add(&mut self, old_index: usize, new_index: usize) {
        // From a stack rather than by recursion, since vectors nest tables
        // without a limit
        let mut entering = vec![(old_index, new_index, None)];

        while let Some((old_index, new_index, holder)) = entering.pop() {
            let old = self.old.definition(old_index);
            let new = self.new.definition(new_index);
            if old.is_struct() || new.is_struct() {
                continue;
            }

            let next = self.tables.len();
            let place = *self.places.entry((old_index, new_index)).or_insert(next);
            if place < next {
                self.tables[place].holders.extend(holder);
                continue;
            }

            let mut relation = match old.fields.len().cmp(&new.fields.len()) {
                std::cmp::Ordering::Less => Relation::Extended,
                std::cmp::Ordering::Equal => Relation::Same,
                std::cmp::Ordering::Greater => Relation::Different,
            };
            for (old_field, new_field) in old.fields.iter().zip(&new.fields) {
                match (old_field.field_type, new_field.field_type) {
                    (FieldType::Table(old_table), FieldType::Table(new_table))
                    | (FieldType::Vector(old_table), FieldType::Vector(new_table)) => {
                        entering.push((old_table, new_table, Some(place)));
                    }
                    (old_type, new_type) => {
                        relation = relation.min(self.value_relation(old_type, new_type));
                    }
                }
            }

            self.tables.push(TablePair {
                relation,
                holders: holder.into_iter().collect(),
            });
        }
    }

    /// Gives each pair the relation of the pairs that it holds too, once
    /// every pair is added: a table relates to another no closer than the
    /// tables in it do
    fn settle(&mut self) {
        let mut changed: Vec<usize> = (0..self.tables.len())
            .filter(|&place| self.tables[place].relation != Relation::Same)
            .collect();

        while let Some(place) = changed.pop() {
            let relation = self.tables[place].relation;
            let holders = std::mem::take(&mut self.tables[place].holders);
            for &holder in &holders {
                if self.tables[holder].relation > relation {
                    self.tables[holder].relation = relation;
                    changed.push(holder);
                }
            }
            self.tables[place].holders = holders;
        }
    }

    /// The relation of a pair that was added, once they're settled
    fn table(&self, old_index: usize, new_index: usize) -> Relation {
        self.places
            .get(&(old_index, new_index))
            .map_or(Relation::Different, |&place| self.tables[place].relation)
    }

    /// The relation of two structs: of their scalars in order, whichever
    /// structs group them
    fn structs(&mut self, old_index: usize, new_index: usize) -> Relation {
        self.scalars_relation(FieldType::Struct(old_index), FieldType::Struct(new_index))
    }

    /// The relation of two fields that are each a scalar or a struct, in
    /// place: of their scalars in order
    fn scalars_relation(&mut self, old_type: FieldType, new_type: FieldType) -> Relation {
        let old_scalars = scalars_of(self.old, old_type, &mut self.old_scalars);
        let new_scalars = scalars_of(self.new, new_type, &mut self.new_scalars);

        if !new_scalars.starts_with(old_scalars) {
            Relation::Different
        } else if new_scalars.len() > old_scalars.len() {
            Relation::Extended
        } else {
            Relation::Same
        }
    }

    /// The relation of the values of two fields that aren't both nested
    /// tables or both vectors. No scalar's bytes are another's: a `uint8`
    /// that isn't 0 or 1 reads as another `bool` than was written, and an
    /// `int8` as another `uint8`. A string's aren't a `[uint8]`'s either,
    /// which needn't be UTF-8.
    fn value_relation(&mut self, old_type: FieldType, new_type: FieldType) -> Relation {
        match (old_type, new_type) {
            (FieldType::Array(old), FieldType::Array(new)) if old == new => Relation::Same,
            (FieldType::String, FieldType::String) => Relation::Same,
            (
                FieldType::Scalar(_) | FieldType::Struct(_),
                FieldType::Scalar(_) | FieldType::Struct(_),
            ) => self.scalars_relation(old_type, new_type),
            // A reader finds struct `i` at `i` times the size that it knows
            (FieldType::StructArray(old), FieldType::StructArray(new)) => {
                match self.structs(old, new) {
                    Relation::Same => Relation::Same,
                    Relation::Extended | Relation::Different => Relation::Different,
                }
            }
            _ => Relation::Different,
        }
    }

    /// The relation of the values of the fields of two tables that were
    /// added, once they're settled
    fn field(&mut self, old_type: FieldType, new_type: FieldType) -> Relation {
        match (old_type, new_type) {
            (FieldType::Table(old), FieldType::Table(new))
            | (FieldType::Vector(old), FieldType::Vector(new)) => self.table(old, new),
            _ => self.value_relation(old_type, new_type),
        }
    }
}

/// The scalars of a field that's a scalar or a struct, in order, those of a
/// struct found once and kept in `structs` by its index
fn scalars_of<'a>(
    schema: &Schema,
    field_type: FieldType,
    structs: &'a mut [Option<Vec<Scalar>>],
) -> &'a [Scalar] {
    match field_type {
        FieldType::Scalar(scalar) => {
            let index = Scalar::ALL
                .iter()
                .position(|&other| other == scalar)
                .expect("every scalar is listed");
            &Scalar::ALL[index..=index]
        }
        FieldType::Struct(index) => {
            structs[index].get_or_insert_with(|| schema.struct_scalars(schema.definition(index)))
        }
        _ => unreachable!("only scalars and structs are scalars in place"),
    }
}

struct Comparer<'a> {
    old: &'a Schema,
    new: &'a Schema,
    declarations: &'a [Declaration],
    relations: Relations<'a>,
    /// Whether a message of the new schema holds each of its definitions
    held: Vec<bool>,
    diagnostics: Vec<Diagnostic>,
}

impl Comparer<'_> {
    /// The new schema's other messages that may read the tables of an old
    /// message whose version changes: those with its layout and its version
    fn readers_of_old(&self, old_index: usize, new_index: usize) -> Vec<usize> {
        let Kind::Message(old_layout) = self.old.definition(old_index).kind else {
            return Vec::new();
        };
        if old_layout.version.is_none()
            || self.new.definition(new_index).layout() == Some(old_layout)
        {
            return Vec::new();
        }

        (0..self.new.definitions.len())
            .filter(|&index| self.new.definition(index).layout() == Some(old_layout))
            .collect()
    }

    /// Reports how the new declaration at `new_index` differs from the old
    /// one of its name
    fn compare(&mut self, old_index: usize, new_index: usize) {
        let old = self.old.definition(old_index);
        let new = self.new.definition(new_index);
        let name = self.declarations[new_index].name.span;

        match (table_layout(old), table_layout(new)) {
            (Some(old_layout), Some(new_layout)) => {
                self.compare_layouts(new_index, old_layout, new_layout);
                self.compare_table_fields(old_index, new_index);
                self.check_version(old_index, new_index, old_layout, new_layout);
            }
            (None, None) => self.compare_structs(old_index, new_index),
            (old_layout, _) => {
                let [was, is] = if old_layout.is_some() {
                    ["a table", "a struct"]
                } else {
                    ["a struct", "a table"]
                };
                self.diagnostics.push(
                    Diagnostic::new(format!("`{}` was {was}, and is {is}", new.name), name)
                        .with_help(format!("{was}'s bytes don't read as {is}")),
                );
            }
        }
    }

    /// A message keeps its layout, which is a nested table's when it's
    /// declared as a table instead
    fn compare_layouts(&mut self, new_index: usize, old: Layout, new: Layout) {
        let declaration = &self.declarations[new_index];
        let name = &declaration.name.text;
        let span_of = |attribute: &str| {
            find_attribute(declaration, attribute)
                .map_or(declaration.name.span, |attribute| attribute.name.span)
        };

        if old.offset_type != new.offset_type {
            let span = find_attribute(declaration, "offset_type")
                .and_then(|attribute| attribute.value.as_ref())
                .map_or(declaration.name.span, crate::parser::AttributeValue::span);
            self.diagnostics.push(
                Diagnostic::new(
                    format!(
                        "`{name}` changes its offset type from `{}` to `{}`",
                        old.offset_type.name(),
                        new.offset_type.name()
                    ),
                    span,
                )
                .with_help("its header, offsets and counts change their width"),
            );
        }
        if old.sized != new.sized {
            let change = if new.sized { "gains" } else { "loses" };
            self.diagnostics.push(
                Diagnostic::new(
                    format!("`{name}` {change} the length in its header"),
                    span_of("sized"),
                )
                .with_help("the version and every offset move by a slot"),
            );
        }
        if old.version.is_some() != new.version.is_some() {
            let change = if new.version.is_some() {
                "gains"
            } else {
                "loses"
            };
            self.diagnostics.push(
                Diagnostic::new(
                    format!("`{name}` {change} the version in its header"),
                    span_of("version"),
                )
                .with_help("every offset moves by a slot"),
            );
        }
    }

    /// A message or a table only appends fields, each of which keeps its
    /// index and its type
    fn compare_table_fields(&mut self, old_index: usize, new_index: usize) {
        let old = self.old.definition(old_index);
        let new = self.new.definition(new_index);
        let declaration = &self.declarations[new_index];

        for (index, (old_field, new_field)) in old.fields.iter().zip(&new.fields).enumerate() {
            let field = &declaration.fields[index];
            let old_type = old_field.field_type;
            let new_type = new_field.field_type;
            let relation = self.relations.field(old_type, new_type);
            let named_alike = type_name(self.old, old_type) == type_name(self.new, new_type);

            if relation == Relation::Different && !named_alike {
                self.diagnostics.push(self.type_change(
                    index,
                    old_index,
                    new_index,
                    field.type_name.span,
                ));
            } else if let (FieldType::StructArray(old_struct), FieldType::StructArray(new_struct)) =
                (old_type, new_type)
                && self.relations.structs(old_struct, new_struct) == Relation::Extended
            {
                // The struct itself only grows, which this array can't
                self.diagnostics.push(
                    Diagnostic::new(
                        format!(
                            "`{}` is an array of `{}`, whose size changes",
                            new_field.name,
                            self.new.definition(new_struct).name
                        ),
                        field.type_name.span,
                    )
                    .with_help("a reader finds each struct of an array by the size that it knows"),
                );
            } else if old_field.name != new_field.name && relation != Relation::Different {
                self.diagnostics
                    .push(renamed(index, &old_field.name, field.name.span));
            }
        }

        if let Some(lost) = lost_fields(old, new) {
            self.diagnostics.push(
                Diagnostic::new(
                    format!("`{}` loses {lost}", new.name),
                    declaration.name.span,
                )
                .with_help(
                    "a version only appends fields: a reader of the old fields reads \
                     their offsets from the bytes of the new table's values",
                ),
            );
        }

        // A message's version stands for its tables' fields too, so a table
        // that grows where no message holds it has nothing to tell its old
        // bytes from its new
        if new.kind == Kind::Table
            && !self.held[new_index]
            && self.relations.table(old_index, new_index) == Relation::Extended
        {
            self.diagnostics.push(
                Diagnostic::new(
                    format!("`{}` changes, and no message holds it", new.name),
                    declaration.name.span,
                )
                .with_help("only a message's version tells a table's old fields from its new ones"),
            );
        }
    }

    /// A message whose tables change changes its version, since a reader of
    /// the new fields reads the old tables' missing offsets from their
    /// values' bytes
    fn check_version(&mut self, old_index: usize, new_index: usize, old: Layout, new: Layout) {
        let new_definition = self.new.definition(new_index);
        let Kind::Message(_) = new_definition.kind else {
            return;
        };
        let declaration = &self.declarations[new_index];
        let name = &new_definition.name;
        let version_span = find_attribute(declaration, "version")
            .and_then(|attribute| attribute.value.as_ref())
            .map_or(declaration.name.span, crate::parser::AttributeValue::span);

        let relation = self.relations.table(old_index, new_index);
        match (old.version, new.version) {
            (Some(old_version), Some(new_version)) if old_version == new_version => {
                if relation != Relation::Same {
                    let help = self.change_of(old_index, new_index);
                    self.diagnostics.push(
                        Diagnostic::new(
                            format!("`{name}` changes, and keeps version {new_version}"),
                            version_span,
                        )
                        .with_help(help),
                    );
                }
            }
            (Some(old_version), Some(new_version)) => {
                if new_version < old_version {
                    self.diagnostics.push(
                        Diagnostic::note(
                            format!(
                                "`{name}` goes from version {old_version} back to {new_version}"
                            ),
                            version_span,
                        )
                        .with_help(
                            "a version that older tables hold would stand for two sets of fields",
                        ),
                    );
                }
                if !self.is_read_by_another(old_index, new_index) {
                    self.diagnostics.push(
                        Diagnostic::note(
                            format!("no message reads version {old_version} of `{name}` any more"),
                            version_span,
                        )
                        .with_help(format!(
                            "to read the tables written before, declare their fields beside it, \
                             such as in `{name}V{old_version}`"
                        )),
                    );
                }
            }
            (None, None) => {
                if relation != Relation::Same {
                    let help = self.change_of(old_index, new_index);
                    self.diagnostics.push(
                        Diagnostic::new(
                            format!("`{name}` changes, and has no version"),
                            declaration.name.span,
                        )
                        .with_help(help),
                    );
                }
            }
            // The layouts differ, which has its error
            (Some(_), None) | (None, Some(_)) => {}
        }
    }

    /// Whether another message of the new schema reads the old message's
    /// tables: one with its layout, its version and the same fields
    fn is_read_by_another(&self, old_index: usize, new_index: usize) -> bool {
        self.readers_of_old(old_index, new_index)
            .into_iter()
            .any(|reader| self.relations.table(old_index, reader) == Relation::Same)
    }

    /// What changes in a message whose version should: its own fields, or
    /// the first of them whose table or struct does
    fn change_of(&mut self, old_index: usize, new_index: usize) -> String {
        let old = self.old.definition(old_index);
        let new = self.new.definition(new_index);

        for (old_field, new_field) in old.fields.iter().zip(&new.fields) {
            let relation = self
                .relations
                .field(old_field.field_type, new_field.field_type);
            if relation == Relation::Same {
                continue;
            }
            let named_alike = type_name(self.old, old_field.field_type)
                == type_name(self.new, new_field.field_type);
            return match definition_of(new_field.field_type) {
                Some(held) if named_alike => format!(
                    "`{}` changes, which `{}` holds, and only the version tells its old tables from its new ones",
                    self.new.definition(held).name,
                    new_field.name
                ),
                _ => format!(
                    "`{}` changes, and only the version tells its old tables from its new ones",
                    new_field.name
                ),
            };
        }

        // The fields that the two share are the same, so the new one
        // appends fields to the old, or lacks some of them
        match new.fields.get(old.fields.len()) {
            Some(appended) => format!(
                "`{}` is appended, and only the version tells the old tables from the new ones",
                appended.name
            ),
            None => format!(
                "its tables held {} fields before, and only the version tells them from the new ones",
                old.fields.len()
            ),
        }
    }

    /// A struct keeps its scalars, since they lie at fixed offsets, or
    /// appends to them where a message's version tells its old bytes from
    /// its new
    fn compare_structs(&mut self, old_index: usize, new_index: usize) {
        let old = self.old.definition(old_index);
        let new = self.new.definition(new_index);
        let declaration = &self.declarations[new_index];
        let relation = self.relations.structs(old_index, new_index);

        for (index, (old_field, new_field)) in old.fields.iter().zip(&new.fields).enumerate() {
            let field = &declaration.fields[index];
            let old_type = old_field.field_type;
            let new_type = new_field.field_type;
            let field_relation = self.relations.value_relation(old_type, new_type);
            let named_alike = type_name(self.old, old_type) == type_name(self.new, new_type);
            let is_last = index + 1 == new.fields.len();

            if relation != Relation::Same && field_relation == Relation::Different && !named_alike {
                self.diagnostics.push(self.type_change(
                    index,
                    old_index,
                    new_index,
                    field.type_name.span,
                ));
            } else if relation != Relation::Same && field_relation == Relation::Extended && !is_last
            {
                self.diagnostics.push(
                    Diagnostic::new(
                        format!(
                            "`{}` grows, which moves the fields after it in `{}`",
                            type_name(self.new, new_type),
                            new.name
                        ),
                        field.type_name.span,
                    )
                    .with_help("a struct's scalars lie at fixed offsets"),
                );
            } else if old_field.name != new_field.name && field_relation == Relation::Same {
                self.diagnostics
                    .push(renamed(index, &old_field.name, field.name.span));
            }
        }

        if let Some(lost) = lost_fields(old, new) {
            self.diagnostics.push(
                Diagnostic::new(
                    format!("`{}` loses {lost}", new.name),
                    declaration.name.span,
                )
                .with_help("a reader of the old struct reads past the new one's bytes"),
            );
        }

        if relation == Relation::Extended && !self.held[new_index] {
            self.diagnostics.push(
                Diagnostic::new(
                    format!("`{}` grows, and no message holds it", new.name),
                    declaration.name.span,
                )
                .with_help(
                    "a struct that's read on its own has no version, so a reader of the new \
                     struct reads past the old one's bytes",
                ),
            );
        }
    }

    /// The error of a field whose type isn't the one that the old schema
    /// gives its index
    fn type_change(
        &self,
        index: usize,
        old_index: usize,
        new_index: usize,
        span: Span,
    ) -> Diagnostic {
        let old_field = &self.old.definition(old_index).fields[index];
        let new_field = &self.new.definition(new_index).fields[index];
        let old_type = type_name(self.old, old_field.field_type);
        let new_type = type_name(self.new, new_field.field_type);

        let message = if old_field.name == new_field.name {
            format!(
                "`{}` changes its type from `{old_type}` to `{new_type}`",
                new_field.name
            )
        } else {
            format!(
                "field {index} changes from `{}: {old_type}` to `{}: {new_type}`",
                old_field.name, new_field.name
            )
        };
        Diagnostic::new(message, span).with_help(
            "a field keeps its index and its type, and new fields come after the old ones",
        )
    }
}

/// A field's type as schemas spell it
fn type_name(schema: &Schema, field_type: FieldType) -> String {
    match field_type {
        FieldType::Scalar(scalar) => scalar.name().to_owned(),
        FieldType::Array(scalar) => format!("[{}]", scalar.name()),
        FieldType::String => "string".to_owned(),
        FieldType::Struct(index) | FieldType::Table(index) => schema.definition(index).name.clone(),
        FieldType::StructArray(index) | FieldType::Vector(index) => {
            format!("[{}]", schema.definition(index).name)
        }
    }
}

fn find_attribute<'a>(declaration: &'a Declaration, name: &str) -> Option<&'a Attribute> {
    declaration
        .attributes
        .iter()
        .find(|attribute| attribute.name.text == name)
}

/// The note of a field that keeps its bytes under another name
fn renamed(index: usize, old_name: &str, span: Span) -> Diagnostic {
    Diagnostic::note(format!("field {index} was named `{old_name}`"), span)
        .with_help("its bytes are the same, and the generated code names it anew")
}

/// The old fields that the new definition doesn't have, as an error names
/// them, if there are any
fn lost_fields(old: &Definition, new: &Definition) -> Option<String> {
    match old.fields.get(new.fields.len()..)? {
        [] => None,
        [field] => Some(format!("its field `{}`", field.name)),
        fields => Some(format!(
            "its last {} fields, from `{}`",
            fields.len(),
            fields[0].name
        )),
    }
}
