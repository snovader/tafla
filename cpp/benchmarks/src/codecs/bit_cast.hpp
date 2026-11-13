// The fixed-size record copied as it is, with std::bit_cast

#ifndef TAFLA_BENCHMARKS_CODECS_BIT_CAST_HPP
#define TAFLA_BENCHMARKS_CODECS_BIT_CAST_HPP

#include <array>
#include <bit>
#include <cstdint>
#include <cstring>
#include <span>

#include "../codec.hpp"

namespace tafla::benchmarks {

static_assert(sizeof(Sample) == 12 && std::is_trivially_copyable_v<Sample>, "A sample is its 12 bytes");

/// The fixed-size record copied as it is, with `std::bit_cast`: C++'s closest
/// counterpart of the record, whose fields lie at fixed offsets without
/// padding, in the host's byte order. It writes and reads the format's
/// little-endian bytes only on a little-endian host, where tafla's
/// `store_le()` and `load_le()` compile to the same stores and loads, so the
/// comparison leaves it out on a big-endian host.
struct BitCast {
    static constexpr std::string_view name = "bit_cast";

    [[nodiscard]] std::array<std::uint8_t, 12> serialize(SampleShape, const Sample& input) const {
        return std::bit_cast<std::array<std::uint8_t, 12>>(input);
    }

    [[nodiscard]] Sample deserialize(SampleShape, std::span<const std::uint8_t> bytes) const {
        if (bytes.size() < sizeof(Sample)) {
            throw CodecError("The record is shorter than 12 bytes");
        }

        std::array<std::uint8_t, 12> record;
        std::memcpy(record.data(), bytes.data(), record.size());
        return std::bit_cast<Sample>(record);
    }
};

}  // namespace tafla::benchmarks

#endif
