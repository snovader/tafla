//! The fields whose sizes the schema fixes: scalars, structs, and nested
//! tables of only such fields, which have no padding. Generated code that
//! lays them out itself, at positions that the generator works out, writes
//! them without the runtime's sizer and writer, as Go's, Rust's and C++'s
//! do.

use crate::model::{FieldType, Kind, OffsetType, Schema};

/// The offset size of nested tables, which have `uint32` offsets, since the
/// checker rejects tables nested in messages of a narrower offset type
pub const NESTED_OFFSET_SIZE: usize = 4;

/// The width of an offset type's header slots, offsets and counts
pub const fn offset_size(offset_type: OffsetType) -> usize {
    match offset_type {
        OffsetType::Uint8 => 1,
        OffsetType::Uint16 => 2,
        OffsetType::Uint32 => 4,
    }
}

/// The byte length of each nested table whose fields' sizes the schema
/// fixes, which generated code may write whole: its offsets and its fields
pub struct FixedSizes {
    /// By each definition's index, and `None` for every other definition
    tables: Vec<Option<usize>>,
}

impl FixedSizes {
    /// The sizes of the schema's tables of fixed fields, up to `max` bytes;
    /// longer ones are left out, as a target's positions may not hold them
    pub fn new(schema: &Schema, max: usize) -> Self {
        let mut tables = vec![None; schema.definitions.len()];
        let mut visited = vec![false; schema.definitions.len()];
        for index in 0..schema.definitions.len() {
            fixed_table_size(schema, index, max, &mut tables, &mut visited);
        }

        Self { tables }
    }

    /// The byte length of the table at `index`, when its fields' sizes are
    /// all fixed
    pub fn table(&self, index: usize) -> Option<usize> {
        self.tables[index]
    }

    /// The byte length of a field whose size the schema fixes: a scalar, a
    /// struct, or a table of only such fields
    pub fn field(&self, schema: &Schema, field_type: FieldType) -> Option<usize> {
        fixed_field_size(schema, field_type, |index| self.tables[index])
    }
}

/// Finds the byte length of the table at `index` when its fields' sizes are
/// all fixed, and its nested tables' first, into `sizes` by its index, which
/// is `None` for any other definition, or one longer than `max`, and marks
/// it `visited`. Tables nest in place at most 64 deep, and each is sized once.
fn fixed_table_size(
    schema: &Schema,
    index: usize,
    max: usize,
    sizes: &mut [Option<usize>],
    visited: &mut [bool],
) -> Option<usize> {
    if visited[index] {
        return sizes[index];
    }

    let definition = schema.definition(index);
    let mut size = Some(definition.fields.len().saturating_mul(NESTED_OFFSET_SIZE));
    if definition.kind != Kind::Table {
        size = None;
    }
    for field in &definition.fields {
        let Some(table) = size else {
            break;
        };
        let field_size = fixed_field_size(schema, field.field_type, |nested| {
            fixed_table_size(schema, nested, max, sizes, visited)
        });
        size = field_size.map(|field_size| table.saturating_add(field_size));
    }

    let size = size.filter(|&size| size <= max);
    sizes[index] = size;
    visited[index] = true;
    size
}

/// The byte length of a field whose size the schema fixes: a scalar's, a
/// struct's, or the length of a nested table that `table` gives, when its
/// fields' sizes are all fixed
fn fixed_field_size(
    schema: &Schema,
    field_type: FieldType,
    table: impl FnOnce(usize) -> Option<usize>,
) -> Option<usize> {
    match field_type {
        FieldType::Scalar(scalar) => Some(usize::from(scalar.size())),
        FieldType::Struct(index) => Some(schema.definition(index).struct_layout().size),
        FieldType::Table(index) => table(index),
        FieldType::Array(_)
        | FieldType::String
        | FieldType::StructArray(_)
        | FieldType::Vector(_) => None,
    }
}
