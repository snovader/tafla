//! A builder whose setter fails is left as it was, whatever fails: a field
//! that the table has no room for, or the fill of a table nested in place,
//! which returns an error, panics or leaves fields unset. Setting the field
//! again, without the failure, sets it as a builder that never failed does.
//!
//! Generated tables are built in their own offset types, or all in one, so
//! that their fields often pass its limits, with a field more or fewer than
//! they hold, from borrowed or owned values, and with the tables nested in
//! them from builders or in place. A table built whole reads back the values
//! that it was built from. Vectors in place that make their tables too long,
//! with vectors in place in their items, fail as from builders, whatever
//! fails while their items run.

mod common;

use std::{
    cell::Cell,
    panic::{self, AssertUnwindSafe},
    sync::Once,
};

use common::{
    description::{
        Field, Layout, Nested, Source, Table, struct_alignment, struct_bytes, struct_size,
    },
    read::read_table,
    strategy,
};
use proptest::{collection::vec, option, prelude::*, sample::select};
use tafla::{BuildError, InPlaceTable, NestedTable, OffsetType, TableBuilder, TableLayout};

/// The message of the panics that a test's fill raises, which the panic hook
/// keeps quiet
const INJECTED_PANIC: &str = "A fill's panic, which the test injects";

/// How a test builds a table: the fields that the root is started with,
/// whether tables are nested in place where they can be, whether values are
/// given owned, and a fill that fails
#[derive(Clone, Copy, Debug)]
struct Plan {
    /// The root's fields beyond the description's: -1, 0 or 1
    extra_fields: isize,
    in_place: bool,
    /// Whether a table in place is given the alignment that its fields
    /// need, and padded before they're set, rather than moved after
    aligned: bool,
    owned: bool,
    failure: Option<Failure>,
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
/// setters, or of the constructor, and of `to_bytes()`, and the result of
/// the setter that the plan's failure failed, before it was set again
#[derive(Debug)]
struct Outcome {
    started: bool,
    steps: Vec<Result<(), Error>>,
    bytes: Result<Vec<u8>, BuildError>,
    failed: Option<Result<(), Error>>,
}

/// The plan, and the fills called so far, as a table is built
struct Context {
    plan: Plan,
    fills: Cell<usize>,
    /// Whether the plan's failure has failed a fill
    failed: Cell<bool>,
}

impl Context {
    const fn new(plan: Plan) -> Self {
        Self {
            plan,
            fills: Cell::new(0),
            failed: Cell::new(false),
        }
    }

    /// Whether a table is nested in place in a table of `offset_type`
    /// offsets, as `Table::to_bytes_in_place()` nests it: from a builder
    /// without a header and of those offsets
    fn in_place(&self, nested: &Nested, offset_type: OffsetType) -> bool {
        let layout = nested.table.layout;
        self.plan.in_place
            && nested.from == Source::Builder
            && !layout.sized
            && !layout.versioned
            && layout.offset_type == offset_type
    }

    /// Whether a vector of `items` is nested in place: each of them is, and
    /// they hold as many fields
    fn all_in_place(&self, items: &[Nested], offset_type: OffsetType) -> bool {
        let fields = items.first().map(|item| item.table.fields.len());
        items
            .iter()
            .all(|item| self.in_place(item, offset_type) && Some(item.table.fields.len()) == fields)
    }

    /// Counts a call of a fill
    fn next_fill(&self) -> usize {
        let fill = self.fills.get();
        self.fills.set(fill + 1);
        fill
    }

