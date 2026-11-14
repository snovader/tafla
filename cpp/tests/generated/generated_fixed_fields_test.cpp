// The code generated from `schemas/fixed_fields.tafla`, whose fields' sizes
// the schema fixes, writes every message as `TableBuilder` writes it by hand,
// in every offset type and header: those that it writes whole, laying out
// their fields and the fixed tables in them itself, and those that it writes
// with `TableSizer` and `TableWriter`, into new bytes and into the start of
// bytes that the caller gives. It reads back what it writes, and fails to
// write what `TableBuilder` fails to: a message written whole whose string
// makes it too long for its offsets, and a message whose fixed fields alone
// are. The Go tests run the same schema.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <bit>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <span>
#include <string>
#include <string_view>
#include <tuple>
#include <utility>
#include <variant>
#include <vector>

#include "common/build.hpp"
#include "common/given_bytes.hpp"
#include "common/strategy.hpp"
#include "common/values.hpp"
#include "fixed_fields/fixed_fields.hpp"

namespace tafla::test {
namespace {

using namespace fixed_fields;

constexpr Vec2 kOrigin{.x = 1.5F, .y = -0.0F};

FrameFields frame() {
    return {.origin = kOrigin, .range = {.start = 3, .end = 7}, .flags = 0xa5a5};
}

template <class T>
std::vector<T> vector_of(std::span<const T> values) {
    return {values.begin(), values.end()};
}

// The descriptions of what each fields struct writes, and of what each reader
// reads

Field described(const Vec2& vec2) {
    return {Struct{{{vec2.x}, {vec2.y}}}};
}

void push_scalars(const Quad& quad, std::vector<Field>& scalars) {
    scalars.insert(scalars.end(), {{quad.a}, {quad.b}, {quad.c}, {quad.d}, {quad.e}, {quad.f}, {quad.g}, {quad.h}});
}

Field described(const Block& block) {
    Struct structure;
    for (const auto& quad : {block.first, block.second, block.third, block.fourth}) {
        push_scalars(quad, structure.scalars);
    }
    return {structure};
}

/// A table nested with `set_table()`, which has no header
Table nested(std::vector<Field> fields) {
    return {Layout::headerless(OffsetType::uint32), std::nullopt, std::move(fields)};
}

Field field_of(Table table) {
    return {TableField{{Source::builder, std::move(table)}, std::nullopt}};
}

Table describe(const RangeFields& range) {
    return nested({{range.start}, {range.end}});
}

Table describe(const FrameFields& frame) {
    return nested({described(frame.origin), field_of(describe(frame.range)), {frame.flags}});
}

Table read(const Range& range) {
    return nested({{range.start()}, {range.end()}});
}

Table read(const Frame& frame) {
    return nested({described(frame.origin()), field_of(read(frame.range())), {frame.flags()}});
}

Table describe(const TinyFields& fields) {
    return {{true, false, OffsetType::uint8}, std::nullopt, {{fields.id}, {fields.kind}}};
}

Table read(const Tiny& tiny) {
    return {{true, false, OffsetType::uint8}, std::nullopt, {{tiny.id()}, {tiny.kind()}}};
}

Table describe(const TaggedFields& fields) {
    return {{true, true, OffsetType::uint8}, 3, {{fields.id}, described(fields.position), {std::string(fields.label)}}};
}

Table read(const Tagged& tagged) {
    return {{true, true, OffsetType::uint8},
            3,
            {{tagged.id()}, described(tagged.position()), {std::string(tagged.label())}}};
}

Table describe(const HeaderFields& fields) {
    return {
        {false, false, OffsetType::uint8}, std::nullopt, {{fields.kind}, {fields.length}, {vector_of(fields.values)}}};
}

Table read(const Header& header) {
    const auto values = header.values();
    return {{false, false, OffsetType::uint8},
            std::nullopt,
            {{header.kind()}, {header.length()}, {std::vector<std::uint8_t>(values.begin(), values.end())}}};
}

Table describe(const PixelFields& fields) {
    return {{true, true, OffsetType::uint16}, 2, {described(fields.position), {fields.color}}};
}

Table read(const Pixel& pixel) {
    return {{true, true, OffsetType::uint16}, 2, {described(pixel.position()), {pixel.color()}}};
}

Table describe(const StripFields& fields) {
    return {{false, false, OffsetType::uint16},
            std::nullopt,
            {{fields.id}, described(fields.origin), {vector_of(fields.values)}}};
}

Table read(const Strip& strip) {
    return {{false, false, OffsetType::uint16},
            std::nullopt,
            {{strip.id()}, described(strip.origin()), {strip.values().to_vector()}}};
}

Table describe(const BoxFields& fields) {
    return {{false, true, OffsetType::uint32},
            1,
            {described(fields.min_), described(fields.max_), field_of(describe(fields.range))}};
}

Table read(const Box& box) {
    return {{false, true, OffsetType::uint32},
            1,
            {described(box.min_()), described(box.max_()), field_of(read(box.range()))}};
}

Table describe(const TrackFields& fields) {
    return {Layout{},
            std::nullopt,
            {{fields.id},
             described(fields.origin),
             field_of(describe(fields.frame)),
             {vector_of(fields.points)},
             {std::string(fields.name)}}};
}

Table read(const Track& track) {
    return {Layout{},
            std::nullopt,
            {{track.id()},
             described(track.origin()),
             field_of(read(track.frame())),
             {track.points().to_vector()},
             {std::string(track.name())}}};
}

Table describe(const LabelFields& fields) {
    return {Layout{}, std::nullopt, {{std::string(fields.text)}}};
}

Table read(const Label& label) {
    return {Layout{}, std::nullopt, {{std::string(label.text())}}};
}

Table describe(const SpanFields& fields) {
    VectorField ranges;
    for (const auto& range : fields.ranges) {
        ranges.items.push_back({Source::builder, describe(range)});
    }
    return {Layout{},
            std::nullopt,
            {{std::string(fields.name)},
             field_of(describe(fields.range)),
             field_of(describe(fields.frame)),
             {std::move(ranges)}}};
}

Table read(const Span& span) {
    VectorField ranges;
    for (const auto& range : span.ranges()) {
        ranges.items.push_back({Source::builder, read(range)});
    }
    return {
        Layout{},
        std::nullopt,
        {{std::string(span.name())}, field_of(read(span.range())), field_of(read(span.frame())), {std::move(ranges)}}};
}

Table describe(const OverflowFields& fields) {
    return {{false, false, OffsetType::uint8}, std::nullopt, {{fields.id}, described(fields.block)}};
}

Table read(const Overflow& overflow) {
    return {{false, false, OffsetType::uint8}, std::nullopt, {{overflow.id()}, described(overflow.block())}};
}

// Generators of the fields, whose strings and arrays the tests own

rc::Gen<Vec2> vec2s() {
    return rc::gen::construct<Vec2>(strategy::float32(), strategy::float32());
}

rc::Gen<Quad> quads() {
    const auto float64 = strategy::float64();
    return rc::gen::construct<Quad>(float64, float64, float64, float64, float64, float64, float64, float64);
}

rc::Gen<RangeFields> ranges() {
    return rc::gen::construct<RangeFields>(values::integers<std::uint32_t>(), values::integers<std::uint32_t>());
}

rc::Gen<FrameFields> frames() {
    return rc::gen::construct<FrameFields>(vec2s(), ranges(), values::integers<std::uint16_t>());
}

/// Strings of characters of every length in UTF-8, and strings of letters
/// about as long as a `Tagged`'s `uint8` offsets address after its fixed
/// fields, 237 bytes, and longer
rc::Gen<std::string> labels() {
    const auto letters = [](std::size_t length) {
        return rc::gen::container<std::string>(length, rc::gen::inRange('a', static_cast<char>('z' + 1)));
    };
    return rc::gen::oneOf(values::strings(), rc::gen::mapcat(rc::gen::inRange<std::size_t>(230, 246), letters));
}

/// What writing returned: the bytes, or the kind, index, length and offset
/// type of the `BuildError` that it threw
using Written =
    std::variant<std::vector<std::uint8_t>, std::tuple<BuildError::Kind, std::size_t, std::uint64_t, OffsetType>>;

template <class Write>
Written written(Write write) {
    try {
        return write().to_vector();
    } catch (const BuildError& error) {
        return std::tuple(error.kind(), error.index(), error.length(), error.offset_type());
    }
}

/// The message's fields are written as `TableBuilder` writes their
/// description, into new bytes and into given ones, and read back as it, or
/// fail to write as it fails
template <class Message, class Fields>
void check_written(const Fields& fields) {
    const auto expected = describe(fields);
    const auto bytes = written([&] { return fields.serialize(); });

    RC_ASSERT(bytes == written([&] { return to_bytes(expected); }));
    if (const auto* table = std::get_if<std::vector<std::uint8_t>>(&bytes)) {
        RC_ASSERT(read(Message(*table)) == expected);
        RC_ASSERT(written_into_any_bytes(fields, *table));
    }

    // Into bytes longer than any of the messages, it fails alike
    std::vector<std::uint8_t> given(1U << 16U);
    RC_ASSERT(bytes == written([&] { return Bytes::copy_of(std::span(given).first(fields.serialize_into(given))); }));
}

RC_GTEST_PROP(GeneratedFixedFields, MessagesWrittenWholeAreWrittenAsTableBuilderWritesThemAndReadBack, ()) {
    const auto label = *labels();
    const auto text = *values::strings();

    check_written<Tiny>(
        TinyFields{.id = *values::integers<std::uint16_t>(), .kind = *values::integers<std::uint8_t>()});
    check_written<Tagged>(TaggedFields{.id = *values::integers<std::uint32_t>(), .position = *vec2s(), .label = label});
    check_written<Pixel>(PixelFields{.position = *vec2s(), .color = *values::integers<std::uint32_t>()});
    check_written<Box>(BoxFields{.min_ = *vec2s(), .max_ = *vec2s(), .range = *ranges()});
    check_written<Label>(LabelFields{.text = text});
}

RC_GTEST_PROP(GeneratedFixedFields, MessagesOfFixedFieldsAndOthersAreWrittenAsTableBuilderWritesThemAndReadBack, ()) {
    const auto header_values = *values::arrays(values::integers<std::uint8_t>());
    const auto strip_values = *values::arrays(values::integers<std::uint16_t>());
    const auto points = *values::arrays(strategy::float32());
    const auto name = *values::strings();
    const auto span_ranges = *values::arrays(ranges(), 8);

    check_written<Header>(HeaderFields{.kind = *values::integers<std::uint8_t>(),
                                       .length = *values::integers<std::uint16_t>(),
                                       .values = header_values});
    check_written<Strip>(
        StripFields{.id = *values::integers<std::uint32_t>(), .origin = *vec2s(), .values = strip_values});
    check_written<Track>(TrackFields{.id = *values::integers<std::uint32_t>(),
                                     .origin = *vec2s(),
                                     .frame = *frames(),
                                     .points = points,
                                     .name = name});
    check_written<Span>(SpanFields{.name = name, .range = *ranges(), .frame = *frames(), .ranges = span_ranges});
    check_written<Overflow>(
        OverflowFields{.id = *values::integers<std::uint8_t>(),
                       .block = {.first = *quads(), .second = *quads(), .third = *quads(), .fourth = *quads()}});
}

/// A table of only fixed fields writes, at any address, the `SIZE` bytes that
/// `TableBuilder` writes for it, which it nests in its parent as they are
RC_GTEST_PROP(GeneratedFixedFields, FixedTablesWriteTheBytesThatTableBuilderNests, ()) {
    const auto range = *ranges();
    const auto frame = *frames();
    const auto offset = *rc::gen::inRange<std::size_t>(0, 8);

    std::vector<std::uint8_t> range_bytes(offset + RangeFields::SIZE);
    range.to_bytes(range_bytes.data() + offset);
    RC_ASSERT(std::vector(range_bytes.begin() + static_cast<std::ptrdiff_t>(offset), range_bytes.end()) ==
              to_bytes(describe(range)).to_vector());

    std::vector<std::uint8_t> frame_bytes(offset + FrameFields::SIZE);
    frame.to_bytes(frame_bytes.data() + offset);
    RC_ASSERT(std::vector(frame_bytes.begin() + static_cast<std::ptrdiff_t>(offset), frame_bytes.end()) ==
              to_bytes(describe(frame)).to_vector());
}

TEST(GeneratedFixedFields, WholeMessagesOfUint8OffsetsReadBackTheirFields) {
    const auto tiny = TinyFields{.id = 513, .kind = 9}.serialize();
    EXPECT_EQ(tiny.size(), 6U);
    const Tiny read_tiny(tiny);
    EXPECT_EQ(read_tiny.id(), 513U);
    EXPECT_EQ(read_tiny.kind(), 9U);

    const auto max = std::numeric_limits<std::uint32_t>::max();
    const auto tagged = TaggedFields{.id = max, .position = kOrigin, .label = "a \U0001F600"}.serialize();
    const Tagged read_tagged(tagged);
    EXPECT_EQ(Tagged::version_of(tagged), 3U);
    EXPECT_EQ(read_tagged.id(), max);
    EXPECT_EQ(std::bit_cast<std::uint32_t>(read_tagged.position().y), std::bit_cast<std::uint32_t>(-0.0F));
    EXPECT_EQ(read_tagged.label(), "a \U0001F600");
}

TEST(GeneratedFixedFields, RunsOfFixedFieldsReadBackWithTheFieldsAfterThem) {
    const std::vector<std::uint8_t> header_values{1, 2, 3};
    const auto header = HeaderFields{.kind = 1, .length = 300, .values = header_values}.serialize();
    const Header read_header(header);
    EXPECT_EQ(read_header.kind(), 1U);
    EXPECT_EQ(read_header.length(), 300U);
    EXPECT_EQ(std::vector(read_header.values().begin(), read_header.values().end()), header_values);

    const std::vector<std::uint16_t> strip_values{1, std::numeric_limits<std::uint16_t>::max()};
    const auto strip = StripFields{.id = 7, .origin = kOrigin, .values = strip_values}.serialize();
    const Strip read_strip(strip);
    EXPECT_EQ(read_strip.id(), 7U);
    EXPECT_EQ(read_strip.origin(), kOrigin);
    EXPECT_EQ(read_strip.values().to_vector(), strip_values);

    const std::vector<float> points{0.5F, 1.5F};
    const auto track =
        TrackFields{.id = 8, .origin = kOrigin, .frame = frame(), .points = points, .name = "track"}.serialize();
    const Track read_track(track);
    const auto read_frame = read_track.frame();
    EXPECT_EQ(read_track.id(), 8U);
    EXPECT_EQ(read_track.origin(), kOrigin);
    EXPECT_EQ(read_frame.origin(), kOrigin);
    EXPECT_EQ(read_frame.flags(), 0xa5a5U);
    EXPECT_EQ(read_frame.range().start(), 3U);
    EXPECT_EQ(read_frame.range().end(), 7U);
    EXPECT_EQ(read_track.points().to_vector(), points);
    EXPECT_EQ(read_track.name(), "track");
}

TEST(GeneratedFixedFields, WholeMessagesOfUint16AndUint32OffsetsReadBackTheirFields) {
    const auto pixel = PixelFields{.position = kOrigin, .color = 0x01020304}.serialize();
    const Pixel read_pixel(pixel);
    EXPECT_EQ(Pixel::version_of(pixel), 2U);
    EXPECT_EQ(read_pixel.position(), kOrigin);
    EXPECT_EQ(read_pixel.color(), 0x01020304U);

    constexpr Vec2 corner{.x = 2.0F, .y = 3.0F};
    const auto box = BoxFields{.min_ = kOrigin, .max_ = corner, .range = {.start = 1, .end = 2}}.serialize();
    const Box read_box(box);
    EXPECT_EQ(Box::version_of(box), 1U);
    EXPECT_EQ(read_box.min_(), kOrigin);
    EXPECT_EQ(read_box.max_(), corner);
    EXPECT_EQ(read_box.range().start(), 1U);
    EXPECT_EQ(read_box.range().end(), 2U);

    const auto label = LabelFields{.text = ""}.serialize();
    EXPECT_EQ(Label(label).text(), "");
}

TEST(GeneratedFixedFields, FixedTablesAfterOtherFieldsReadBackAloneAndInAVector) {
    const std::vector<RangeFields> ranges{{.start = 1, .end = 2}, {.start = 3, .end = 4}};
    const auto span =
        SpanFields{.name = "span", .range = {.start = 5, .end = 6}, .frame = frame(), .ranges = ranges}.serialize();

    const Span read(span);
    EXPECT_EQ(read.name(), "span");
    EXPECT_EQ(read.range().start(), 5U);
    EXPECT_EQ(read.range().end(), 6U);
    EXPECT_EQ(read.frame().flags(), 0xa5a5U);

    std::vector<std::pair<std::uint32_t, std::uint32_t>> items;
    for (const auto& range : read.ranges()) {
        items.emplace_back(range.start(), range.end());
    }
    const std::vector<std::pair<std::uint32_t, std::uint32_t>> expected{{1, 2}, {3, 4}};
    EXPECT_EQ(items, expected);

    const auto view = read.ranges();
    ASSERT_EQ(view.size(), 2U);
    EXPECT_EQ(view.get(1).start(), 3U);
    EXPECT_EQ(view.get(1).end(), 4U);
}

TEST(GeneratedFixedFields, AMessageWrittenWholeHoldsAStringUpToTheLongestThatItsOffsetsAddress) {
    // The header, the offsets, the id, the position and the label's count
    // take 18 of the 255 bytes that `uint8` offsets address
    const std::string longest(237, 'a');
    const TaggedFields fields{.id = 1, .position = kOrigin, .label = longest};
    const auto tagged = fields.serialize();
    EXPECT_EQ(tagged.size(), 255U);
    EXPECT_EQ(tagged[0], 255U);
    EXPECT_EQ(Tagged(tagged).label(), longest);
    EXPECT_TRUE(written_into(fields, tagged.span(), 1, 0xa5));
    EXPECT_TRUE(rejects_too_short(fields, tagged.size(), 0xa5));

    const std::string longer(238, 'a');
    const TaggedFields too_long{.id = 1, .position = kOrigin, .label = longer};
    try {
        static_cast<void>(too_long.serialize());
        ADD_FAILURE() << "A table too long for its offsets was written";
    } catch (const BuildError& error) {
        EXPECT_EQ(error.kind(), BuildError::Kind::too_long);
        EXPECT_EQ(error.index(), 2U);
        EXPECT_EQ(error.length(), 256U);
        EXPECT_EQ(error.offset_type(), OffsetType::uint8);
    }

    // Into bytes that would hold it, it fails alike
    std::vector<std::uint8_t> given(256);
    try {
        static_cast<void>(too_long.serialize_into(given));
        ADD_FAILURE() << "A table too long for its offsets was written into given bytes";
    } catch (const BuildError& error) {
        EXPECT_EQ(error.kind(), BuildError::Kind::too_long);
        EXPECT_EQ(error.length(), 256U);
    }
}

TEST(GeneratedFixedFields, AMessageWhoseFixedFieldsAreTooLongForItsOffsetsFailsToWrite) {
    constexpr Quad quad{.a = 1, .b = 2, .c = 3, .d = 4, .e = 5, .f = 6, .g = 7, .h = 8};
    const OverflowFields fields{.id = 1, .block = {.first = quad, .second = quad, .third = quad, .fourth = quad}};

    // The offsets, the id and then the block's 256 bytes
    try {
        static_cast<void>(fields.serialize());
        ADD_FAILURE() << "A table too long for its offsets was written";
    } catch (const BuildError& error) {
        EXPECT_EQ(error.kind(), BuildError::Kind::too_long);
        EXPECT_EQ(error.index(), 1U);
        EXPECT_EQ(error.length(), 259U);
        EXPECT_EQ(error.offset_type(), OffsetType::uint8);
    }

    // A table that something else wrote reads as values or decode errors
    const std::vector<std::uint8_t> other{2, 3, 1};
    const Overflow read(other);
    EXPECT_EQ(read.id(), 1U);
    EXPECT_THROW(static_cast<void>(read.block()), DecodeError);
}

}  // namespace
}  // namespace tafla::test
