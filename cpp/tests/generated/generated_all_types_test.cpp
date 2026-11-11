// The code generated from `schemas/all_types.tafla` writes every type of field
// as `TableBuilder` writes it by hand: each scalar and bool, each array,
// strings, tables nested in tables, vectors of tables that hold vectors of
// themselves, fields named after keywords, and structs alone, within structs,
// in tables and in arrays. It reads back what it writes, through its views as
// through its getters, and reads corrupted bytes as values or decode errors.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <cmath>
#include <list>
#include <optional>
#include <string>
#include <vector>

#include "all_types/all_types.hpp"
#include "common/build.hpp"
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

// Structs, whose scalars the descriptions hold one after another, with a bool
// as its uint8 and a struct within a struct in its place

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

// The descriptions of what each generated reader reads, through its getters
// or its views

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

std::vector<Field> read_tree(const all_types::Tree& tree, bool views) {
    VectorField children;
    if (views) {
        const auto view = tree.children_view();
        for (std::size_t index = 0; index < view.size(); ++index) {
            children.items.push_back(item(read_tree(view.get(index), views)));
        }
    } else {
        for (const auto& child : tree.children()) {
            children.items.push_back(item(read_tree(child, views)));
        }
    }
    return {{tree.value()}, {children}};
}

Table read(const all_types::Nested& nested_table, bool views) {
    const auto outer = nested_table.outer();
    const auto inner = outer.inner();
    VectorField trees;
    if (views) {
        for (const auto tree : nested_table.trees_view()) {
            trees.items.push_back(item(read_tree(tree, views)));
        }
    } else {
        for (const auto& tree : nested_table.trees()) {
            trees.items.push_back(item(read_tree(tree, views)));
        }
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

template <class View>
auto from_view(const View& view, bool by_index) {
    std::vector<typename View::value_type> items;
    if (by_index) {
        for (std::size_t index = 0; index < view.size(); ++index) {
            items.push_back(view.at(index));
        }
    } else {
        items.assign(view.begin(), view.end());
    }
    return items;
}

Table read(const all_types::Structs& structs, std::optional<bool> views) {
    const auto path = structs.path();
    const auto documented = views ? from_view(structs.documented_view(), *views) : structs.documented();
    const auto segments = views ? from_view(structs.segments_view(), *views) : structs.segments();
    const auto points = views ? from_view(path.points_view(), *views) : path.points();
    return {Layout{},
            std::nullopt,
            {structure(structs.packed()), struct_array(std::span<const all_types::Documented>(documented), kDocumented),
             structure(structs.segment()), struct_array(std::span<const all_types::Segment>(segments), kSegment),
             nested({structure(path.start()), struct_array(std::span<const all_types::Point>(points), kPoint)})}};
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

RC_GTEST_PROP(GeneratedAllTypes, ScalarsAreWrittenAsTableBuilderWritesThemAndReadBack, ()) {
    const auto fields = *scalars();
    const auto bytes = fields.serialize();

    RC_ASSERT(bytes == to_bytes(describe(fields)));
    RC_ASSERT(read(all_types::Scalars(bytes)) == describe(fields));
}

RC_GTEST_PROP(GeneratedAllTypes, ArraysAreWrittenAsTableBuilderWritesThemAndReadBack, ()) {
    const auto owned = *arrays();
    const auto fields = owned.fields();
    const auto bytes = fields.serialize();

    RC_ASSERT(bytes == to_bytes(describe(fields)));
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
    RC_ASSERT(read(all_types::Nested(bytes), false) == describe(fields));
    RC_ASSERT(read(all_types::Nested(bytes), true) == describe(fields));
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
    RC_ASSERT(read(all_types::Reserved(bytes)) == describe(fields));
}

RC_GTEST_PROP(GeneratedAllTypes, StructsAreWrittenAsTableBuilderWritesThemAndReadBack, ()) {
    const auto owned = *owned_structs();
    const auto fields = owned.fields();
    const auto bytes = fields.serialize();

    RC_ASSERT(bytes == to_bytes(describe(fields)));
    const all_types::Structs structs(bytes);
    RC_ASSERT(read(structs, std::nullopt) == describe(fields));
    RC_ASSERT(read(structs, true) == describe(fields));
    RC_ASSERT(read(structs, false) == describe(fields));
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

/// Corrupted and arbitrary bytes read as values or decode errors, and views
/// read what getters read
RC_GTEST_PROP(GeneratedAllTypes, CorruptedTablesReadAsValuesOrDecodeErrors, ()) {
    const auto nested_values = *owned_nested();
    const auto structs_values = *owned_structs();
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
        const auto from_getters = try_read([&] { return read(nested_table, false); });
        RC_ASSERT(try_read([&] { return read(nested_table, true); }) == from_getters);
    }

    for (const auto& bytes : values::corrupted(structs_values.fields().serialize().to_vector(), corruption)) {
        const all_types::Structs structs(bytes);
        const auto from_getters = try_read([&] { return read(structs, std::nullopt); });
        RC_ASSERT(try_read([&] { return read(structs, true); }) == from_getters);
        RC_ASSERT(try_read([&] { return read(structs, false); }) == from_getters);

        static_cast<void>(try_read([&] { return read(all_types::Scalars(bytes)); }));
        static_cast<void>(try_read([&] { return read(all_types::Arrays(bytes)); }));
        static_cast<void>(try_read([&] { return read(all_types::Reserved(bytes)); }));
    }
}

TEST(GeneratedAllTypes, ViewsOfEmptyVectorsAndArraysOfStructsHoldNoItems) {
    const auto nested_bytes = all_types::NestedFields{}.serialize();
    const auto structs_bytes = all_types::StructsFields{}.serialize();

    EXPECT_TRUE(all_types::Nested(nested_bytes).trees_view().empty());
    EXPECT_TRUE(all_types::Nested(nested_bytes).trees().empty());
    EXPECT_TRUE(all_types::Structs(structs_bytes).segments_view().empty());
    EXPECT_TRUE(all_types::Structs(structs_bytes).documented().empty());
    EXPECT_THROW(static_cast<void>(all_types::Structs(structs_bytes).segments_view().at(0)), std::out_of_range);
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
