// A vector of strings reads back every string, at once and each from its ends
// alone, takes the bytes of a vector whose items are the strings' UTF-8, and
// reads corrupted bytes as reading each string on its own reads them, failing
// with the error of the first string that fails. The examples document the
// errors.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <cstddef>
#include <cstdint>
#include <optional>
#include <span>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include <tafla/tafla.hpp>

#include "common/strategy.hpp"

namespace tafla::test {
namespace {

/// Lists of up to `max` strings of `strings`
rc::Gen<std::vector<std::string>> lists_of(rc::Gen<std::string> strings, std::size_t max) {
    return rc::gen::resize(rc::kNominalSize,
                           rc::gen::mapcat(rc::gen::inRange<std::size_t>(0, max + 1), [strings](std::size_t count) {
                               return rc::gen::container<std::vector<std::string>>(count, strings);
                           }));
}

/// Lists of up to 8 strings of characters of every width in UTF-8, empty too
rc::Gen<std::vector<std::string>> lists() {
    return lists_of(strategy::strings(16), 8);
}

/// Strings of up to 40 characters of printable ASCII
rc::Gen<std::string> ascii() {
    return rc::gen::resize(rc::kNominalSize,
                           rc::gen::mapcat(rc::gen::inRange<std::size_t>(0, 41), [](std::size_t length) {
                               return rc::gen::container<std::string>(length, rc::gen::inRange<char>(' ', '~' + 1));
                           }));
}

/// Lists of strings of ASCII among others
rc::Gen<std::vector<std::string>> mixed_lists() {
    return lists_of(rc::gen::oneOf(ascii(), strategy::strings(16)), 16);
}

/// Lists of 100 to 1,500 strings of ASCII of one length, with up to 8 others
/// among them
rc::Gen<std::vector<std::string>> long_lists() {
    return rc::gen::exec([] {
        const auto count = *rc::gen::inRange<std::size_t>(100, 1501);
        const auto length = *rc::gen::inRange<std::size_t>(1, 61);

        std::vector<std::string> strings;
        for (std::size_t index = 0; index < count; ++index) {
            auto digits = std::to_string(index);
            strings.push_back(std::string(length > digits.size() ? length - digits.size() : 0, 'x') + digits);
        }

        const auto others = *rc::gen::container<std::vector<std::pair<std::size_t, std::string>>>(
            *rc::gen::inRange<std::size_t>(0, 9),
            rc::gen::pair(rc::gen::arbitrary<std::size_t>(), strategy::strings(16)));
        for (const auto& [at, string] : others) {
            strings[at % count] = string;
        }
        return strings;
    });
}

std::vector<std::string_view> views_of(const std::vector<std::string>& strings) {
    return {strings.begin(), strings.end()};
}

/// A table whose only field is a vector of `strings`
Bytes table_of(std::span<const std::string_view> strings, TableLayout<false> layout = TableLayout()) {
    TableBuilder builder(1, layout);
    builder.set_string_vector(strings);
    return builder.to_bytes();
}

Bytes table_of(const std::vector<std::string>& strings) {
    return table_of(views_of(strings));
}

/// The view of the vector of strings of a table whose first field it is
StringVector view_of(std::span<const std::uint8_t> bytes, TableLayout<false> layout = TableLayout()) {
    const TableReader reader(bytes, layout);
    return reader.get_string_vector(reader.root_offset(0));
}

/// The strings that views view
std::vector<std::string> strings_of(const std::vector<std::string_view>& views) {
    return {views.begin(), views.end()};
}

/// What reading strings returned: the views, or the decode error
struct Result {
    std::optional<std::vector<std::string_view>> strings;
    std::optional<DecodeError> error;

