//! The prototype `DirectTableBuilder` writes the bytes that `TableBuilder`
//! writes, and fails as it fails: for every table of the conformance suite,
//! and for generated tables, built in every offset type so that their fields
//! often pass its limits, with fields set past the table's count or left
//! unset, and with tables nested in place whose fills fail, panic or leave
//! fields unset. Each setter's result is compared, and the setters that
//! follow a failure show that it left the builder as it was.
//!
//! Builders that keep every run, or runs of 8 bytes or more, as segments,
//! with their first segments within the builder or none, and builders whose
//! bytes grow in each way that the prototype offers, run the same checks.

#![cfg(feature = "prototype")]

mod common;

use std::{
    cell::Cell,
    fs,
    panic::{self, AssertUnwindSafe},
    path::Path,
    sync::Once,
};

use common::{
    cases::table_of,
    description::{Field, Nested, Source, Table, struct_alignment, struct_bytes, struct_size},
    strategy,
};
use proptest::{option, prelude::*, sample::select};
use serde_json::Value;
use tafla::{
    Alignment, BuildError, InPlaceTable, NestedTable, OffsetType, TableBuilder, TableLayout,
    prototype::{
        Buffer, DirectInPlaceTable, DirectNestedTable, DirectTableBuilder, InlineBuffer,
        NO_SEGMENTS, VecBuffer,
    },
};

const CONFORMANCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance");

/// The message of the panics that a test's fill raises, which the panic hook
/// keeps quiet
const INJECTED_PANIC: &str = "A fill's panic, which the test injects";

/// How a test builds a table: the offset types of the root and of the
/// builders nested in it, which replace the description's, the fields that
/// the root is started with, whether tables are nested in place where they
/// can be, whether values are given owned, and a fill that fails
#[derive(Clone, Copy, Debug)]
struct Plan {
    root: Option<OffsetType>,
    nested: Option<OffsetType>,
    /// The root's fields beyond the description's: -1, 0 or 1
    extra_fields: isize,
    in_place: bool,
    owned: bool,
    failure: Option<Failure>,
}

impl Plan {
    /// The description's own layouts and fields, with tables nested in
    /// place or from builders, and values borrowed
    const fn as_described(in_place: bool) -> Self {
        Self {
            root: None,
            nested: None,
            extra_fields: 0,
            in_place,
            owned: false,
            failure: None,
        }
    }
}

/// The `fill`th call of a fill, counted from 0 over the whole table, fails
/// before it sets its `before`th field
#[derive(Clone, Copy, Debug)]
struct Failure {
    fill: usize,
    before: usize,
    how: Fails,
}

#[derive(Clone, Copy, Debug)]
enum Fails {
    /// The fill returns an error of its own
    Error,
    Panics,
    /// The fill returns, leaving its other fields unset
    Returns,
}

/// An error of a setter, or of the test's fills
#[derive(Debug, PartialEq, Eq)]
enum Error {
    Build(BuildError),
    Injected,
    Panicked,
}

impl From<BuildError> for Error {
    fn from(error: BuildError) -> Self {
        Self::Build(error)
    }
}

/// What a builder did with a table: the result of each of the root's
/// setters, or of the constructor, and of `to_bytes()`
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    steps: Vec<Result<(), Error>>,
    bytes: Result<Vec<u8>, BuildError>,
}

/// The plan, and the fills called so far, as a table is built
struct Context {
    plan: Plan,
    /// The offset type of tables written in place, which is the root's
    in_place: Option<OffsetType>,
    fills: Cell<usize>,
    /// Whether fills fail as the plan says
    failing: Cell<bool>,
}

impl Context {
    fn new(table: &Table, plan: Plan) -> Self {
        let root = plan.root.unwrap_or(table.layout.offset_type);

        Self {
            plan,
            in_place: plan.in_place.then_some(root),
            fills: Cell::new(0),
            failing: Cell::new(true),
        }
    }

    /// The offset type that a nested builder of `table` is started with
    fn nested_offset_type(&self, table: &Table) -> OffsetType {
        self.plan.nested.unwrap_or(table.layout.offset_type)
    }

    /// Whether a table is nested in place, as `Table::to_bytes_in_place()`
    /// nests it: from a builder without a header whose offset type is the
    /// root's
    fn in_place(&self, nested: &Nested) -> bool {
        let layout = nested.table.layout;
        nested.from == Source::Builder
            && !layout.sized
            && !layout.versioned
            && Some(self.nested_offset_type(&nested.table)) == self.in_place
    }

