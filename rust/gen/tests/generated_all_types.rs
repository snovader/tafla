//! The code generated from `schemas/all_types.tafla`, which has a field of
//! every type, writes the tables that `TableBuilder` writes by hand, reads
//! back what it writes, and reads corrupted and arbitrary bytes as values or
//! decode errors without panicking. Its structs are written and read at
//! their fixed sizes, as fields and in arrays. Its enums are written and read
//! as the integers that store them, whether the schema names them or not.
//! Its vectors of strings read every string whole as they read each, and a
//! string that isn't valid UTF-8 fails alone.

#[path = "golden/all_types/rust/all_types.rs"]
mod all_types;
mod tables;

use std::collections::HashSet;
use std::fmt::Debug;
use std::ops::Range;

use all_types::{
    Arrays, ArraysFields, Change, Direction, Documented, Entry, EntryFields, EntryVector, Enums,
    EnumsFields, Extent, Inner, InnerFields, Keywords, KeywordsFields, Labels, LabelsFields, Level,
    Mark, Mask, NarrowStringVectors, NarrowStringVectorsFields, Nested, NestedFields, Outer,
    OuterFields, Packed, Path, PathFields, Point, Reserved, ReservedFields, Scalars, ScalarsFields,
    Segment, Sign, Status, Step, StepFields, StepVector, StringVectors, StringVectorsFields,
    Structs, StructsFields, Tree, TreeFields, TreeVector, Turn, Unit, Word,
};
use proptest::arbitrary::Arbitrary;
use proptest::array::uniform5;
use proptest::collection::vec;
use proptest::prelude::*;
use tables::{
    CASES, Fields, Header, MAX_LENGTH, Value, arbitrary_bytes, arrays, build, check_truncated,
    expected, float32s, float64s, integers, overwrites, placements, strings, struct_bytes, table,
    to_vec, truncations, vector,
};
use tafla::{
    Alignment, BuildError, DecodeError, OffsetType, StringVector, TableLayout, TableReader,
};

/// The header of every message but `NarrowStringVectors`, none of which has
/// attributes
const MESSAGE: Header = Header::new(TableLayout::DEFAULT);

/// The header of `NarrowStringVectors`, of `uint16` offsets
const NARROW: Header = Header::new(TableLayout::DEFAULT.with_offset_type(OffsetType::Uint16));

// Reading every field with the generated getters, including those after one
// that fails

fn read_scalars(scalars: &Scalars) -> Fields {
    vec![
        scalars.int8().map(Value::I8),
        scalars.uint8().map(Value::U8),
        scalars.int16().map(Value::I16),
        scalars.uint16().map(Value::U16),
        scalars.int32().map(Value::I32),
        scalars.uint32().map(Value::U32),
        scalars.int64().map(Value::I64),
        scalars.uint64().map(Value::U64),
        scalars.float32().map(Value::f32),
        scalars.float64().map(Value::f64),
        scalars.bool().map(Value::Bool),
    ]
}

fn read_arrays(arrays: &Arrays) -> Fields {
    vec![
        arrays.int8().map(to_vec).map(Value::I8Array),
        arrays.uint8().map(<[u8]>::to_vec).map(Value::U8Array),
        arrays.int16().map(to_vec).map(Value::I16Array),
        arrays.uint16().map(to_vec).map(Value::U16Array),
        arrays.int32().map(to_vec).map(Value::I32Array),
        arrays.uint32().map(to_vec).map(Value::U32Array),
        arrays.int64().map(to_vec).map(Value::I64Array),
        arrays.uint64().map(to_vec).map(Value::U64Array),
        arrays.float32().map(Value::f32_array),
        arrays.float64().map(Value::f64_array),
    ]
}

fn read_outer(outer: &Outer) -> Fields {
    vec![
        outer.count().map(Value::U32),
        outer.inner().and_then(|inner| table(read_inner(&inner))),
    ]
}

fn read_inner(inner: &Inner) -> Fields {
    vec![
        inner.name().map(Value::string),
        inner.values().map(Value::f64_array),
    ]
}

fn read_reserved(reserved: &Reserved) -> Fields {
    vec![
        reserved
            .keywords()
            .and_then(|keywords| table(read_keywords(&keywords))),
    ]
}

fn read_keywords(keywords: &Keywords) -> Fields {
    vec![
        keywords.class().map(Value::U8),
        keywords.default().map(Value::U8),
        keywords.r#in().map(Value::U8),
        keywords.r#match().map(Value::U8),
        keywords.self_().map(Value::U8),
        keywords.r#type().map(Value::string),
    ]
}

// Reading the vectors and arrays of structs through the views that their
// getters return

/// Ways to read a view's items: by index from the last, as a reader that
/// skips items would, and in order with its iterator
#[derive(Clone, Copy, Debug)]
enum ViewOrder {
    FromLast,
    Iterated,
}

impl ViewOrder {
    const ALL: [Self; 2] = [Self::FromLast, Self::Iterated];

    /// The view's items in order, from `get()` or `iter()`
    fn items<T>(
        self,
        len: usize,
        get: impl Fn(usize) -> Option<T>,
        iter: impl Iterator<Item = T>,
    ) -> Vec<T> {
        match self {
            Self::FromLast => {
                let mut items: Vec<T> = (0..len)
                    .rev()
                    .map(|index| get(index).expect("an item within the view"))
                    .collect();
                items.reverse();
                items
            }
            Self::Iterated => iter.collect(),
        }
    }
}

fn read_nested(nested: &Nested, order: ViewOrder) -> Fields {
    vec![
        nested.label().map(Value::string),
        nested.outer().and_then(|outer| table(read_outer(&outer))),
        nested.trees().and_then(|trees| tree_vector(trees, order)),
    ]
}

fn read_tree(tree: &Tree, order: ViewOrder) -> Fields {
    vec![
        tree.value().map(Value::I32),
        tree.children()
            .and_then(|children| tree_vector(children, order)),
    ]
}

/// The value of a vector of trees, or the first error of its items in order
fn tree_vector(view: TreeVector<'_>, order: ViewOrder) -> Result<Value, DecodeError> {
    let trees: Vec<Tree> = order
        .items(view.len(), |index| view.get(index), view.iter())
        .into_iter()
        .collect::<Result<_, _>>()?;

    vector(trees.iter().map(|tree| read_tree(tree, order)))
}

fn read_structs(structs: &Structs, order: ViewOrder) -> Fields {
    vec![
        structs
            .packed()
            .map(|packed| Value::Struct(describe_packed(&packed))),
        structs.documented().map(|view| {
            let items = order.items(view.len(), |index| view.get(index), view.iter());
            struct_array(
                Documented::SIZE,
                Documented::ALIGNMENT,
                items.into_iter().map(describe_documented),
            )
        }),
        structs
            .segment()
            .map(|segment| Value::Struct(describe_segment(segment))),
        structs.segments().map(|view| {
            let items = order.items(view.len(), |index| view.get(index), view.iter());
            struct_array(
                Segment::SIZE,
                Segment::ALIGNMENT,
                items.into_iter().map(describe_segment),
            )
        }),
        structs
            .path()
            .and_then(|path| table(read_path(&path, order))),
    ]
}

fn read_path(path: &Path, order: ViewOrder) -> Fields {
    vec![
        path.start()
            .map(|start| Value::Struct(describe_point(start))),
        path.points().map(|view| {
            let items = order.items(view.len(), |index| view.get(index), view.iter());
            struct_array(
                Point::SIZE,
                Point::ALIGNMENT,
                items.into_iter().map(describe_point),
            )
        }),
    ]
}

/// Reads each enum as the integer that stores it
fn read_enums(enums: &Enums, order: ViewOrder) -> Fields {
    vec![
        enums.level().map(|level| Value::U8(level.0)),
        enums.levels().map(<[u8]>::to_vec).map(Value::U8Array),
        enums.direction().map(|direction| Value::I8(direction.0)),
        enums.directions().map(to_vec).map(Value::I8Array),
        enums.unit().map(|unit| Value::U16(unit.0)),
        enums.units().map(to_vec).map(Value::U16Array),
        enums.change().map(|change| Value::I16(change.0)),
        enums.changes().map(to_vec).map(Value::I16Array),
        enums.status().map(|status| Value::U32(status.0)),
        enums.statuses().map(to_vec).map(Value::U32Array),
        enums.sign().map(|sign| Value::I32(sign.0)),
        enums.signs().map(to_vec).map(Value::I32Array),
        enums.mask().map(|mask| Value::U64(mask.0)),
        enums.masks().map(to_vec).map(Value::U64Array),
        enums.extent().map(|extent| Value::I64(extent.0)),
        enums.extents().map(to_vec).map(Value::I64Array),
        enums.word().map(|word| Value::U8(word.0)),
        enums.mark().map(|mark| Value::Struct(describe_mark(mark))),
        enums.marks().map(|view| {
            let items = order.items(view.len(), |index| view.get(index), view.iter());
            struct_array(
                Mark::SIZE,
                Mark::ALIGNMENT,
                items.into_iter().map(describe_mark),
            )
        }),
        enums.steps().and_then(|steps| step_vector(steps, order)),
    ]
}

