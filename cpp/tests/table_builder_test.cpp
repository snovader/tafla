// Builders write any table that fits in its offset type, and reject what they
// can't write with a `BuildError` from the setter, leaving the builder as it
// was. The examples document the errors.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

#include "common/build.hpp"
#include "common/read.hpp"
#include "common/strategy.hpp"

namespace tafla::test {
namespace {

/// Throws the build error of `build`
template <class Build>
BuildError build_error_of(Build build) {
    try {
        build();
    } catch (const BuildError& error) {
        return error;
    }
    throw std::logic_error("The build didn't fail");
}

bool same_error(const BuildError& error, BuildError::Kind kind, OffsetType offset_type, std::uint64_t first,
                std::uint64_t second) {
    const auto& details = error.details();
    return details.kind == kind && details.offset_type == offset_type && details.first == first &&
           details.second == second;
}

/// Tables of `uint8` and `uint16` offsets, whose limits a test can reach
rc::Gen<Table> tables_of_short_offsets() {
    return strategy::tables(strategy::layouts(rc::gen::element(OffsetType::uint8, OffsetType::uint16)));
}

Table with_field(const Table& table, Field field) {
    auto result = table;
    result.fields.push_back(std::move(field));
    return result;
}

/// A table of `fields` with no header and `uint32` offsets, which fits in any
/// parent
Nested nested(Source from, std::vector<Field> fields) {
    return {from, {Layout::headerless(OffsetType::uint32), std::nullopt, std::move(fields)}};
}

/// A field whose byte length grows by a unit with each of its units
enum class LongField : std::uint8_t {
    bytes,
    floats,
    string,
    /// An array of structs of a uint16 and a uint8, of 3 bytes aligned to 2
    structs,
    /// A vector of empty tables, each an end of its own
    empty_items,
    /// A vector of a table of bytes, from its builder
    item,
    /// A table of bytes, nested from its bytes
    nested_bytes,
    /// A table of bytes, nested from its builder
    nested_builder,
};

rc::Gen<LongField> long_field() {
    return rc::gen::element(LongField::bytes, LongField::floats, LongField::string, LongField::structs,
                            LongField::empty_items, LongField::item, LongField::nested_bytes,
                            LongField::nested_builder);
}

Field long_field_of(LongField kind, std::size_t units) {
    const auto bytes = [units] { return std::vector<Field>{{std::vector<std::uint8_t>(units, 0)}}; };

    switch (kind) {
        case LongField::bytes:
            return {std::vector<std::uint8_t>(units, 0)};
        case LongField::floats:
            return {std::vector<double>(units, 0.0)};
        case LongField::string:
            return {std::string(units, 'a')};
        case LongField::structs:
            return {StructArray{{ScalarType::uint16, ScalarType::uint8},
                                std::vector<std::vector<Field>>(units, {{std::uint16_t{1}}, {std::uint8_t{2}}})}};
        case LongField::empty_items:
            return {VectorField{std::vector<Nested>(units, nested(Source::bytes, {}))}};
        case LongField::item:
            return {VectorField{{nested(Source::builder, bytes())}}};
        case LongField::nested_bytes:
            return {TableField{nested(Source::bytes, bytes()), std::nullopt}};
        case LongField::nested_builder:
            break;
    }
    return {TableField{nested(Source::builder, bytes()), std::nullopt}};
}

/// The bytes that each unit adds in a table of `width`-byte offsets
std::size_t unit_of(LongField kind, std::size_t width) {
    switch (kind) {
        case LongField::floats:
            return 8;
        case LongField::structs:
            return 3;
        case LongField::empty_items:
            return width;
        default:
            return 1;
    }
}

/// After any fields, a field as long as fits is written and reads back, and a
/// unit longer is rejected with the length that it would make the table
RC_GTEST_PROP(TableBuilder, TheLongestFieldThatFitsIsWrittenAndAUnitMoreIsRejected, ()) {
    const auto table = *tables_of_short_offsets();
    const auto kind = *long_field();

    const auto offset_type = table.layout.offset_type;
    const std::size_t max = max_value(offset_type);
    const auto unit = unit_of(kind, offset_size(offset_type));

    // The fields can leave no room for even the empty field
    const auto empty = with_field(table, long_field_of(kind, 0));
    std::optional<Bytes> empty_bytes;
    try {
        empty_bytes = to_bytes(empty);
    } catch (const BuildError&) {
        RC_DISCARD("No room for the empty field");
    }

    const auto units = (max - empty_bytes->size()) / unit;
    const auto longest = with_field(table, long_field_of(kind, units));
    const auto bytes = to_bytes(longest);
    RC_ASSERT(max - unit < bytes.size());
    RC_ASSERT(bytes.size() <= max);
    RC_ASSERT(read_table(bytes, longest) == longest);

    const auto too_long = long_field_of(kind, units + 1);
    auto builder = builder_of(empty);
    for (const auto& field : table.fields) {
        set_field(field, builder);
    }

    const auto length = empty_bytes->size() + (units + 1) * unit;
    const auto error = build_error_of([&] { set_field(too_long, builder); });
    RC_ASSERT(same_error(error, BuildError::Kind::too_long, offset_type, table.fields.size(), length));

    set_field(empty.fields.back(), builder);
    RC_ASSERT(builder.to_bytes() == *empty_bytes);
}

/// A field longer than the offsets address on its own, set after any of the
/// fields, is rejected, and the builder writes the same table once the other
/// fields are set
RC_GTEST_PROP(TableBuilder, AFieldTooLongForTheOffsetTypeIsRejectedAndLeavesTheBuilderAsItWas, ()) {
    const auto table = *tables_of_short_offsets();
    const auto kind = *long_field();
    RC_PRE(!table.fields.empty());
    const auto at = *rc::gen::inRange<std::size_t>(0, table.fields.size());

    const auto offset_type = table.layout.offset_type;
    const std::size_t max = max_value(offset_type);
    const auto too_long = long_field_of(kind, max / unit_of(kind, offset_size(offset_type)) + 1);

    auto builder = builder_of(table);
    for (std::size_t index = 0; index < at; ++index) {
        set_field(table.fields[index], builder);
    }

    const auto error = build_error_of([&] { set_field(too_long, builder); });
    RC_ASSERT(error.kind() == BuildError::Kind::too_long);
    RC_ASSERT(error.index() == at);
    RC_ASSERT(error.length() > max);
    RC_ASSERT(error.offset_type() == offset_type);

    for (std::size_t index = at; index < table.fields.size(); ++index) {
        set_field(table.fields[index], builder);
    }
    RC_ASSERT(builder.to_bytes() == to_bytes(table));
}

/// The most bytes that `parent_of()` writes for a table of `length` bytes,
/// with `offset_type` offsets
std::size_t longest_parent(OffsetType offset_type, std::size_t before, std::size_t length) {
    const auto width = offset_size(offset_type);
    return 3 * width + (width + before) + (8 - 1 + length) + (3 * width + 2 * length);
}

/// A parent of `before` bytes in a `uint8[]`, the table at alignment 8, and a
/// vector of the table twice
template <class Nest>
Bytes parent_of(Nest nest, std::size_t before, TableLayout<false> layout) {
    TableBuilder parent(3, layout);
    parent.set_uint8_array(std::vector<std::uint8_t>(before, 0));
    parent.set_table(nest(), Alignment::eight);

    std::vector<NestedTable> items;
    items.push_back(nest());
    items.push_back(nest());
    parent.set_vector(std::move(items));

    return parent.to_bytes();
}

/// A table nested from its builder, borrowed or owned, writes the same parent
/// as from its bytes: the builder is written in place when it has the parent's
/// offset type and no header, and copied otherwise. The table starts at the
/// alignment it's set with and holds its bytes, and the vector's items read
/// back as its bytes.
RC_GTEST_PROP(TableBuilder, ATableNestedFromItsBuilderWritesTheSameParentAsFromItsBytes, ()) {
    const auto table = *strategy::table();
    const auto before = *rc::gen::inRange<std::size_t>(0, 8);
    const auto bytes = to_bytes(table);

    // Of the table's offset type when the parent fits in it, so that a builder
    // without a header is written in place whatever its offset type, and
    // otherwise of uint32 offsets
    auto offset_type = table.layout.offset_type;
    if (longest_parent(offset_type, before, bytes.size()) > max_value(offset_type)) {
        offset_type = OffsetType::uint32;
    }
    const auto layout = TableLayout().with_offset_type(offset_type);

    const auto borrowed = build(table);
    const auto from_borrowed = parent_of([&] { return NestedTable(borrowed); }, before, layout);
    const auto from_owned = parent_of([&] { return NestedTable(build(table)); }, before, layout);
    const auto from_bytes = parent_of([&] { return NestedTable(bytes.span()); }, before, layout);
    RC_ASSERT(from_borrowed == from_bytes);
    RC_ASSERT(from_owned == from_bytes);

    const ByteBuffer buffer(from_bytes, layout);
    const auto position = buffer.root_offset(1);
    RC_ASSERT(position % 8 == 0U);
    RC_ASSERT(std::ranges::equal(from_bytes.span().subspan(position, bytes.size()), bytes.span()));

    for (auto item : buffer.get_vector(buffer.root_offset(2)).items()) {
        RC_ASSERT(std::ranges::equal(item, bytes.span()));
    }
}

TEST(TableBuilder, RejectsAFieldBeyondItsCount) {
    TableBuilder builder(1, TableLayout());
    builder.set_uint8(1);
    const auto bytes = builder.to_bytes();

    const auto error = build_error_of([&] { builder.set_uint8(2); });
    EXPECT_TRUE(same_error(error, BuildError::Kind::too_many_fields, OffsetType::uint32, 1, 0));
    EXPECT_EQ(error.fields(), 1U);
    EXPECT_STREQ(error.what(), "Can't add field 2 to a table of 1");
    EXPECT_EQ(builder.to_bytes(), bytes);
}

TEST(TableBuilder, RejectsWritingBeforeEveryFieldIsSet) {
    for (std::size_t set = 0; set < 5; ++set) {
        TableBuilder builder(set + 1, TableLayout());
        for (std::size_t field = 0; field < set; ++field) {
            builder.set_uint8(1);
        }

        const auto error = build_error_of([&] { static_cast<void>(builder.to_bytes()); });
        EXPECT_EQ(error.kind(), BuildError::Kind::incomplete);
        EXPECT_EQ(error.set(), set);
        EXPECT_EQ(error.fields(), set + 1);
        EXPECT_EQ(std::string(error.what()),
                  "Only " + std::to_string(set) + " of the table's " + std::to_string(set + 1) + " fields were set");
    }
}

TEST(TableBuilder, RejectsAFieldThatMakesTheTableTooLong) {
    for (const auto offset_type : {OffsetType::uint8, OffsetType::uint16}) {
        const auto layout = TableLayout().with_offset_type(offset_type);
        const std::size_t max = max_value(offset_type);
        const auto width = offset_size(offset_type);

        // Two offsets, the array's length prefix and bytes, and then one byte
        const std::vector<std::uint8_t> fitting(max - 3 * width - 1, 0);
        TableBuilder fits(2, layout);
        fits.set_uint8_array(fitting);
        fits.set_uint8(7);

        const auto table = fits.to_bytes();
        const ByteBuffer buffer(table, layout);
        EXPECT_EQ(table.size(), max);
        EXPECT_EQ(buffer.get_uint8(buffer.root_offset(1)), 7);

        const std::vector<std::uint8_t> filling(max - 3 * width, 0);
        TableBuilder too_long(2, layout);
        too_long.set_uint8_array(filling);

        const auto error = build_error_of([&] { too_long.set_uint8(7); });
        EXPECT_TRUE(same_error(error, BuildError::Kind::too_long, offset_type, 1, max + 1));
        EXPECT_EQ(std::string(error.what()), "Field 2 makes the table " + std::to_string(max + 1) +
                                                 " bytes, too long for " + std::string(name(offset_type)) + " offsets");
    }
}

TEST(TableBuilder, RejectsBytesThatArentWholeStructs) {
    const std::vector<std::tuple<std::size_t, std::size_t, const char*>> cases{
        {5, 2, "5 bytes aren't a whole number of 2-byte structs"},
        {4, 0, "A struct can't be 0 bytes"},
        {0, 0, "A struct can't be 0 bytes"},
    };

    for (const auto& [length, size, message] : cases) {
        TableBuilder builder(1, TableLayout());
        const auto error = build_error_of(
            [&] { builder.set_struct_array(std::vector<std::uint8_t>(length, 0), size, Alignment::one); });
        EXPECT_TRUE(same_error(error, BuildError::Kind::not_whole_structs, OffsetType::uint32, length, size));
        EXPECT_STREQ(error.what(), message);

        // The builder was left as it was
        const std::vector<std::uint8_t> structs{1, 2, 3, 4};
        builder.set_struct_array(structs, 2, Alignment::two);
        EXPECT_EQ(builder.to_bytes().to_vector(), (std::vector<std::uint8_t>{4, 0, 0, 0, 2, 0, 0, 0, 1, 2, 3, 4}));
    }
}

TEST(TableBuilder, RejectsAHeaderTooLong) {
    for (const auto offset_type : {OffsetType::uint8, OffsetType::uint16}) {
        const std::size_t max = max_value(offset_type);
        const auto width = offset_size(offset_type);

        for (const bool sized : {false, true}) {
            auto layout = TableLayout().with_offset_type(offset_type);
            if (sized) {
                layout = layout.sized();
            }
            const auto fields = max / width - (sized ? 1 : 0);

            EXPECT_NO_THROW(TableBuilder(fields, layout));
            const auto error = build_error_of([&] { TableBuilder(fields + 1, layout); });
            EXPECT_TRUE(same_error(error, BuildError::Kind::header_too_long, offset_type, fields + 1, 0));
        }
    }

    EXPECT_EQ(build_error_of([] { TableBuilder(SIZE_MAX, TableLayout()); }).kind(), BuildError::Kind::header_too_long);
}

// A table of 4 GiB can't be allocated in a test, but a vector that nests a
// builder of 64 MiB 64 times is that long without allocating more
TEST(TableBuilder, RejectsATableOf32BitOffsetsPast4GiBBeforeWritingIt) {
    const std::vector<std::uint8_t> bytes(std::size_t{1} << 26, 0);
    TableBuilder nested(1, TableLayout());
    nested.set_uint8_array(bytes);

    const auto items = [&nested](std::size_t count) {
        std::vector<NestedTable> builders;
        for (std::size_t item = 0; item < count; ++item) {
            builders.emplace_back(nested);
        }
        return builders;
    };

    TableBuilder builder(1, TableLayout());
    const auto error = build_error_of([&] { builder.set_vector(items(64)); });
    EXPECT_EQ(error.kind(), BuildError::Kind::too_long);
    EXPECT_EQ(error.index(), 0U);
    EXPECT_GT(error.length(), std::uint64_t{1} << 32);

    builder.set_vector(items(63));
}

TEST(TableBuilder, RejectsAVersionTooLargeForItsSlot) {
    for (const auto& [offset_type, version] :
         {std::pair{OffsetType::uint8, 0x100U}, std::pair{OffsetType::uint16, 0x1'0000U}}) {
        const auto layout = TableLayout().with_offset_type(offset_type).versioned();

        const auto error = build_error_of([&] { TableBuilder(1, layout, version); });
        EXPECT_TRUE(same_error(error, BuildError::Kind::version_too_large, offset_type, version, 0));
        EXPECT_EQ(error.version(), version);
        EXPECT_EQ(std::string(error.what()), "Version " + std::to_string(version) + " doesn't fit in a " +
                                                 std::string(name(offset_type)) + " slot");
        EXPECT_NO_THROW(TableBuilder(1, layout, version - 1));
    }
}

TEST(TableBuilder, RejectsAnIncompleteNestedBuilder) {
    TableBuilder incomplete(2, TableLayout());
    incomplete.set_uint8(1);

    TableBuilder parent(2, TableLayout());
    const auto expect_incomplete = [](const BuildError& error) {
        EXPECT_EQ(error.kind(), BuildError::Kind::incomplete);
        EXPECT_EQ(error.set(), 1U);
        EXPECT_EQ(error.fields(), 2U);
    };
    expect_incomplete(build_error_of([&] { parent.set_table(incomplete); }));

    const std::vector<std::uint8_t> bytes{0};
    std::vector<NestedTable> items;
    items.emplace_back(bytes);
    items.emplace_back(incomplete);
    expect_incomplete(build_error_of([&] { parent.set_vector(std::move(items)); }));

    // The parent was left as it was
    TableBuilder complete(2, TableLayout());
    complete.set_uint8(1);
    complete.set_uint8(2);
    parent.set_table(complete);
    std::vector<NestedTable> complete_items;
    complete_items.emplace_back(complete);
    parent.set_vector(std::move(complete_items));

    const auto table = parent.to_bytes();
    const ByteBuffer buffer(table, TableLayout());
    const auto position = buffer.root_offset(0);
    EXPECT_EQ(position, 8U);
    EXPECT_EQ(buffer.get_uint8(buffer.field_offset(position, 1)), 2);

    const auto vector = buffer.get_vector(buffer.root_offset(1));
    ASSERT_EQ(vector.size(), 1U);
    EXPECT_TRUE(std::ranges::equal(vector.get(0), complete.to_bytes().span()));
}

TEST(TableBuilder, OwnsStringsAndArraysGivenAsRvalues) {
    TableBuilder builder(3, TableLayout());
    {
        // Short strings lie within the string itself, which the builder
        // moves to where it keeps it
        std::string string = "short";
        builder.set_string(std::move(string));
        builder.set_uint16_array(std::vector<std::uint16_t>{1, 2, 3});
        builder.set_struct(std::vector<std::uint8_t>{9, 8, 7});
    }

    const auto bytes = builder.to_bytes();
    const ByteBuffer buffer(bytes, TableLayout());
    EXPECT_EQ(buffer.get_string(buffer.root_offset(0)), "short");
    EXPECT_EQ(buffer.get_uint16_array(buffer.root_offset(1)).to_vector(), (std::vector<std::uint16_t>{1, 2, 3}));
    EXPECT_TRUE(std::ranges::equal(buffer.get_struct(buffer.root_offset(2), 3), std::vector<std::uint8_t>{9, 8, 7}));
}

TEST(TableBuilder, LeavesAnRvalueItRejectsWithTheCaller) {
    TableBuilder builder(1, TableLayout().with_offset_type(OffsetType::uint8));
    std::vector<std::uint8_t> values(300, 1);

    EXPECT_THROW(builder.set_uint8_array(std::move(values)), BuildError);
    EXPECT_EQ(values.size(), 300U);  // NOLINT(bugprone-use-after-move): rejected, so not moved
}

}  // namespace
}  // namespace tafla::test
