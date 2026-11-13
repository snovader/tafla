// Copies of tables at any offset from an aligned address

#ifndef TAFLA_TEST_AT_BYTE_OFFSET_HPP
#define TAFLA_TEST_AT_BYTE_OFFSET_HPP

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <span>
#include <vector>

namespace tafla::test {

/// A copy of bytes that starts `byte_offset` bytes past a multiple of 8 in a
/// larger buffer of 0xff. Its arrays are aligned in memory when `byte_offset`
/// is a multiple of 8.
class AtByteOffset {
public:
    AtByteOffset(std::span<const std::uint8_t> bytes, std::size_t byte_offset)
        : backing_(std::make_unique<std::uint64_t[]>((byte_offset + bytes.size()) / 8 + 2)), size_(bytes.size()) {
        auto* start = reinterpret_cast<std::uint8_t*>(backing_.get());
        std::fill_n(start, ((byte_offset + bytes.size()) / 8 + 2) * 8, std::uint8_t{0xff});

        start_ = start + byte_offset;
        std::copy(bytes.begin(), bytes.end(), start_);
    }

    [[nodiscard]] std::span<const std::uint8_t> bytes() const {
        return {start_, size_};
    }

private:
    std::unique_ptr<std::uint64_t[]> backing_;
    std::uint8_t* start_;
    std::size_t size_;
};

}  // namespace tafla::test

#endif