    /// Whether a vector of `items` is nested in place: each of them is, and
    /// they hold as many fields
    fn all_in_place(&self, items: &[Nested]) -> bool {
        let fields = items.first().map(|item| item.table.fields.len());
        items
            .iter()
            .all(|item| self.in_place(item) && Some(item.table.fields.len()) == fields)
    }

    /// Counts a call of a fill
    fn next_fill(&self) -> usize {
        let fill = self.fills.get();
        self.fills.set(fill + 1);
        fill
    }

    /// How fill call `fill` fails before it sets field `index`, if it does
    fn fails(&self, fill: usize, index: usize) -> Option<Fails> {
        self.plan
            .failure
            .filter(|_| self.failing.get())
            .filter(|failure| failure.fill == fill && failure.before == index)
            .map(|failure| failure.how)
    }
}

/// Sets `$field` on `$target`, a builder or a table nested in place, with
/// the plan of `$context`, and evaluates to the setter's result
macro_rules! set_field {
    ($target:ident, $field:ident, $context:ident) => {{
        let owned = $context.plan.owned;
        macro_rules! array {
            ($setter:ident, $values:expr) => {
                if owned {
                    $target.$setter($values.clone())
                } else {
                    $target.$setter($values.as_slice())
                }
            };
        }

        let result: Result<(), Error> = match $field {
            Field::I8(value) => Ok($target.set_i8(*value)?),
            Field::U8(value) => Ok($target.set_u8(*value)?),
            Field::I16(value) => Ok($target.set_i16(*value)?),
            Field::U16(value) => Ok($target.set_u16(*value)?),
            Field::I32(value) => Ok($target.set_i32(*value)?),
            Field::U32(value) => Ok($target.set_u32(*value)?),
            Field::I64(value) => Ok($target.set_i64(*value)?),
            Field::U64(value) => Ok($target.set_u64(*value)?),
            Field::F32(value) => Ok($target.set_f32(*value)?),
            Field::F64(value) => Ok($target.set_f64(*value)?),
            Field::Bool(value) => Ok($target.set_u8(u8::from(*value))?),
            Field::I8Array(values) => Ok(array!(set_i8_array, values)?),
            Field::U8Array(values) => Ok(array!(set_u8_array, values)?),
            Field::I16Array(values) => Ok(array!(set_i16_array, values)?),
            Field::U16Array(values) => Ok(array!(set_u16_array, values)?),
            Field::I32Array(values) => Ok(array!(set_i32_array, values)?),
            Field::U32Array(values) => Ok(array!(set_u32_array, values)?),
            Field::I64Array(values) => Ok(array!(set_i64_array, values)?),
            Field::U64Array(values) => Ok(array!(set_u64_array, values)?),
            Field::F32Array(values) => Ok(array!(set_f32_array, values)?),
            Field::F64Array(values) => Ok(array!(set_f64_array, values)?),
            Field::String(string) => {
                if owned {
                    Ok($target.set_str(string.clone())?)
                } else {
                    Ok($target.set_str(string.as_str())?)
                }
            }
            Field::Struct(scalars) => Ok($target.set_struct(struct_bytes(scalars))?),
            Field::StructArray { types, items } => {
                let bytes = items.iter().flat_map(|scalars| struct_bytes(scalars));
                let size = struct_size(types.iter().copied());
                let alignment = struct_alignment(types.iter().copied());

                Ok($target.set_struct_array(bytes.collect::<Vec<_>>(), size, alignment)?)
            }
            Field::Table {
                nested,
                alignment: None,
            } if $context.in_place(nested) => {
                let fields = &nested.table.fields;
                $target.set_table_with(fields.len(), |table| fill(table, fields, $context))
            }
            Field::Table { nested, alignment } => {
                let table = nest(nested, $context)?;
                match alignment {
                    Some(alignment) => Ok($target.set_table_aligned(table, *alignment)?),
                    None => Ok($target.set_table(table)?),
                }
            }
            Field::Vector(items) if $context.all_in_place(items) => {
                let fields = items.first().map_or(0, |item| item.table.fields.len());
                $target.set_vector_with(items.len(), fields, |index, item| {
                    fill(item, &items[index].table.fields, $context)
                })
            }
            Field::Vector(items) => {
                let items = items
                    .iter()
                    .map(|item| nest(item, $context))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok($target.set_vector(items)?)
            }
        };

        result
    }};
}

