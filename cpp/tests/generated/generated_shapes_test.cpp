// The code generated from `schemas/shapes.tafla` writes the tables that
// `TableBuilder` writes by hand, into new bytes and into the start of bytes
// that the caller gives, reads back what it writes, and reads
// corrupted and arbitrary bytes as values or decode errors, alike through its
// views and its getters, and without reading outside them, which the
// sanitizers check

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

#include "common/at_byte_offset.hpp"
#include "common/build.hpp"
#include "common/given_bytes.hpp"
#include "common/values.hpp"
#include "shapes_describe.hpp"

namespace tafla::test {
namespace {

namespace tables = shapes_tables;

/// The values of a `Document`'s fields, which `DocumentFields` views
struct OwnedDocument {
    double x;
    double y;
    std::string name;
    std::int64_t base;
    std::vector<std::uint32_t> keys;
    std::vector<float> a;
    std::vector<float> b;

    [[nodiscard]] shapes::DocumentFields fields() const {
        return {.x = x, .y = y, .name = name, .columns = {.base = base, .keys = keys, .a = a, .b = b}};
    }
};

rc::Gen<OwnedDocument> documents() {
    return rc::gen::construct<OwnedDocument>(strategy::float64(), strategy::float64(), values::strings(),
                                             values::integers<std::int64_t>(),
                                             values::arrays(values::integers<std::uint32_t>()),
                                             values::arrays(strategy::float32()), values::arrays(strategy::float32()));
}

rc::Gen<std::vector<shapes::ItemFields>> items() {
    const auto pairs = rc::gen::map(rc::gen::pair(strategy::float32(), strategy::float32()), [](auto pair) {
        return shapes::PairFields{.first = pair.first, .second = pair.second};
    });
    const auto item = rc::gen::map(rc::gen::pair(strategy::float32(), pairs), [](auto pair) {
        return shapes::ItemFields{.value = pair.first, .pair = pair.second};
    });
    return values::arrays(item, 8);
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

RC_GTEST_PROP(GeneratedShapes, APointIsWrittenAsTableBuilderWritesItAndReadsBack, ()) {
    const auto x = *strategy::float64();
    const auto y = *strategy::float64();
    const auto id = *values::integers<std::int64_t>();
    const auto name = *values::strings();
    const auto byte_offset = *rc::gen::inRange<std::size_t>(0, 8);
    const shapes::PointFields fields{.x = x, .y = y, .id = id, .name = name};

    const auto bytes = fields.serialize();
    RC_ASSERT(bytes == to_bytes(tables::describe(fields)));
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));

    const AtByteOffset copy(bytes, byte_offset);
    RC_ASSERT(tables::read(shapes::Point(copy.bytes())) == tables::describe(fields));
}

RC_GTEST_PROP(GeneratedShapes, ADocumentIsWrittenAsTableBuilderWritesItAndReadsBack, ()) {
    const auto document = *documents();
    const auto byte_offset = *rc::gen::inRange<std::size_t>(0, 8);
    const auto fields = document.fields();

    const auto bytes = fields.serialize();
    RC_ASSERT(bytes == to_bytes(tables::describe(fields)));
    RC_ASSERT(shapes::Document::version_of(bytes) == shapes::Document::FORMAT_VERSION);
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));

    // At any address, so that its arrays are read in place or copied
    const AtByteOffset copy(bytes, byte_offset);
    RC_ASSERT(tables::read(shapes::Document(copy.bytes())) == tables::describe(fields));
}

RC_GTEST_PROP(GeneratedShapes, AnItemListIsWrittenAsTableBuilderWritesItAndReadsBackThroughItsView, ()) {
    const auto list = *items();
    const shapes::ItemListFields fields{.items = list};

    const auto bytes = fields.serialize();
    RC_ASSERT(bytes == to_bytes(tables::describe(fields)));
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));

    const shapes::ItemList read(bytes);
    RC_ASSERT(tables::read(read) == tables::describe(fields));
    RC_ASSERT(tables::read_view(read, true) == tables::describe(fields));
    RC_ASSERT(tables::read_view(read, false) == tables::describe(fields));
}

