// The code generated from `schemas/all_types.tafla` writes every type of field
// as `TableBuilder` writes it by hand: each scalar and bool, each array,
// strings, tables nested in tables, vectors of tables that hold vectors of
// themselves, fields named after keywords, structs alone, within structs,
// in tables and in arrays, and enums of every integer type, as the integers
// that store them, into new bytes and into the start of bytes that the
// caller gives, arrays of structs from views of the caller's own items too.
// It reads back what it writes, collecting the views that its
// getters of vectors and arrays of structs return, or reading their items by
// index or by iterating, and reads corrupted bytes as values or decode errors.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <cmath>
#include <limits>
#include <list>
#include <optional>
#include <ranges>
#include <span>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

#include "all_types/all_types.hpp"
#include "common/build.hpp"
#include "common/given_bytes.hpp"
#include "common/values.hpp"

namespace tafla::test {
namespace {

Layout headerless() {
    return Layout::headerless(OffsetType::uint32);
}

Field nested(std::vector<Field> fields) {
    return {TableField{{Source::builder, {headerless(), std::nullopt, std::move(fields)}}, std::nullopt}};
}

Nested item(std::vector<Field> fields) {
    return {Source::builder, {headerless(), std::nullopt, std::move(fields)}};
}

template <class T>
std::vector<T> vector_of(std::span<const T> values) {
    return {values.begin(), values.end()};
}

/// What reading returned: the table read, or nothing for a decode error
template <class Read>
std::optional<Table> try_read(Read read) {
    try {
        return read();
    } catch (const DecodeError&) {
        return std::nullopt;
    }
}

/// The integer that stores an enum's value, as C++23's `std::to_underlying()`
/// gives it
template <class E>
constexpr std::underlying_type_t<E> integer(E value) {
    return static_cast<std::underlying_type_t<E>>(value);
}

// Structs, whose scalars the descriptions hold one after another, with a bool
// as its uint8, an enum as its integer and a struct within a struct in its
// place

void push_scalars(const all_types::Point& point, std::vector<Field>& scalars) {
    scalars.push_back({point.x});
    scalars.push_back({point.y});
}

void push_scalars(const all_types::Segment& segment, std::vector<Field>& scalars) {
    push_scalars(segment.start, scalars);
    push_scalars(segment.end, scalars);
}

void push_scalars(const all_types::Documented& documented, std::vector<Field>& scalars) {
    scalars.push_back({documented.one});
    scalars.push_back({documented.two});
    scalars.push_back({documented.undocumented});
}

void push_scalars(const all_types::Packed& packed, std::vector<Field>& scalars) {
    scalars.insert(scalars.end(), {{packed.int8},
                                   {packed.uint8},
                                   {packed.int16},
                                   {packed.uint16},
                                   {packed.int32},
                                   {packed.uint32},
                                   {packed.int64},
                                   {packed.uint64},
                                   {packed.float32},
                                   {packed.float64},
                                   {static_cast<std::uint8_t>(packed.bool_)}});
}

void push_scalars(const all_types::Turn& turn, std::vector<Field>& scalars) {
    scalars.push_back({integer(turn.direction)});
    scalars.push_back({integer(turn.mask)});
}

void push_scalars(const all_types::Mark& mark, std::vector<Field>& scalars) {
    scalars.push_back({integer(mark.level)});
    scalars.push_back({integer(mark.change)});
    scalars.push_back({mark.weight});
    scalars.push_back({integer(mark.extent)});
    push_scalars(mark.turn, scalars);
}

template <class S>
Field structure(const S& value) {
    Struct structure;
    push_scalars(value, structure.scalars);
    return {structure};
}

template <class S>
Field struct_array(std::span<const S> values, std::vector<ScalarType> types) {
    StructArray array{std::move(types), {}};
    for (const auto& value : values) {
        std::vector<Field> scalars;
        push_scalars(value, scalars);
        array.items.push_back(std::move(scalars));
    }
    return {array};
}

const std::vector<ScalarType> kPoint{ScalarType::float32, ScalarType::float32};
const std::vector<ScalarType> kSegment{ScalarType::float32, ScalarType::float32, ScalarType::float32,
                                       ScalarType::float32};
const std::vector<ScalarType> kDocumented{ScalarType::uint8, ScalarType::uint16, ScalarType::uint32};
const std::vector<ScalarType> kMark{ScalarType::uint8, ScalarType::int16, ScalarType::float32,
                                    ScalarType::int64, ScalarType::int8,  ScalarType::uint64};

// The descriptions of what each fields struct writes

Table describe(const all_types::ScalarsFields& fields) {
    return {Layout{},
            std::nullopt,
            {{fields.int8},
             {fields.uint8},
             {fields.int16},
             {fields.uint16},
             {fields.int32},
             {fields.uint32},
             {fields.int64},
             {fields.uint64},
             {fields.float32},
             {fields.float64},
             {Bool{fields.bool_}}}};
}

Table describe(const all_types::ArraysFields& fields) {
    return {Layout{},
            std::nullopt,
            {{vector_of(fields.int8)},
             {vector_of(fields.uint8)},
             {vector_of(fields.int16)},
             {vector_of(fields.uint16)},
             {vector_of(fields.int32)},
             {vector_of(fields.uint32)},
             {vector_of(fields.int64)},
             {vector_of(fields.uint64)},
             {vector_of(fields.float32)},
             {vector_of(fields.float64)}}};
}

std::vector<Field> describe_tree(const all_types::TreeFields& tree) {
    VectorField children;
    for (const auto& child : tree.children) {
        children.items.push_back(item(describe_tree(child)));
    }
    return {{tree.value}, {children}};
}

Table describe(const all_types::NestedFields& fields) {
    const auto& inner = fields.outer.inner;
    VectorField trees;
    for (const auto& tree : fields.trees) {
        trees.items.push_back(item(describe_tree(tree)));
    }
    return {Layout{},
            std::nullopt,
            {{std::string(fields.label)},
             nested({{fields.outer.count}, nested({{std::string(inner.name)}, {vector_of(inner.values)}})}),
             {trees}}};
}

Table describe(const all_types::ReservedFields& fields) {
    const auto& keywords = fields.keywords;
    return {Layout{},
            std::nullopt,
            {nested({{keywords.class_},
                     {keywords.default_},
                     {keywords.in},
                     {keywords.match},
                     {keywords.self},
                     {std::string(keywords.type)}})}};
}

Table describe(const all_types::StructsFields& fields) {
    // The path's array of structs aligns it to 4
    return {Layout{},
            std::nullopt,
            {structure(fields.packed), struct_array(fields.documented, kDocumented), structure(fields.segment),
             struct_array(fields.segments, kSegment),
             nested({structure(fields.path.start), struct_array(fields.path.points, kPoint)})}};
}

std::vector<Field> describe_step(const all_types::StepFields& step) {
    return {{integer(step.level)}, structure(step.mark), {std::string(step.name)}};
}

Table describe(const all_types::EnumsFields& fields) {
    VectorField steps;
    for (const auto& step : fields.steps) {
        steps.items.push_back(item(describe_step(step)));
    }
    return {Layout{},
            std::nullopt,
            {{integer(fields.level)},
             {vector_of(fields.levels)},
             {integer(fields.direction)},
             {vector_of(fields.directions)},
             {integer(fields.unit)},
             {vector_of(fields.units)},
             {integer(fields.change)},
             {vector_of(fields.changes)},
             {integer(fields.status)},
             {vector_of(fields.statuses)},
             {integer(fields.sign)},
             {vector_of(fields.signs)},
             {integer(fields.mask)},
             {vector_of(fields.masks)},
             {integer(fields.extent)},
             {vector_of(fields.extents)},
             {integer(fields.word)},
             structure(fields.mark),
             struct_array(fields.marks, kMark),
             {steps}}};
}

// The descriptions of what each generated reader reads, through its getters

Table read(const all_types::Scalars& scalars) {
    return {Layout{},
            std::nullopt,
            {{scalars.int8()},
             {scalars.uint8()},
             {scalars.int16()},
             {scalars.uint16()},
             {scalars.int32()},
             {scalars.uint32()},
             {scalars.int64()},
             {scalars.uint64()},
             {scalars.float32()},
             {scalars.float64()},
             {Bool{scalars.bool_()}}}};
}

Table read(const all_types::Arrays& arrays) {
    return {Layout{},
            std::nullopt,
            {{arrays.int8().to_vector()},
             {vector_of(arrays.uint8())},
             {arrays.int16().to_vector()},
             {arrays.uint16().to_vector()},
             {arrays.int32().to_vector()},
             {arrays.uint32().to_vector()},
             {arrays.int64().to_vector()},
             {arrays.uint64().to_vector()},
             {arrays.float32().to_vector()},
             {arrays.float64().to_vector()}}};
}

/// How a test reads the items of a vector or an array of structs from the
/// view that the getter returns: at once with `to_vector()`, one at a time by
/// index, or by iterating the view
enum class Items { collected, by_index, iterated };

template <class T>
T item_at(const tafla::TableVector<T>& view, std::size_t index) {
    return view.get(index);
}

template <class T>
T item_at(const tafla::StructArray<T>& view, std::size_t index) {
    return view.at(index);
}

template <class View>
auto items_of(const View& view, Items way) {
    switch (way) {
        case Items::collected:
            return view.to_vector();
        case Items::by_index: {
            std::vector<typename View::value_type> items;
            for (std::size_t index = 0; index < view.size(); ++index) {
                items.push_back(item_at(view, index));
            }
            return items;
        }
        case Items::iterated:
            break;
    }
    return std::vector<typename View::value_type>(view.begin(), view.end());
}

std::vector<Field> read_tree(const all_types::Tree& tree, Items way) {
    VectorField children;
    for (const auto& child : items_of(tree.children(), way)) {
        children.items.push_back(item(read_tree(child, way)));
    }
    return {{tree.value()}, {children}};
}

Table read(const all_types::Nested& nested_table, Items way) {
    const auto outer = nested_table.outer();
    const auto inner = outer.inner();
    VectorField trees;
    for (const auto& tree : items_of(nested_table.trees(), way)) {
        trees.items.push_back(item(read_tree(tree, way)));
    }
    return {Layout{},
            std::nullopt,
            {{std::string(nested_table.label())},
             nested({{outer.count()}, nested({{std::string(inner.name())}, {inner.values().to_vector()}})}),
             {trees}}};
}

Table read(const all_types::Reserved& reserved) {
    const auto keywords = reserved.keywords();
    return {Layout{},
            std::nullopt,
            {nested({{keywords.class_()},
                     {keywords.default_()},
                     {keywords.in()},
                     {keywords.match()},
                     {keywords.self()},
                     {std::string(keywords.type())}})}};
}

Table read(const all_types::Structs& structs, Items way) {
    const auto path = structs.path();
    const auto documented = items_of(structs.documented(), way);
    const auto segments = items_of(structs.segments(), way);
    const auto points = items_of(path.points(), way);
    return {Layout{},
            std::nullopt,
            {structure(structs.packed()), struct_array(std::span<const all_types::Documented>(documented), kDocumented),
             structure(structs.segment()), struct_array(std::span<const all_types::Segment>(segments), kSegment),
             nested({structure(path.start()), struct_array(std::span<const all_types::Point>(points), kPoint)})}};
}

std::vector<Field> read_step(const all_types::Step& step) {
    return {{integer(step.level())}, structure(step.mark()), {std::string(step.name())}};
}

Table read(const all_types::Enums& enums, Items way) {
    const auto marks = items_of(enums.marks(), way);
    VectorField steps;
    for (const auto& step : items_of(enums.steps(), way)) {
        steps.items.push_back(item(read_step(step)));
    }
    return {Layout{},
            std::nullopt,
            {{integer(enums.level())},
             {vector_of(enums.levels())},
             {integer(enums.direction())},
             {enums.directions().to_vector()},
             {integer(enums.unit())},
             {enums.units().to_vector()},
             {integer(enums.change())},
             {enums.changes().to_vector()},
             {integer(enums.status())},
             {enums.statuses().to_vector()},
             {integer(enums.sign())},
             {enums.signs().to_vector()},
             {integer(enums.mask())},
             {enums.masks().to_vector()},
             {integer(enums.extent())},
             {enums.extents().to_vector()},
             {integer(enums.word())},
             structure(enums.mark()),
             struct_array(std::span<const all_types::Mark>(marks), kMark),
             {steps}}};
}

// Values of every type

rc::Gen<all_types::ScalarsFields> scalars() {
    return rc::gen::exec([] {
        return all_types::ScalarsFields{
            .int8 = *values::integers<std::int8_t>(),
            .uint8 = *values::integers<std::uint8_t>(),
            .int16 = *values::integers<std::int16_t>(),
            .uint16 = *values::integers<std::uint16_t>(),
            .int32 = *values::integers<std::int32_t>(),
            .uint32 = *values::integers<std::uint32_t>(),
            .int64 = *values::integers<std::int64_t>(),
            .uint64 = *values::integers<std::uint64_t>(),
            .float32 = *strategy::float32(),
            .float64 = *strategy::float64(),
            .bool_ = *rc::gen::arbitrary<bool>(),
        };
    });
}

/// The values of an `Arrays`, which `ArraysFields` views
struct OwnedArrays {
    std::vector<std::int8_t> int8;
    std::vector<std::uint8_t> uint8;
    std::vector<std::int16_t> int16;
    std::vector<std::uint16_t> uint16;
    std::vector<std::int32_t> int32;
    std::vector<std::uint32_t> uint32;
    std::vector<std::int64_t> int64;
    std::vector<std::uint64_t> uint64;
    std::vector<float> float32;
    std::vector<double> float64;

