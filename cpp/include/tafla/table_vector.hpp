// A view of a vector of tables of one type, read in place.

#ifndef TAFLA_TABLE_VECTOR_HPP
#define TAFLA_TABLE_VECTOR_HPP

#include <cstddef>
#include <cstdint>
#include <iterator>
#include <span>
#include <type_traits>
#include <vector>

#include "vector.hpp"

namespace tafla {

/// A view of a vector of tables that `T` reads, which reads each item when
/// it's asked for, in place: each item from its bytes and those after it, as
/// `Vector::get_in_place()` returns them, at position 0. `T` reads a table at
/// a position of the bytes that it's constructed with, as the readers of the
/// tables that tafla-gen generates do, and `to_vector()` is faster when `T`
/// also has a default constructor, as theirs have.
///
/// `T` needn't be complete where the view is named, so that a table can hold a
/// vector of its own type, but only where it's used.
///
/// `get()` finds any item from its end and the one before it, without reading
/// the others, and iterating reads each item's end once. Reading an item
/// throws a `DecodeError` if its bytes are corrupt. It views bytes that the
/// caller owns, so it's valid for as long as they are.
template <class T>
class TableVector {
public:
    class iterator;

    using value_type = T;
    using size_type = std::size_t;

    /// An empty vector
    TableVector() noexcept = default;

    /// Views the items of `vector`, which `ByteBuffer::get_vector()` read
    explicit TableVector(Vector vector) noexcept : vector_(vector) {}

    /// The number of items
    [[nodiscard]] std::size_t size() const noexcept {
        return vector_.size();
    }

    [[nodiscard]] bool empty() const noexcept {
        return vector_.empty();
    }

    /// Reads item `index`, without reading the others. Throws
    /// `std::out_of_range` if `index` isn't less than `size()`, or a
    /// `DecodeError` if the item lies outside the bytes.
    [[nodiscard]] TAFLA_ALWAYS_INLINE T get(std::size_t index) const {
        return T(vector_.get_in_place(index), 0);
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE iterator begin() const noexcept {
        return iterator(vector_.items_in_place().begin());
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE iterator end() const noexcept {
        return iterator(vector_.items_in_place().end());
    }

    /// Reads every item, reading each item's end once.
    ///
    /// It allocates a reader of each item, `size() * sizeof(T)` bytes: 16
    /// for tafla-gen's readers of tables on a 64-bit host, which view the
    /// bytes from their table's start. Each item takes at least the 4 bytes
    /// of its end in a table of `uint32` offsets, so arbitrary bytes allocate
    /// at most about 4 times their length; the view allocates nothing.
    [[nodiscard]] std::vector<T> to_vector() const {
        // Assigning to default readers, which tafla-gen's readers of tables
        // have, compiles to a tighter loop than `emplace_back()`'s, which
        // copies each item through the stack and stores the vector's end
        if constexpr (std::is_default_constructible_v<T>) {
            std::vector<T> items(size());
            auto into = items.begin();
            for (const auto item : vector_.items_in_place()) {
                *into++ = T(item, 0);
            }
            return items;
        } else {
            std::vector<T> items;
            items.reserve(size());
            for (const auto item : vector_.items_in_place()) {
                items.emplace_back(item, 0);
            }
            return items;
        }
    }

    /// The vector of the items' bytes
    [[nodiscard]] Vector vector() const noexcept {
        return vector_;
    }

    /// A forward iterator over the items, which reads each one as it's
    /// dereferenced
    class iterator {
    public:
        using iterator_category = std::input_iterator_tag;
        using iterator_concept = std::forward_iterator_tag;
        using value_type = T;
        using difference_type = std::ptrdiff_t;
        using reference = T;
        using pointer = void;

        iterator() noexcept = default;

        [[nodiscard]] TAFLA_ALWAYS_INLINE T operator*() const {
            return T(*items_, 0);
        }

        TAFLA_ALWAYS_INLINE iterator& operator++() noexcept {
            ++items_;
            return *this;
        }

        iterator operator++(int) noexcept {
            auto previous = *this;
            ++*this;
            return previous;
        }

        [[nodiscard]] friend bool operator==(const iterator& left, const iterator& right) noexcept {
            return left.items_ == right.items_;
        }

    private:
        friend class TableVector;

        explicit iterator(Vector::Items::iterator items) noexcept : items_(items) {}

        Vector::Items::iterator items_;
    };

private:
    Vector vector_;
};

}  // namespace tafla

#endif
