//! The runtime's `TableBuilder`, which writes into one buffer, and #217's
//! rewrite write the bytes that main's builder writes, and fail as it fails: for every table of the conformance suite, and for
//! generated tables, built in every offset type so that their fields often
//! pass its limits, with fields set past the table's count or left unset,
//! and with tables nested in place whose fills fail, panic or leave fields
//! unset. Each setter's result is compared, and the setters that follow a
//! failure show that it left the builder as it was. Scratch only, with
//! main's builder copied into the runtime as `tafla::main_builder`.

#![cfg(feature = "main-builder")]


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
    BuildError, InPlaceTable, NestedTable, OffsetType, TableBuilder, TableLayout, declared_builder,
    early_builder, main_builder, rewrite_builder,
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
            Field::BoolArray(values) => Ok(array!(set_bool_array, values)?),
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
            // The list of strings is owned, as only the builder holds it,
            // and the strings borrowed
            Field::StringVector(strings) => {
                let strings: Vec<&str> = strings.iter().map(String::as_str).collect();
                Ok($target.set_str_vector(strings)?)
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
        new: $new:expr $(,)?
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
                        ($new)(fields, layout.with_version(version))
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
    builder: main_builder::TableBuilder<'a>,
    in_place: main_builder::InPlaceTable<'_, 'a>,
    nested: main_builder::NestedTable<'a>,
    new: main_builder::TableBuilder::new,
);

driver!(
    rewrite<'a>,
    builder: rewrite_builder::TableBuilder<'a>,
    in_place: rewrite_builder::InPlaceTable<'_, 'a>,
    nested: rewrite_builder::NestedTable<'a>,
    new: rewrite_builder::TableBuilder::new,
);

driver!(
    direct<'a>,
    builder: TableBuilder,
    in_place: InPlaceTable<'_>,
    nested: NestedTable<'a>,
    new: TableBuilder::new,
);

driver!(
    early<'a>,
    builder: early_builder::TableBuilder,
    in_place: early_builder::InPlaceTable<'_>,
    nested: early_builder::NestedTable<'a>,
    new: early_builder::TableBuilder::new,
);

driver!(
    declared<'a>,
    builder: declared_builder::TableBuilder,
    in_place: declared_builder::InPlaceTable<'_>,
    nested: declared_builder::NestedTable<'a>,
    new: declared_builder::TableBuilder::new,
);

/// Builds `table` with main's builder, with #217's and with the runtime's,
/// which writes into one buffer, and its variants, and checks that they did
/// the same. The variant of declared alignments declares none here, so it
/// moves each table in place that needs padding, as the runtime's does.
fn check(table: &Table, plan: Plan) -> Result<(), TestCaseError> {
    let (expected, _) = table_builder::build(table, plan);
    let (outcome, _) = direct::build(table, plan);
    prop_assert_eq!(&outcome, &expected, "{:?}", plan);
    let (outcome, _) = rewrite::build(table, plan);
    prop_assert_eq!(&outcome, &expected, "#217: {:?}", plan);
    let (outcome, _) = early::build(table, plan);
    prop_assert_eq!(&outcome, &expected, "early moves: {:?}", plan);
    let (outcome, _) = declared::build(table, plan);
    prop_assert_eq!(&outcome, &expected, "declared alignment: {:?}", plan);

    Ok(())
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
        0..4_usize,
        0..4_usize,
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
    fn test_the_rewrite_builds_and_fails_as_main_does(
        table in strategy::table(),
        nested_in_place in any::<bool>(),
        plan in plan(),
    ) {
        quiet_injected_panics();
        let table = if nested_in_place { table.nested_in_place() } else { table };

        check(&table, plan)?;
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
            let (outcome, _) = direct::build(&table, plan);
            assert_eq!(outcome.bytes.as_ref(), Ok(&bytes), "{name}, {plan:?}");

            check(&table, plan).unwrap_or_else(|error| panic!("{name}: {error}"));
        }
    }
}
