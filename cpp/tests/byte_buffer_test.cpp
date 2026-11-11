// `ByteBuffer` reads every value where the format puts it, exactly when it
// lies within the bytes, and fails with `DecodeError` otherwise, at any
// position, in every offset type and header

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <cstddef>
#include <cstdint>
#include <optional>
#include <vector>

#include <tafla/tafla.hpp>

namespace tafla::test {
namespace {

/// The sum of positions, which saturates rather than wrapping around, as a
/// position past any bytes
std::uint64_t plus(std::uint64_t left, std::uint64_t right) {
    return right > UINT64_MAX - left ? UINT64_MAX : left + right;
}

/// The little-endian unsigned integer of `width` bytes at `position`, if they
/// lie within the bytes, read a byte at a time, as the format defines it
std::optional<std::uint64_t> little_endian(const std::vector<std::uint8_t>& bytes, std::uint64_t position,
                                           std::uint64_t width) {
    if (position > bytes.size() || width > bytes.size() - position) {
        return std::nullopt;
    }

    std::uint64_t value = 0;
    for (std::uint64_t byte = width; byte > 0; --byte) {
        value = value << 8 | bytes[position + byte - 1];
    }
    return value;
}

template <class Read>
std::optional<decltype(std::declval<Read>()())> try_read(Read read) {
    try {
        return read();
    } catch (const DecodeError& error) {
        EXPECT_EQ(error.kind(), DecodeError::Kind::out_of_range);
        return std::nullopt;
    }
}

template <class T, class Get>
void check_scalar(const std::vector<std::uint8_t>& bytes, std::size_t position, Get get) {
    const auto expected = little_endian(bytes, position, sizeof(T));
    const auto read = try_read([&] { return get(position); });

    RC_ASSERT(read.has_value() == expected.has_value());
    if (read) {
        RC_ASSERT(std::bit_cast<detail::BitsOf<T>>(*read) == static_cast<detail::BitsOf<T>>(*expected));
    }
}

template <class T, class Get>
void check_array(const std::vector<std::uint8_t>& bytes, std::size_t position, std::uint64_t width, Get get) {
    const auto count = little_endian(bytes, position, width);
    std::optional<std::uint64_t> end;
    if (count) {
        const auto start = plus(position, width);
        end = little_endian(bytes, start, 0).has_value() && *count * sizeof(T) <= bytes.size() - start
                  ? std::optional(start + *count * sizeof(T))
                  : std::nullopt;
    }

    const auto read = try_read([&] { return get(position); });
    RC_ASSERT(read.has_value() == end.has_value());
    if (read) {
        RC_ASSERT(read->size() == *count);
        for (std::size_t index = 0; index < read->size(); ++index) {
            const auto element = little_endian(bytes, plus(position, width) + index * sizeof(T), sizeof(T));
            RC_ASSERT(std::bit_cast<detail::BitsOf<T>>((*read)[index]) == static_cast<detail::BitsOf<T>>(*element));
        }
    }
}

template <bool Versioned>
void check_reads(const std::vector<std::uint8_t>& bytes, ByteBuffer<Versioned> buffer, std::size_t position) {
    const std::uint64_t width = buffer.layout().offset_size();

    check_scalar<std::int8_t>(bytes, position, [&](std::size_t at) { return buffer.get_int8(at); });
    check_scalar<std::uint8_t>(bytes, position, [&](std::size_t at) { return buffer.get_uint8(at); });
    check_scalar<std::int16_t>(bytes, position, [&](std::size_t at) { return buffer.get_int16(at); });
    check_scalar<std::uint16_t>(bytes, position, [&](std::size_t at) { return buffer.get_uint16(at); });
    check_scalar<std::int32_t>(bytes, position, [&](std::size_t at) { return buffer.get_int32(at); });
    check_scalar<std::uint32_t>(bytes, position, [&](std::size_t at) { return buffer.get_uint32(at); });
    check_scalar<std::int64_t>(bytes, position, [&](std::size_t at) { return buffer.get_int64(at); });
    check_scalar<std::uint64_t>(bytes, position, [&](std::size_t at) { return buffer.get_uint64(at); });
    check_scalar<float>(bytes, position, [&](std::size_t at) { return buffer.get_float32(at); });
    check_scalar<double>(bytes, position, [&](std::size_t at) { return buffer.get_float64(at); });

    check_array<std::int8_t>(bytes, position, width, [&](std::size_t at) { return buffer.get_int8_array(at); });
    check_array<std::uint8_t>(bytes, position, width, [&](std::size_t at) { return buffer.get_uint8_array(at); });
    check_array<std::int16_t>(bytes, position, width, [&](std::size_t at) { return buffer.get_int16_array(at); });
    check_array<std::uint16_t>(bytes, position, width, [&](std::size_t at) { return buffer.get_uint16_array(at); });
    check_array<std::int32_t>(bytes, position, width, [&](std::size_t at) { return buffer.get_int32_array(at); });
    check_array<std::uint32_t>(bytes, position, width, [&](std::size_t at) { return buffer.get_uint32_array(at); });
    check_array<std::int64_t>(bytes, position, width, [&](std::size_t at) { return buffer.get_int64_array(at); });
    check_array<std::uint64_t>(bytes, position, width, [&](std::size_t at) { return buffer.get_uint64_array(at); });
    check_array<float>(bytes, position, width, [&](std::size_t at) { return buffer.get_float32_array(at); });
    check_array<double>(bytes, position, width, [&](std::size_t at) { return buffer.get_float64_array(at); });

    // Offsets lie where the header and the earlier offsets end
    const std::uint64_t header = buffer.layout().header_size();
    const auto offset =
        little_endian(bytes, position > UINT64_MAX / width ? UINT64_MAX : plus(header, position * width), width);
    const auto root = try_read([&] { return buffer.root_offset(position); });
    RC_ASSERT(root == (offset ? std::optional<std::size_t>(*offset) : std::nullopt));

    const auto slot = little_endian(bytes, plus(position, width), width);
    const auto field = try_read([&] { return buffer.field_offset(position, 1); });
    RC_ASSERT(field == (slot ? std::optional<std::size_t>(position + *slot) : std::nullopt));

    // A vector holds a count and then that many ends
    const auto count = little_endian(bytes, position, width);
    const auto vector = try_read([&] { return buffer.get_vector(position); });
    const bool ends_fit = count && little_endian(bytes, plus(position, width), *count * width).has_value();
    RC_ASSERT(vector.has_value() == ends_fit);
}

/// Every getter reads arbitrary bytes at every position, past the end too,
/// in every offset type and header, as the bytes that lie where the format
/// puts them, exactly when they lie within the bytes
RC_GTEST_PROP(ByteBuffer, ReadsTheBytesWhereTheFormatPutsThemOrFails, ()) {
    const auto bytes =
        *rc::gen::resize(rc::kNominalSize, rc::gen::mapcat(rc::gen::inRange(0, 48), [](int count) {
                             return rc::gen::container<std::vector<std::uint8_t>>(
                                 static_cast<std::size_t>(count),
                                 rc::gen::weightedOneOf<std::uint8_t>({{3, rc::gen::inRange<std::uint8_t>(0, 8)},
                                                                       {1, rc::gen::arbitrary<std::uint8_t>()}}));
                         }));
    const auto offset_type = *rc::gen::element(OffsetType::uint8, OffsetType::uint16, OffsetType::uint32);
    const auto sized = *rc::gen::arbitrary<bool>();

    auto layout = TableLayout().with_offset_type(offset_type);
    if (sized) {
        layout = layout.sized();
    }

    for (std::size_t position = 0; position <= bytes.size() + 8; ++position) {
        check_reads(bytes, ByteBuffer(bytes, layout), position);
        check_reads(bytes, ByteBuffer(bytes, layout.versioned()), position);
    }
    check_reads(bytes, ByteBuffer(bytes, layout), SIZE_MAX);
    check_reads(bytes, ByteBuffer(bytes, layout), SIZE_MAX - 3);

    // The header's slots
    const auto width = offset_size(offset_type);
    const auto size = try_read([&] { return ByteBuffer(bytes, layout).size(); });
    RC_ASSERT(size ==
              (sized ? (little_endian(bytes, 0, width) ? std::optional<std::size_t>(*little_endian(bytes, 0, width))
                                                       : std::nullopt)
                     : std::optional<std::size_t>(bytes.size())));

    const auto version = try_read([&] { return ByteBuffer(bytes, layout.versioned()).version(); });
    const auto expected = little_endian(bytes, sized ? width : 0, width);
    RC_ASSERT(version ==
              (expected ? std::optional<std::uint32_t>(static_cast<std::uint32_t>(*expected)) : std::nullopt));
}

TEST(ByteBuffer, ReadsAnEmptyBufferAsOutOfRange) {
    const ByteBuffer buffer{std::span<const std::uint8_t>(), TableLayout()};
    EXPECT_EQ(buffer.size(), 0U);
    EXPECT_THROW(static_cast<void>(buffer.root_offset(0)), DecodeError);
    EXPECT_THROW(static_cast<void>(buffer.get_uint8(0)), DecodeError);
    EXPECT_THROW(static_cast<void>(buffer.get_string(0)), DecodeError);
}

TEST(ByteBuffer, IsALiteralOfAConstantLayout) {
    static constexpr std::uint8_t bytes[] = {4, 0, 0, 0, 42};
    constexpr ByteBuffer buffer{bytes, TableLayout()};
    static_assert(!buffer.layout().is_sized());
    EXPECT_EQ(buffer.get_uint8(buffer.root_offset(0)), 42);
}

}  // namespace
}  // namespace tafla::test
