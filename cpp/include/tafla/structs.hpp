// Structs, which tables hold in place and in arrays, and views of their
// arrays.

#ifndef TAFLA_STRUCTS_HPP
#define TAFLA_STRUCTS_HPP

#include <compare>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <iterator>
#include <span>
#include <stdexcept>
#include <type_traits>
#include <vector>

#include "layout.hpp"

namespace tafla {

/// A struct: scalars at fixed offsets, without a header, offsets or padding,
/// which a type reads from and writes to its `SIZE` bytes, a constant of at
/// least 1, and whose arrays are aligned to `ALIGNMENT`, its largest
/// scalar's size. The structs that tafla-gen generates are, and so can an
/// application's own types be, which `TableWriter` then writes and
/// `StructArray` reads. An array counts its structs by their size, so a
/// struct of 0 bytes, whose arrays a count can't describe, isn't one.
template <class T>
concept Struct = requires(const T& value, const std::uint8_t* from, std::uint8_t* to) {
    { T::SIZE } -> std::convertible_to<std::size_t>;
    { T::ALIGNMENT } -> std::convertible_to<Alignment>;
    { T::from_bytes(from) } -> std::same_as<T>;
    value.to_bytes(to);
} && (T::SIZE > 0);

/// A view of an array of structs, which reads each struct from its bytes when
/// it's asked for, at any alignment and in any host byte order.
///
/// It's a random-access range of `T`, whose elements are values rather than
/// references, and views bytes that the caller owns, so it's valid for as
/// long as they are.
template <Struct T>
class StructArray {
public:
    class iterator;

    using value_type = T;
    using size_type = std::size_t;
    using difference_type = std::ptrdiff_t;
    using const_iterator = iterator;

    /// An empty array
    constexpr StructArray() noexcept = default;

    /// Views the structs whose bytes `bytes` hold, one after another, as
    /// `ByteBuffer::get_struct_array()` returns them. A remainder shorter
    /// than a struct is left out.
    constexpr explicit StructArray(std::span<const std::uint8_t> bytes) noexcept
        : bytes_(bytes.data()), size_(bytes.size() / T::SIZE) {}

    /// The number of structs
    [[nodiscard]] constexpr std::size_t size() const noexcept {
        return size_;
    }

    [[nodiscard]] constexpr bool empty() const noexcept {
        return size_ == 0;
    }

    /// Reads struct `index`, which must be less than `size()`
    [[nodiscard]] T operator[](std::size_t index) const noexcept {
        return T::from_bytes(bytes_ + index * T::SIZE);
    }

    /// Reads struct `index`, or throws `std::out_of_range` past the end
    [[nodiscard]] T at(std::size_t index) const {
        if (index >= size_) {
            throw std::out_of_range("tafla::StructArray::at: index past the end");
        }
        return (*this)[index];
    }

    [[nodiscard]] iterator begin() const noexcept {
        return iterator(bytes_);
    }

    [[nodiscard]] iterator end() const noexcept {
        return iterator(bytes_ + size_ * T::SIZE);
    }

    /// Reads every struct, into a vector allocated once
    [[nodiscard]] std::vector<T> to_vector() const {
        // Assigning to default structs took a fifth of the time of
        // `push_back()` for 100 structs of 12 bytes, whose loop copies each
        // struct through the stack and stores the vector's end. The iterators
        // are input iterators to `std::vector`, which would grow to their
        // length.
        if constexpr (std::is_default_constructible_v<T>) {
            std::vector<T> structs(size_);
            for (std::size_t index = 0; index < size_; ++index) {
                structs[index] = (*this)[index];
            }
            return structs;
        } else {
            std::vector<T> structs;
            structs.reserve(size_);
            for (std::size_t index = 0; index < size_; ++index) {
                structs.push_back((*this)[index]);
            }
            return structs;
        }
    }

    /// The structs' bytes, without the count before them
    [[nodiscard]] constexpr std::span<const std::uint8_t> bytes() const noexcept {
        return {bytes_, size_ * T::SIZE};
    }

    /// A random-access iterator over the structs, which reads each one as
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
            return T::from_bytes(bytes_);
        }

        [[nodiscard]] T operator[](difference_type offset) const noexcept {
            return *(*this + offset);
        }

        constexpr iterator& operator++() noexcept {
            bytes_ += T::SIZE;
            return *this;
        }

        constexpr iterator operator++(int) noexcept {
            auto previous = *this;
            ++*this;
            return previous;
        }

        constexpr iterator& operator--() noexcept {
            bytes_ -= T::SIZE;
            return *this;
        }

        constexpr iterator operator--(int) noexcept {
            auto previous = *this;
            --*this;
            return previous;
        }

        constexpr iterator& operator+=(difference_type offset) noexcept {
            bytes_ += offset * static_cast<difference_type>(T::SIZE);
            return *this;
        }

        constexpr iterator& operator-=(difference_type offset) noexcept {
            bytes_ -= offset * static_cast<difference_type>(T::SIZE);
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
            return (left.bytes_ - right.bytes_) / static_cast<difference_type>(T::SIZE);
        }

        [[nodiscard]] friend constexpr bool operator==(iterator left, iterator right) noexcept {
            return left.bytes_ == right.bytes_;
        }

        [[nodiscard]] friend constexpr std::strong_ordering operator<=>(iterator left, iterator right) noexcept {
            return std::compare_three_way()(left.bytes_, right.bytes_);
        }

    private:
        friend class StructArray;

        constexpr explicit iterator(const std::uint8_t* bytes) noexcept : bytes_(bytes) {}

        const std::uint8_t* bytes_ = nullptr;
    };

private:
    const std::uint8_t* bytes_ = nullptr;
    std::size_t size_ = 0;
};

}  // namespace tafla

#endif