    [[nodiscard]] all_types::ArraysFields fields() const {
        return {int8, uint8, int16, uint16, int32, uint32, int64, uint64, float32, float64};
    }
};

rc::Gen<OwnedArrays> arrays() {
    return rc::gen::exec([] {
        return OwnedArrays{
            *values::arrays(values::integers<std::int8_t>()),
            *values::arrays(values::integers<std::uint8_t>()),
            *values::arrays(values::integers<std::int16_t>()),
            *values::arrays(values::integers<std::uint16_t>()),
            *values::arrays(values::integers<std::int32_t>()),
            *values::arrays(values::integers<std::uint32_t>()),
            *values::arrays(values::integers<std::int64_t>()),
            *values::arrays(values::integers<std::uint64_t>()),
            *values::arrays(strategy::float32()),
            *values::arrays(strategy::float64()),
        };
    });
}

/// A tree of values, which `TreeFields` view through spans of their children
struct OwnedTree {
    std::int32_t value;
    std::vector<OwnedTree> children;
};

rc::Gen<std::vector<OwnedTree>> forests(int depth) {
    return values::arrays(rc::gen::exec([depth] {
                              OwnedTree tree{*values::integers<std::int32_t>(), {}};
                              if (depth > 0) {
                                  tree.children = *forests(depth - 1);
                              }
                              return tree;
                          }),
                          3);
}

/// The fields structs of trees, which point into storage that keeps each
/// level's children where they are
std::vector<all_types::TreeFields> tree_fields(const std::vector<OwnedTree>& trees,
                                               std::list<std::vector<all_types::TreeFields>>& storage) {
    std::vector<all_types::TreeFields> fields;
    for (const auto& tree : trees) {
        const auto& children = storage.emplace_back(tree_fields(tree.children, storage));
        fields.push_back({.value = tree.value, .children = children});
    }
    return fields;
}

struct OwnedNested {
    std::string label;
    std::uint32_t count;
    std::string name;
    std::vector<double> values;
    std::vector<OwnedTree> trees;
};

rc::Gen<OwnedNested> owned_nested() {
    return rc::gen::construct<OwnedNested>(values::strings(), values::integers<std::uint32_t>(), values::strings(),
                                           values::arrays(strategy::float64()), forests(2));
}

rc::Gen<all_types::Point> points() {
    return rc::gen::exec([] { return all_types::Point{.x = *strategy::float32(), .y = *strategy::float32()}; });
}

rc::Gen<all_types::Segment> segments() {
    return rc::gen::exec([] { return all_types::Segment{.start = *points(), .end = *points()}; });
}

rc::Gen<all_types::Packed> packeds() {
    return rc::gen::exec([] {
        return all_types::Packed{
            .int8 = *values::integers<std::int8_t>(),
            .uint8 = *values::integers<std::uint8_t>(),
            .int16 = *values::integers<std::int16_t>(),
            .uint16 = *values::integers<std::uint16_t>(),
            .int32 = *values::integers<std::int32_t>(),
            .uint32 = *values::integers<std::uint32_t>(),
            .int64 = *values::integers<std::int64_t>(),
            .uint64 = *values::integers<std::uint64_t>(),
            .float32 = *strategy::float32(),
            .float64 = *strategy::float64(),
            .bool_ = *rc::gen::arbitrary<bool>(),
        };
    });
}

rc::Gen<all_types::Documented> documenteds() {
    return rc::gen::exec([] {
        return all_types::Documented{
            .one = *values::integers<std::uint8_t>(),
            .two = *values::integers<std::uint16_t>(),
            .undocumented = *values::integers<std::uint32_t>(),
        };
    });
}

struct OwnedStructs {
    all_types::Packed packed;
    std::vector<all_types::Documented> documented;
    all_types::Segment segment;
    std::vector<all_types::Segment> segments;
    all_types::Point start;
    std::vector<all_types::Point> points;