/// Defines a module that builds tables as plans say, with a builder whose
/// types are given, generic over the given parameters
macro_rules! driver {
    (
        $module:ident<$a:lifetime $(, $param:ident: $bound:path)* $(; const $constant:ident: $type:ty)*>,
        builder: $builder:ty,
        in_place: $in_place:ty,
        nested: $nested:ty,
        new: $new:expr,
        versioned: $versioned:expr $(,)?
    ) => {
        mod $module {
            use super::*;

            /// Builds `table` as `plan` says, and returns what the builder
            /// did, and the builder when it was started
            pub fn build<$a $(, $param: $bound)* $(, const $constant: $type)*>(
                table: &$a Table,
                plan: Plan,
            ) -> (Outcome, Option<$builder>) {
                let context = Context::new(table, plan);
                let fields = table.fields.len().saturating_add_signed(plan.extra_fields);
                let offset_type = plan.root.unwrap_or(table.layout.offset_type);

                let mut builder: $builder = match start(table, fields, offset_type) {
                    Ok(builder) => builder,
                    Err(error) => {
                        let steps = vec![Err(Error::Build(error))];
                        return (Outcome { steps, bytes: Err(error) }, None);
                    }
                };

                // A field that fails is set again without the plan's
                // failure, and then, if it fails again, replaced by a uint8,
                // so that the table can be complete, and its bytes show
                // that the failure left the builder as it was
                let mut steps = Vec::with_capacity(table.fields.len());
                for field in &table.fields {
                    let mut set = || set_on_builder(&mut builder, field, &context);
                    let step = panic::catch_unwind(AssertUnwindSafe(&mut set));
                    let step = step.unwrap_or(Err(Error::Panicked));
                    if step.is_ok() {
                        steps.push(step);
                        continue;
                    }
                    steps.push(step);

                    context.failing.set(false);
                    let again = panic::catch_unwind(AssertUnwindSafe(&mut set));
                    let again = again.unwrap_or(Err(Error::Panicked));
                    context.failing.set(true);
                    if again.is_ok() {
                        steps.push(again);
                        continue;
                    }
                    steps.push(again);

                    steps.push(builder.set_u8(0).map_err(Error::from));
                }

                let bytes = builder.to_bytes();
                (Outcome { steps, bytes }, Some(builder))
            }

            /// Starts a builder of `fields` fields in the table's layout,
            /// but of `offset_type` offsets
            fn start<$a $(, $param: $bound)* $(, const $constant: $type)*>(
                table: &Table,
                fields: usize,
                offset_type: OffsetType,
            ) -> Result<$builder, BuildError> {
                let layout = TableLayout::DEFAULT.with_offset_type(offset_type);
                let layout = if table.layout.sized { layout.sized() } else { layout };

                match table.version {
                    Some(version) if table.layout.versioned => {
                        ($versioned)(fields, layout.versioned(), version)
                    }
                    _ => ($new)(fields, layout),
                }
            }

            fn set_on_builder<$a $(, $param: $bound)* $(, const $constant: $type)*>(
                builder: &mut $builder,
                field: &$a Field,
                context: &Context,
            ) -> Result<(), Error> {
                set_field!(builder, field, context)
            }

            fn set_in_place<$a $(, $param: $bound)* $(, const $constant: $type)*>(
                table: &mut $in_place,
                field: &$a Field,
                context: &Context,
            ) -> Result<(), Error> {
                set_field!(table, field, context)
            }

            /// Sets `fields` on a table nested in place, failing as the plan
            /// says
            fn fill<$a $(, $param: $bound)* $(, const $constant: $type)*>(
                table: &mut $in_place,
                fields: &$a [Field],
                context: &Context,
            ) -> Result<(), Error> {
                let call = context.next_fill();
                for (index, field) in fields.iter().enumerate() {
                    if stops(context, call, index)? {
                        return Ok(());
                    }
                    set_in_place(table, field, context)?;
                }

                stops(context, call, fields.len()).map(|_| ())
            }

            /// Builds a nested table, as its builder or its bytes
            fn nest<$a $(, $param: $bound)* $(, const $constant: $type)*>(
                nested: &$a Nested,
                context: &Context,
            ) -> Result<$nested, Error> {
                let table = &nested.table;
                let offset_type = context.nested_offset_type(table);
                let mut builder: $builder = start(table, table.fields.len(), offset_type)?;
                for field in &table.fields {
                    set_on_builder(&mut builder, field, context)?;
                }

                Ok(match nested.from {
                    Source::Builder => builder.into(),
                    Source::Bytes => builder.to_bytes()?.into(),
                })
            }
        }
    };
}

