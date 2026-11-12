// The code generated from `schemas/fixed_fields.tafla`, whose fields' sizes
// the schema fixes, reads back what it writes in every offset type and header,
// and fails to write a message whose fixed fields alone are too long for its
// offset type, as `TableBuilder` fails. The Go tests run the same schema, whose
// generated code lays these fields out itself.

#include <gtest/gtest.h>

#include <bit>
#include <cstdint>
#include <limits>
#include <string_view>
#include <utility>
#include <vector>

#include "fixed_fields/fixed_fields.hpp"

namespace tafla::test {
namespace {

using namespace fixed_fields;

constexpr Vec2 kOrigin{.x = 1.5F, .y = -0.0F};

FrameFields frame() {
    return {.origin = kOrigin, .range = {.start = 3, .end = 7}, .flags = 0xa5a5};
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

    const auto view = read.ranges_view();
    ASSERT_EQ(view.size(), 2U);
    EXPECT_EQ(view.get(1).start(), 3U);
    EXPECT_EQ(view.get(1).end(), 4U);
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
