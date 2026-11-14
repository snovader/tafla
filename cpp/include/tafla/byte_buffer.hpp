// Reads a table's fields from bytes, checking every read.

#ifndef TAFLA_BYTE_BUFFER_HPP
#define TAFLA_BYTE_BUFFER_HPP

#include <cstddef>
#include <cstdint>
#include <span>
#include <string_view>

#include "array_view.hpp"
#include "element.hpp"
#include "errors.hpp"
#include "layout.hpp"
#include "utf8.hpp"
#include "vector.hpp"

namespace tafla {

/// Reads a table from the supplied bytes, in the layout it was written with.
///
/// Fields are found by index with `root_offset()`, or `field_offset()` for a
/// table nested in another, and then read at that position by their type's
/// getter.
///
/// Every read checks that it stays within the bytes and throws a
/// `DecodeError` otherwise, so no bytes, however corrupted, make a read
/// crash or return values from outside them. Strings must be valid UTF-8.
///
/// Arrays, strings, structs and vector items are read in place, as views of
/// the bytes rather than copies, so they're valid for as long as the bytes
/// are. A `ByteBuffer` is two pointers and its layout, which is cheap to
/// make for each read: with a layout that's a constant, as tafla-gen's code
/// makes it, the compiler reads offsets of that type without branching on
/// it.
template <bool Versioned = false>
class ByteBuffer {
public:
    constexpr ByteBuffer(std::span<const std::uint8_t> bytes, TableLayout<Versioned> layout) noexcept
        : data_(bytes.data()), size_(bytes.size()), layout_(layout) {}

    [[nodiscard]] constexpr std::span<const std::uint8_t> bytes() const noexcept {
        return {data_, size_};
    }

    [[nodiscard]] constexpr TableLayout<Versioned> layout() const noexcept {
        return layout_;
    }

    /// Returns the table's byte length from its header when the layout is
    /// sized, and otherwise the length of the bytes.
    ///
    /// Throws a `DecodeError` if the header doesn't lie within the bytes.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t size() const {
        return layout_.is_sized() ? read_size(0) : size_;
    }

    /// Returns the version that the table's header carries after its length.
    /// It's as wide as the header's other slots.
    ///
    /// Throws a `DecodeError` if the header doesn't lie within the bytes.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint32_t version() const
        requires Versioned
    {
        const std::size_t position = layout_.is_sized() ? layout_.offset_size() : 0;
        return static_cast<std::uint32_t>(read_size(position));
    }