RC_GTEST_PROP(GeneratedShapes, ASampleIsWrittenLittleEndianAtItsOffsetsAndReadsBack, ()) {
    const shapes::Sample sample{
        .key = *values::integers<std::uint32_t>(),
        .a = *strategy::float32(),
        .b = *strategy::float32(),
    };

    std::vector<std::uint8_t> bytes(shapes::Sample::SIZE);
    sample.to_bytes(bytes.data());
    std::vector<std::uint8_t> expected(12);
    detail::store(expected.data(), sample.key);
    detail::store(expected.data() + 4, sample.a);
    detail::store(expected.data() + 8, sample.b);
    RC_ASSERT(bytes == expected);
    RC_ASSERT(tables::describe(shapes::Sample::read(bytes)) == tables::describe(sample));

    const std::vector<shapes::Sample> samples{sample, sample};
    const auto array = shapes::Sample::array_to_bytes(samples);
    expected.insert(expected.end(), bytes.begin(), bytes.end());
    RC_ASSERT(array == expected);
}

RC_GTEST_PROP(GeneratedShapes, ADocumentOfAnotherVersionIsRejected, ()) {
    const auto version = *rc::gen::suchThat(values::integers<std::uint32_t>(), [](std::uint32_t other) {
        return other != shapes::Document::FORMAT_VERSION;
    });
    // The version follows the length in the header
    auto bytes = shapes::DocumentFields{}.serialize().to_vector();
    for (std::size_t byte = 0; byte < 4; ++byte) {
        bytes.at(4 + byte) = static_cast<std::uint8_t>(version >> (8 * byte));
    }

    RC_ASSERT(shapes::Document::version_of(bytes) == version);
    try {
        static_cast<void>(shapes::Document(bytes));
        RC_FAIL("A document of another version was read");
    } catch (const DecodeError& error) {
        RC_ASSERT(error.kind() == DecodeError::Kind::unsupported_version);
        RC_ASSERT(error.version() == version);
        RC_ASSERT(error.supported() == shapes::Document::FORMAT_VERSION);
    }
}

/// Corrupted and arbitrary bytes read as values or decode errors, and an item
/// list's view reads what its getter reads
RC_GTEST_PROP(GeneratedShapes, CorruptedTablesReadAsValuesOrDecodeErrors, ()) {
    const auto document = *documents();
    const auto list = *items();
    const auto corruption = *values::corruptions();

    for (const auto& bytes : values::corrupted(document.fields().serialize().to_vector(), corruption)) {
        static_cast<void>(try_read([&] { return tables::read(shapes::Point(bytes)); }));
        static_cast<void>(try_read([&] { return tables::read(shapes::Document(bytes)); }));
        static_cast<void>(try_read([&] { return tables::describe(shapes::Sample::read(bytes)); }));
    }

    for (const auto& bytes :
         values::corrupted(shapes::ItemListFields{.items = list}.serialize().to_vector(), corruption)) {
        const shapes::ItemList read(bytes);
        const auto from_getter = try_read([&] { return tables::read(read); });
        RC_ASSERT(try_read([&] { return tables::read_view(read, true); }) == from_getter);
        RC_ASSERT(try_read([&] { return tables::read_view(read, false); }) == from_getter);
    }
}