fn read_step(step: &Step) -> Fields {
    vec![
        step.level().map(|level| Value::U8(level.0)),
        step.mark().map(|mark| Value::Struct(describe_mark(mark))),
        step.name().map(Value::string),
    ]
}

/// The value of a vector of steps, or the first error of its items in order
fn step_vector(view: StepVector<'_>, order: ViewOrder) -> Result<Value, DecodeError> {
    let steps: Vec<Step> = order
        .items(view.len(), |index| view.get(index), view.iter())
        .into_iter()
        .collect::<Result<_, _>>()?;

    vector(steps.iter().map(read_step))
}

fn read_string_vectors(string_vectors: &StringVectors, order: ViewOrder) -> Fields {
    vec![
        string_vectors.id().map(Value::U32),
        string_vectors
            .tags()
            .and_then(|tags| string_vector(tags, order)),
        string_vectors
            .labels()
            .and_then(|labels| table(read_labels(&labels, order))),
        string_vectors
            .entries()
            .and_then(|entries| entry_vector(entries, order)),
    ]
}

fn read_labels(labels: &Labels, order: ViewOrder) -> Fields {
    vec![
        labels
            .values()
            .and_then(|values| string_vector(values, order)),
    ]
}

fn read_entry(entry: &Entry, order: ViewOrder) -> Fields {
    vec![
        entry.key().map(Value::string),
        entry
            .aliases()
            .and_then(|aliases| string_vector(aliases, order)),
    ]
}

/// The value of a vector of entries, or the first error of its items in
/// order
fn entry_vector(view: EntryVector<'_>, order: ViewOrder) -> Result<Value, DecodeError> {
    let entries: Vec<Entry> = order
        .items(view.len(), |index| view.get(index), view.iter())
        .into_iter()
        .collect::<Result<_, _>>()?;

    vector(entries.iter().map(|entry| read_entry(entry, order)))
}

fn read_narrow(narrow: &NarrowStringVectors, order: ViewOrder) -> Fields {
    vec![
        narrow.names().and_then(|names| string_vector(names, order)),
        narrow.count().map(Value::U8),
    ]
}

/// The value of a vector of strings, or the error of its first string that
/// fails, read a string at a time in `order`, after checking that reading
/// them whole with `to_vec()` reads the same
fn string_vector(view: StringVector<'_>, order: ViewOrder) -> Result<Value, DecodeError> {
    let each = order
        .items(view.len(), |index| view.get(index), view.iter())
        .into_iter()
        .collect::<Result<Vec<_>, _>>();
    assert_eq!(view.to_vec(), each, "to_vec() reads what each string reads");

    each.map(|strings| Value::strings(&strings))
}

fn struct_array(
    size: usize,
    alignment: Alignment,
    items: impl Iterator<Item = Vec<Value>>,
) -> Value {
    Value::StructArray {
        size,
        alignment,
        items: items.collect(),
    }
}

// The values of the fields, of their types in the schema

fn describe_scalars(fields: &ScalarsFields) -> Vec<Value> {
    vec![
        Value::I8(fields.int8),
        Value::U8(fields.uint8),
        Value::I16(fields.int16),
        Value::U16(fields.uint16),
        Value::I32(fields.int32),
        Value::U32(fields.uint32),
        Value::I64(fields.int64),
        Value::U64(fields.uint64),
        Value::f32(fields.float32),
        Value::f64(fields.float64),
        Value::Bool(fields.bool),
    ]
}

fn describe_arrays(fields: &ArraysFields) -> Vec<Value> {
    vec![
        Value::I8Array(fields.int8.to_vec()),
        Value::U8Array(fields.uint8.to_vec()),
        Value::I16Array(fields.int16.to_vec()),
        Value::U16Array(fields.uint16.to_vec()),
        Value::I32Array(fields.int32.to_vec()),
        Value::U32Array(fields.uint32.to_vec()),
        Value::I64Array(fields.int64.to_vec()),
        Value::U64Array(fields.uint64.to_vec()),
        Value::f32_array(fields.float32.iter().copied()),
        Value::f64_array(fields.float64.iter().copied()),
    ]
}

fn describe_nested(fields: &NestedFields) -> Vec<Value> {
    vec![
        Value::string(fields.label),
        Value::Table(describe_outer(&fields.outer)),
        Value::Vector(fields.trees.iter().map(describe_tree).collect()),
    ]
}

fn describe_outer(fields: &OuterFields) -> Vec<Value> {
    vec![
        Value::U32(fields.count),
        Value::Table(describe_inner(&fields.inner)),
    ]
}

fn describe_inner(fields: &InnerFields) -> Vec<Value> {
    vec![
        Value::string(fields.name),
        Value::f64_array(fields.values.iter().copied()),
    ]
}

fn describe_tree(fields: &TreeFields) -> Vec<Value> {
    vec![
        Value::I32(fields.value),
        Value::Vector(fields.children.iter().map(describe_tree).collect()),
    ]
}

fn describe_reserved(fields: &ReservedFields) -> Vec<Value> {
    vec![Value::Table(describe_keywords(&fields.keywords))]
}

