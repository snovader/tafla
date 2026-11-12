// A view of an array's little-endian elements, read in place.

#ifndef TAFLA_ARRAY_VIEW_HPP
#define TAFLA_ARRAY_VIEW_HPP

#include <compare>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <iterator>
#include <optional>
#include <span>
#include <stdexcept>
#include <vector>

#include "element.hpp"

namespace tafla {

/// An array read from a table: its elements' little-endian bytes, which it
/// reads in place at any alignment and in any host byte order. Reading it
/// never allocates, except for `to_vector()`.
///
/// It's a random-access range of `T`, whose elements are values rather than
/// references. `as_span()` returns the elements as a `std::span` when the
/// host is little-endian and the bytes are aligned to `T`, as `TableBuilder`
/// aligns them within an aligned table.
///
/// It views bytes that the caller owns, so it's valid for as long as they
/// are.
template <Element T>
class ArrayView {
public:
    class iterator;

    using value_type = T;
    using size_type = std::size_t;
    using difference_type = std::ptrdiff_t;
    using const_iterator = iterator;

    /// An empty array
    constexpr ArrayView() noexcept = default;

    /// Views `size` elements at `bytes`, which must hold `size * sizeof(T)`
    /// bytes
    constexpr ArrayView(const std::uint8_t* bytes, std::size_t size) noexcept : bytes_(bytes), size_(size) {}

    /// The number of elements
    [[nodiscard]] constexpr std::size_t size() const noexcept {
        return size_;
    }

    [[nodiscard]] constexpr bool empty() const noexcept {
        return size_ == 0;
    }

    /// Returns the element at `index`, which must be less than `size()`
    [[nodiscard]] T operator[](std::size_t index) const noexcept {
        return detail::load<T>(bytes_ + index * sizeof(T));
    }

    /// Returns the element at `index`, or throws `std::out_of_range` past the
    /// end
    [[nodiscard]] T at(std::size_t index) const {
        if (index >= size_) {
            throw std::out_of_range("tafla::ArrayView::at: index past the end");
        }
        return (*this)[index];
    }

    /// Returns the first element of an array that isn't empty
    [[nodiscard]] T front() const noexcept {
        return (*this)[0];
    }

    /// Returns the last element of an array that isn't empty
    [[nodiscard]] T back() const noexcept {
        return (*this)[size_ - 1];
    }

    [[nodiscard]] iterator begin() const noexcept {
        return iterator(bytes_);
    }

    [[nodiscard]] iterator end() const noexcept {
        return iterator(bytes_ + size_ * sizeof(T));
    }

    /// Returns the elements in place, or nothing when they aren't aligned to
    /// their type in memory or the host is big-endian. Single bytes are
    /// always in place.
    ///
    /// The span deliberately reads the table's bytes as elements of `T`, as
    /// FlatBuffers reads its vectors. It relies on the bytes' storage holding
    /// elements of `T`, which C++20's implicit creation of objects provides
    /// for storage from `new`, `std::malloc` or an allocator, and which the
    /// compilers that CI tests read as such from any bytes. `operator[]`,
    /// the iterators and `to_vector()` read each element with `std::memcpy`
    /// instead.
    [[nodiscard]] std::optional<std::span<const T>> as_span() const noexcept {
        if constexpr (sizeof(T) > 1) {
            if (!detail::kHostOrder || !detail::is_aligned<T>(bytes_)) {
                return std::nullopt;
            }
        }
        return std::span<const T>(reinterpret_cast<const T*>(bytes_), size_);
    }

    /// Copies the elements, in the host's byte order, into a vector allocated
    /// once
    [[nodiscard]] std::vector<T> to_vector() const {
        if (auto span = as_span()) {
            return std::vector<T>(span->begin(), span->end());
        }

        // Unaligned elements in the host's order are copied as bytes, and
        // others assigned one by one, which compiles to a tighter loop than
        // `push_back()`'s
        std::vector<T> elements(size_);
        if constexpr (detail::kHostOrder) {
            if (size_ != 0) {
                std::memcpy(elements.data(), bytes_, size_ * sizeof(T));
            }
        } else {
            for (std::size_t index = 0; index < size_; ++index) {
                elements[index] = (*this)[index];
            }
        }
        return elements;
    }

    /// Returns the elements' little-endian bytes, without the count before
    /// them
    [[nodiscard]] constexpr std::span<const std::uint8_t> bytes() const noexcept {
        return {bytes_, size_ * sizeof(T)};
    }

    /// A random-access iterator over the elements, which reads each one as
    /// it's dereferenced
    class iterator {
    public:
        using iterator_concept = std::random_access_iterator_tag;
        using iterator_category = std::input_iterator_tag;
        using value_type = T;
        using difference_type = std::ptrdiff_t;
        using reference = T;
        using pointer = void;

        constexpr iterator() noexcept = default;

        [[nodiscard]] T operator*() const noexcept {
            return detail::load<T>(bytes_);
        }

        [[nodiscard]] T operator[](difference_type offset) const noexcept {
            return *(*this + offset);
        }

        constexpr iterator& operator++() noexcept {
            bytes_ += sizeof(T);
            return *this;
        }

        constexpr iterator operator++(int) noexcept {
            auto previous = *this;
            ++*this;
            return previous;
        }

        constexpr iterator& operator--() noexcept {
            bytes_ -= sizeof(T);
            return *this;
        }

        constexpr iterator operator--(int) noexcept {
            auto previous = *this;
            --*this;
            return previous;
        }

        constexpr iterator& operator+=(difference_type offset) noexcept {
            bytes_ += offset * static_cast<difference_type>(sizeof(T));
            return *this;
        }

        constexpr iterator& operator-=(difference_type offset) noexcept {
            bytes_ -= offset * static_cast<difference_type>(sizeof(T));
            return *this;
        }

        [[nodiscard]] friend constexpr iterator operator+(iterator it, difference_type offset) noexcept {
            return it += offset;
        }

        [[nodiscard]] friend constexpr iterator operator+(difference_type offset, iterator it) noexcept {
            return it += offset;
        }

        [[nodiscard]] friend constexpr iterator operator-(iterator it, difference_type offset) noexcept {
            return it -= offset;
        }

        [[nodiscard]] friend constexpr difference_type operator-(iterator left, iterator right) noexcept {
            return (left.bytes_ - right.bytes_) / static_cast<difference_type>(sizeof(T));
        }

        [[nodiscard]] friend constexpr bool operator==(iterator left, iterator right) noexcept {
            return left.bytes_ == right.bytes_;
        }

        [[nodiscard]] friend constexpr std::strong_ordering operator<=>(iterator left, iterator right) noexcept {
            return std::compare_three_way()(left.bytes_, right.bytes_);
        }

    private:
        friend class ArrayView;

        constexpr explicit iterator(const std::uint8_t* bytes) noexcept : bytes_(bytes) {}

        const std::uint8_t* bytes_ = nullptr;
    };

private:
    const std::uint8_t* bytes_ = nullptr;
    std::size_t size_ = 0;
};

}  // namespace tafla

#endif
