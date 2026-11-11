// A `TableSizer` counts each table as `TableBuilder` counts it, padding
// included, and its `TableWriter` writes the same bytes from the tests' calls
// as from the builder's, which writes its tables through a `TableWriter` too,
// with nested tables written in place. They fail as the builder fails: for a
// table too long for its offsets, a header too long and a version too large.
// A writer given other fields than the sizer added fails rather than writing
// past the bytes, and no calls make it crash, which the sanitizers check.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include "common/build.hpp"
#include "common/read.hpp"
#include "common/strategy.hpp"
#include "common/write.hpp"

namespace tafla::test {
namespace {

/// What writing a table returned: its bytes or its error
struct Written {
    std::optional<std::vector<std::uint8_t>> bytes;
    std::optional<BuildError> error;
};

template <class Write>
Written written_by(Write write) {
    try {
        return {write().to_vector(), std::nullopt};
    } catch (const BuildError& error) {
        return {std::nullopt, error};
    }
}

bool same_details(const BuildError& left, const BuildError& right) {
    const auto& a = left.details();
    const auto& b = right.details();
    return a.kind == b.kind && a.offset_type == b.offset_type && a.first == b.first && a.second == b.second;
}

template <class Build>
BuildError build_error_of(Build build) {
    try {
        build();
    } catch (const BuildError& error) {
        return error;
    }
    throw std::logic_error("The build didn't fail");
}

/// Tables of the layouts that `layouts` generates whose nested tables a
/// `TableWriter` writes in place
rc::Gen<Table> in_place_tables(rc::Gen<Layout> layouts) {
    return rc::gen::map(strategy::tables(std::move(layouts)), nested_in_place);
}

rc::Gen<Layout> short_layouts() {
    return strategy::layouts(rc::gen::element(OffsetType::uint8, OffsetType::uint16));
}

RC_GTEST_PROP(TableWriter, SizesAnyTableAsTableBuilderCountsItAndWritesTheSameBytes, ()) {
    const auto table = *in_place_tables(strategy::layout());
    const auto built = written_by([&] { return to_bytes(table); });
    RC_PRE(built.bytes.has_value());

    RC_ASSERT(write(table).to_vector() == *built.bytes);
}

/// The index of the root field that `TableBuilder` fails to set, whether the
/// root or a table nested in that field is too long
std::optional<std::size_t> failing_field(const Table& table) {
    auto builder = builder_of(table);
    for (std::size_t index = 0; index < table.fields.size(); ++index) {
        try {
            set_field(table.fields[index], builder);
        } catch (const BuildError&) {
            return index;
        }
    }
    return std::nullopt;
}

/// TableBuilder reports the error of a nested table too long for its offsets,
/// which the sizer counts as part of the root, so only the errors' kinds are
/// the same, and the field of the root that fails
RC_GTEST_PROP(TableWriter, FailsAsTableBuilderFailsForAnyTableTooLongForItsOffsets, ()) {
    auto longer = *in_place_tables(short_layouts());
    const auto length = *rc::gen::inRange<std::size_t>(0, 301);
    longer.fields.push_back({std::vector<std::uint8_t>(length, 0)});

    const auto written = written_by([&] { return write(longer); });
    const auto built = written_by([&] { return to_bytes(longer); });

    RC_ASSERT(written.bytes.has_value() == built.bytes.has_value());
    if (written.bytes) {
        RC_ASSERT(*written.bytes == *built.bytes);
    } else {
        RC_ASSERT(written.error->kind() == built.error->kind());
        if (written.error->kind() == BuildError::Kind::too_long) {
            RC_ASSERT(std::optional(written.error->index()) == failing_field(longer));
        }
    }
}

/// Fields after which a table's length doesn't depend on where they start:
/// those that aren't padded, from tables of `uint8` offsets, which hold few
/// enough values to fit after others more often than not
rc::Gen<Field> unpadded_field() {
    const auto unpadded = [](const Field& field) {
        return field.alignment() == Alignment::one && !std::holds_alternative<TableField>(field.value);
    };
    const auto tables =
        rc::gen::suchThat(strategy::tables(strategy::layouts(rc::gen::just(OffsetType::uint8))),
                          [unpadded](const Table& table) { return std::ranges::any_of(table.fields, unpadded); });

    return rc::gen::map(tables,
                        [unpadded](const Table& table) { return *std::ranges::find_if(table.fields, unpadded); });
}

/// The fields' lengths don't depend on where they start after the bytes, so
/// that the bytes can make the table exactly as long as its offsets address
RC_GTEST_PROP(TableWriter, WritesATableAsLongAsItsOffsetsAddressAndFailsForOneAByteLonger, ()) {
    const auto table = *in_place_tables(short_layouts());
    const auto last = *unpadded_field();

    const auto offset_type = table.layout.offset_type;
    const std::size_t max = max_value(offset_type);
    const auto with_bytes = [&](std::size_t length) {
        auto longer = table;
        longer.fields.push_back({std::vector<std::uint8_t>(length, 0)});
        longer.fields.push_back(last);
        return nested_in_place(longer);
    };

    const auto shortest = written_by([&] { return to_bytes(with_bytes(0)); });
    RC_PRE(shortest.bytes.has_value());
    const auto room = max - shortest.bytes->size();

    const auto longest = with_bytes(room);
    const auto bytes = write(longest);
    RC_ASSERT(bytes.size() == max);
    RC_ASSERT(bytes == to_bytes(longest));

    const auto error = BuildError::too_long(table.fields.size() + 1, max + 1, offset_type);
    const auto too_long = with_bytes(room + 1);
    RC_ASSERT(same_details(build_error_of([&] { static_cast<void>(write(too_long)); }), error));
    RC_ASSERT(same_details(build_error_of([&] { static_cast<void>(to_bytes(too_long)); }), error));
}

/// A writer given a field of another length than the sizer added fails, and
/// one given a field of the same length writes it as TableBuilder does, since
/// the bytes come from the fields written
RC_GTEST_PROP(TableWriter, AWriterOfOtherFieldsThanThoseSizedFailsOrWritesThemAsTableBuilderDoes, ()) {
    const auto table = *in_place_tables(strategy::layout());
    const auto replacements = *strategy::table();
    RC_PRE(!table.fields.empty() && !replacements.fields.empty());
    const auto built = written_by([&] { return to_bytes(table); });
    RC_PRE(built.bytes.has_value());

    const auto at = *rc::gen::inRange<std::size_t>(0, table.fields.size());
    const auto with = *rc::gen::inRange<std::size_t>(0, replacements.fields.size());

    auto replaced = table;
    replaced.fields[at] = replacements.fields[with];
    const auto written_table = nested_in_place(replaced);

    const auto written = written_by([&] { return write_as(table, written_table); });
    if (written.bytes) {
        RC_ASSERT(*written.bytes == to_bytes(written_table).to_vector());
    } else {
        RC_ASSERT(written.error->kind() == BuildError::Kind::not_as_sized);
        RC_ASSERT(written.error->length() == built.bytes->size());
    }
}

/// Writes a table of an array of `before` bytes and an array of `structs`,
/// with `set_structs()` and with `set_struct_array()`
template <std::size_t Size>
std::pair<Written, Written> write_structs(OffsetType offset_type, std::size_t before,
                                          const std::vector<std::array<std::uint8_t, Size>>& structs,
                                          Alignment alignment) {
    const auto write_with = [&](auto set) {
        return written_by([&] {
            TableSizer sizer(2, TableLayout().with_offset_type(offset_type));
            sizer.add_uint8_array(before);
            sizer.add_struct_array(structs.size(), Size, alignment);

            auto writer = sizer.writer();
            writer.set_uint8_array(0, std::vector<std::uint8_t>(before, 7));
            set(writer);
            return writer.finish();
        });
    };

    std::vector<std::uint8_t> all;
    for (const auto& item : structs) {
        all.insert(all.end(), item.begin(), item.end());
    }

    return {
        write_with([&](TableWriter& writer) {
            const auto copy = [](const std::array<std::uint8_t, Size>& item, std::uint8_t* bytes) {
                std::ranges::copy(item, bytes);
            };
            writer.set_structs<Size>(1, structs, copy, alignment);
        }),
        write_with([&](TableWriter& writer) { writer.set_struct_array(1, all, Size, alignment); }),
    };
}

bool same_written(const Written& left, const Written& right) {
    if (left.bytes || right.bytes) {
        return left.bytes == right.bytes;
    }
    return same_details(*left.error, *right.error);
}

template <std::size_t Size>
rc::Gen<std::vector<std::array<std::uint8_t, Size>>> structs_of(std::size_t max) {
    return rc::gen::mapcat(rc::gen::inRange<std::size_t>(0, max), [](std::size_t count) {
        return rc::gen::container<std::vector<std::array<std::uint8_t, Size>>>(
            count, rc::gen::container<std::array<std::uint8_t, Size>>(rc::gen::arbitrary<std::uint8_t>()));
    });
}

/// `set_structs()` writes the bytes that `set_struct_array()` writes from all
/// the structs' bytes, wherever the array starts, and fails as it fails for a
/// table too long for its offsets
RC_GTEST_PROP(TableWriter, WritesStructsFromTheirConversionAsFromAllTheirBytes, ()) {
    const auto offset_type = *strategy::offset_type();
    const auto before = *rc::gen::inRange<std::size_t>(0, 8);
    const auto alignment = *rc::gen::element(Alignment::one, Alignment::two, Alignment::four, Alignment::eight);
    const auto triples = *structs_of<3>(24);
    const auto octets = *structs_of<8>(40);

    const auto [from_triples, from_triple_bytes] = write_structs(offset_type, before, triples, alignment);
    RC_ASSERT(same_written(from_triples, from_triple_bytes));

    const auto [from_octets, from_octet_bytes] = write_structs(offset_type, before, octets, alignment);
    RC_ASSERT(same_written(from_octets, from_octet_bytes));
}

/// A sizer of the table with every field added
TableSizer sized(const Table& table) {
    auto sizer = sizer_of(table);
    for (const auto& field : table.fields) {
        add_field(field, sizer);
    }
    return sizer;
}

/// Calls in any order, with any indices, counts and items, write a table or
/// fail with an error, and never write past its length
RC_GTEST_PROP(TableWriter, NoCallsMakeAWriterCrash, ()) {
    const auto table = *strategy::table();

    // Mostly small, so that they fit, and sometimes any
    const auto small = rc::gen::weightedOneOf<std::size_t>({
        {4, rc::gen::inRange<std::size_t>(0, 8)},
        {1, rc::gen::arbitrary<std::size_t>()},
    });
    const auto alignments = rc::gen::element(Alignment::one, Alignment::two, Alignment::four, Alignment::eight);
    const auto calls = *rc::gen::resize(
        rc::kNominalSize, rc::gen::mapcat(rc::gen::inRange<std::size_t>(0, 25), [=](std::size_t count) {
            return rc::gen::container<std::vector<std::tuple<int, std::size_t, std::size_t, Alignment>>>(
                count, rc::gen::tuple(rc::gen::inRange(0, 10), small, small, alignments));
        }));

    std::optional<TableSizer> sizer;
    try {
        sizer = sized(table);
        static_cast<void>(sizer->byte_length());
    } catch (const BuildError&) {
        return;
    }
    const auto length = sizer->byte_length();
    auto writer = sizer->writer();

    std::vector<WriterParent> parents;
    std::vector<VectorPosition> vectors;
    for (const auto& [kind, index, count, alignment] : calls) {
        const auto few = count % 12;
        switch (kind) {
            case 0:
                writer.set_uint8(index, 1);
                break;
            case 1:
                writer.set_float64_array(index, std::vector<double>(few, -0.0));
                break;
            case 2: {
                std::string string;
                for (std::size_t character = 0; character < few; ++character) {
                    string += "\xf0\x9f\xa6\x80";
                }
                writer.set_string(index, string);
                break;
            }
            case 3:
                writer.set_struct_array(index, std::vector<std::uint8_t>(few, 1), count % 4, Alignment::two);
                break;
            case 4: {
                const std::vector<std::int16_t> structs(few, -1);
                const auto store = [](std::int16_t value, std::uint8_t* bytes) { detail::store(bytes, value); };
                writer.set_structs<2>(index, structs, store, Alignment::two);
                break;
            }
            case 5:
                writer.set_table(index, alignment);
                break;
            case 6:
                parents.push_back(writer.start_table(index));
                break;
            case 7:
                if (!parents.empty()) {
                    writer.end_table(parents.back());
                    parents.pop_back();
                }
                break;
            case 8:
                vectors.push_back(writer.set_vector(index, count));
                break;
            default:
                if (!vectors.empty()) {
                    writer.set_item_end(vectors.back(), index);
                }
                break;
        }
    }

    try {
        RC_ASSERT(writer.finish().size() == length);
    } catch (const BuildError&) {
        // Calls other than those sized fail
    }
}

TEST(TableWriter, PadsAStructArrayANestedTableAndAVectorAsTableBuilderCountsTheirPadding) {
    // A uint8 leaves the uint32[] 3 bytes short of its alignment, the array
    // leaves the structs 2 bytes short of theirs, and the structs leave the
    // nested table of float64s 6 bytes short
    const auto nested = [](std::vector<Field> fields) {
        return Nested{Source::builder, {Layout::headerless(OffsetType::uint16), std::nullopt, std::move(fields)}};
    };
    const Table table{
        {true, true, OffsetType::uint16},
        7,
        {
            {std::uint8_t{1}},
            {std::vector<std::uint32_t>{2, 3}},
            {StructArray{{ScalarType::uint16, ScalarType::float64}, {{{std::uint16_t{4}}, {5.0}}}}},
            {TableField{nested({{std::vector<double>{6.0}}}), std::nullopt}},
            {VectorField{{nested({{std::string("\xf0\x9f\xa6\x80")}})}}},
        },
    };

    const std::vector<std::uint8_t> expected{
        // The length, the version and the offsets of the fields
        76,
        0,
        7,
        0,
        14,
        0,
        18,
        0,
        30,
        0,
        48,
        0,
        64,
        0,
        // The uint8, the padding, the count and the uint32s
        1,
        0,
        0,
        0,
        2,
        0,
        2,
        0,
        0,
        0,
        3,
        0,
        0,
        0,
        // The padding, the count and the struct of a uint16 and a float64
        0,
        0,
        1,
        0,
        4,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0x14,
        0x40,
        // The padding, and the nested table: its offset, its padding, the
        // count and the float64
        0,
        0,
        0,
        0,
        0,
        0,
        6,
        0,
        0,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0x18,
        0x40,
        // The vector's count and end, and its item: its offset, and the
        // string's length and bytes
        1,
        0,
        12,
        0,
        2,
        0,
        4,
        0,
        0xf0,
        0x9f,
        0xa6,
        0x80,
    };

    const auto bytes = write(table);
    EXPECT_EQ(bytes.to_vector(), expected);
    EXPECT_EQ(to_bytes(table).to_vector(), expected);
    EXPECT_EQ(read_table(bytes, table), table);
}

TEST(TableWriter, NamesTheRootFieldThatMakesTheTableTooLongWithTheLengthAfterIt) {
    // A vector of three items of 102 bytes, which is too long for `uint8`
    // offsets after its second item, and makes the table 314 bytes
    const Nested item{Source::builder,
                      {Layout::headerless(OffsetType::uint8), std::nullopt, {{std::vector<std::uint8_t>(100, 0)}}}};
    const Table table{
        Layout::headerless(OffsetType::uint8),
        std::nullopt,
        {{std::uint8_t{1}}, {VectorField{{item, item, item}}}, {std::uint8_t{2}}},
    };

    const auto error = BuildError::too_long(1, 314, OffsetType::uint8);
    EXPECT_TRUE(same_details(build_error_of([&] { static_cast<void>(write(table)); }), error));
    EXPECT_TRUE(same_details(build_error_of([&] { static_cast<void>(to_bytes(table)); }), error));
}

TEST(TableWriter, RejectsATableOfUint32OffsetsPast4GiBBeforeAllocatingIt) {
    // The table would hold its offset, its count and 2^30 uint32s
    TableSizer sizer(1, TableLayout());
    sizer.add_uint32_array(std::size_t{1} << 30);

    const auto error = BuildError::too_long(0, 4'294'967'304, OffsetType::uint32);
    EXPECT_TRUE(same_details(build_error_of([&] { static_cast<void>(sizer.byte_length()); }), error));
    EXPECT_TRUE(same_details(build_error_of([&] { static_cast<void>(sizer.writer()); }), error));
    EXPECT_STREQ(error.what(), "Field 1 makes the table 4294967304 bytes, too long for uint32 offsets");
}

TEST(TableWriter, CountsLengthsPastAnySizeWithoutOverflowing) {
    TableSizer sizer(3, TableLayout());
    sizer.add_uint8();
    sizer.add_float64_array(SIZE_MAX);
    sizer.add_vector(SIZE_MAX);
    for (int item = 0; item < 3; ++item) {
        const auto parent = sizer.start_table(SIZE_MAX);
        sizer.add_struct_array(SIZE_MAX, SIZE_MAX, Alignment::eight);
        sizer.add_table(Alignment::eight);
        sizer.end_table(parent);
    }

    const auto error = build_error_of([&] { static_cast<void>(sizer.writer()); });
    EXPECT_EQ(error.kind(), BuildError::Kind::too_long);
    EXPECT_EQ(error.index(), 1U);
    EXPECT_EQ(error.offset_type(), OffsetType::uint32);
}

TEST(TableWriter, RejectsAHeaderTooLongForItsOffsetsAsTableBuilderDoes) {
    for (const auto offset_type : {OffsetType::uint8, OffsetType::uint16}) {
        const std::size_t max = max_value(offset_type);
        const auto width = offset_size(offset_type);

        for (const bool sized : {false, true}) {
            auto layout = TableLayout().with_offset_type(offset_type);
            if (sized) {
                layout = layout.sized();
            }
            const auto fields = max / width - (sized ? 1 : 0);

            EXPECT_NO_THROW(TableSizer(fields, layout));
            EXPECT_TRUE(same_details(build_error_of([&] { TableSizer(fields + 1, layout); }),
                                     build_error_of([&] { TableBuilder(fields + 1, layout); })));

            const auto versioned = layout.versioned();
            EXPECT_NO_THROW(TableSizer(fields - 1, versioned, 0));
            EXPECT_TRUE(same_details(build_error_of([&] { TableSizer(fields, versioned, 0); }),
                                     build_error_of([&] { TableBuilder(fields, versioned, 0); })));
        }
    }

    EXPECT_TRUE(same_details(build_error_of([] { TableSizer(SIZE_MAX, TableLayout()); }),
                             build_error_of([] { TableBuilder(SIZE_MAX, TableLayout()); })));
}

TEST(TableWriter, RejectsAVersionTooLargeForItsSlotAsTableBuilderDoes) {
    for (const auto& [offset_type, version] :
         {std::pair{OffsetType::uint8, 0x100U}, std::pair{OffsetType::uint16, 0x1'0000U}}) {
        const auto layout = TableLayout().with_offset_type(offset_type).versioned();
        const auto error = BuildError::version_too_large(version, offset_type);

        EXPECT_TRUE(same_details(build_error_of([&] { TableSizer(1, layout, version); }), error));
        EXPECT_TRUE(same_details(build_error_of([&] { TableBuilder(1, layout, version); }), error));
        EXPECT_NO_THROW(TableSizer(1, layout, version - 1));
    }
}

TEST(TableWriter, FailsForAStringLongerThanTheOneSized) {
    TableSizer sizer(1, TableLayout());
    sizer.add_string("a");
    auto writer = sizer.writer();
    writer.set_string(0, "ab");

    // The offset and length were written before the string didn't fit
    EXPECT_TRUE(
        same_details(build_error_of([&] { static_cast<void>(writer.finish()); }), BuildError::not_as_sized(8, 9)));
}

TEST(TableWriter, FailsForFieldsShorterThanThoseSized) {
    TableSizer sizer(1, TableLayout());
    sizer.add_string("ab");
    auto writer = sizer.writer();
    writer.set_string(0, "a");

    const auto error = build_error_of([&] { static_cast<void>(writer.finish()); });
    EXPECT_TRUE(same_details(error, BuildError::not_as_sized(9, 10)));
    EXPECT_EQ(error.written(), 9U);
    EXPECT_EQ(error.length(), 10U);
    EXPECT_STREQ(error.what(), "The fields wrote 9 of the table's 10 bytes");
}

TEST(TableWriter, FailsForAFieldPastItsTablesFields) {
    TableSizer sizer(2, TableLayout());
    sizer.add_uint8();
    sizer.add_table(Alignment::one);
    const auto sizer_parent = sizer.start_table(1);
    sizer.add_uint8();
    sizer.end_table(sizer_parent);

    auto writer = sizer.writer();
    writer.set_uint8(0, 1);
    writer.set_table(1, Alignment::one);
    const auto parent = writer.start_table(1);
    writer.set_uint8(1, 2);
    writer.end_table(parent);

    EXPECT_TRUE(
        same_details(build_error_of([&] { static_cast<void>(writer.finish()); }), BuildError::too_many_fields(1)));
}

TEST(TableWriter, FailsForAnItemPastItsVectorsItems) {
    TableSizer sizer(1, TableLayout());
    sizer.add_vector(1);
    const auto sizer_parent = sizer.start_table(0);
    sizer.end_table(sizer_parent);

    auto writer = sizer.writer();
    const auto vector = writer.set_vector(0, 1);
    const auto parent = writer.start_table(0);
    writer.end_table(parent);
    writer.set_item_end(vector, 1);

    EXPECT_TRUE(
        same_details(build_error_of([&] { static_cast<void>(writer.finish()); }), BuildError::not_as_sized(12, 12)));
}

TEST(TableWriter, RejectsAnArrayOf0ByteStructsFromTheSizerAsTableBuilderDoes) {
    TableSizer sizer(1, TableLayout());
    sizer.add_struct_array(2, 0, Alignment::one);

    TableBuilder builder(1, TableLayout());
    const auto error =
        build_error_of([&] { builder.set_struct_array(std::vector<std::uint8_t>(), 0, Alignment::one); });
    EXPECT_TRUE(same_details(build_error_of([&] { static_cast<void>(sizer.byte_length()); }), error));
    EXPECT_TRUE(same_details(build_error_of([&] { static_cast<void>(sizer.writer()); }), error));
}

TEST(TableWriter, SizesAndWritesAStructOf0BytesAtTheNextFieldsOffset) {
    TableSizer sizer(2, TableLayout());
    sizer.add_struct(0);
    sizer.add_uint8();
    auto writer = sizer.writer();
    writer.set_struct(0, {});
    writer.set_uint8(1, 7);

    TableBuilder builder(2, TableLayout());
    builder.set_struct(std::span<const std::uint8_t>());
    builder.set_uint8(7);

    // Both fields' offsets are 8, after the two offsets
    const std::vector<std::uint8_t> expected{8, 0, 0, 0, 8, 0, 0, 0, 7};
    EXPECT_EQ(writer.finish().to_vector(), expected);
    EXPECT_EQ(builder.to_bytes().to_vector(), expected);
}

TEST(TableWriter, FailsForStructsOf0BytesAsForTheirBytes) {
    TableSizer sizer(1, TableLayout());
    sizer.add_struct_array(2, 2, Alignment::one);

    auto from_structs = sizer.writer();
    const std::vector<int> items{1, 2};
    from_structs.set_structs<0>(0, items, [](int, std::uint8_t*) {}, Alignment::one);
    auto from_bytes = sizer.writer();
    from_bytes.set_struct_array(0, {}, 0, Alignment::one);

    const auto error = BuildError::not_whole_structs(0, 0);
    EXPECT_TRUE(same_details(build_error_of([&] { static_cast<void>(from_structs.finish()); }), error));
    EXPECT_TRUE(same_details(build_error_of([&] { static_cast<void>(from_bytes.finish()); }), error));
}

/// `set_structs()` takes each item by reference, so the items can be any type,
/// such as one that holds a string, that converts into the structs
TEST(TableWriter, WritesStructsFromItemsOfAnyType) {
    const std::vector<std::pair<std::string, std::uint16_t>> items{{"a", 1}, {"b", 2}};
    TableSizer sizer(1, TableLayout());
    sizer.add_struct_array(items.size(), 2, Alignment::two);

    auto from_items = sizer.writer();
    const auto store = [](const std::pair<std::string, std::uint16_t>& item, std::uint8_t* bytes) {
        detail::store(bytes, item.second);
    };
    from_items.set_structs<2>(0, items, store, Alignment::two);
    auto from_bytes = sizer.writer();
    from_bytes.set_struct_array(0, std::vector<std::uint8_t>{1, 0, 2, 0}, 2, Alignment::two);

    EXPECT_EQ(from_items.finish(), from_bytes.finish());
}

TEST(TableWriter, FailsForBytesThatArentWholeStructsAsTableBuilderDoes) {
    for (const auto& [length, size] : {std::pair<std::size_t, std::size_t>{5, 2}, {4, 0}, {0, 0}}) {
        TableSizer sizer(1, TableLayout());
        sizer.add_struct_array(2, 2, Alignment::one);
        auto writer = sizer.writer();
        writer.set_struct_array(0, std::vector<std::uint8_t>(length, 0), size, Alignment::one);

        TableBuilder builder(1, TableLayout());
        const auto error = build_error_of(
            [&] { builder.set_struct_array(std::vector<std::uint8_t>(length, 0), size, Alignment::one); });
        EXPECT_TRUE(same_details(build_error_of([&] { static_cast<void>(writer.finish()); }), error));
    }
}

TEST(TableWriter, WritesAnEmptyTableWithoutBytes) {
    TableSizer sizer(0, TableLayout());
    auto writer = sizer.writer();
    const auto bytes = writer.finish();
    EXPECT_TRUE(bytes.empty());
}

}  // namespace
}  // namespace tafla::test