fn describe_keywords(fields: &KeywordsFields) -> Vec<Value> {
    vec![
        Value::U8(fields.class),
        Value::U8(fields.default),
        Value::U8(fields.r#in),
        Value::U8(fields.r#match),
        Value::U8(fields.self_),
        Value::string(fields.r#type),
    ]
}

/// The fields of a `Packed`, which is read whole, to compare by their bits
fn describe_packed(packed: &Packed) -> Vec<Value> {
    vec![
        Value::I8(packed.int8),
        Value::U8(packed.uint8),
        Value::I16(packed.int16),
        Value::U16(packed.uint16),
        Value::I32(packed.int32),
        Value::U32(packed.uint32),
        Value::I64(packed.int64),
        Value::U64(packed.uint64),
        Value::f32(packed.float32),
        Value::f64(packed.float64),
        Value::Bool(packed.bool),
    ]
}

fn describe_documented(documented: Documented) -> Vec<Value> {
    vec![
        Value::U8(documented.one),
        Value::U16(documented.two),
        Value::U32(documented.undocumented),
    ]
}

/// The scalars of a `Segment`, those of its points in their place
fn describe_segment(segment: Segment) -> Vec<Value> {
    [describe_point(segment.start), describe_point(segment.end)].concat()
}

fn describe_point(point: Point) -> Vec<Value> {
    vec![Value::f32(point.x), Value::f32(point.y)]
}

/// The values of `Structs`, whose arrays are aligned to their structs'
/// largest scalars, and the scalars of a struct within a struct in its place
fn describe_structs(fields: &StructsFields) -> Vec<Value> {
    vec![
        Value::Struct(describe_packed(&fields.packed)),
        Value::StructArray {
            size: 7,
            alignment: Alignment::Four,
            items: fields
                .documented
                .iter()
                .copied()
                .map(describe_documented)
                .collect(),
        },
        Value::Struct(describe_segment(fields.segment)),
        Value::StructArray {
            size: 16,
            alignment: Alignment::Four,
            items: fields
                .segments
                .iter()
                .copied()
                .map(describe_segment)
                .collect(),
        },
        Value::Table(describe_path(&fields.path)),
    ]
}

fn describe_path(fields: &PathFields) -> Vec<Value> {
    vec![
        Value::Struct(describe_point(fields.start)),
        Value::StructArray {
            size: 8,
            alignment: Alignment::Four,
            items: fields.points.iter().copied().map(describe_point).collect(),
        },
    ]
}

/// The values of `Enums`, whose enums are the integers that store them,
/// from its fields with steps of the tests' own type
fn describe_enums(fields: &EnumsFields<Mark, OwnedStep>) -> Vec<Value> {
    vec![
        Value::U8(fields.level.0),
        Value::U8Array(fields.levels.to_vec()),
        Value::I8(fields.direction.0),
        Value::I8Array(fields.directions.to_vec()),
        Value::U16(fields.unit.0),
        Value::U16Array(fields.units.to_vec()),
        Value::I16(fields.change.0),
        Value::I16Array(fields.changes.to_vec()),
        Value::U32(fields.status.0),
        Value::U32Array(fields.statuses.to_vec()),
        Value::I32(fields.sign.0),
        Value::I32Array(fields.signs.to_vec()),
        Value::U64(fields.mask.0),
        Value::U64Array(fields.masks.to_vec()),
        Value::I64(fields.extent.0),
        Value::I64Array(fields.extents.to_vec()),
        Value::U8(fields.word.0),
        Value::Struct(describe_mark(fields.mark)),
        Value::StructArray {
            size: 24,
            alignment: Alignment::Eight,
            items: fields.marks.iter().copied().map(describe_mark).collect(),
        },
        Value::Vector(fields.steps.iter().map(describe_step).collect()),
    ]
}

fn describe_step(step: &OwnedStep) -> Vec<Value> {
    vec![
        Value::U8(step.level.0),
        Value::Struct(describe_mark(step.mark)),
        Value::string(&step.name),
    ]
}

/// The scalars of a `Mark`, its enums as the integers that store them, and
/// those of its turn in their place
fn describe_mark(mark: Mark) -> Vec<Value> {
    vec![
        Value::U8(mark.level.0),
        Value::I16(mark.change.0),
        Value::f32(mark.weight),
        Value::I64(mark.extent.0),
        Value::I8(mark.turn.direction.0),
        Value::U64(mark.turn.mask.0),
    ]
}

fn describe_string_vectors(fields: &StringVectorsFields) -> Vec<Value> {
    vec![
        Value::U32(fields.id),
        Value::strings(fields.tags),
        Value::Table(describe_labels(&fields.labels)),
        Value::Vector(fields.entries.iter().map(describe_entry).collect()),
    ]
}

fn describe_labels(fields: &LabelsFields) -> Vec<Value> {
    vec![Value::strings(fields.values)]
}

fn describe_entry(fields: &EntryFields) -> Vec<Value> {
    vec![Value::string(fields.key), Value::strings(fields.aliases)]
}

fn describe_narrow(fields: &NarrowStringVectorsFields) -> Vec<Value> {
    vec![Value::strings(fields.names), Value::U8(fields.count)]
}

/// A `Packed`'s fields little-endian at their offsets, one after another,
/// written with the standard library rather than the generated code
fn packed_bytes(packed: &Packed) -> Vec<u8> {
    [
        &packed.int8.to_le_bytes()[..],
        &packed.uint8.to_le_bytes(),
        &packed.int16.to_le_bytes(),
        &packed.uint16.to_le_bytes(),
        &packed.int32.to_le_bytes(),
        &packed.uint32.to_le_bytes(),
        &packed.int64.to_le_bytes(),
        &packed.uint64.to_le_bytes(),
        &packed.float32.to_le_bytes(),
        &packed.float64.to_le_bytes(),
        &[u8::from(packed.bool)],
    ]
    .concat()
}

/// A `Documented`'s fields little-endian at their offsets, as
/// [`packed_bytes()`] writes a `Packed`'s
fn documented_bytes(documented: Documented) -> Vec<u8> {
    [
        &documented.one.to_le_bytes()[..],
        &documented.two.to_le_bytes(),
        &documented.undocumented.to_le_bytes(),
    ]
    .concat()
}

// Arbitrary values of the fields

fn scalars() -> impl Strategy<Value = ScalarsFields> {
    let integers = (
        integers(i8::MIN, i8::MAX),
        integers(0, u8::MAX),
        integers(i16::MIN, i16::MAX),
        integers(0, u16::MAX),
        integers(i32::MIN, i32::MAX),
        integers(0, u32::MAX),
        integers(i64::MIN, i64::MAX),
        integers(0, u64::MAX),
    );

    (integers, float32s(), float64s(), any::<bool>()).prop_map(
        |(integers, float32, float64, bool)| {
            let (int8, uint8, int16, uint16, int32, uint32, int64, uint64) = integers;
            ScalarsFields {
                int8,
                uint8,
                int16,
                uint16,
                int32,
                uint32,
                int64,
                uint64,
                float32,
                float64,
                bool,
            }
        },
    )
}

/// The values of an `Arrays`' fields, which `ArraysFields` borrows
#[derive(Clone, Debug)]
struct OwnedArrays {
    int8: Vec<i8>,
    uint8: Vec<u8>,
    int16: Vec<i16>,
    uint16: Vec<u16>,
    int32: Vec<i32>,
    uint32: Vec<u32>,
    int64: Vec<i64>,
    uint64: Vec<u64>,
    float32: Vec<f32>,
    float64: Vec<f64>,
}

impl OwnedArrays {
    fn fields(&self) -> ArraysFields<'_> {
        ArraysFields {
            int8: &self.int8,
            uint8: &self.uint8,
            int16: &self.int16,
            uint16: &self.uint16,
            int32: &self.int32,
            uint32: &self.uint32,
            int64: &self.int64,
            uint64: &self.uint64,
            float32: &self.float32,
            float64: &self.float64,
        }
    }
}

fn owned_arrays() -> impl Strategy<Value = OwnedArrays> {
    let integers = (
        arrays(integers(i8::MIN, i8::MAX)),
        arrays(integers(0, u8::MAX)),
        arrays(integers(i16::MIN, i16::MAX)),
        arrays(integers(0, u16::MAX)),
        arrays(integers(i32::MIN, i32::MAX)),
        arrays(integers(0, u32::MAX)),
        arrays(integers(i64::MIN, i64::MAX)),
        arrays(integers(0, u64::MAX)),
    );

    (integers, arrays(float32s()), arrays(float64s())).prop_map(|(integers, float32, float64)| {
        let (int8, uint8, int16, uint16, int32, uint32, int64, uint64) = integers;
        OwnedArrays {
            int8,
            uint8,
            int16,
            uint16,
            int32,
            uint32,
            int64,
            uint64,
            float32,
            float64,
        }
    })
}

/// A `Tree` that owns its children, rather than borrowing them as
/// `TreeFields` does
#[derive(Clone, Debug)]
struct OwnedTree {
    value: i32,
    children: Vec<OwnedTree>,
}

/// The most trees in a vector of them, fewer than of other elements, since
/// they nest
const MAX_TREES: usize = 3;

/// Vectors of trees of up to four levels, empty too
fn forests() -> impl Strategy<Value = Vec<OwnedTree>> {
    let values = || integers(i32::MIN, i32::MAX);
    let leaves = values().prop_map(|value| OwnedTree {
        value,
        children: Vec::new(),
    });
    let trees = leaves.prop_recursive(3, 24, 3, move |trees| {
        let children = vec(trees, 0..=MAX_TREES);
        (values(), children).prop_map(|(value, children)| OwnedTree { value, children })
    });

    vec(trees, 0..=MAX_TREES)
}

/// A tree in a level of trees: its value, and the range of its children in
/// the level below
struct Node {
    value: i32,
    children: Range<usize>,
}

/// Calls `f` with the `TreeFields` of `trees`. The fields of a tree borrow
/// those of its children, so they're made a level at a time from the
/// deepest, each level in a frame of the recursion that the level above
/// borrows from.
fn with_tree_fields<R>(trees: &[OwnedTree], f: impl FnOnce(&[TreeFields<'_>]) -> R) -> R {
    with_levels(&levels(trees), &[], f)
}

/// The trees by level from the vector that holds them, with the children of
/// each level's trees in the next, in order
fn levels(trees: &[OwnedTree]) -> Vec<Vec<Node>> {
    let mut levels = Vec::new();
    let mut level: Vec<&OwnedTree> = trees.iter().collect();

    while !level.is_empty() {
        let mut below = Vec::new();
        let nodes = level
            .iter()
            .map(|&tree| {
                let start = below.len();
                below.extend(&tree.children);

                Node {
                    value: tree.value,
                    children: start..below.len(),
                }
            })
            .collect();

        levels.push(nodes);
        level = below;
    }

    levels
}

fn with_levels<R>(
    levels: &[Vec<Node>],
    below: &[TreeFields<'_>],
    f: impl FnOnce(&[TreeFields<'_>]) -> R,
) -> R {
    let Some((deepest, above)) = levels.split_last() else {
        return f(below);
    };

    let fields: Vec<TreeFields<'_>> = deepest
        .iter()
        .map(|node| TreeFields {
            value: node.value,
            children: &below[node.children.clone()],
        })
        .collect();

    with_levels(above, &fields, f)
}

/// The values of a `Nested`'s fields and of the tables it nests
#[derive(Clone, Debug, Default)]
struct OwnedNested {
    label: String,
    count: u32,
    name: String,
    values: Vec<f64>,
    trees: Vec<OwnedTree>,
}

impl OwnedNested {
    /// Writes the `Nested`
    // `NestedFields::serialize` itself isn't general enough over the fields'
    // lifetimes, which clippy doesn't see
    #[allow(clippy::redundant_closure_for_method_calls)]
    fn serialize(&self) -> Result<Vec<u8>, BuildError> {
        self.with_fields(|fields| fields.serialize())
    }

    /// Calls `f` with the fields, which borrow those of the trees
    fn with_fields<R>(&self, f: impl FnOnce(&NestedFields<'_>) -> R) -> R {
        with_tree_fields(&self.trees, |trees| {
            f(&NestedFields {
                label: &self.label,
                outer: OuterFields {
                    count: self.count,
                    inner: InnerFields {
                        name: &self.name,
                        values: &self.values,
                    },
                },
                trees,
            })
        })
    }
}

fn owned_nested() -> impl Strategy<Value = OwnedNested> {
    let outer = (integers(0, u32::MAX), strings(), arrays(float64s()));

    (strings(), outer, forests()).prop_map(|(label, (count, name, values), trees)| OwnedNested {
        label,
        count,
        name,
        values,
        trees,
    })
}

/// The values of a `Keywords`' fields, which a `Reserved` nests
#[derive(Clone, Debug)]
struct OwnedKeywords {
    /// `class`, `default`, `in`, `match` and `self`
    bytes: [u8; 5],
    r#type: String,
}

impl OwnedKeywords {
    fn fields(&self) -> ReservedFields<'_> {
        let [class, default, r#in, r#match, self_] = self.bytes;

        ReservedFields {
            keywords: KeywordsFields {
                class,
                default,
                r#in,
                r#match,
                self_,
                r#type: &self.r#type,
            },
        }
    }
}

fn owned_keywords() -> impl Strategy<Value = OwnedKeywords> {
    (uniform5(integers(0, u8::MAX)), strings())
        .prop_map(|(bytes, r#type)| OwnedKeywords { bytes, r#type })
}

fn packeds() -> impl Strategy<Value = Packed> {
    scalars().prop_map(|fields| Packed {
        int8: fields.int8,
        uint8: fields.uint8,
        int16: fields.int16,
        uint16: fields.uint16,
        int32: fields.int32,
        uint32: fields.uint32,
        int64: fields.int64,
        uint64: fields.uint64,
        float32: fields.float32,
        float64: fields.float64,
        bool: fields.bool,
    })
}

fn documenteds() -> impl Strategy<Value = Documented> {
    let fields = (
        integers(0, u8::MAX),
        integers(0, u16::MAX),
        integers(0, u32::MAX),
    );

    fields.prop_map(|(one, two, undocumented)| Documented {
        one,
        two,
        undocumented,
    })
}

fn points() -> impl Strategy<Value = Point> {
    (float32s(), float32s()).prop_map(|(x, y)| Point { x, y })
}

fn segments() -> impl Strategy<Value = Segment> {
    (points(), points()).prop_map(|(start, end)| Segment { start, end })
}

/// The values of a `Structs`' fields, whose arrays it borrows
#[derive(Clone, Debug)]
struct OwnedStructs {
    packed: Packed,
    documented: Vec<Documented>,
    segment: Segment,
    segments: Vec<Segment>,
    start: Point,
    points: Vec<Point>,
}

impl OwnedStructs {
    fn fields(&self) -> StructsFields<'_> {
        StructsFields {
            packed: self.packed,
            documented: &self.documented,
            segment: self.segment,
            segments: &self.segments,
            path: PathFields {
                start: self.start,
                points: &self.points,
            },
        }
    }
}

fn owned_structs() -> impl Strategy<Value = OwnedStructs> {
    let arrays_of_structs = (arrays(documenteds()), arrays(segments()), arrays(points()));

    (packeds(), segments(), points(), arrays_of_structs).prop_map(
        |(packed, segment, start, (documented, segments, points))| OwnedStructs {
            packed,
            documented,
            segment,
            segments,
            start,
            points,
        },
    )
}

// The integers that each enum names, in the order declared

const LEVELS: [u8; 3] = [Level::LOW.0, Level::MEDIUM.0, Level::HIGH.0];
const DIRECTIONS: [i8; 4] = [
    Direction::BACKWARD.0,
    Direction::NONE.0,
    Direction::FORWARD.0,
    Direction::FURTHEST_BACK.0,
];
const UNITS: [u16; 3] = [Unit::LAST.0, Unit::FIRST.0, Unit::V_2.0];
const CHANGES: [i16; 2] = [Change::LEAST.0, Change::GREATEST.0];
const STATUSES: [u32; 3] = [Status::OK.0, Status::NOT_FOUND.0, Status::GREATEST.0];
const SIGNS: [i32; 2] = [Sign::LEAST.0, Sign::GREATEST.0];
const MASKS: [u64; 4] = [
    Mask::NONE.0,
    Mask::ONE.0,
    Mask::BEYOND_FLOAT64.0,
    Mask::ALL.0,
];
const EXTENTS: [i64; 3] = [Extent::LEAST.0, Extent::ZERO.0, Extent::GREATEST.0];
const WORDS: [u8; 13] = [
    Word::CLASS.0,
    Word::DEFAULT.0,
    Word::IN.0,
    Word::MATCH.0,
    Word::SELF.0,
    Word::TYPE.0,
    Word::TRUE.0,
    Word::NULL.0,
    Word::NONE.0,
    Word::INIT.0,
    Word::NEW.0,
    Word::NAME.0,
    Word::VALUE.0,
];

/// Integers of an enum's type: any of them, most of which the enum doesn't
/// name, and half of the time one of `named`, which it does
fn enum_integers<T>(min: T, max: T, named: &[T]) -> impl Strategy<Value = T> + use<T>
where
    T: Arbitrary + Copy + Debug + Default + 'static,
{
    prop_oneof![integers(min, max), prop::sample::select(named.to_vec())]
}

/// An enum's integer and an array of them, as an `Enums` holds each enum
fn one_and_many<T>(min: T, max: T, named: &[T]) -> impl Strategy<Value = (T, Vec<T>)> + use<T>
where
    T: Arbitrary + Copy + Debug + Default + 'static,
{
    (
        enum_integers(min, max, named),
        arrays(enum_integers(min, max, named)),
    )
}

fn marks() -> impl Strategy<Value = Mark> {
    let enums = (
        enum_integers(0, u8::MAX, &LEVELS),
        enum_integers(i16::MIN, i16::MAX, &CHANGES),
        enum_integers(i64::MIN, i64::MAX, &EXTENTS),
        enum_integers(i8::MIN, i8::MAX, &DIRECTIONS),
        enum_integers(0, u64::MAX, &MASKS),
    );

    (enums, float32s()).prop_map(|((level, change, extent, direction, mask), weight)| Mark {
        level: Level(level),
        change: Change(change),
        weight,
        extent: Extent(extent),
        turn: Turn {
            direction: Direction(direction),
            mask: Mask(mask),
        },
    })
}

/// The values of a `Step`'s fields, whose string `StepFields` borrows. An
/// `Enums` is written from these, as items of the caller's own type.
#[derive(Clone, Debug)]
struct OwnedStep {
    level: Level,
    mark: Mark,
    name: String,
}

impl<'a> From<&'a OwnedStep> for StepFields<'a> {
    fn from(step: &'a OwnedStep) -> Self {
        Self {
            level: step.level,
            mark: step.mark,
            name: &step.name,
        }
    }
}

fn owned_steps() -> impl Strategy<Value = OwnedStep> {
    let levels = enum_integers(0, u8::MAX, &LEVELS);

    (levels, marks(), strings()).prop_map(|(level, mark, name)| OwnedStep {
        level: Level(level),
        mark,
        name,
    })
}

/// The values of an `Enums`' fields, whose arrays and steps it borrows
#[derive(Clone, Debug)]
struct OwnedEnums {
    level: Level,
    levels: Vec<u8>,
    direction: Direction,
    directions: Vec<i8>,
    unit: Unit,
    units: Vec<u16>,
    change: Change,
    changes: Vec<i16>,
    status: Status,
    statuses: Vec<u32>,
    sign: Sign,
    signs: Vec<i32>,
    mask: Mask,
    masks: Vec<u64>,
    extent: Extent,
    extents: Vec<i64>,
    word: Word,
    mark: Mark,
    marks: Vec<Mark>,
    steps: Vec<OwnedStep>,
}

impl OwnedEnums {
    fn fields(&self) -> EnumsFields<'_, Mark, OwnedStep> {
        EnumsFields {
            level: self.level,
            levels: &self.levels,
            direction: self.direction,
            directions: &self.directions,
            unit: self.unit,
            units: &self.units,
            change: self.change,
            changes: &self.changes,
            status: self.status,
            statuses: &self.statuses,
            sign: self.sign,
            signs: &self.signs,
            mask: self.mask,
            masks: &self.masks,
            extent: self.extent,
            extents: &self.extents,
            word: self.word,
            mark: self.mark,
            marks: &self.marks,
            steps: &self.steps,
        }
    }
}

fn owned_enums() -> impl Strategy<Value = OwnedEnums> {
    let enums = (
        one_and_many(0, u8::MAX, &LEVELS),
        one_and_many(i8::MIN, i8::MAX, &DIRECTIONS),
        one_and_many(0, u16::MAX, &UNITS),
        one_and_many(i16::MIN, i16::MAX, &CHANGES),
        one_and_many(0, u32::MAX, &STATUSES),
        one_and_many(i32::MIN, i32::MAX, &SIGNS),
        one_and_many(0, u64::MAX, &MASKS),
        one_and_many(i64::MIN, i64::MAX, &EXTENTS),
    );
    let words = enum_integers(0, u8::MAX, &WORDS);
    let structs = (marks(), arrays(marks()), arrays(owned_steps()));

    (enums, words, structs).prop_map(|(enums, word, (mark, marks, steps))| {
        let (
            (level, levels),
            (direction, directions),
            (unit, units),
            (change, changes),
            (status, statuses),
            (sign, signs),
            (mask, masks),
            (extent, extents),
        ) = enums;

        OwnedEnums {
            level: Level(level),
            levels,
            direction: Direction(direction),
            directions,
            unit: Unit(unit),
            units,
            change: Change(change),
            changes,
            status: Status(status),
            statuses,
            sign: Sign(sign),
            signs,
            mask: Mask(mask),
            masks,
            extent: Extent(extent),
            extents,
            word: Word(word),
            mark,
            marks,
            steps,
        }
    })
}

/// The values of a `StringVectors`' fields and of the tables it nests
#[derive(Clone, Debug)]
struct OwnedStringVectors {
    id: u32,
    tags: Vec<String>,
    labels: Vec<String>,
    /// Each entry's key and aliases
    entries: Vec<(String, Vec<String>)>,
}

impl OwnedStringVectors {
    /// Writes the `StringVectors`
    // `StringVectorsFields::serialize` itself isn't general enough over the
    // fields' lifetimes, which clippy doesn't see
    #[allow(clippy::redundant_closure_for_method_calls)]
    fn serialize(&self) -> Result<Vec<u8>, BuildError> {
        self.with_fields(|fields| fields.serialize())
    }

    /// Calls `f` with the fields, which borrow lists of the strings
    fn with_fields<R>(&self, f: impl FnOnce(&StringVectorsFields<'_>) -> R) -> R {
        let tags = as_strs(&self.tags);
        let labels = as_strs(&self.labels);
        let aliases: Vec<Vec<&str>> = self
            .entries
            .iter()
            .map(|(_, aliases)| as_strs(aliases))
            .collect();
        let entries: Vec<EntryFields> = self
            .entries
            .iter()
            .zip(&aliases)
            .map(|((key, _), aliases)| EntryFields { key, aliases })
            .collect();

        f(&StringVectorsFields {
            id: self.id,
            tags: &tags,
            labels: LabelsFields { values: &labels },
            entries: &entries,
        })
    }
}

fn owned_string_vectors() -> impl Strategy<Value = OwnedStringVectors> {
    let entries = arrays((strings(), arrays(strings())));

    (
        integers(0, u32::MAX),
        arrays(strings()),
        arrays(strings()),
        entries,
    )
        .prop_map(|(id, tags, labels, entries)| OwnedStringVectors {
            id,
            tags,
            labels,
            entries,
        })
}

/// The values of a `NarrowStringVectors`' fields
#[derive(Clone, Debug)]
struct OwnedNarrow {
    names: Vec<String>,
    count: u8,
}

impl OwnedNarrow {
    /// Calls `f` with the fields, which borrow a list of the names
    fn with_fields<R>(&self, f: impl FnOnce(&NarrowStringVectorsFields<'_>) -> R) -> R {
        let names = as_strs(&self.names);

        f(&NarrowStringVectorsFields {
            names: &names,
            count: self.count,
        })
    }
}

fn owned_narrows() -> impl Strategy<Value = OwnedNarrow> {
    (arrays(strings()), integers(0, u8::MAX))
        .prop_map(|(names, count)| OwnedNarrow { names, count })
}

/// A list of the strings, which the generated code writes from
fn as_strs(strings: &[String]) -> Vec<&str> {
    strings.iter().map(String::as_str).collect()
}

/// Bytes too short for a struct of `size` bytes
fn shorter_than(size: usize) -> impl Strategy<Value = Vec<u8>> {
    vec(any::<u8>(), 0..size)
}

/// Bytes to follow a struct's
fn after() -> impl Strategy<Value = Vec<u8>> {
    vec(any::<u8>(), 1..=MAX_LENGTH)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    // Round trips

    #[test]
    fn scalars_read_back_every_scalar_type_wherever_their_bytes_lie(
        fields in scalars(),
        placement in placements(),
    ) {
        let bytes = placement.place(&fields.serialize()?);

        let read = read_scalars(&Scalars::new(bytes.bytes()));
        prop_assert_eq!(read, expected(describe_scalars(&fields)));
    }

    #[test]
    fn arrays_read_back_an_array_of_every_scalar_type_wherever_their_bytes_lie(
        arrays in owned_arrays(),
        placement in placements(),
    ) {
        let fields = arrays.fields();
        let bytes = placement.place(&fields.serialize()?);

        let read = read_arrays(&Arrays::new(bytes.bytes()));
        prop_assert_eq!(read, expected(describe_arrays(&fields)));
    }

    #[test]
    fn nested_reads_back_its_nested_tables_and_trees_wherever_its_bytes_lie(
        nested in owned_nested(),
        placement in placements(),
    ) {
        nested.with_fields(|fields| -> Result<(), TestCaseError> {
            let bytes = placement.place(&fields.serialize()?);

            for order in ViewOrder::ALL {
                let read = read_nested(&Nested::new(bytes.bytes()), order);
                prop_assert_eq!(read, expected(describe_nested(fields)), "{:?}", order);
            }
            Ok(())
        })?;
    }

    #[test]
    fn reserved_reads_back_the_fields_named_after_keywords_wherever_its_bytes_lie(
        keywords in owned_keywords(),
        placement in placements(),
    ) {
        let fields = keywords.fields();
        let bytes = placement.place(&fields.serialize()?);

        let read = read_reserved(&Reserved::new(bytes.bytes()));
        prop_assert_eq!(read, expected(describe_reserved(&fields)));
    }

    #[test]
    fn structs_read_back_its_structs_in_place_in_arrays_and_within_structs_wherever_its_bytes_lie(
        structs in owned_structs(),
        placement in placements(),
    ) {
        let fields = structs.fields();
        let bytes = placement.place(&fields.serialize()?);

        for order in ViewOrder::ALL {
            let read = read_structs(&Structs::new(bytes.bytes()), order);
            prop_assert_eq!(read, expected(describe_structs(&fields)), "{:?}", order);
        }
    }

    #[test]
    fn enums_read_back_any_integer_of_their_types_named_or_not_wherever_their_bytes_lie(
        enums in owned_enums(),
        placement in placements(),
    ) {
        let fields = enums.fields();
        let bytes = placement.place(&fields.serialize()?);

        for order in ViewOrder::ALL {
            let read = read_enums(&Enums::new(bytes.bytes()), order);
            prop_assert_eq!(read, expected(describe_enums(&fields)), "{:?}", order);
        }
    }

    #[test]
    fn string_vectors_read_back_every_string_wherever_their_bytes_lie(
        string_vectors in owned_string_vectors(),
        narrow in owned_narrows(),
        placement in placements(),
    ) {
        string_vectors.with_fields(|fields| -> Result<(), TestCaseError> {
            let bytes = placement.place(&fields.serialize()?);

            for order in ViewOrder::ALL {
                let read = read_string_vectors(&StringVectors::new(bytes.bytes()), order);
                prop_assert_eq!(read, expected(describe_string_vectors(fields)), "{:?}", order);
            }
            Ok(())
        })?;
        narrow.with_fields(|fields| -> Result<(), TestCaseError> {
            let bytes = placement.place(&fields.serialize()?);

            for order in ViewOrder::ALL {
                let read = read_narrow(&NarrowStringVectors::new(bytes.bytes()), order);
                prop_assert_eq!(read, expected(describe_narrow(fields)), "{:?}", order);
            }
            Ok(())
        })?;
    }

    #[test]
    fn scalars_reads_any_byte_of_its_bool_but_0_as_true(fields in scalars(), byte in 2..=u8::MAX) {
        let mut bytes = ScalarsFields { bool: false, ..fields }.serialize()?;
        let position = TableReader::new(&bytes, TableLayout::DEFAULT).root_offset(10)?;
        bytes[position] = byte;

        let read = read_scalars(&Scalars::new(&bytes));
        prop_assert_eq!(read, expected(describe_scalars(&ScalarsFields { bool: true, ..fields })));
    }

    #[test]
    fn enums_read_any_bytes_of_an_enum_as_its_value(
        enums in owned_enums(),
        level in any::<u8>(),
        extent in any::<i64>(),
    ) {
        let mut bytes = enums.fields().serialize()?;
        let reader = TableReader::new(&bytes, TableLayout::DEFAULT);
        let (level_at, extent_at) = (reader.root_offset(0)?, reader.root_offset(14)?);
        bytes[level_at] = level;
        bytes[extent_at..extent_at + 8].copy_from_slice(&extent.to_le_bytes());

        let overwritten = OwnedEnums { level: Level(level), extent: Extent(extent), ..enums };
        let read = read_enums(&Enums::new(&bytes), ViewOrder::Iterated);
        prop_assert_eq!(read, expected(describe_enums(&overwritten.fields())));
    }

    // Tables built by hand

    #[test]
    fn scalars_are_written_as_table_builder_writes_them(fields in scalars()) {
        prop_assert_eq!(fields.serialize(), build(MESSAGE, &describe_scalars(&fields)));
    }

    #[test]
    fn arrays_are_written_as_table_builder_writes_them(arrays in owned_arrays()) {
        let fields = arrays.fields();

        prop_assert_eq!(fields.serialize(), build(MESSAGE, &describe_arrays(&fields)));
    }

    #[test]
    fn nested_is_written_as_table_builder_writes_it(nested in owned_nested()) {
        nested.with_fields(|fields| -> Result<(), TestCaseError> {
            prop_assert_eq!(fields.serialize(), build(MESSAGE, &describe_nested(fields)));
            Ok(())
        })?;
    }

    #[test]
    fn reserved_is_written_as_table_builder_writes_it(keywords in owned_keywords()) {
        let fields = keywords.fields();

        prop_assert_eq!(fields.serialize(), build(MESSAGE, &describe_reserved(&fields)));
    }

    #[test]
    fn structs_is_written_as_table_builder_writes_it(structs in owned_structs()) {
        let fields = structs.fields();

        prop_assert_eq!(fields.serialize(), build(MESSAGE, &describe_structs(&fields)));
    }

    #[test]
    fn enums_are_written_as_table_builder_writes_their_integers(enums in owned_enums()) {
        let fields = enums.fields();

        prop_assert_eq!(fields.serialize(), build(MESSAGE, &describe_enums(&fields)));
    }

    #[test]
    fn string_vectors_are_written_as_table_builder_writes_them(
        string_vectors in owned_string_vectors(),
        narrow in owned_narrows(),
    ) {
        string_vectors.with_fields(|fields| -> Result<(), TestCaseError> {
            prop_assert_eq!(fields.serialize(), build(MESSAGE, &describe_string_vectors(fields)));
            Ok(())
        })?;
        narrow.with_fields(|fields| -> Result<(), TestCaseError> {
            prop_assert_eq!(fields.serialize(), build(NARROW, &describe_narrow(fields)));
            Ok(())
        })?;
    }

    // Decode errors

    #[test]
    fn corrupted_scalars_read_as_values_or_decode_errors(
        fields in scalars(),
        overwrite in overwrites(),
    ) {
        let bytes = fields.serialize()?;
        let written = describe_scalars(&fields);

        for truncated in truncations(&bytes) {
            check_truncated(Ok(read_scalars(&Scalars::new(truncated))), &written)?;
        }

        // Any values can be read from overwritten bytes, as long as no getter
        // panics
        read_scalars(&Scalars::new(&overwrite.apply(&bytes)));
    }

    #[test]
    fn corrupted_arrays_read_as_values_or_decode_errors(
        arrays in owned_arrays(),
        overwrite in overwrites(),
    ) {
        let fields = arrays.fields();
        let bytes = fields.serialize()?;
        let written = describe_arrays(&fields);

        for truncated in truncations(&bytes) {
            check_truncated(Ok(read_arrays(&Arrays::new(truncated))), &written)?;
        }

        read_arrays(&Arrays::new(&overwrite.apply(&bytes)));
    }

    #[test]
    fn a_corrupted_nested_reads_as_values_or_decode_errors(
        nested in owned_nested(),
        overwrite in overwrites(),
    ) {
        nested.with_fields(|fields| -> Result<(), TestCaseError> {
            let bytes = fields.serialize()?;
            let written = describe_nested(fields);

            for order in ViewOrder::ALL {
                for truncated in truncations(&bytes) {
                    check_truncated(Ok(read_nested(&Nested::new(truncated), order)), &written)?;
                }

                read_nested(&Nested::new(&overwrite.apply(&bytes)), order);
            }
            Ok(())
        })?;
    }

    #[test]
    fn a_corrupted_reserved_reads_as_values_or_decode_errors(
        keywords in owned_keywords(),
        overwrite in overwrites(),
    ) {
        let fields = keywords.fields();
        let bytes = fields.serialize()?;
        let written = describe_reserved(&fields);

        for truncated in truncations(&bytes) {
            check_truncated(Ok(read_reserved(&Reserved::new(truncated))), &written)?;
        }

        read_reserved(&Reserved::new(&overwrite.apply(&bytes)));
    }

    #[test]
    fn a_corrupted_structs_reads_as_values_or_decode_errors(
        structs in owned_structs(),
        overwrite in overwrites(),
    ) {
        let fields = structs.fields();
        let bytes = fields.serialize()?;
        let written = describe_structs(&fields);

        for order in ViewOrder::ALL {
            for truncated in truncations(&bytes) {
                check_truncated(Ok(read_structs(&Structs::new(truncated), order)), &written)?;
            }

            read_structs(&Structs::new(&overwrite.apply(&bytes)), order);
        }
    }

    #[test]
    fn corrupted_enums_read_as_values_or_decode_errors(
        enums in owned_enums(),
        overwrite in overwrites(),
    ) {
        let fields = enums.fields();
        let bytes = fields.serialize()?;
        let written = describe_enums(&fields);

        for order in ViewOrder::ALL {
            for truncated in truncations(&bytes) {
                check_truncated(Ok(read_enums(&Enums::new(truncated), order)), &written)?;
            }

            // An enum reads any integer of its type, so no overwritten byte
            // of one is an error
            read_enums(&Enums::new(&overwrite.apply(&bytes)), order);
        }
    }

    #[test]
    fn corrupted_string_vectors_read_as_values_or_decode_errors(
        string_vectors in owned_string_vectors(),
        narrow in owned_narrows(),
        overwrite in overwrites(),
    ) {
        string_vectors.with_fields(|fields| -> Result<(), TestCaseError> {
            let bytes = fields.serialize()?;
            let written = describe_string_vectors(fields);

            for order in ViewOrder::ALL {
                for truncated in truncations(&bytes) {
                    let read = read_string_vectors(&StringVectors::new(truncated), order);
                    check_truncated(Ok(read), &written)?;
                }

                read_string_vectors(&StringVectors::new(&overwrite.apply(&bytes)), order);
            }
            Ok(())
        })?;
        narrow.with_fields(|fields| -> Result<(), TestCaseError> {
            let bytes = fields.serialize()?;
            let written = describe_narrow(fields);

            for order in ViewOrder::ALL {
                for truncated in truncations(&bytes) {
                    let read = read_narrow(&NarrowStringVectors::new(truncated), order);
                    check_truncated(Ok(read), &written)?;
                }

                read_narrow(&NarrowStringVectors::new(&overwrite.apply(&bytes)), order);
            }
            Ok(())
        })?;
    }

    #[test]
    fn arbitrary_bytes_read_as_any_message_or_decode_errors(bytes in arbitrary_bytes()) {
        read_scalars(&Scalars::new(&bytes));
        read_arrays(&Arrays::new(&bytes));
        read_reserved(&Reserved::new(&bytes));
        for order in ViewOrder::ALL {
            read_nested(&Nested::new(&bytes), order);
            read_structs(&Structs::new(&bytes), order);
            read_enums(&Enums::new(&bytes), order);
            read_string_vectors(&StringVectors::new(&bytes), order);
            read_narrow(&NarrowStringVectors::new(&bytes), order);
        }
    }

    // Views

    #[test]
    fn views_read_corrupted_bytes_alike_by_index_and_in_order(
        nested in owned_nested(),
        structs in owned_structs(),
        overwrite in overwrites(),
    ) {
        use ViewOrder::{FromLast, Iterated};
        let nested = nested.serialize()?;
        let structs = structs.fields().serialize()?;

        for bytes in truncations(&nested).chain([&overwrite.apply(&nested)[..]]) {
            let nested = Nested::new(bytes);
            prop_assert_eq!(read_nested(&nested, FromLast), read_nested(&nested, Iterated));
        }
        for bytes in truncations(&structs).chain([&overwrite.apply(&structs)[..]]) {
            let structs = Structs::new(bytes);
            prop_assert_eq!(read_structs(&structs, FromLast), read_structs(&structs, Iterated));
        }
    }

    #[test]
    fn string_vectors_read_corrupted_bytes_alike_by_index_and_in_order(
        string_vectors in owned_string_vectors(),
        overwrite in overwrites(),
    ) {
        use ViewOrder::{FromLast, Iterated};
        let bytes = string_vectors.serialize()?;

        for bytes in truncations(&bytes).chain([&overwrite.apply(&bytes)[..]]) {
            let read = StringVectors::new(bytes);
            prop_assert_eq!(
                read_string_vectors(&read, FromLast),
                read_string_vectors(&read, Iterated)
            );
        }
    }

    #[test]
    fn views_read_arbitrary_bytes_alike_by_index_and_in_order(bytes in arbitrary_bytes()) {
        use ViewOrder::{FromLast, Iterated};
        let (nested, structs) = (Nested::new(&bytes), Structs::new(&bytes));

        prop_assert_eq!(read_nested(&nested, FromLast), read_nested(&nested, Iterated));
        prop_assert_eq!(read_structs(&structs, FromLast), read_structs(&structs, Iterated));
        let string_vectors = StringVectors::new(&bytes);
        prop_assert_eq!(
            read_string_vectors(&string_vectors, FromLast),
            read_string_vectors(&string_vectors, Iterated)
        );
    }

    #[test]
    fn views_have_no_item_past_their_last(
        nested in owned_nested(),
        structs in owned_structs(),
        past in 0..1000_usize,
    ) {
        let nested = nested.serialize()?;
        let structs = structs.fields().serialize()?;

        let trees = Nested::new(&nested).trees()?;
        prop_assert!(trees.get(trees.len() + past).is_none());
        let documented = Structs::new(&structs).documented()?;
        prop_assert!(documented.get(documented.len() + past).is_none());
    }

    #[test]
    fn vectors_of_strings_have_no_string_past_their_last(
        string_vectors in owned_string_vectors(),
        past in 0..1000_usize,
    ) {
        let bytes = string_vectors.serialize()?;

        let tags = StringVectors::new(&bytes).tags()?;
        prop_assert!(tags.get(tags.len() + past).is_none());
        prop_assert_eq!(tags.iter().len(), tags.len());
    }

    // The structs

    #[test]
    fn packed_is_written_in_43_bytes_at_fixed_offsets_and_read_back_wherever_they_lie(
        packed in packeds(),
        placement in placements(),
    ) {
        let bytes = packed.to_bytes();
        prop_assert_eq!(Packed::SIZE, 43);
        prop_assert_eq!(bytes.to_vec(), packed_bytes(&packed));

        let read = Packed::read(placement.place(&bytes).bytes())?;
        prop_assert_eq!(describe_packed(&read), describe_packed(&packed));
    }

    #[test]
    fn packed_is_read_from_the_start_of_longer_bytes(packed in packeds(), after in after()) {
        let bytes = [&packed.to_bytes()[..], &after].concat();

        let read = Packed::read(&bytes)?;
        prop_assert_eq!(describe_packed(&read), describe_packed(&packed));
    }

    #[test]
    fn packed_reads_any_byte_of_its_bool_but_0_as_true(packed in packeds(), byte in 2..=u8::MAX) {
        let mut bytes = packed_bytes(&Packed { bool: false, ..packed });
        bytes[42] = byte;

        let read = Packed::read(&bytes)?;
        prop_assert_eq!(describe_packed(&read), describe_packed(&Packed { bool: true, ..packed }));
    }

    #[test]
    fn packed_fails_to_read_from_fewer_than_43_bytes(bytes in shorter_than(Packed::SIZE)) {
        let read = Packed::read(&bytes);
        prop_assert!(matches!(read, Err(DecodeError::OutOfRange { .. })), "{:?}", read);
    }

    #[test]
    fn documented_is_written_in_7_bytes_at_fixed_offsets_and_read_back_wherever_they_lie(
        documented in documenteds(),
        placement in placements(),
    ) {
        let bytes = documented.to_bytes();
        prop_assert_eq!(Documented::SIZE, 7);
        prop_assert_eq!(bytes.to_vec(), documented_bytes(documented));

        let read = Documented::read(placement.place(&bytes).bytes())?;
        prop_assert_eq!(read, documented);
    }

    #[test]
    fn documented_is_read_from_the_start_of_longer_bytes(
        documented in documenteds(),
        after in after(),
    ) {
        let bytes = [&documented.to_bytes()[..], &after].concat();

        prop_assert_eq!(Documented::read(&bytes)?, documented);
    }

    #[test]
    fn documented_fails_to_read_from_fewer_than_7_bytes(bytes in shorter_than(Documented::SIZE)) {
        let read = Documented::read(&bytes);
        prop_assert!(matches!(read, Err(DecodeError::OutOfRange { .. })), "{:?}", read);
    }

    #[test]
    fn structs_are_written_one_after_another_as_arrays_aligned_to_their_largest_scalar_hold_them(
        structs in owned_structs(),
    ) {
        let packed = [structs.packed, structs.packed];
        prop_assert_eq!(Packed::ALIGNMENT, Alignment::Eight);
        prop_assert_eq!(Packed::array_to_bytes(&packed), [packed_bytes(&packed[0]), packed_bytes(&packed[1])].concat());

        prop_assert_eq!(Documented::ALIGNMENT, Alignment::Four);
        let documented: Vec<u8> = structs.documented.iter().copied().flat_map(documented_bytes).collect();
        prop_assert_eq!(Documented::array_to_bytes(&structs.documented), documented);

        prop_assert_eq!((Segment::SIZE, Segment::ALIGNMENT), (16, Alignment::Four));
        let segments: Vec<u8> = structs.segments.iter().flat_map(|segment| struct_bytes(&describe_segment(*segment))).collect();
        prop_assert_eq!(Segment::array_to_bytes(&structs.segments), segments);

        prop_assert_eq!((Point::SIZE, Point::ALIGNMENT), (8, Alignment::Four));
        let points: Vec<u8> = structs.points.iter().flat_map(|point| struct_bytes(&describe_point(*point))).collect();
        prop_assert_eq!(Point::array_to_bytes(&structs.points), points);
    }

    #[test]
    fn a_segment_is_read_back_from_its_bytes_wherever_they_lie(
        segment in segments(),
        placement in placements(),
    ) {
        let bytes = segment.to_bytes();
        prop_assert_eq!(bytes.to_vec(), struct_bytes(&describe_segment(segment)));

        let read = Segment::read(placement.place(&bytes).bytes())?;
        prop_assert_eq!(describe_segment(read), describe_segment(segment));
        prop_assert_eq!(describe_segment(Segment::from_bytes(&bytes)), describe_segment(segment));
    }

    #[test]
    fn a_mark_is_written_as_the_integers_of_its_enums_in_24_bytes_and_read_back_wherever_they_lie(
        mark in marks(),
        placement in placements(),
    ) {
        let bytes = mark.to_bytes();
        prop_assert_eq!((Mark::SIZE, Mark::ALIGNMENT), (24, Alignment::Eight));
        prop_assert_eq!(bytes.to_vec(), struct_bytes(&describe_mark(mark)));
        prop_assert_eq!(Mark::array_to_bytes(&[mark, mark]), [bytes, bytes].concat());

        let read = Mark::read(placement.place(&bytes).bytes())?;
        prop_assert_eq!(describe_mark(read), describe_mark(mark));
        prop_assert_eq!(describe_mark(Mark::from_bytes(&bytes)), describe_mark(mark));
    }

    #[test]
    fn a_turn_of_enums_is_read_from_its_9_bytes_within_a_mark(mark in marks()) {
        let mark_bytes = mark.to_bytes();
        let bytes = &mark_bytes[15..];
        prop_assert_eq!((Turn::SIZE, Turn::ALIGNMENT), (9, Alignment::Eight));
        prop_assert_eq!(&mark.turn.to_bytes()[..], bytes);
        prop_assert_eq!(Turn::array_to_bytes(&[mark.turn, mark.turn]), [bytes, bytes].concat());

        prop_assert_eq!(Turn::read(bytes), Ok(mark.turn));
    }

    #[test]
    fn a_mark_reads_any_bytes_of_its_enums_as_their_values(bytes in any::<[u8; Mark::SIZE]>()) {
        let mark = Mark::from_bytes(&bytes);

        prop_assert_eq!(mark.level, Level(bytes[0]));
        prop_assert_eq!(mark.change, Change(i16::from_le_bytes([bytes[1], bytes[2]])));
        prop_assert_eq!(mark.turn.direction, Direction(i8::from_le_bytes([bytes[15]])));
        prop_assert_eq!(mark.to_bytes(), bytes);
    }

    #[test]
    fn a_mark_fails_to_read_from_fewer_than_24_bytes(bytes in shorter_than(Mark::SIZE)) {
        let read = Mark::read(&bytes);
        prop_assert!(matches!(read, Err(DecodeError::OutOfRange { .. })), "{:?}", read);
    }
}

// The enums

/// The constants are the integers that the schema gives the values, the
/// least and the greatest of each integer type among them
#[test]
fn enums_name_the_integers_that_the_schema_declares() {
    assert_eq!(LEVELS, [0, 1, 2]);
    assert_eq!(DIRECTIONS, [-1, 0, 1, -128]);
    assert_eq!(UNITS, [65_535, 0, 2]);
    assert_eq!(CHANGES, [-32_768, 32_767]);
    assert_eq!(STATUSES, [0, 404, 4_294_967_295]);
    assert_eq!(SIGNS, [-2_147_483_648, 2_147_483_647]);
    assert_eq!(
        MASKS,
        [0, 1, 9_007_199_254_740_993, 18_446_744_073_709_551_615]
    );
    assert_eq!(
        EXTENTS,
        [-9_223_372_036_854_775_808, 0, 9_223_372_036_854_775_807]
    );
    assert_eq!(WORDS, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
}

/// An enum's value is its integer: equal, ordered and hashed as it, and
/// matched by the constants, which leave every other integer to match
#[test]
fn enums_compare_order_hash_and_match_as_their_integers() {
    let name = |level| match level {
        Level::LOW => "low",
        Level::MEDIUM => "medium",
        Level::HIGH => "high",
        Level(_) => "unnamed",
    };

    assert_eq!(Level(2), Level::HIGH);
    assert_ne!(Level(3), Level::HIGH);
    assert!(Direction::FURTHEST_BACK < Direction::BACKWARD && Direction::BACKWARD < Direction(0));
    assert_eq!(Direction(i8::MAX).max(Direction::FORWARD), Direction(127));
    assert_eq!(
        [Level::LOW, Level(1), Level(2), Level(255)].map(name),
        ["low", "medium", "high", "unnamed"]
    );
    assert_eq!(HashSet::from([Level::LOW, Level(0), Level(7)]).len(), 2);
}

#[test]
fn the_values_that_enums_name_are_written_as_their_integers() {
    let mark = Mark {
        level: Level::HIGH,
        change: Change::LEAST,
        weight: 0.5,
        extent: Extent::GREATEST,
        turn: Turn {
            direction: Direction::BACKWARD,
            mask: Mask::BEYOND_FLOAT64,
        },
    };
    let bytes = [
        &[2][..],
        &i16::MIN.to_le_bytes(),
        &0.5_f32.to_le_bytes(),
        &i64::MAX.to_le_bytes(),
        &[0xff],
        &9_007_199_254_740_993_u64.to_le_bytes(),
    ]
    .concat();

    assert_eq!(mark.to_bytes().to_vec(), bytes);
    assert_eq!(Mark::read(&bytes), Ok(mark));
}

/// A message written from the constants, and from integers for its arrays,
/// reads back as the constants, with views of no items for its empty array
/// of structs and vector
#[test]
fn enums_written_from_their_constants_read_back_as_them() -> Result<(), Box<dyn std::error::Error>>
{
    let mark = Mark {
        level: Level::LOW,
        change: Change::GREATEST,
        weight: -0.0,
        extent: Extent::LEAST,
        turn: Turn {
            direction: Direction::FURTHEST_BACK,
            mask: Mask::ALL,
        },
    };
    let fields: EnumsFields = EnumsFields {
        level: Level::MEDIUM,
        levels: &[Level::LOW.0, Level::HIGH.0, 255],
        direction: Direction::FURTHEST_BACK,
        directions: &[],
        unit: Unit::LAST,
        units: &[Unit::V_2.0, 3],
        change: Change::LEAST,
        changes: &[],
        status: Status::NOT_FOUND,
        statuses: &[],
        sign: Sign::LEAST,
        signs: &[],
        mask: Mask::ALL,
        masks: &[Mask::BEYOND_FLOAT64.0],
        extent: Extent::LEAST,
        extents: &[],
        word: Word::SELF,
        mark,
        marks: &[],
        steps: &[],
    };
    let bytes = fields.serialize()?;

    let enums = Enums::new(&bytes);
    assert_eq!(enums.level()?, Level::MEDIUM);
    assert_eq!(enums.levels()?, [0, 2, 255]);
    assert_eq!(enums.direction()?, Direction(-128));
    assert_eq!(enums.unit()?, Unit(65_535));
    assert_eq!(enums.units()?.to_vec(), [2, 3]);
    assert_eq!(enums.change()?, Change(i16::MIN));
    assert_eq!(enums.status()?, Status(404));
    assert_eq!(enums.sign()?, Sign(i32::MIN));
    assert_eq!(enums.mask()?, Mask(u64::MAX));
    assert_eq!(enums.masks()?.to_vec(), [9_007_199_254_740_993]);
    assert_eq!(enums.extent()?, Extent(i64::MIN));
    assert_eq!(enums.word()?, Word(4));
    assert_eq!(describe_mark(enums.mark()?), describe_mark(mark));

    let marks = enums.marks()?;
    assert!(marks.is_empty() && marks.get(0).is_none() && marks.iter().len() == 0);
    let steps = enums.steps()?;
    assert!(steps.is_empty() && steps.get(0).is_none() && steps.iter().len() == 0);
    Ok(())
}

#[test]
fn views_of_empty_vectors_and_arrays_of_structs_hold_no_items()
-> Result<(), Box<dyn std::error::Error>> {
    let nested = OwnedNested::default().serialize()?;
    let structs = OwnedStructs {
        packed: Packed::from_bytes(&[0; Packed::SIZE]),
        documented: vec![],
        segment: Segment::from_bytes(&[0; Segment::SIZE]),
        segments: vec![],
        start: Point::from_bytes(&[0; Point::SIZE]),
        points: vec![],
    }
    .fields()
    .serialize()?;

    let trees = Nested::new(&nested).trees()?;
    assert!(trees.is_empty() && trees.get(0).is_none() && trees.iter().len() == 0);

    let structs = Structs::new(&structs);
    let documented = structs.documented()?;
    assert!(documented.is_empty() && documented.get(0).is_none());
    let segments = structs.segments()?;
    assert!(segments.is_empty() && segments.get(0).is_none());
    let points = structs.path()?.points()?;
    assert!(points.is_empty() && points.iter().len() == 0);

    let empty = StringVectorsFields {
        id: 0,
        tags: &[],
        labels: LabelsFields { values: &[] },
        entries: &[],
    };
    let string_vectors = empty.serialize()?;
    let string_vectors = StringVectors::new(&string_vectors);
    let tags = string_vectors.tags()?;
    assert!(tags.is_empty() && tags.get(0).is_none() && tags.to_vec()?.is_empty());
    let entries = string_vectors.entries()?;
    assert!(entries.is_empty() && entries.get(0).is_none() && entries.iter().len() == 0);
    Ok(())
}

/// A nested table is padded to the largest alignment of its arrays, of
/// scalars and of structs, and of the tables nested in it, as `TableBuilder`
/// pads one from its builder. The items of a vector aren't padded, so they
/// don't count, and a table of none of them is aligned to 1.
#[test]
fn tables_are_aligned_to_their_arrays_and_nested_tables() {
    // An array of float64s, and the table that nests it
    assert_eq!(InnerFields::ALIGNMENT, Alignment::Eight);
    assert_eq!(OuterFields::ALIGNMENT, Alignment::Eight);
    // An array of structs of float32s, after a struct of them
    assert_eq!(PathFields::ALIGNMENT, Alignment::Four);
    // A vector of itself, and scalars and a string
    assert_eq!(TreeFields::ALIGNMENT, Alignment::One);
    assert_eq!(KeywordsFields::ALIGNMENT, Alignment::One);
    // Enums, a struct of them and a string
    assert_eq!(StepFields::ALIGNMENT, Alignment::One);
    // Vectors of strings, which aren't padded, and a string
    assert_eq!(LabelsFields::ALIGNMENT, Alignment::One);
    assert_eq!(EntryFields::ALIGNMENT, Alignment::One);
}

/// `NarrowStringVectors` has `uint16` offsets, which its names fill, after
/// its two offsets and their count and end, and before its count, and a
/// byte more fails as `TableBuilder` fails
#[test]
fn narrow_string_vectors_fill_their_uint16_offsets_and_fail_for_a_byte_more() {
    let room = 65_535 - 9;
    let longest = "a".repeat(room);
    let too_long = "a".repeat(room + 1);

    let fields = NarrowStringVectorsFields {
        names: &[&longest],
        count: 7,
    };
    let bytes = fields.serialize().unwrap();
    assert_eq!(bytes.len(), 65_535);
    let narrow = NarrowStringVectors::new(&bytes);
    assert_eq!(narrow.names().unwrap().to_vec(), Ok(vec![longest.as_str()]));
    assert_eq!(narrow.count(), Ok(7));

    let error = BuildError::TooLong {
        index: 1,
        length: 65_536,
        offset_type: OffsetType::Uint16,
    };
    let fields = NarrowStringVectorsFields {
        names: &[&too_long],
        count: 7,
    };
    assert_eq!(fields.serialize(), Err(error));
    assert_eq!(build(NARROW, &describe_narrow(&fields)), Err(error));
}

/// A tag that isn't valid UTF-8 fails on its own, and reading every tag
/// fails, while the other tags and fields read
#[test]
fn a_tag_of_invalid_utf8_fails_alone() {
    let fields = StringVectorsFields {
        id: 7,
        tags: &["ok", "bad", "fine"],
        labels: LabelsFields { values: &["label"] },
        entries: &[EntryFields {
            key: "key",
            aliases: &["alias"],
        }],
    };
    let mut bytes = fields.serialize().unwrap();
    // The second tag's first byte, after the tags' count, their three ends
    // and the first tag
    let reader = TableReader::new(&bytes, TableLayout::DEFAULT);
    let bad = reader.root_offset(1).unwrap() + 4 + 3 * 4 + 2;
    bytes[bad] = 0xff;

    let string_vectors = StringVectors::new(&bytes);
    let tags = string_vectors.tags().unwrap();
    assert_eq!(tags.get(0), Some(Ok("ok")));
    assert!(matches!(
        tags.get(1),
        Some(Err(DecodeError::InvalidUtf8 { position, .. })) if position == bad
    ));
    assert_eq!(tags.get(2), Some(Ok("fine")));
    assert_eq!(tags.to_vec(), Err(tags.get(1).unwrap().unwrap_err()));

    let read = read_string_vectors(&string_vectors, ViewOrder::Iterated);
    let written = describe_string_vectors(&fields);
    for index in [0, 2, 3] {
        assert_eq!(read[index], Ok(written[index].clone()), "field {index}");
    }
}