    [[nodiscard]] all_types::StructsFields fields() const {
        return {packed, documented, segment, segments, {start, points}};
    }
};

rc::Gen<OwnedStructs> owned_structs() {
    return rc::gen::construct<OwnedStructs>(packeds(), values::arrays(documenteds(), 8), segments(),
                                            values::arrays(segments(), 8), points(), values::arrays(points(), 8));
}

/// Values of an enum: any integer of its type, most of which it doesn't name
template <class E>
rc::Gen<E> enums() {
    return rc::gen::map(values::integers<std::underlying_type_t<E>>(),
                        [](std::underlying_type_t<E> value) { return static_cast<E>(value); });
}

rc::Gen<all_types::Mark> marks() {
    return rc::gen::exec([] {
        return all_types::Mark{
            .level = *enums<all_types::Level>(),
            .change = *enums<all_types::Change>(),
            .weight = *strategy::float32(),
            .extent = *enums<all_types::Extent>(),
            .turn = {.direction = *enums<all_types::Direction>(), .mask = *enums<all_types::Mask>()},
        };
    });
}

/// The values of a `Step`, whose fields struct views its name
struct OwnedStep {
    all_types::Level level;
    all_types::Mark mark;
    std::string name;
};

rc::Gen<OwnedStep> owned_steps() {
    return rc::gen::construct<OwnedStep>(enums<all_types::Level>(), marks(), values::strings());
}

std::vector<all_types::StepFields> step_fields(const std::vector<OwnedStep>& steps) {
    std::vector<all_types::StepFields> fields;
    for (const auto& step : steps) {
        fields.push_back({.level = step.level, .mark = step.mark, .name = step.name});
    }
    return fields;
}

/// The values of an `Enums`, which `EnumsFields` views, but for the fields of
/// its steps, which view their own values
struct OwnedEnums {
    all_types::Level level;
    std::vector<std::uint8_t> levels;
    all_types::Direction direction;
    std::vector<std::int8_t> directions;
    all_types::Unit unit;
    std::vector<std::uint16_t> units;
    all_types::Change change;
    std::vector<std::int16_t> changes;
    all_types::Status status;
    std::vector<std::uint32_t> statuses;
    all_types::Sign sign;
    std::vector<std::int32_t> signs;
    all_types::Mask mask;
    std::vector<std::uint64_t> masks;
    all_types::Extent extent;
    std::vector<std::int64_t> extents;
    all_types::Word word;
    all_types::Mark mark;
    std::vector<all_types::Mark> marks;
    std::vector<OwnedStep> steps;