/// Fails as the plan says, before fill call `call` sets field `index`, or
/// returns whether the fill returns there, leaving its fields unset
fn stops(context: &Context, call: usize, index: usize) -> Result<bool, Error> {
    match context.fails(call, index) {
        Some(Fails::Error) => Err(Error::Injected),
        Some(Fails::Panics) => panic!("{INJECTED_PANIC}"),
        Some(Fails::Returns) => Ok(true),
        None => Ok(false),
    }
}

driver!(
    table_builder<'a>,
    builder: TableBuilder<'a>,
    in_place: InPlaceTable<'_, 'a>,
    nested: NestedTable<'a>,
    new: TableBuilder::new,
    versioned: TableBuilder::versioned,
);

driver!(
    direct<'a, B: Buffer; const N: usize>,
    builder: DirectTableBuilder<'a, B, N>,
    in_place: DirectInPlaceTable<'_, 'a, B, N>,
    nested: DirectNestedTable<'a, B, N>,
    new: DirectTableBuilder::<B, N>::with_buffer,
    versioned: DirectTableBuilder::<B, N>::versioned_with_buffer,
);

/// Builds `table` with `TableBuilder` and with a direct builder of bytes `B`
/// and `N` segments within it, and checks that they did the same, and that
/// `into_bytes()` returns what `to_bytes()` does
fn check<B: Buffer, const N: usize>(table: &Table, plan: Plan) -> Result<(), TestCaseError> {
    let (expected, _) = table_builder::build(table, plan);
    let (outcome, builder) = direct::build::<B, N>(table, plan);
    prop_assert_eq!(&outcome, &expected, "{:?}", plan);

    if let Some(builder) = builder {
        prop_assert_eq!(builder.into_bytes(), outcome.bytes);
    }

    Ok(())
}

/// Builds `table` with each of the direct builders that the tests check
fn check_every_builder(table: &Table, plan: Plan) -> Result<(), TestCaseError> {
    // The proposed design
    check::<VecBuffer, 4>(table, plan)?;
    // Bytes within the builder, then on the heap
    check::<InlineBuffer, 4>(table, plan)?;
    // Every run a segment, even an empty one, and the segments in a `Vec`;
    // bytes grown by `Vec` from none
    check::<VecBuffer<0, true, 0>, 0>(table, plan)?;
    // Runs of a byte or more, and of 8 or more, as segments, one or two
    // within the builder; bytes grown by allocating anew and copying
    check::<VecBuffer<8, false, 1>, 1>(table, plan)?;
    check::<InlineBuffer<8>, 2>(table, plan)?;
    // No segments
    check::<VecBuffer<16, true, NO_SEGMENTS>, 4>(table, plan)?;
    check::<InlineBuffer<NO_SEGMENTS>, 0>(table, plan)
}

/// Keeps the panics that the tests inject from printing, and prints others
fn quiet_injected_panics() {
    static HOOK: Once = Once::new();

    HOOK.call_once(|| {
        let hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let payload = info.payload();
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied());
            if message != Some(INJECTED_PANIC) {
                hook(info);
            }
        }));
    });
}

/// Plans of any offset types, as described or replaced, and as often of
/// one offset type for every table, so that tables nested from builders are
/// written in place, and their parents' limits reached
fn plan() -> impl Strategy<Value = Plan> {
    let offset_type = || option::of(strategy::offset_type());
    let offset_types = prop_oneof![
        (offset_type(), offset_type()),
        strategy::offset_type().prop_map(|offset_type| (Some(offset_type), Some(offset_type))),
    ];
    let failure = (
        0..6_usize,
        0..6_usize,
        select(vec![Fails::Error, Fails::Panics, Fails::Returns]),
    )
        .prop_map(|(fill, before, how)| Failure { fill, before, how });

    (
        offset_types,
        -1..=1_isize,
        any::<bool>(),
        any::<bool>(),
        option::of(failure),
    )
        .prop_map(
            |((root, nested), extra_fields, in_place, owned, failure)| Plan {
                root,
                nested,
                extra_fields,
                in_place,
                owned,
                failure,
            },
        )
}