/// Items are read in place, so an item whose offsets lead past its end reads
/// the bytes after it, within its item list's bytes, as every target reads it
TEST(GeneratedShapes, AnItemIsReadInPlacePastItsEndWithinTheBytesOfItsItemList) {
    const std::vector<shapes::ItemFields> list{
        {.value = 1.0F, .pair = {.first = 2.0F, .second = 3.0F}},
        {.value = 4.0F, .pair = {.first = 5.0F, .second = 6.0F}},
    };
    auto bytes = shapes::ItemListFields{.items = list}.serialize().to_vector();

    // The first item's offset of its value, at the start of the item, points
    // at the second item's value
    const ByteBuffer buffer(bytes, TableLayout());
    const auto vector = buffer.get_vector(buffer.root_offset(0));
    const auto first = static_cast<std::size_t>(vector.get(0).data() - bytes.data());
    const auto second = static_cast<std::size_t>(vector.get(1).data() - bytes.data());
    const auto value = second - first + 8;
    detail::store(bytes.data() + first, static_cast<std::uint32_t>(value));

    const shapes::ItemList read(bytes);
    EXPECT_EQ(read.items()[0].value(), 4.0F);
    EXPECT_EQ(read.items_view().get(0).value(), 4.0F);
}

TEST(GeneratedShapes, ASampleShorterThanItsSizeIsRejected) {
    const std::vector<std::uint8_t> bytes(shapes::Sample::SIZE - 1);
    try {
        static_cast<void>(shapes::Sample::read(bytes));
        FAIL() << "A short sample was read";
    } catch (const DecodeError& error) {
        EXPECT_EQ(error.kind(), DecodeError::Kind::out_of_range);
        EXPECT_EQ(error.byte_length(), shapes::Sample::SIZE);
        EXPECT_EQ(error.available(), bytes.size());
    }
}

TEST(GeneratedShapes, AViewOfAnEmptyVectorHoldsNoItems) {
    const auto bytes = shapes::ItemListFields{}.serialize();
    const auto view = shapes::ItemList(bytes).items_view();

    EXPECT_TRUE(view.empty());
    EXPECT_EQ(view.begin(), view.end());
    EXPECT_THROW(static_cast<void>(view.get(0)), std::out_of_range);
}

/// A default reader of a table reads no bytes, so that a vector of them can
/// be sized before they're read
TEST(GeneratedShapes, ADefaultTableReadsAsDecodeErrors) {
    const shapes::Pair pair;
    EXPECT_THROW(static_cast<void>(pair.first()), DecodeError);

    const shapes::Columns columns;
    EXPECT_THROW(static_cast<void>(columns.base()), DecodeError);
    EXPECT_THROW(static_cast<void>(columns.keys()), DecodeError);
}

/// The error that `read` throws
template <class Read>
DecodeError decode_error_of(Read read) {
    try {
        read();
    } catch (const DecodeError& error) {
        return error;
    }
    throw std::logic_error("The read didn't fail");
}

/// A reader of a table nested in a field reads it from the table's start, so
/// its decode errors report positions from there, as a vector item's do, and
/// as a `ByteBuffer` of the table's bytes reports them
TEST(GeneratedShapes, ATableNestedInAFieldReportsDecodeErrorsFromItsOwnStart) {
    const std::vector<std::uint32_t> keys{1, 2, 3};
    const std::vector<float> a{0.5F, 1.5F, 2.5F};
    const std::vector<float> b{2.5F, 3.5F, 4.5F};
    const auto bytes =
        shapes::DocumentFields{.x = 1, .y = 2, .name = "a", .columns = {.base = 0, .keys = keys, .a = a, .b = b}}
            .serialize();

    // Cut inside `b`, the last column, which the columns' offsets still find
    const auto truncated = bytes.span().first(bytes.size() - 2);
    const auto columns = ByteBuffer(truncated, shapes::Document::LAYOUT).root_offset(3);
    const ByteBuffer nested(truncated.subspan(columns), TableLayout());

    const auto expected =
        decode_error_of([&] { static_cast<void>(nested.get_float32_array(nested.field_offset(0, 3))); });
    const auto error = decode_error_of([&] { static_cast<void>(shapes::Document(truncated).columns().b()); });
    EXPECT_EQ(error.kind(), DecodeError::Kind::out_of_range);
    EXPECT_EQ(error.position(), expected.position());
    EXPECT_EQ(error.byte_length(), expected.byte_length());
    EXPECT_EQ(error.available(), truncated.size() - columns);
}

}  // namespace
}  // namespace tafla::test