    /// The fields of these values with the fields of the steps, which they
    /// view, as `EnumsFields` or as a generic fields struct of the same
    /// members
    template <class Fields = all_types::EnumsFields, class Steps>
    [[nodiscard]] Fields fields(const Steps& steps_view) const {
        return {
            .level = level,
            .levels = levels,
            .direction = direction,
            .directions = directions,
            .unit = unit,
            .units = units,
            .change = change,
            .changes = changes,
            .status = status,
            .statuses = statuses,
            .sign = sign,
            .signs = signs,
            .mask = mask,
            .masks = masks,
            .extent = extent,
            .extents = extents,
            .word = word,
            .mark = mark,
            .marks = marks,
            .steps = steps_view,
        };
    }
};

rc::Gen<OwnedEnums> owned_enums() {
    return rc::gen::exec([] {
        return OwnedEnums{
            .level = *enums<all_types::Level>(),
            .levels = *values::arrays(values::integers<std::uint8_t>(), 8),
            .direction = *enums<all_types::Direction>(),
            .directions = *values::arrays(values::integers<std::int8_t>(), 8),
            .unit = *enums<all_types::Unit>(),
            .units = *values::arrays(values::integers<std::uint16_t>(), 8),
            .change = *enums<all_types::Change>(),
            .changes = *values::arrays(values::integers<std::int16_t>(), 8),
            .status = *enums<all_types::Status>(),
            .statuses = *values::arrays(values::integers<std::uint32_t>(), 8),
            .sign = *enums<all_types::Sign>(),
            .signs = *values::arrays(values::integers<std::int32_t>(), 8),
            .mask = *enums<all_types::Mask>(),
            .masks = *values::arrays(values::integers<std::uint64_t>(), 8),
            .extent = *enums<all_types::Extent>(),
            .extents = *values::arrays(values::integers<std::int64_t>(), 8),
            .word = *enums<all_types::Word>(),
            .mark = *marks(),
            .marks = *values::arrays(marks(), 8),
            .steps = *values::arrays(owned_steps(), 8),
        };
    });
}

RC_GTEST_PROP(GeneratedAllTypes, ScalarsAreWrittenAsTableBuilderWritesThemAndReadBack, ()) {
    const auto fields = *scalars();
    const auto bytes = fields.serialize();

    RC_ASSERT(bytes == to_bytes(describe(fields)));
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));
    RC_ASSERT(read(all_types::Scalars(bytes)) == describe(fields));
}