proptest! {
    #![proptest_config(common::config())]

    /// Generated tables, in any layout, with nested tables that can be
    /// written in place, are built by each direct builder as
    /// `TableBuilder` builds them, whatever the plan
    #[test]
    fn test_direct_builders_build_and_fail_as_table_builder(
        table in strategy::table(),
        nested_in_place in any::<bool>(),
        plan in plan(),
    ) {
        quiet_injected_panics();
        let table = if nested_in_place { table.nested_in_place() } else { table };

        check_every_builder(&table, plan)?;
    }
}

/// Every valid table of the conformance suite is rebuilt byte for byte, from
/// builders and in place, borrowed and owned, as `TableBuilder` rebuilds it
#[test]
#[cfg_attr(miri, ignore = "reads files, which Miri isolates")]
fn test_rebuilds_every_valid_table_byte_for_byte() {
    let json = fs::read_to_string(Path::new(CONFORMANCE).join("cases.json")).unwrap();
    let suite: Value = serde_json::from_str(&json).unwrap();

    for case in suite["valid"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let file = Path::new(CONFORMANCE).join(case["file"].as_str().unwrap());
        let bytes = fs::read(file).unwrap();
        let table = table_of(&case["table"]);

        for (in_place, owned) in [(false, false), (true, false), (false, true), (true, true)] {
            let plan = Plan {
                owned,
                ..Plan::as_described(in_place)
            };
            let (outcome, _) = direct::build::<VecBuffer, 4>(&table, plan);
            assert_eq!(outcome.bytes.as_ref(), Ok(&bytes), "{name}, {plan:?}");

            check_every_builder(&table, plan).unwrap_or_else(|error| panic!("{name}: {error}"));
        }
    }
}

/// A float64 array of 40 elements: 320 bytes, a segment, which the builder
/// owns when it's given as a `Vec`
fn long_column() -> Vec<f64> {
    (0..40).map(|index| f64::from(index) * 0.5).collect()
}

/// A builder borrowed into its parent lends it its segments, those that it
/// owns too, and its sized header gets its length in the parent's copy
#[test]
fn test_a_borrowed_builder_is_copied_with_its_segments_and_length() {
    let layout = TableLayout::DEFAULT.sized();
    let build_nested = |column: Vec<f64>| -> Result<TableBuilder<'static>, BuildError> {
        let mut nested = TableBuilder::new(2, layout)?;
        nested.set_u8(1)?;
        nested.set_f64_array(column)?;
        Ok(nested)
    };
    let nested = build_nested(long_column()).unwrap();
    let mut parent = TableBuilder::new(3, TableLayout::DEFAULT).unwrap();
    parent.set_u8(2).unwrap();
    parent.set_table(&nested).unwrap();
    parent.set_u8(3).unwrap();
    let expected = parent.to_bytes().unwrap();

    let mut direct = DirectTableBuilder::new(2, layout).unwrap();
    direct.set_u8(1).unwrap();
    direct.set_f64_array(long_column()).unwrap();
    let mut parent = DirectTableBuilder::new(3, TableLayout::DEFAULT).unwrap();
    parent.set_u8(2).unwrap();
    parent.set_table(&direct).unwrap();
    parent.set_u8(3).unwrap();

    assert_eq!(parent.to_bytes(), Ok(expected));
}

/// A table written in place that its alignment moves forward moves the
/// segments written in it, but not those that its parent wrote before it
#[test]
fn test_a_table_moved_by_its_padding_moves_its_segments() {
    let column = long_column();
    let name = "x".repeat(300);

    let mut expected = TableBuilder::new(3, TableLayout::DEFAULT).unwrap();
    expected.set_str(name.as_str()).unwrap();
    expected.set_u8(1).unwrap();
    expected
        .set_table_with(2, |table| {
            table.set_u8(2)?;
            table.set_f64_array(column.as_slice())
        })
        .unwrap();
    let expected = expected.to_bytes().unwrap();

    let mut builder = DirectTableBuilder::new(3, TableLayout::DEFAULT).unwrap();
    builder.set_str(name.as_str()).unwrap();
    builder.set_u8(1).unwrap();
    builder
        .set_table_with(2, |table| {
            table.set_u8(2)?;
            table.set_f64_array(column.as_slice())
        })
        .unwrap();

    assert_eq!(builder.to_bytes(), Ok(expected));
}