    /// Fails as the plan says, before fill call `call` sets field `index`,
    /// or returns whether the fill returns there, leaving its fields unset
    fn stops(&self, call: usize, index: usize) -> Result<bool, Error> {
        let failure = self.plan.failure.filter(|failure| {
            !self.failed.get() && failure.fill == call && failure.before == index
        });
        let Some(failure) = failure else {
            return Ok(false);
        };

        self.failed.set(true);
        match failure.how {
            Fails::Error => Err(Error::Injected),
            Fails::Panics => panic!("{INJECTED_PANIC}"),
            Fails::Returns => Ok(true),
        }
    }
}

/// Sets `$field` on `$target`, a builder or a table nested in place, of
/// `$offset_type` offsets, with the plan of `$context`, and evaluates to the
/// setter's result
macro_rules! set_field {
    ($target:ident, $offset_type:expr, $field:ident, $context:ident) => {{
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
            } if $context.in_place(nested, $offset_type) => {
                let fields = &nested.table.fields;
                let set =
                    |table: &mut InPlaceTable<'_>| fill(table, $offset_type, fields, $context);
                if $context.plan.aligned {
                    $target.set_table_with_alignment(fields.len(), nested.table.alignment(), set)
                } else {
                    $target.set_table_with(fields.len(), set)
                }
            }
            Field::Table { nested, alignment } => {
                let table = nest(nested, $context)?;
                match alignment {
                    Some(alignment) => Ok($target.set_table_aligned(table, *alignment)?),
                    None => Ok($target.set_table(table)?),
                }
            }
            Field::Vector(items) if $context.all_in_place(items, $offset_type) => {
                let fields = items.first().map_or(0, |item| item.table.fields.len());
                $target.set_vector_with(items.len(), fields, |index, item| {
                    fill(item, $offset_type, &items[index].table.fields, $context)
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

/// Builds `table` as `plan` says, and returns what the builder did
fn build(table: &Table, plan: Plan) -> Outcome {
    let context = Context::new(plan);
    let fields = table.fields.len().saturating_add_signed(plan.extra_fields);
    let offset_type = table.layout.offset_type;

    let mut builder = match start(table, fields) {
        Ok(builder) => builder,
        Err(error) => {
            let steps = vec![Err(Error::Build(error))];
            let bytes = Err(error);
            return Outcome {
                started: false,
                steps,
                bytes,
                failed: None,
            };
        }
    };

    // A field that the plan's failure fails is set again, without it. A
    // field that fails then, or fails without it, is replaced by a uint8,
    // so that the table can be complete, and its bytes show that the
    // failure left the builder as it was.
    let mut steps = Vec::with_capacity(table.fields.len());
    let mut failed = None;
    for field in &table.fields {
        let mut set = || {
            let set = || set_on_builder(&mut builder, offset_type, field, &context);
            panic::catch_unwind(AssertUnwindSafe(set)).unwrap_or(Err(Error::Panicked))
        };

        let failed_before = context.failed.get();
        let mut step = set();
        if step.is_err() && !failed_before && context.failed.get() {
            failed = Some(step);
            step = set();
        }

        let replace = step.is_err();
        steps.push(step);
        if replace {
            steps.push(builder.set_u8(0).map_err(Error::from));
        }
    }

    let bytes = builder.to_bytes();
    Outcome {
        started: true,
        steps,
        bytes,
        failed,
    }
}

/// Starts a builder of `fields` fields in the table's layout
fn start(table: &Table, fields: usize) -> Result<TableBuilder, BuildError> {
    let layout = table.layout.unversioned();

    match table.version {
        Some(version) if table.layout.versioned => {
            TableBuilder::new(fields, layout.with_version(version))
        }
        _ => TableBuilder::new(fields, layout),
    }
}

fn set_on_builder(
    builder: &mut TableBuilder,
    offset_type: OffsetType,
    field: &Field,
    context: &Context,
) -> Result<(), Error> {
    set_field!(builder, offset_type, field, context)
}

fn set_in_place(
    table: &mut InPlaceTable<'_>,
    offset_type: OffsetType,
    field: &Field,
    context: &Context,
) -> Result<(), Error> {
    set_field!(table, offset_type, field, context)
}

/// Sets `fields` on a table nested in place in a table of `offset_type`
/// offsets, failing as the plan says
fn fill(
    table: &mut InPlaceTable<'_>,
    offset_type: OffsetType,
    fields: &[Field],
    context: &Context,
) -> Result<(), Error> {
    let call = context.next_fill();
    for (index, field) in fields.iter().enumerate() {
        if context.stops(call, index)? {
            return Ok(());
        }
        set_in_place(table, offset_type, field, context)?;
    }

    context.stops(call, fields.len()).map(|_| ())
}

/// Builds a nested table, as its builder or its bytes
fn nest<'a>(nested: &'a Nested, context: &Context) -> Result<NestedTable<'a>, Error> {
    let table = &nested.table;
    let mut builder = start(table, table.fields.len())?;
    for field in &table.fields {
        set_on_builder(&mut builder, table.layout.offset_type, field, context)?;
    }

    Ok(match nested.from {
        Source::Builder => builder.into(),
        Source::Bytes => builder.to_bytes()?.into(),
    })
}

/// The table with it and every table nested in it of `offset_type`
/// offsets, which reads as the table does
fn of_offset_type(table: &Table, offset_type: OffsetType) -> Table {
    let nested = |nested: &Nested| Nested {
        from: nested.from,
        table: of_offset_type(&nested.table, offset_type),
    };
    let fields = table.fields.iter().map(|field| match field {
        Field::Table {
            nested: table,
            alignment,
        } => Field::Table {
            nested: nested(table),
            alignment: *alignment,
        },
        Field::Vector(items) => Field::Vector(items.iter().map(nested).collect()),
        field => field.clone(),
    });

    let mut layout = table.layout;
    layout.offset_type = offset_type;
    Table {
        layout,
        version: table.version,
        fields: fields.collect(),
    }
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

fn plan() -> impl Strategy<Value = Plan> {
    let failure = (
        0..4_usize,
        0..4_usize,
        select(vec![Fails::Error, Fails::Panics, Fails::Returns]),
    )
        .prop_map(|(fill, before, how)| Failure { fill, before, how });

    (
        -1..=1_isize,
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        option::of(failure),
    )
        .prop_map(|(extra_fields, in_place, aligned, owned, failure)| Plan {
            extra_fields,
            in_place,
            aligned,
            owned,
            failure,
        })
}

/// Plans that build tables in place, and fail one of the first 80 fills,
/// mostly once a vector has made its table too long
fn in_place_plan() -> impl Strategy<Value = Plan> {
    let failure = (
        0..80_usize,
        0..3_usize,
        select(vec![Fails::Error, Fails::Panics, Fails::Returns]),
    )
        .prop_map(|(fill, before, how)| Failure { fill, before, how });

    (any::<bool>(), any::<bool>(), option::of(failure)).prop_map(|(aligned, owned, failure)| Plan {
        extra_fields: 0,
        in_place: true,
        aligned,
        owned,
        failure,
    })
}

/// A table of `uint8` or `uint16` offsets of a uint8 and a vector in place,
/// whose items, each a string and a vector in place of strings, make the
/// table too long more often than not
fn long_vectors() -> impl Strategy<Value = Table> {
    select(vec![OffsetType::Uint8, OffsetType::Uint16]).prop_flat_map(|offset_type| {
        let longest: usize = if offset_type == OffsetType::Uint8 {
            24
        } else {
            2_400
        };
        let string = move || (0..=longest).prop_map(|length| Field::String("x".repeat(length)));
        let entry = string().prop_map(move |name| in_place(offset_type, vec![name]));
        let item = (string(), vec(entry, 0..=3)).prop_map(move |(name, entries)| {
            in_place(offset_type, vec![name, Field::Vector(entries)])
        });

        vec(item, 1..=40).prop_map(move |items| Table {
            layout: in_place(offset_type, Vec::new()).table.layout,
            version: None,
            fields: vec![Field::U8(1), Field::Vector(items)],
        })
    })
}

/// A table of `fields` to nest in place in a table of `offset_type` offsets
const fn in_place(offset_type: OffsetType, fields: Vec<Field>) -> Nested {
    let layout = Layout {
        sized: false,
        versioned: false,
        offset_type,
    };

    Nested {
        from: Source::Builder,
        table: Table {
            layout,
            version: None,
            fields,
        },
    }
}

/// Generated tables, as described or with tables nested from builders that
/// can be nested in place, in their own offset types or all in one, more
/// often than not, so that their fields often pass its limits
fn table() -> impl Strategy<Value = Table> {
    let offset_type = option::weighted(0.75, strategy::offset_type());

    (strategy::table(), any::<bool>(), offset_type).prop_map(
        |(table, nested_in_place, offset_type)| {
            let table = if nested_in_place {
                table.nested_in_place()
            } else {
                table
            };

            match offset_type {
                Some(offset_type) => of_offset_type(&table, offset_type),
                None => table,
            }
        },
    )
}

proptest! {
    #![proptest_config(common::config())]

    /// The plan's failure, a fill that fails, panics or returns early, and
    /// values given owned rather than borrowed, change nothing that the
    /// builder does, but the failed setter's result, which is the failure's
    #[test]
    fn test_a_failure_leaves_the_builder_as_it_was(table in table(), plan in plan()) {
        quiet_injected_panics();
        let unfailing = Plan {
            owned: false,
            failure: None,
            ..plan
        };

        let expected = build(&table, unfailing);
        let outcome = build(&table, plan);

        prop_assert_eq!(&outcome.steps, &expected.steps);
        prop_assert_eq!(&outcome.bytes, &expected.bytes);
        if let (Some(failed), Some(failure)) = (&outcome.failed, plan.failure) {
            let as_planned = match failure.how {
                Fails::Error => *failed == Err(Error::Injected),
                Fails::Panics => *failed == Err(Error::Panicked),
                Fails::Returns => matches!(failed, Err(Error::Build(BuildError::Incomplete { .. }))),
            };
            prop_assert!(as_planned, "{:?}", failed);
        }
    }

    /// A table whose fields were all set, whatever failed on the way, reads
    /// back the values that it was built from. A table started with a field
    /// more is incomplete, and one started with a field fewer holds all but
    /// the last.
    #[test]
    fn test_a_table_built_after_failures_reads_back_its_values(table in table(), plan in plan()) {
        quiet_injected_panics();
        let outcome = build(&table, plan);

        let fields = table.fields.len();
        // Whether the first `count` fields were each set by their first
        // step
        let first_set = |count: usize| {
            outcome.started && outcome.steps.iter().take(count).all(Result::is_ok)
        };
        let all_set = first_set(fields) && outcome.steps.len() == fields;
        match plan.extra_fields {
            0 if all_set => {
                let bytes = outcome.bytes.as_deref().unwrap();
                prop_assert_eq!(read_table(bytes, &table), Ok(table));
            }
            1 if all_set => {
                let incomplete = BuildError::Incomplete { set: fields, fields: fields + 1 };
                prop_assert_eq!(outcome.bytes, Err(incomplete));
            }
            -1 if fields > 0 && first_set(fields - 1) => {
                let mut table = table;
                table.fields.pop();
                let bytes = outcome.bytes.as_deref().unwrap();
                prop_assert_eq!(read_table(bytes, &table), Ok(table));
            }
            _ => {}
        }
    }

    /// A vector in place, and the vectors in place in its items, fail as
    /// from builders, with the length that all the items make, when they
    /// make the table too long
    #[test]
    fn test_a_vector_in_place_fails_as_from_builders(table in long_vectors()) {
        let in_place = Plan {
            extra_fields: 0,
            in_place: true,
            aligned: false,
            owned: false,
            failure: None,
        };
        let from_builders = Plan {
            in_place: false,
            ..in_place
        };

        let expected = build(&table, from_builders);
        let outcome = build(&table, in_place);
        prop_assert_eq!(&outcome.steps, &expected.steps);
        prop_assert_eq!(&outcome.bytes, &expected.bytes);
    }

    /// A fill that fails, panics or returns early while a vector in place
    /// runs its items, after they made the table too long too, changes
    /// nothing but the failed setter's result
    #[test]
    fn test_a_failure_in_a_vector_too_long_leaves_the_builder_as_it_was(
        table in long_vectors(),
        plan in in_place_plan(),
    ) {
        quiet_injected_panics();
        let unfailing = Plan {
            owned: false,
            failure: None,
            ..plan
        };

        let expected = build(&table, unfailing);
        let outcome = build(&table, plan);
        prop_assert_eq!(&outcome.steps, &expected.steps);
        prop_assert_eq!(&outcome.bytes, &expected.bytes);
    }
}

/// A float64 array of 40 elements: 320 bytes
fn long_column() -> Vec<f64> {
    (0..40).map(|index| f64::from(index) * 0.5).collect()
}

/// A vector nested in place whose fill panics, after its items' long arrays
/// were written, leaves the builder as it was, with the values set before it
#[test]
fn test_a_panic_after_long_items_leaves_the_builder_as_it_was() {
    quiet_injected_panics();
    let column = long_column();
    let name = "x".repeat(300);

    let build = |panics: bool| -> Result<Vec<u8>, BuildError> {
        let mut builder = TableBuilder::new(4, TableLayout::DEFAULT)?;
        builder.set_f64_array(column.as_slice())?;
        builder.set_str(name.clone())?;
        builder.set_u8(1)?;

        if panics {
            let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
                builder.set_vector_with(3, 2, |index, item| {
                    item.set_f64_array(column.as_slice())?;
                    item.set_u64_array(vec![1; 40])?;
                    assert!(index < 2, "{INJECTED_PANIC}");
                    Ok::<_, BuildError>(())
                })
            }));
            assert!(panicked.is_err());
        }

        builder.set_vector_with(1, 1, |_, item| item.set_f64_array(column.as_slice()))?;
        builder.to_bytes()
    };

    assert_eq!(build(true), build(false));
}