RC_GTEST_PROP(GeneratedAllTypes, ArraysAreWrittenAsTableBuilderWritesThemAndReadBack, ()) {
    const auto owned = *arrays();
    const auto fields = owned.fields();
    const auto bytes = fields.serialize();

    RC_ASSERT(bytes == to_bytes(describe(fields)));
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));
    RC_ASSERT(read(all_types::Arrays(bytes)) == describe(fields));
}

RC_GTEST_PROP(GeneratedAllTypes, NestedTablesAndTreesAreWrittenAsTableBuilderWritesThemAndReadBack, ()) {
    const auto owned = *owned_nested();
    std::list<std::vector<all_types::TreeFields>> storage;
    const auto trees = tree_fields(owned.trees, storage);
    const all_types::NestedFields fields{
        .label = owned.label,
        .outer = {.count = owned.count, .inner = {.name = owned.name, .values = owned.values}},
        .trees = trees,
    };
    const auto bytes = fields.serialize();

    RC_ASSERT(bytes == to_bytes(describe(fields)));
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));
    for (const auto way : {Items::collected, Items::by_index, Items::iterated}) {
        RC_ASSERT(read(all_types::Nested(bytes), way) == describe(fields));
    }
}

RC_GTEST_PROP(GeneratedAllTypes, FieldsNamedAfterKeywordsAreWrittenAndReadBack, ()) {
    const auto type = *values::strings();
    const all_types::ReservedFields fields{.keywords = {
                                               .class_ = *values::integers<std::uint8_t>(),
                                               .default_ = *values::integers<std::uint8_t>(),
                                               .in = *values::integers<std::uint8_t>(),
                                               .match = *values::integers<std::uint8_t>(),
                                               .self = *values::integers<std::uint8_t>(),
                                               .type = type,
                                           }};
    const auto bytes = fields.serialize();

    RC_ASSERT(bytes == to_bytes(describe(fields)));
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));
    RC_ASSERT(read(all_types::Reserved(bytes)) == describe(fields));
}

RC_GTEST_PROP(GeneratedAllTypes, StructsAreWrittenAsTableBuilderWritesThemAndReadBack, ()) {
    const auto owned = *owned_structs();
    const auto fields = owned.fields();
    const auto bytes = fields.serialize();

    RC_ASSERT(bytes == to_bytes(describe(fields)));
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));
    const all_types::Structs structs(bytes);
    for (const auto way : {Items::collected, Items::by_index, Items::iterated}) {
        RC_ASSERT(read(structs, way) == describe(fields));
    }
}

/// A documented struct of the caller's own, which converts into the generated
/// struct
struct OwnDocumented {
    std::uint8_t one;
    std::uint16_t two;
    std::uint32_t undocumented;

    operator all_types::Documented() const {
        return {.one = one, .two = two, .undocumented = undocumented};
    }
};

/// The generic fields struct writes views of the caller's own items, which it
/// converts into the structs as it writes them, as `StructsFields` writes
/// spans of the structs: through the items' own conversion, through
/// `std::views::transform()`, and with a view's default, a span of the
/// structs, for an array that it isn't given
RC_GTEST_PROP(GeneratedAllTypes, ArraysOfStructsAreWrittenFromViewsOfOtherItemsAsFromTheStructs, ()) {
    const auto owned = *owned_structs();
    std::vector<OwnDocumented> documented;
    for (const auto& item : owned.documented) {
        documented.push_back({.one = item.one, .two = item.two, .undocumented = item.undocumented});
    }
    std::vector<std::pair<all_types::Point, all_types::Point>> segments;
    for (const auto& segment : owned.segments) {
        segments.emplace_back(segment.start, segment.end);
    }
    const auto segment_of = [](const auto& pair) {
        return all_types::Segment{.start = pair.first, .end = pair.second};
    };

    const all_types::generic::StructsFields fields{
        .packed = owned.packed,
        .documented = std::span<const OwnDocumented>(documented),
        .segment = owned.segment,
        .segments = segments | std::views::transform(segment_of),
        .path = {.start = owned.start, .points = owned.points},
    };
    const auto bytes = fields.serialize();
    RC_ASSERT(bytes == owned.fields().serialize());
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));

    const all_types::generic::StructsFields defaulted{.segments = segments | std::views::transform(segment_of),
                                                      .path = {}};
    const auto from_spans = all_types::StructsFields{.segments = owned.segments, .path = {}}.serialize();
    RC_ASSERT(defaulted.serialize() == from_spans);
}