/// `into_bytes()` hands over the builder's bytes when it has no segments,
/// with a sized table's length, and copies them otherwise
#[test]
fn test_into_bytes_returns_what_to_bytes_returns() {
    let layout = TableLayout::DEFAULT.sized();
    let column = long_column();

    let mut short = DirectTableBuilder::new(1, layout).unwrap();
    short.set_f64_array(&column[..2]).unwrap();
    let expected = short.to_bytes().unwrap();
    assert_eq!(short.into_bytes(), Ok(expected));

    let mut long = DirectTableBuilder::new(1, layout).unwrap();
    long.set_f64_array(column.as_slice()).unwrap();
    let expected = long.to_bytes().unwrap();
    assert_eq!(long.into_bytes(), Ok(expected));
}

/// A table nested in place whose fill panics after segments were kept, and
/// its bytes grew onto the heap, leaves the builder as it was
#[test]
fn test_a_panic_after_segments_leaves_the_builder_as_it_was() {
    quiet_injected_panics();
    let column = long_column();

    let mut builder =
        DirectTableBuilder::<InlineBuffer, 1>::with_buffer(2, TableLayout::DEFAULT).unwrap();
    builder.set_u8(1).unwrap();
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        builder.set_vector_with(3, 2, |index, item| {
            item.set_f64_array(column.as_slice())?;
            item.set_u64_array(vec![1; 30])?;
            assert!(index < 2, "{INJECTED_PANIC}");
            Ok::<_, BuildError>(())
        })
    }));
    assert!(panicked.is_err());
    builder.set_u8(2).unwrap();

    let mut expected = TableBuilder::new(2, TableLayout::DEFAULT).unwrap();
    expected.set_u8(1).unwrap();
    expected.set_u8(2).unwrap();
    assert_eq!(builder.to_bytes(), expected.to_bytes());
}

/// The vector's count and ends are checked before any item is written, and
/// the items once they're written, with `TableBuilder`'s errors
#[test]
fn test_reject_vector_too_long_before_and_after_its_items() {
    let layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint8);

    let mut builder = DirectTableBuilder::new(1, layout).unwrap();
    let error = builder.set_vector_with(300, 1, |_, item| item.set_u8(1));
    assert_eq!(
        error,
        Err(BuildError::TooLong {
            index: 0,
            length: 1 + 301,
            offset_type: OffsetType::Uint8
        })
    );

    let error = builder.set_vector_with(60, 1, |_, item| item.set_u32(1));
    assert_eq!(
        error,
        Err(BuildError::TooLong {
            index: 0,
            length: 1 + 61 + 60 * 5,
            offset_type: OffsetType::Uint8
        })
    );

    builder
        .set_vector_with(2, 1, |_, item| {
            item.set_table_aligned(&[1, 2][..], Alignment::Two)
        })
        .unwrap();

    let mut expected = TableBuilder::new(1, layout).unwrap();
    expected
        .set_vector_with(2, 1, |_, item| {
            item.set_table_aligned(&[1, 2][..], Alignment::Two)
        })
        .unwrap();
    assert_eq!(builder.to_bytes(), expected.to_bytes());
}

/// A table nested in place that fits its own offsets but makes its parent
/// too long is rejected once it's written, as the parent's field, and
/// leaves the parent as it was
#[test]
fn test_reject_table_in_place_that_makes_its_parent_too_long() {
    let layout = TableLayout::DEFAULT.with_offset_type(OffsetType::Uint8);
    let error = Err(BuildError::TooLong {
        index: 1,
        length: 2 + 1 + 200 + 1 + 1 + 100,
        offset_type: OffsetType::Uint8,
    });

    let mut expected = TableBuilder::new(2, layout).unwrap();
    expected.set_u8_array(vec![1; 200]).unwrap();
    let rejected = expected.set_table_with(1, |table| table.set_u8_array(vec![2; 100]));
    assert_eq!(rejected, error);
    expected
        .set_table_with(1, |table| table.set_u8_array(vec![3; 10]))
        .unwrap();

    let mut builder = DirectTableBuilder::new(2, layout).unwrap();
    builder.set_u8_array(vec![1; 200]).unwrap();
    let rejected = builder.set_table_with(1, |table| table.set_u8_array(vec![2; 100]));
    assert_eq!(rejected, error);
    builder
        .set_table_with(1, |table| table.set_u8_array(vec![3; 10]))
        .unwrap();

    assert_eq!(builder.to_bytes(), expected.to_bytes());
}