    /// Returns the position of field `index` of the root table.
    ///
    /// Throws a `DecodeError` if the field's offset doesn't lie within the
    /// bytes.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t root_offset(std::size_t index) const {
        return read_size(slot(layout_.header_size(), index));
    }

    /// Returns the position of field `index` of a nested table, which has no
    /// header and stores its field offsets from its own start.
    ///
    /// `table_position` is the nested table's position, which is the
    /// position of the field that holds it.
    ///
    /// Throws a `DecodeError` if the field's offset doesn't lie within the
    /// bytes.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t field_offset(std::size_t table_position, std::size_t index) const {
        const std::size_t offset = read_size(slot(table_position, index));

        // The slot lies within the bytes, so only a 32-bit host can
        // overflow, at a position past any bytes
        if constexpr (sizeof(std::size_t) < sizeof(std::uint64_t)) {
            if (offset > SIZE_MAX - table_position) [[unlikely]] {
                detail::throw_out_of_range(table_position, offset, size_);
            }
        }
        return table_position + offset;
    }

    /// Reads the `std::int8_t` at `position`, or throws a `DecodeError` if
    /// it doesn't lie within the bytes. So do the other scalars' getters.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::int8_t get_int8(std::size_t position) const {
        return get<std::int8_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint8_t get_uint8(std::size_t position) const {
        return get<std::uint8_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE std::int16_t get_int16(std::size_t position) const {
        return get<std::int16_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint16_t get_uint16(std::size_t position) const {
        return get<std::uint16_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE std::int32_t get_int32(std::size_t position) const {
        return get<std::int32_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint32_t get_uint32(std::size_t position) const {
        return get<std::uint32_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE std::int64_t get_int64(std::size_t position) const {
        return get<std::int64_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint64_t get_uint64(std::size_t position) const {
        return get<std::uint64_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE float get_float32(std::size_t position) const {
        return get<float>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE double get_float64(std::size_t position) const {
        return get<double>(position);
    }

    /// Reads the array of `std::int8_t` at `position`, or throws a
    /// `DecodeError` if it doesn't lie within the bytes. So do the other
    /// arrays' getters.
    [[nodiscard]] TAFLA_ALWAYS_INLINE ArrayView<std::int8_t> get_int8_array(std::size_t position) const {
        return get_array<std::int8_t>(position);
    }

    /// Reads the array of bytes at `position`, which are always in place
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::span<const std::uint8_t> get_uint8_array(std::size_t position) const {
        const auto array = get_array<std::uint8_t>(position);
        return array.bytes();
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE ArrayView<std::int16_t> get_int16_array(std::size_t position) const {
        return get_array<std::int16_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE ArrayView<std::uint16_t> get_uint16_array(std::size_t position) const {
        return get_array<std::uint16_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE ArrayView<std::int32_t> get_int32_array(std::size_t position) const {
        return get_array<std::int32_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE ArrayView<std::uint32_t> get_uint32_array(std::size_t position) const {
        return get_array<std::uint32_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE ArrayView<std::int64_t> get_int64_array(std::size_t position) const {
        return get_array<std::int64_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE ArrayView<std::uint64_t> get_uint64_array(std::size_t position) const {
        return get_array<std::uint64_t>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE ArrayView<float> get_float32_array(std::size_t position) const {
        return get_array<float>(position);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE ArrayView<double> get_float64_array(std::size_t position) const {
        return get_array<double>(position);
    }

    /// Reads the string at `position`, which must be valid UTF-8. A leading
    /// U+FEFF is part of the string rather than a byte order mark.
    ///
    /// Throws a `DecodeError` if the string doesn't lie within the bytes, or
    /// isn't valid UTF-8.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::string_view get_string(std::size_t position) const {
        const auto bytes = get_array<std::uint8_t>(position).bytes();
        if (!detail::valid_utf8(bytes.data(), bytes.size())) [[unlikely]] {
            throw_invalid_utf8(position);
        }

        return {reinterpret_cast<const char*>(bytes.data()), bytes.size()};
    }

    /// Returns the bytes from the table at `position` to the end of the
    /// bytes, which a table nested in a field is read from in place, at
    /// position 0, with a `ByteBuffer` of its parent's offset type, as a
    /// vector's item is from `Vector::get_in_place()`. Its fields are checked
    /// when they're read.
    ///
    /// Throws a `DecodeError` if `position` lies past the bytes.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::span<const std::uint8_t> get_table(std::size_t position) const {
        return {range(position, 0), size_ - position};
    }

    /// Reads the `size` bytes of the struct at `position`, whose scalars lie
    /// at fixed offsets from its start.
    ///
    /// Throws a `DecodeError` if the struct doesn't lie within the bytes.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::span<const std::uint8_t> get_struct(std::size_t position,
                                                                               std::size_t size) const {
        return {range(position, size), size};
    }

    /// Reads the bytes of the structs of the array at `position`, each of
    /// `size` bytes, one after another after the count. Their number is the
    /// bytes' length over `size`. Structs of 0 bytes take none, so a `size`
    /// of 0 returns no bytes, whatever the count.
    ///
    /// Throws a `DecodeError` if the array doesn't lie within the bytes.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::span<const std::uint8_t> get_struct_array(std::size_t position,
                                                                                     std::size_t size) const {
        const std::size_t count = read_size(position);

        // A struct can be as large as a size_t, and a byte length past
        // 2^64 lies past any bytes, so the product saturates
        const std::uint64_t length =
            count != 0 && size > UINT64_MAX / count ? UINT64_MAX : static_cast<std::uint64_t>(count) * size;
        const std::size_t start = position + layout_.offset_size();

        return {range(start, length), static_cast<std::size_t>(length)};
    }

    /// Reads the vector at `position`, checking that its count and the ends
    /// of its items lie within the bytes. Each item is checked when it's
    /// read.
    ///
    /// Throws a `DecodeError` if the count or the ends don't lie within the
    /// bytes.
    [[nodiscard]] TAFLA_ALWAYS_INLINE Vector get_vector(std::size_t position) const {
        const std::size_t width = layout_.offset_size();
        const std::size_t count = read_size(position);

        // Every item has an end, so this rejects an impossible count before
        // any item is read
        static_cast<void>(range(position + width, static_cast<std::uint64_t>(count) * width));

        return {data_ + position, size_ - position, position, count, layout_.offset_type()};
    }

private:
    template <class T>
    [[nodiscard]] TAFLA_ALWAYS_INLINE T get(std::size_t position) const {
        return detail::load<T>(range(position, sizeof(T)));
    }

    template <class T>
    [[nodiscard]] TAFLA_ALWAYS_INLINE ArrayView<T> get_array(std::size_t position) const {
        const std::size_t count = read_size(position);

        // The count lies within the bytes, so this doesn't overflow, and
        // neither does the byte length of 2^32 elements in 64 bits
        const std::size_t start = position + layout_.offset_size();
        const auto* elements = range(start, static_cast<std::uint64_t>(count) * sizeof(T));

        return {elements, count};
    }

    /// Reads a length, offset or header slot at `position`
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t read_size(std::size_t position) const {
        const auto offset_type = layout_.offset_type();
        return detail::read_slot(range(position, offset_size(offset_type)), offset_type);
    }

    /// The position of slot `index` after `base`, saturating past any bytes
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t slot(std::size_t base, std::size_t index) const noexcept {
        if (index > SIZE_MAX / 4) [[unlikely]] {
            return SIZE_MAX;
        }

        const std::size_t offset = index * layout_.offset_size();
        return offset > SIZE_MAX - base ? SIZE_MAX : base + offset;
    }

    /// Returns the start of the `byte_length` bytes at `position`, which
    /// must lie within the bytes
    [[nodiscard]] TAFLA_ALWAYS_INLINE const std::uint8_t* range(std::size_t position, std::uint64_t byte_length) const {
        if (position > size_ || byte_length > size_ - position) [[unlikely]] {
            detail::throw_out_of_range(position, byte_length, size_);
        }
        return data_ + position;
    }

    [[noreturn]] TAFLA_COLD static void throw_invalid_utf8(std::size_t position) {
        throw DecodeError::invalid_utf8(position);
    }

    const std::uint8_t* data_;
    std::size_t size_;
    TableLayout<Versioned> layout_;
};

template <bool Versioned>
ByteBuffer(std::span<const std::uint8_t>, TableLayout<Versioned>) -> ByteBuffer<Versioned>;

}  // namespace tafla

#endif