/// A struct's bytes are its scalars little-endian, one after another, a bool
/// as 0 or 1 and a struct within it in its place, which it reads back, and an
/// array of them is their bytes one after another
RC_GTEST_PROP(GeneratedAllTypes, StructsAreTheirScalarsLittleEndianOneAfterAnother, ()) {
    const auto packed = *packeds();
    const auto segment = *segments();

    std::vector<std::uint8_t> bytes(all_types::Packed::SIZE);
    packed.to_bytes(bytes.data());
    RC_ASSERT(bytes == struct_bytes(std::get<Struct>(structure(packed).value).scalars));
    RC_ASSERT(all_types::Packed::read(bytes) == packed || std::isnan(packed.float32) || std::isnan(packed.float64));
    RC_ASSERT(structure(all_types::Packed::read(bytes)) == structure(packed));

    const std::vector<all_types::Segment> segments{segment, segment};
    const auto array = all_types::Segment::array_to_bytes(segments);
    auto expected = struct_bytes(std::get<Struct>(structure(segment).value).scalars);
    expected.insert(expected.end(), expected.begin(), expected.end());
    RC_ASSERT(array == expected);
    RC_ASSERT(all_types::Segment::SIZE == 16U);
    RC_ASSERT(all_types::Packed::SIZE == 43U);
    RC_ASSERT(all_types::Documented::ALIGNMENT == Alignment::four);
}

/// Each enum's values are constants of its integer type, the integers that
/// the schema gives them, at the limits of each type too, and those named
/// after keywords take a `_`
TEST(GeneratedAllTypes, EnumValuesAreTheIntegersThatTheSchemaNames) {
    static_assert(std::is_same_v<std::underlying_type_t<all_types::Level>, std::uint8_t>);
    static_assert(integer(all_types::Level::low) == 0);
    static_assert(integer(all_types::Level::medium) == 1);
    static_assert(integer(all_types::Level::high) == 2);

    static_assert(std::is_same_v<std::underlying_type_t<all_types::Direction>, std::int8_t>);
    static_assert(integer(all_types::Direction::backward) == -1);
    static_assert(integer(all_types::Direction::none) == 0);
    static_assert(integer(all_types::Direction::forward) == 1);
    static_assert(integer(all_types::Direction::furthest_back) == std::numeric_limits<std::int8_t>::min());

    static_assert(std::is_same_v<std::underlying_type_t<all_types::Unit>, std::uint16_t>);
    static_assert(integer(all_types::Unit::last) == std::numeric_limits<std::uint16_t>::max());
    static_assert(integer(all_types::Unit::first) == 0);
    static_assert(integer(all_types::Unit::v_2) == 2);

    static_assert(std::is_same_v<std::underlying_type_t<all_types::Change>, std::int16_t>);
    static_assert(integer(all_types::Change::least) == std::numeric_limits<std::int16_t>::min());
    static_assert(integer(all_types::Change::greatest) == std::numeric_limits<std::int16_t>::max());

    static_assert(std::is_same_v<std::underlying_type_t<all_types::Status>, std::uint32_t>);
    static_assert(integer(all_types::Status::ok) == 0U);
    static_assert(integer(all_types::Status::not_found) == 404U);
    static_assert(integer(all_types::Status::greatest) == std::numeric_limits<std::uint32_t>::max());

    static_assert(std::is_same_v<std::underlying_type_t<all_types::Sign>, std::int32_t>);
    static_assert(integer(all_types::Sign::least) == std::numeric_limits<std::int32_t>::min());
    static_assert(integer(all_types::Sign::greatest) == std::numeric_limits<std::int32_t>::max());

    static_assert(std::is_same_v<std::underlying_type_t<all_types::Mask>, std::uint64_t>);
    static_assert(integer(all_types::Mask::none) == 0U);
    static_assert(integer(all_types::Mask::one) == 1U);
    static_assert(integer(all_types::Mask::beyond_float64) == 9'007'199'254'740'993U);
    static_assert(integer(all_types::Mask::all) == std::numeric_limits<std::uint64_t>::max());

    static_assert(std::is_same_v<std::underlying_type_t<all_types::Extent>, std::int64_t>);
    static_assert(integer(all_types::Extent::least) == std::numeric_limits<std::int64_t>::min());
    static_assert(integer(all_types::Extent::zero) == 0);
    static_assert(integer(all_types::Extent::greatest) == std::numeric_limits<std::int64_t>::max());

    static_assert(std::is_same_v<std::underlying_type_t<all_types::Word>, std::uint8_t>);
    static_assert(integer(all_types::Word::class_) == 0);
    static_assert(integer(all_types::Word::default_) == 1);
    static_assert(integer(all_types::Word::in) == 2);
    static_assert(integer(all_types::Word::match) == 3);
    static_assert(integer(all_types::Word::self) == 4);
    static_assert(integer(all_types::Word::type) == 5);
    static_assert(integer(all_types::Word::true_) == 6);
    static_assert(integer(all_types::Word::null) == 7);
    static_assert(integer(all_types::Word::none) == 8);
    static_assert(integer(all_types::Word::init) == 9);
    static_assert(integer(all_types::Word::new_) == 10);
    static_assert(integer(all_types::Word::name) == 11);
    static_assert(integer(all_types::Word::value) == 12);
}

/// The getters of enums return them, and fields structs and structs hold
/// them, where an array of enums is one of their integers
TEST(GeneratedAllTypes, FieldsOfEnumsAreTheEnumsAndArraysOfThemTheirIntegers) {
    using Enums = all_types::Enums;
    static_assert(std::is_same_v<decltype(std::declval<Enums>().level()), all_types::Level>);
    static_assert(std::is_same_v<decltype(std::declval<Enums>().extent()), all_types::Extent>);
    static_assert(std::is_same_v<decltype(std::declval<Enums>().levels()), std::span<const std::uint8_t>>);
    static_assert(std::is_same_v<decltype(std::declval<Enums>().units()), ArrayView<std::uint16_t>>);
    static_assert(std::is_same_v<decltype(std::declval<all_types::Step>().level()), all_types::Level>);

    static_assert(std::is_same_v<decltype(all_types::EnumsFields::mask), all_types::Mask>);
    static_assert(std::is_same_v<decltype(all_types::EnumsFields::masks), std::span<const std::uint64_t>>);
    static_assert(std::is_same_v<decltype(all_types::StepFields::level), all_types::Level>);
    static_assert(std::is_same_v<decltype(all_types::Mark::change), all_types::Change>);
    static_assert(std::is_same_v<decltype(all_types::Turn::direction), all_types::Direction>);
}

/// Any integer of each enum's type, named or not, is written as
/// `TableBuilder` writes the integer, and reads back as the same value
RC_GTEST_PROP(GeneratedAllTypes, EnumsOfAnyIntegersAreWrittenAsTableBuilderWritesThemAndReadBack, ()) {
    const auto owned = *owned_enums();
    const auto steps = step_fields(owned.steps);
    const auto fields = owned.fields(steps);
    const auto bytes = fields.serialize();

    RC_ASSERT(bytes == to_bytes(describe(fields)));
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));
    const all_types::Enums enums(bytes);
    for (const auto way : {Items::collected, Items::by_index, Items::iterated}) {
        RC_ASSERT(read(enums, way) == describe(fields));
    }
}

