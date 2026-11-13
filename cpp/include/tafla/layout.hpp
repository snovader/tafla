// A table's offset type and header, and the alignments of its fields.

#ifndef TAFLA_LAYOUT_HPP
#define TAFLA_LAYOUT_HPP

#include <cstddef>
#include <cstdint>
#include <string_view>

namespace tafla {

/// A table's offset type, which limits how long the table can be.
///
/// It's the unsigned integer type of the table's header slots, field offsets,
/// counts, lengths and item ends. Each enumerator is named after its type, as
/// `SPEC.md` and schemas name it.
enum class OffsetType : std::uint8_t {
    /// For tables of up to 255 bytes
    uint8,
    /// For tables of up to 65,535 bytes
    uint16,
    /// For tables of up to 4 GiB
    uint32,
};

/// The offset size, the type's width in bytes: 1, 2 or 4
constexpr std::size_t offset_size(OffsetType offset_type) noexcept {
    switch (offset_type) {
        case OffsetType::uint8:
            return 1;
        case OffsetType::uint16:
            return 2;
        case OffsetType::uint32:
            break;
    }
    return 4;
}

/// The type's largest value, which is also the longest table that its
/// offsets address
constexpr std::uint32_t max_value(OffsetType offset_type) noexcept {
    switch (offset_type) {
        case OffsetType::uint8:
            return 0xff;
        case OffsetType::uint16:
            return 0xffff;
        case OffsetType::uint32:
            break;
    }
    return 0xffff'ffff;
}

/// The type's name, as `SPEC.md` and schemas spell it: `uint8`, `uint16` or
/// `uint32`
constexpr std::string_view name(OffsetType offset_type) noexcept {
    switch (offset_type) {
        case OffsetType::uint8:
            return "uint8";
        case OffsetType::uint16:
            return "uint16";
        case OffsetType::uint32:
            break;
    }
    return "uint32";
}

/// A multiple of bytes from a table's start that a field starts at, so that
/// its elements can be read in place
enum class Alignment : std::uint8_t {
    one = 1,
    two = 2,
    four = 4,
    eight = 8,
};

/// The alignment in bytes: 1, 2, 4 or 8
constexpr std::size_t alignment_bytes(Alignment alignment) noexcept {
    return static_cast<std::size_t>(alignment);
}

namespace detail {

/// The bytes of padding that bring `offset` to a multiple of `alignment`.
/// Only the remainder matters, so `offset` may have wrapped around.
template <class Offset>
constexpr Offset padding(Offset offset, Alignment alignment) noexcept {
    return static_cast<Offset>(-offset) & static_cast<Offset>(alignment_bytes(alignment) - 1);
}

}  // namespace detail

/// The header and offset type of a table.
///
/// A format creates its layout once, as a constant, and passes it to both the
/// `TableBuilder` or `TableSizer` that writes the table and the `ByteBuffer`
/// that reads it, so that the two can't disagree. The default layout has the
/// offset type `uint32` and no header slots, which nested tables have.
///
/// `Versioned` says whether the header has a slot for a version, so that the
/// compiler requires a version exactly where a versioned table is written,
/// and offers `ByteBuffer::version()` only where there is one.
///
/// ```cpp
/// constexpr auto document = tafla::TableLayout().sized().versioned();
/// constexpr auto frame = tafla::TableLayout().with_offset_type(tafla::OffsetType::uint8);
/// static_assert(document.header_size() == 8 && frame.header_size() == 0);
/// ```
template <bool Versioned = false>
class TableLayout {
public:
    /// No header slots, and offsets and lengths of the offset type `uint32`
    constexpr TableLayout() noexcept = default;

    /// Starts the header with the table's total byte length
    [[nodiscard]] constexpr TableLayout sized() const noexcept {
        return TableLayout(true, offset_type_);
    }

    /// Adds a slot for a version to the header, after the length when sized.
    /// Readers can check it before reading any field.
    [[nodiscard]] constexpr TableLayout<true> versioned() const noexcept
        requires(!Versioned)
    {
        return TableLayout<true>(sized_, offset_type_);
    }

    /// Sets the offset type, the unsigned integer type of the header slots,
    /// field offsets, counts, lengths and item ends, which limits how long
    /// the table can be. The default is `OffsetType::uint32`.
    [[nodiscard]] constexpr TableLayout with_offset_type(OffsetType offset_type) const noexcept {
        return TableLayout(sized_, offset_type);
    }

    [[nodiscard]] constexpr bool is_sized() const noexcept {
        return sized_;
    }

    [[nodiscard]] constexpr bool is_versioned() const noexcept {
        return Versioned;
    }

    [[nodiscard]] constexpr OffsetType offset_type() const noexcept {
        return offset_type_;
    }

    /// The offset size, the width in bytes of the offset type: 1, 2 or 4
    [[nodiscard]] constexpr std::size_t offset_size() const noexcept {
        return tafla::offset_size(offset_type_);
    }

    /// The byte length of the header slots before the field offsets
    [[nodiscard]] constexpr std::size_t header_size() const noexcept {
        const std::size_t slots = (sized_ ? 1U : 0U) + (Versioned ? 1U : 0U);
        return slots * offset_size();
    }

    friend constexpr bool operator==(TableLayout, TableLayout) noexcept = default;

private:
    template <bool>
    friend class TableLayout;

    constexpr TableLayout(bool sized, OffsetType offset_type) noexcept : sized_(sized), offset_type_(offset_type) {}

    bool sized_ = false;
    OffsetType offset_type_ = OffsetType::uint32;
};

}  // namespace tafla

#endif
