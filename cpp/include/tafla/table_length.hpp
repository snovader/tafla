// How TableBuilder and TableSizer count a table's length.

#ifndef TAFLA_TABLE_LENGTH_HPP
#define TAFLA_TABLE_LENGTH_HPP

#include <cstddef>
#include <cstdint>

#include "element.hpp"
#include "errors.hpp"
#include "layout.hpp"

namespace tafla::detail {

// Lengths are counted in 64 bits, so that they don't wrap at the limit of
// `uint32` offsets in a 32-bit size_t, and saturate at UINT64_MAX, far past
// the longest table, so that no count or size of a field wraps them. A table
// is too long for its offsets when its length is more than `max_value()` of
// its offset type, which a saturated length always is.

/// `left + right`, or UINT64_MAX if it's more
TAFLA_ALWAYS_INLINE constexpr std::uint64_t saturating_sum(std::uint64_t left, std::uint64_t right) noexcept {
    return right > UINT64_MAX - left ? UINT64_MAX : left + right;
}

/// `left * right`, or UINT64_MAX if it's more
TAFLA_ALWAYS_INLINE constexpr std::uint64_t saturating_product(std::uint64_t left, std::uint64_t right) noexcept {
    return left != 0 && right > UINT64_MAX / left ? UINT64_MAX : left * right;
}

/// The bytes of an array field of `byte_length` bytes of elements, at
/// `offset` from the start of its table: its count of `width` bytes, the
/// padding that aligns its elements to `alignment` from the table's start,
/// and its elements
TAFLA_ALWAYS_INLINE constexpr std::uint64_t array_bytes(std::uint64_t offset, std::uint64_t width,
                                                        std::uint64_t byte_length, Alignment alignment) noexcept {
    // The padding depends only on the remainder, which a sum that wraps
    // keeps, and a field at such an offset saturates the table's length
    const auto padding = detail::padding(offset + width, alignment);
    return saturating_sum(width + padding, byte_length);
}

/// The bytes of a vector of `count` tables before its items: its count and
/// the end of each item, of `width` bytes each
TAFLA_ALWAYS_INLINE constexpr std::uint64_t vector_bytes(std::uint64_t count, std::uint64_t width) noexcept {
    return saturating_product(saturating_sum(count, 1), width);
}

/// The byte length of the header of a table of `fields` fields in a layout:
/// its slots and an offset per field, which `TableBuilder` and `TableSizer`
/// start a table with.
///
/// Throws a `BuildError` if the header is longer than the layout's offsets
/// address.
inline std::uint32_t header_length(std::size_t fields, bool sized, bool versioned, OffsetType offset_type) {
    const std::uint64_t width = offset_size(offset_type);
    const std::uint64_t slots = (sized ? 1U : 0U) + (versioned ? 1U : 0U);
    const std::uint64_t length = saturating_product(saturating_sum(fields, slots), width);

    if (length > max_value(offset_type)) {
        throw BuildError::header_too_long(fields, offset_type);
    }
    return static_cast<std::uint32_t>(length);
}

}  // namespace tafla::detail

#endif