/// The generic fields struct writes the enums of the caller's own items, which
/// it converts as it writes them, as `EnumsFields` writes those of their
/// fields
RC_GTEST_PROP(GeneratedAllTypes, EnumsAreWrittenFromViewsOfOtherItemsAsFromTheirFields, ()) {
    const auto owned = *owned_enums();
    const auto steps = step_fields(owned.steps);
    const auto fields_of = [](const OwnedStep& step) {
        return all_types::StepFields{.level = step.level, .mark = step.mark, .name = step.name};
    };
    auto view = owned.steps | std::views::transform(fields_of);
    using GenericFields = all_types::generic::EnumsFields<std::span<const all_types::Mark>, decltype(view)>;

    const auto expected = owned.fields(steps).serialize();
    const auto fields = owned.fields<GenericFields>(view);
    RC_ASSERT(fields.serialize() == expected);
    RC_ASSERT(written_into_any_bytes(fields, expected.span()));
}

/// A struct's enums are their integers little-endian, one after another, in
/// the struct within it too, and it reads them back
RC_GTEST_PROP(GeneratedAllTypes, AStructOfEnumsIsTheirIntegersLittleEndianOneAfterAnother, ()) {
    const auto mark = *marks();

    std::vector<std::uint8_t> bytes(all_types::Mark::SIZE);
    mark.to_bytes(bytes.data());
    RC_ASSERT(bytes == struct_bytes(std::get<Struct>(structure(mark).value).scalars));
    RC_ASSERT(structure(all_types::Mark::read(bytes)) == structure(mark));
    RC_ASSERT(all_types::Mark::SIZE == 24U);
}

