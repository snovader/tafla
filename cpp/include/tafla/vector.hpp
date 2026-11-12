// A vector of tables, read in place.

#ifndef TAFLA_VECTOR_HPP
#define TAFLA_VECTOR_HPP

#include <cstddef>
#include <cstdint>
#include <iterator>
#include <span>
#include <stdexcept>

#include "element.hpp"
#include "errors.hpp"
#include "layout.hpp"

namespace tafla {

namespace detail {

/// Reads a length, offset or header slot of `offset_type` at `bytes`, which
/// must hold it
TAFLA_ALWAYS_INLINE std::size_t read_slot(const std::uint8_t* bytes, OffsetType offset_type) noexcept {
    switch (offset_type) {
        case OffsetType::uint8:
            return bytes[0];
        case OffsetType::uint16:
            return load<std::uint16_t>(bytes);
        case OffsetType::uint32:
            break;
    }
    return load<std::uint32_t>(bytes);
}

}  // namespace detail

/// A vector read from a table: its items, which are tables.
///
/// The vector holds its count, the end of each item, counted from the
/// vector's position, and then the items. `ByteBuffer::get_vector()` checks
/// that the ends lie within the bytes, and each item is checked when it's
/// read, so reading one can throw a `DecodeError`. `get()` and
/// `get_in_place()` find any item from its end and the one before it,
/// without reading the others.
///
/// An item that has no header and its parent's offset type is read in
/// place: from the bytes that `get_in_place()` returns, which start where it
/// does and end where its parent's do, with a `ByteBuffer` of its parent's
/// offset type, at position 0. Its reads may reach past its end, as those of
/// any table read in place may. Any other item is read as a table of its
/// own, from the bytes that `get()` returns, with a `ByteBuffer` of its own
/// layout.
///
/// It views bytes that the caller owns, so it's valid for as long as they
/// are.
class Vector {
public:
    class Items;

    /// An empty vector
    constexpr Vector() noexcept = default;

    /// The number of items
    [[nodiscard]] constexpr std::size_t size() const noexcept {
        return size_;
    }

    [[nodiscard]] constexpr bool empty() const noexcept {
        return size_ == 0;
    }

    /// Returns the bytes of item `index`, from its start to its end, to read
    /// it as a table of its own. Only the ends of the item and the one before
    /// it are read.
    ///
    /// Throws `std::out_of_range` if `index` isn't less than `size()`, or a
    /// `DecodeError` if the item ends before the item before it, or past the
    /// bytes.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::span<const std::uint8_t> get(std::size_t index) const {
        check_index(index);
        return item(start(index), end(index));
    }

    /// Returns the bytes of item `index` to read it in place, from its start
    /// to the end of the buffer. Only the ends of the item and the one before
    /// it are read.
    ///
    /// Throws as `get()` does.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::span<const std::uint8_t> get_in_place(std::size_t index) const {
        check_index(index);
        return item_in_place(start(index), end(index));
    }

    /// The items' bytes, as `get()` returns them, reading each item's end
    /// once
    [[nodiscard]] Items items() const noexcept;

    /// The items' bytes, as `get_in_place()` returns them, reading each
    /// item's end once
    [[nodiscard]] Items items_in_place() const noexcept;

    /// The bytes of the buffer from the vector's position, which is its
    /// count's
    [[nodiscard]] constexpr std::span<const std::uint8_t> bytes() const noexcept {
        return {bytes_, length_};
    }