    /// Whether the other read the same bytes as the same strings, or failed
    /// with the same error, of the same kind at the same position
    [[nodiscard]] bool same(const Result& other) const {
        if (strings.has_value() != other.strings.has_value()) {
            return false;
        }
        if (!strings) {
            return error->kind() == other.error->kind() && error->position() == other.error->position();
        }
        return std::ranges::equal(*strings, *other.strings, [](std::string_view left, std::string_view right) {
            return left.data() == right.data() && left.size() == right.size();
        });
    }
};

template <class Read>
Result result_of(Read read) {
    try {
        return {read(), std::nullopt};
    } catch (const DecodeError& error) {
        return {std::nullopt, error};
    }
}

/// Each string of a view from its ends alone, in order, up to the first that
/// fails
Result each_of(const StringVector& view) {
    return result_of([&view] {
        std::vector<std::string_view> strings;
        for (std::size_t index = 0; index < view.size(); ++index) {
            strings.push_back(view.get(index));
        }
        return strings;
    });
}

Result iterated(const StringVector& view) {
    return result_of([&view] { return std::vector<std::string_view>(view.begin(), view.end()); });
}

Result every(const StringVector& view) {
    return result_of([&view] { return view.to_vector(); });
}

/// Throws the decode error of `read`
template <class Read>
DecodeError decode_error_of(Read read) {
    try {
        static_cast<void>(read());
    } catch (const DecodeError& error) {
        return error;
    }
    throw std::logic_error("The read didn't fail");
}

RC_GTEST_PROP(StringVector, ReadsBackEveryStringAtOnceByIteratingAndEachFromItsEndsAlone, ()) {
    const auto strings = *lists();
    const auto offset_type = *rc::gen::element(OffsetType::uint16, OffsetType::uint32);
    const auto layout = TableLayout().with_offset_type(offset_type);

    const auto bytes = table_of(views_of(strings), layout);
    const auto view = view_of(bytes, layout);

    RC_ASSERT(view.size() == strings.size());
    RC_ASSERT(view.empty() == strings.empty());
    RC_ASSERT(strings_of(view.to_vector()) == strings);
    RC_ASSERT(strings_of(*each_of(view).strings) == strings);
    RC_ASSERT(strings_of(*iterated(view).strings) == strings);
    RC_ASSERT(every(view).same(each_of(view)));
}

/// Its bytes are those of a vector of tables whose bytes are the strings'
/// UTF-8, since a vector's items aren't padded
RC_GTEST_PROP(StringVector, TakesTheBytesOfAVectorWhoseItemsAreTheStringsUtf8, ()) {
    const auto strings = *lists();

    TableBuilder builder(1, TableLayout());
    std::vector<NestedTable> items;
    for (const auto& string : strings) {
        items.emplace_back(std::vector<std::uint8_t>(string.begin(), string.end()));
    }
    builder.set_vector(std::move(items));

    RC_ASSERT(table_of(strings) == builder.to_bytes());
}

RC_GTEST_PROP(StringVector, TakesTheEndOfEachStringAndACountBesideTheStringsUtf8, ()) {
    const auto strings = *lists();

    std::size_t utf8 = 0;
    for (const auto& string : strings) {
        utf8 += string.size();
    }

    // The table's one offset, the count and the ends, of 4 bytes each
    RC_ASSERT(table_of(strings).size() == 4 + 4 + 4 * strings.size() + utf8);
}

/// A sizer and its writer, a builder that owns the strings, and a vector of
/// strings written in a table in place, write what a builder that borrows
/// views of them writes, of short strings and of strings past the 64 bytes
/// from which `TableBuilder` keeps a string rather than copying it
RC_GTEST_PROP(StringVector, IsWrittenByEveryWriterAsByABuilderOfViews, ()) {
    const auto strings = *rc::gen::oneOf(lists(), lists_of(strategy::strings(96), 4));
    const auto views = views_of(strings);
    const auto expected = table_of(views);

    TableSizer sizer(1, TableLayout());
    sizer.add_string_vector(views);
    auto writer = sizer.writer();
    writer.set_string_vector(0, views);
    RC_ASSERT(writer.finish() == expected);

    TableBuilder owner(1, TableLayout());
    owner.set_string_vector(std::vector<std::string>(strings));
    RC_ASSERT(owner.to_bytes() == expected);

    TableBuilder nested(1, TableLayout());
    nested.set_string_vector(views);
    TableBuilder parent(1, TableLayout());
    parent.set_table(nested);
    TableBuilder in_place(1, TableLayout());
    in_place.set_table(1, [&views](TableBuilder::Item& item) { item.set_string_vector(views); });
    RC_ASSERT(in_place.to_bytes() == parent.to_bytes());
}

RC_GTEST_PROP(StringVector, ReadsListsOfAsciiAmongOtherStringsAtOnceAsEachStringOnItsOwn, ()) {
    const auto strings = *mixed_lists();
    const auto bytes = table_of(strings);

    RC_ASSERT(strings_of(view_of(bytes).to_vector()) == strings);
}

RC_GTEST_PROP(StringVector, ReadsListsOfManyStringsAtOnceAsEachStringOnItsOwn, ()) {
    const auto strings = *long_lists();
    const auto bytes = table_of(strings);
    const auto view = view_of(bytes);

    RC_ASSERT(strings_of(view.to_vector()) == strings);
    RC_ASSERT(every(view).same(each_of(view)));
}

/// Overwritten bytes read as what reading each string on its own and
/// iterating read, or fail with the decode error of the first string that
/// fails, and the view itself only with a decode error
RC_GTEST_PROP(StringVector, ReadsWhatReadingEachStringReadsAndFailsWithTheFirstOfTheirErrors, ()) {
    const auto strings = *rc::gen::oneOf(lists(), mixed_lists(), long_lists());
    const auto overwrites = *rc::gen::resize(
        rc::kNominalSize, rc::gen::mapcat(rc::gen::inRange<std::size_t>(1, 5), [](std::size_t count) {
            return rc::gen::container<std::vector<std::pair<std::size_t, std::uint8_t>>>(
                count, rc::gen::pair(rc::gen::arbitrary<std::size_t>(), rc::gen::arbitrary<std::uint8_t>()));
        }));

    auto bytes = table_of(strings).to_vector();
    for (const auto& [at, byte] : overwrites) {
        // Past the table's offset, so that the vector is found
        bytes[4 + at % (bytes.size() - 4)] = byte;
    }

    std::optional<StringVector> view;
    try {
        view = view_of(bytes);
    } catch (const DecodeError& error) {
        RC_ASSERT(error.kind() == DecodeError::Kind::out_of_range);
        return;
    }

    const auto each = each_of(*view);
    RC_ASSERT(iterated(*view).same(each));
    RC_ASSERT(every(*view).same(each));
}

TEST(StringVector, ReadsStringsLongerThanAWordOrABlockOfWords) {
    std::string accented;
    for (int index = 0; index < 40; ++index) {
        accented += "\xc3\xa9";
    }
    std::string ideographs;
    for (int index = 0; index < 300; ++index) {
        ideographs += "\xe6\x9d\xb1";
    }
    const std::vector<std::string> strings{std::string(33, 'a'), accented, ideographs, std::string(4096, 'b')};

    const auto bytes = table_of(strings);
    EXPECT_EQ(strings_of(view_of(bytes).to_vector()), strings);
    EXPECT_EQ(view_of(bytes).get(2), ideographs);
}

TEST(StringVector, AnEmptyViewHoldsNoStrings) {
    const StringVector view;
    EXPECT_EQ(view.size(), 0U);
    EXPECT_TRUE(view.empty());
    EXPECT_TRUE(view.to_vector().empty());
    EXPECT_TRUE(view.begin() == view.end());
    EXPECT_THROW(static_cast<void>(view.get(0)), std::out_of_range);

    const auto bytes = table_of(std::vector<std::string>{});
    EXPECT_TRUE(view_of(bytes).to_vector().empty());
}

TEST(StringVector, ThrowsForTheStringsOnEitherSideOfAnEndThatSplitsACharacterAndReadsTheOthers) {
    // The table's offset, the vector's count at 4, its ends at 8 to 20, and
    // then "a" at 24, the ideographs at 25 and 28, and "b" at 31
    auto bytes = table_of(std::vector<std::string>{"a", "\xe6\x9d\xb1", "\xe4\xba\xac", "b"}).to_vector();
    // The end of the second string, a byte into the third
    --bytes[12];
    const auto view = view_of(bytes);

    EXPECT_EQ(view.get(0), "a");
    const auto second = decode_error_of([&] { return view.get(1); });
    EXPECT_EQ(second.kind(), DecodeError::Kind::invalid_utf8);
    EXPECT_EQ(second.position(), 25U);
    const auto third = decode_error_of([&] { return view.get(2); });
    EXPECT_EQ(third.kind(), DecodeError::Kind::invalid_utf8);
    EXPECT_EQ(third.position(), 27U);
    EXPECT_EQ(view.get(3), "b");

    const auto every = decode_error_of([&] { return view.to_vector(); });
    EXPECT_EQ(every.kind(), DecodeError::Kind::invalid_utf8);
    EXPECT_EQ(every.position(), 25U);
}

TEST(StringVector, ThrowsForAnEndThatSplitsACharacterAmongStringsOfAscii) {
    auto bytes = table_of(std::vector<std::string>{"first", std::string(20, 'a'), "\xc3\xa9\xe6\x9d\xb1",
                                                   std::string(20, 'b'), std::string(20, 'c')})
                     .to_vector();
    // The end of the third string, after the table's offset, the count and
    // two ends, is a byte less, within its ideograph
    --bytes[4 + 4 + 2 * 4];
    const auto view = view_of(bytes);

    const auto error = decode_error_of([&] { return view.to_vector(); });
    EXPECT_EQ(error.kind(), DecodeError::Kind::invalid_utf8);
    EXPECT_TRUE(every(view).same(each_of(view)));
}

TEST(StringVector, ThrowsForAStringOfAsciiThatEndsPastTheStringsAfterIt) {
    auto bytes =
        table_of(std::vector<std::string>{"first", std::string(20, 'a'), std::string(20, 'b'), std::string(20, 'c')})
            .to_vector();
    // The end of the second string, from the vector's position at 4, is a
    // byte before the last string's end, within the bytes
    detail::store(bytes.data() + 4 + 4 + 4, static_cast<std::uint32_t>(bytes.size() - 4 - 1));
    const auto view = view_of(bytes);

    EXPECT_EQ(view.get(1).size(), bytes.size() - 1 - (4 + 4 * 5 + 5));
    const auto error = decode_error_of([&] { return view.to_vector(); });
    EXPECT_EQ(error.kind(), DecodeError::Kind::item_ends_before_start);
    EXPECT_EQ(error.position(), bytes.size() - 1);
    EXPECT_TRUE(every(view).same(each_of(view)));
}

TEST(StringVector, NamesThePositionOfAStringThatIsntValidUtf8) {
    // A string read by halves of a word or by its bytes, and one by blocks
    for (const std::size_t length : {std::size_t{3}, std::size_t{40}}) {
        auto bytes = table_of(std::vector<std::string>{"a", std::string(length, 'b')}).to_vector();
        // The second string starts after the offset, the count, two ends and
        // the first string
        bytes[17] = 0xff;
        const auto view = view_of(bytes);

        const auto error = decode_error_of([&] { return view.get(1); });
        EXPECT_EQ(error.kind(), DecodeError::Kind::invalid_utf8) << length;
        EXPECT_EQ(error.position(), 17U) << length;
        EXPECT_STREQ(error.what(), "The string at 17 isn't valid UTF-8") << length;
        EXPECT_EQ(decode_error_of([&] { return view.to_vector(); }).position(), 17U) << length;
        EXPECT_EQ(decode_error_of([&] { return *std::next(view.begin()); }).position(), 17U) << length;
    }
}

TEST(StringVector, RejectsAnIndexPastTheCountAsTheCallersMistake) {
    const auto bytes = table_of(std::vector<std::string>{"a", "b"});
    const auto view = view_of(bytes);

    EXPECT_THROW(static_cast<void>(view.get(2)), std::out_of_range);
    EXPECT_THROW(static_cast<void>(view.get(SIZE_MAX)), std::out_of_range);
}

TEST(StringVector, RejectsAnImpossibleCountBeforeAnythingAllocatesForIt) {
    const std::vector<std::uint8_t> bytes{255, 255, 255, 255};
    const TableReader reader(bytes, TableLayout());

    const auto error = decode_error_of([&] { return reader.get_string_vector(0); });
    EXPECT_EQ(error.kind(), DecodeError::Kind::out_of_range);
    EXPECT_EQ(error.position(), 4U);
    EXPECT_EQ(error.byte_length(), 4ULL * 0xffff'ffff);
}

TEST(StringVector, ABuilderOwnsTheStringsOfAVectorGivenAsAnRvalue) {
    TableBuilder builder(1, TableLayout());
    {
        // Short strings, which a std::string holds within itself, and a long
        // one
        std::vector<std::string> strings{"a", "", std::string(100, 'c')};
        builder.set_string_vector(std::move(strings));
    }
    const auto bytes = builder.to_bytes();

    EXPECT_EQ(strings_of(view_of(bytes).to_vector()), (std::vector<std::string>{"a", "", std::string(100, 'c')}));

    TableBuilder listed(1, TableLayout());
    listed.set_string_vector({"a", "", std::string(100, 'c')});
    EXPECT_EQ(listed.to_bytes(), bytes);
}

template <class Value>
concept SetsStringVector =
    requires(TableBuilder& builder, Value&& value) { builder.set_string_vector(std::forward<Value>(value)); };

// Views and the containers that the builder owns are set, and a temporary of
// views, which the builder would view after it's destroyed, doesn't compile
static_assert(SetsStringVector<std::vector<std::string_view>&>);
static_assert(SetsStringVector<const std::vector<std::string_view>&>);
static_assert(SetsStringVector<std::span<const std::string_view>>);
static_assert(SetsStringVector<std::vector<std::string>>);
static_assert(!SetsStringVector<std::vector<std::string_view>>);
static_assert(!SetsStringVector<const std::vector<std::string>>);

}  // namespace
}  // namespace tafla::test