/// The values that an enum names are written as the integers that the schema
/// gives them
TEST(GeneratedAllTypes, TheValuesThatAnEnumNamesAreWrittenAsTheirIntegers) {
    const all_types::Mark mark{
        .level = all_types::Level::high,
        .change = all_types::Change::least,
        .weight = 0.5F,
        .extent = all_types::Extent::greatest,
        .turn = {.direction = all_types::Direction::backward, .mask = all_types::Mask::beyond_float64},
    };
    std::vector<std::uint8_t> bytes(all_types::Mark::SIZE);
    mark.to_bytes(bytes.data());

    const std::vector<Field> scalars{
        {std::uint8_t{2}},
        {std::int16_t{-32'768}},
        {0.5F},
        {std::numeric_limits<std::int64_t>::max()},
        {std::int8_t{-1}},
        {std::uint64_t{9'007'199'254'740'993U}},
    };
    EXPECT_EQ(bytes, struct_bytes(scalars));
    EXPECT_EQ(all_types::Mark::read(bytes), mark);

    const auto word = all_types::EnumsFields{.word = all_types::Word::true_}.serialize();
    EXPECT_EQ(integer(all_types::Enums(word).word()), 6);
}

/// Any integer in the bytes of an enum, which the schema may give no name,
/// reads as a value of the enum, as the integer type reads it, in a struct
/// too
RC_GTEST_PROP(GeneratedAllTypes, AnyIntegerInTheBytesOfAnEnumReadsAsItsValue, ()) {
    const auto level = *rc::gen::arbitrary<std::uint8_t>();
    const auto mask = *rc::gen::arbitrary<std::uint64_t>();
    const auto direction = *rc::gen::arbitrary<std::int8_t>();
    auto bytes = all_types::EnumsFields{}.serialize().to_vector();

    // The direction of the mark's turn, at the turn's start, 15 bytes into
    // the mark. `at()` checks that the bytes hold each, so that GCC sees no
    // path that writes through the null data of empty bytes.
    const TableReader reader(bytes, all_types::Enums::LAYOUT);
    store_le(&bytes.at(reader.root_offset(0)), level);
    store_le(&bytes.at(reader.root_offset(12)), mask);
    store_le(&bytes.at(reader.root_offset(17) + 15), direction);

    const all_types::Enums enums(bytes);
    RC_ASSERT(enums.level() == static_cast<all_types::Level>(level));
    RC_ASSERT(enums.mask() == static_cast<all_types::Mask>(mask));
    RC_ASSERT(enums.mark().turn.direction == static_cast<all_types::Direction>(direction));
}

/// Corrupted and arbitrary bytes read as values or decode errors, alike
/// whichever way the items of vectors and arrays of structs are read
RC_GTEST_PROP(GeneratedAllTypes, CorruptedTablesReadAsValuesOrDecodeErrors, ()) {
    const auto nested_values = *owned_nested();
    const auto structs_values = *owned_structs();
    const auto enums_values = *owned_enums();
    const auto corruption = *values::corruptions();

    std::list<std::vector<all_types::TreeFields>> storage;
    const auto trees = tree_fields(nested_values.trees, storage);
    const all_types::NestedFields nested_fields{
        .label = nested_values.label,
        .outer = {.count = nested_values.count, .inner = {.name = nested_values.name, .values = nested_values.values}},
        .trees = trees,
    };

    for (const auto& bytes : values::corrupted(nested_fields.serialize().to_vector(), corruption)) {
        const all_types::Nested nested_table(bytes);
        const auto collected = try_read([&] { return read(nested_table, Items::collected); });
        RC_ASSERT(try_read([&] { return read(nested_table, Items::by_index); }) == collected);
        RC_ASSERT(try_read([&] { return read(nested_table, Items::iterated); }) == collected);
    }

    for (const auto& bytes : values::corrupted(structs_values.fields().serialize().to_vector(), corruption)) {
        const all_types::Structs structs(bytes);
        const auto collected = try_read([&] { return read(structs, Items::collected); });
        RC_ASSERT(try_read([&] { return read(structs, Items::by_index); }) == collected);
        RC_ASSERT(try_read([&] { return read(structs, Items::iterated); }) == collected);

        static_cast<void>(try_read([&] { return read(all_types::Scalars(bytes)); }));
        static_cast<void>(try_read([&] { return read(all_types::Arrays(bytes)); }));
        static_cast<void>(try_read([&] { return read(all_types::Reserved(bytes)); }));
        static_cast<void>(try_read([&] { return read(all_types::Enums(bytes), Items::collected); }));
    }

    const auto steps = step_fields(enums_values.steps);
    for (const auto& bytes : values::corrupted(enums_values.fields(steps).serialize().to_vector(), corruption)) {
        const all_types::Enums enums(bytes);
        const auto collected = try_read([&] { return read(enums, Items::collected); });
        RC_ASSERT(try_read([&] { return read(enums, Items::by_index); }) == collected);
        RC_ASSERT(try_read([&] { return read(enums, Items::iterated); }) == collected);
    }
}

TEST(GeneratedAllTypes, ViewsOfEmptyVectorsAndArraysOfStructsHoldNoItems) {
    const auto nested_bytes = all_types::NestedFields{}.serialize();
    const auto structs_bytes = all_types::StructsFields{}.serialize();

    EXPECT_TRUE(all_types::Nested(nested_bytes).trees().empty());
    EXPECT_TRUE(all_types::Nested(nested_bytes).trees().to_vector().empty());
    EXPECT_TRUE(all_types::Structs(structs_bytes).segments().empty());
    EXPECT_TRUE(all_types::Structs(structs_bytes).documented().to_vector().empty());
    EXPECT_THROW(static_cast<void>(all_types::Structs(structs_bytes).segments().at(0)), std::out_of_range);
}

/// Each nested table is aligned to its arrays' and nested tables' largest
/// alignment, which its fields struct names
TEST(GeneratedAllTypes, TablesAreAlignedToTheirArraysAndNestedTables) {
    EXPECT_EQ(all_types::InnerFields::ALIGNMENT, Alignment::eight);
    EXPECT_EQ(all_types::OuterFields::ALIGNMENT, Alignment::eight);
    EXPECT_EQ(all_types::TreeFields::ALIGNMENT, Alignment::one);
    EXPECT_EQ(all_types::KeywordsFields::ALIGNMENT, Alignment::one);
    EXPECT_EQ(all_types::PathFields::ALIGNMENT, Alignment::four);

    const std::vector<double> values{1.5};
    const auto bytes = all_types::NestedFields{.outer = {.inner = {.name = "a", .values = values}}}.serialize();
    const all_types::Nested nested_table(bytes);
    EXPECT_EQ(nested_table.outer().inner().values().to_vector(), values);
}

}  // namespace
}  // namespace tafla::test