private:
    template <bool>
    friend class ByteBuffer;

    /// Views `size` items in the `length` bytes at `bytes`, from the
    /// vector's `position` to the end of the buffer, which must have been
    /// checked to hold the count and the ends
    constexpr Vector(const std::uint8_t* bytes, std::size_t length, std::size_t position, std::size_t size,
                     OffsetType offset_type) noexcept
        : bytes_(bytes), length_(length), position_(position), size_(size), offset_type_(offset_type) {}

    TAFLA_ALWAYS_INLINE void check_index(std::size_t index) const {
        if (index >= size_) [[unlikely]] {
            throw_past_the_end();
        }
    }

    [[noreturn]] TAFLA_COLD static void throw_past_the_end() {
        throw std::out_of_range("tafla::Vector: index past the end");
    }

    /// Where item 0 starts, from the vector's position: after the count and
    /// the ends, which lie within the bytes
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t items_start() const noexcept {
        return (size_ + 1) * offset_size(offset_type_);
    }

    /// Where item `index` starts, from the vector's position
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t start(std::size_t index) const noexcept {
        return index == 0 ? items_start() : end(index - 1);
    }

    /// The end of item `index`, from the vector's position. The ends were
    /// checked to lie within the bytes.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t end(std::size_t index) const noexcept {
        const auto width = offset_size(offset_type_);
        return detail::read_slot(bytes_ + (index + 1) * width, offset_type_);
    }

    /// The bytes of the item from `start` to `end`, from the vector's
    /// position
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::span<const std::uint8_t> item(std::size_t start, std::size_t end) const {
        check_item(start, end);
        return {bytes_ + start, end - start};
    }

    /// The bytes from the start of the item from `start` to `end`, from the
    /// vector's position, to the end of the buffer
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::span<const std::uint8_t> item_in_place(std::size_t start,
                                                                                  std::size_t end) const {
        check_item(start, end);
        return {bytes_ + start, length_ - start};
    }

    TAFLA_ALWAYS_INLINE void check_item(std::size_t start, std::size_t end) const {
        if (start > end || end > length_) [[unlikely]] {
            fail(position_, length_, start, end);
        }
    }

    /// Throws the error of the item from `start` to `end` of a vector at
    /// `position` of `length` bytes. It takes the vector's fields rather than
    /// the vector, whose address would otherwise keep a copy of it, and of the
    /// iterator that holds it, in memory rather than in registers.
    [[noreturn]] TAFLA_COLD static void fail(std::size_t position, std::size_t length, std::size_t start,
                                             std::size_t end) {
        if (start > end) {
            throw DecodeError::item_ends_before_start(position_of(position, start), position_of(position, end));
        }
        throw DecodeError::out_of_range(position_of(position, start), end - start, position + length);
    }

    /// The position in the buffer of `offset` from the vector's `position`,
    /// which an error reports. Saturating, since only a 32-bit host can
    /// overflow, at a position past any bytes.
    [[nodiscard]] static std::size_t position_of(std::size_t position, std::size_t offset) noexcept {
        return offset > SIZE_MAX - position ? SIZE_MAX : position + offset;
    }

    const std::uint8_t* bytes_ = nullptr;
    std::size_t length_ = 0;
    std::size_t position_ = 0;
    std::size_t size_ = 0;
    OffsetType offset_type_ = OffsetType::uint32;
};

/// The bytes of a `Vector`'s items, from their start to their end or to the
/// end of the buffer. Dereferencing an iterator reads its item, which throws
/// a `DecodeError` as `Vector::get()` does: an item that fails doesn't keep
/// the others from being read.
class Vector::Items {
public:
    class iterator {
    public:
        using iterator_category = std::input_iterator_tag;
        using iterator_concept = std::forward_iterator_tag;
        using value_type = std::span<const std::uint8_t>;
        using difference_type = std::ptrdiff_t;
        using reference = value_type;
        using pointer = void;

        iterator() noexcept = default;

        [[nodiscard]] TAFLA_ALWAYS_INLINE value_type operator*() const {
            return in_place_ ? vector_.item_in_place(start_, end_) : vector_.item(start_, end_);
        }

        TAFLA_ALWAYS_INLINE iterator& operator++() noexcept {
            ++index_;
            start_ = end_;
            if (index_ < vector_.size_) {
                end_ = vector_.end(index_);
            }
            return *this;
        }

        iterator operator++(int) noexcept {
            auto previous = *this;
            ++*this;
            return previous;
        }

        [[nodiscard]] friend bool operator==(const iterator& left, const iterator& right) noexcept {
            return left.index_ == right.index_;
        }

    private:
        friend class Items;

        iterator(const Vector& vector, std::size_t index, bool in_place) noexcept
            : vector_(vector), index_(index), in_place_(in_place) {
            if (index < vector.size_) {
                start_ = vector.start(index);
                end_ = vector.end(index);
            }
        }

        /// A copy, so that the iterator outlives the range it came from
        Vector vector_;
        std::size_t index_ = 0;
        /// Where the item starts and ends, from the vector's position
        std::size_t start_ = 0;
        std::size_t end_ = 0;
        bool in_place_ = false;
    };

    [[nodiscard]] iterator begin() const noexcept {
        return {vector_, 0, in_place_};
    }

    [[nodiscard]] iterator end() const noexcept {
        return {vector_, vector_.size_, in_place_};
    }

    [[nodiscard]] std::size_t size() const noexcept {
        return vector_.size_;
    }

private:
    friend class Vector;

    Items(const Vector& vector, bool in_place) noexcept : vector_(vector), in_place_(in_place) {}

    Vector vector_;
    bool in_place_;
};

inline Vector::Items Vector::items() const noexcept {
    return {*this, false};
}

inline Vector::Items Vector::items_in_place() const noexcept {
    return {*this, true};
}

}  // namespace tafla

#endif
