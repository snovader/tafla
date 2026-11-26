//! The fields whose sizes the schema fixes: scalars, structs, and nested
//! tables of only such fields, which have no padding. Generated code that
//! lays them out itself, at positions that the generator works out, writes
//! them without the runtime's sizer and writer, as Go's, Rust's, C++'s and
//! TypeScript's do. A schema's description gives the same sizes, and the
//! offsets that follow from them.

use crate::model::{Definition, FieldType, Kind, Layout, OffsetType, Schema};

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

/// The byte length of a layout's header: its length and its version, each a
/// slot of the offset type when the layout has it
pub const fn header_size(layout: Layout) -> usize {
    (layout.sized as usize + layout.version.is_some() as usize) * offset_size(layout.offset_type)
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
        fixed_field_size(schema, field_type, |index| self.table(index))
    }

    /// Where a message's or a table's writer puts its fields, as far as the
    /// schema fixes it
    ///
    /// # Panics
    ///
    /// If the definition is a struct, which has no offsets.
    pub fn offsets(&self, schema: &Schema, definition: &Definition) -> FieldOffsets {
        let (header, width, max) = match definition.kind {
            Kind::Message(layout) => (
                header_size(layout),
                offset_size(layout.offset_type),
                layout.offset_type.max_value(),
            ),
            Kind::Table => (0, NESTED_OFFSET_SIZE, OffsetType::Uint32.max_value()),
            Kind::Struct(_) => unreachable!("only messages and tables have offsets"),
        };
        let max = usize::try_from(max).unwrap_or(usize::MAX);

        // The first value follows the header and an offset of each field
        let mut position = definition
            .fields
            .len()
            .checked_mul(width)
            .and_then(|offsets| offsets.checked_add(header))
            .filter(|&values| values <= max);
        let mut fields = Vec::with_capacity(definition.fields.len());

        for field in &definition.fields {
            let size = self.field(schema, field.field_type);
            let offset = position
                .and_then(|position| {
                    let padding = padding_before(schema, field.field_type, position, width);
                    position.checked_add(padding)
                })
                .filter(|&offset| offset <= max);
            fields.push(FieldOffset { offset, size });

            position = offset
                .zip(size)
                .and_then(|(offset, size)| offset.checked_add(size))
                .filter(|&end| end <= max);
        }

        FieldOffsets {
            fields,
            size: position,
        }
    }
}

/// Where a writer puts the fields of a message or a table, from
/// [`FixedSizes::offsets()`]
pub struct FieldOffsets {
    /// Each field's, by its index
    pub fields: Vec<FieldOffset>,
    /// The byte length of the table, header included, when every field's size
    /// is fixed and the table fits in its offset type
    pub size: Option<usize>,
}

/// Where a writer puts a field's value, which the schema fixes for a table's
/// leading fields of fixed size and for the first field after them
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldOffset {
    /// The value's position from the start of its table, which the writer
    /// writes as the field's offset: `None` after a field whose size depends
    /// on its value, or past what the table's offset type addresses
    pub offset: Option<usize>,
    /// The value's byte length, when the schema fixes it
    pub size: Option<usize>,
}

/// The zeros that a writer pads with before a field's value at `position`
/// from the start of its table, as `SPEC.md` describes: before an array's
/// count, of `width` bytes, so that its first element is aligned, and before
/// a nested table, to the table's alignment. Alignments are powers of two,
/// so a sum that wraps pads alike.
fn padding_before(schema: &Schema, field_type: FieldType, position: usize, width: usize) -> usize {
    let (aligned, alignment) = match field_type {
        FieldType::Array(scalar) => (position.wrapping_add(width), scalar.size()),
        FieldType::StructArray(index) => (
            position.wrapping_add(width),
            schema.definition(index).struct_layout().alignment,
        ),
        FieldType::Table(index) => (position, schema.table_alignment(index)),
        FieldType::Scalar(_) | FieldType::String | FieldType::Struct(_) | FieldType::Vector(_) => {
            return 0;
        }
    };

    aligned.wrapping_neg() % usize::from(alignment)
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
