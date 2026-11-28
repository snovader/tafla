// A view of a vector of strings, read in place.

#ifndef TAFLA_STRING_VECTOR_HPP
#define TAFLA_STRING_VECTOR_HPP

#include <cstddef>
#include <cstdint>
#include <iterator>
#include <string_view>
#include <vector>

#include "element.hpp"
#include "errors.hpp"
#include "utf8.hpp"
#include "vector.hpp"

namespace tafla {

/// A view of a vector of strings, which reads each string when it's asked
/// for, as a `std::string_view` of the bytes.
///
/// A vector of strings is laid out as a vector of tables is: its count, the
/// end of each string, counted from the vector's position, and then the
/// strings' UTF-8, one after another, without a count or padding of their
/// own. `TableReader::get_string_vector()` checks that the count and the ends
/// lie within the bytes, so `get()` finds any string from its end and the end
/// of the string before it, without reading the others, and iterating reads
/// each string's end once. String 0 starts after the ends; any other starts
/// where the string before it ends, which isn't checked to lie after the ends,
/// as a vector's items aren't.
///
/// Each string is checked as UTF-8 on its own, from its own ends, so an end
/// that splits a character leaves both strings around it invalid. Reading a
/// string throws a `DecodeError` if it ends before it starts, ends past the
/// bytes, or isn't valid UTF-8. `to_vector()` reads every string as `get()`
/// reads each, and throws the error of the first that fails. It views bytes
/// that the caller owns, so it's valid for as long as they are.
class StringVector {
public:
    class iterator;

    using value_type = std::string_view;
    using size_type = std::size_t;

    /// An empty vector
    constexpr StringVector() noexcept = default;

    /// Views the strings of `vector`, a vector of strings that
    /// `TableReader::get_vector()` read
    explicit constexpr StringVector(Vector vector) noexcept : vector_(vector) {}

    /// The number of strings
    [[nodiscard]] constexpr std::size_t size() const noexcept {
        return vector_.size();
    }

    [[nodiscard]] constexpr bool empty() const noexcept {
        return vector_.empty();
    }

    /// Reads string `index`, without reading the others.
    ///
    /// Throws `std::out_of_range` if `index` isn't less than `size()`, or a
    /// `DecodeError` if the string ends before the string before it, ends
    /// past the bytes or isn't valid UTF-8.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::string_view get(std::size_t index) const {
        vector_.check_index(index);
        return string(vector_, vector_.start(index), vector_.end(index));
    }

    [[nodiscard]] TAFLA_ALWAYS_INLINE iterator begin() const noexcept;

    [[nodiscard]] TAFLA_ALWAYS_INLINE iterator end() const noexcept;

    /// Reads every string, from the first to the last, as `get()` reads each.
    ///
    /// It checks the strings' UTF-8 as one run, from the end of the ends to
    /// the end of the last string, and that each end within the run starts a
    /// character, which, for strings whose ends are in order within the
    /// bytes, is what checking each string on its own finds. Otherwise it
    /// reads each string with `get()`, which throws the error of the first
    /// that fails.
    ///
    /// It allocates a `std::string_view` of each string, `size() * 16` bytes
    /// on a 64-bit host. Each string takes at least the byte of its end, so
    /// arbitrary bytes allocate at most about 16 times their length; the
    /// view allocates nothing.
    [[nodiscard]] std::vector<std::string_view> to_vector() const {
        std::vector<std::string_view> strings(size());
        if (!strings.empty() && !read_run(strings.data())) {
            for (std::size_t index = 0; index < strings.size(); ++index) {
                strings[index] = get(index);
            }
        }
        return strings;
    }

    /// The vector of the strings' bytes
    [[nodiscard]] constexpr Vector vector() const noexcept {
        return vector_;
    }

private:
    /// The string from `start` to `end`, from the position of `vector`,
    /// which must be valid UTF-8
    [[nodiscard]] TAFLA_ALWAYS_INLINE static std::string_view string(const Vector& vector, std::size_t start,
                                                                     std::size_t end) {
        const auto bytes = vector.item(start, end);
        if (!detail::valid_utf8(bytes.data(), bytes.size())) [[unlikely]] {
            throw_invalid_utf8(vector.position_, start);
        }
        return {reinterpret_cast<const char*>(bytes.data()), bytes.size()};
    }

    /// Throws the error of the string at `start` from the vector's
    /// `position`, which isn't valid UTF-8. It takes the vector's position
    /// rather than the vector, as `Vector::fail()` does.
    [[noreturn]] TAFLA_COLD static void throw_invalid_utf8(std::size_t position, std::size_t start) {
        throw DecodeError::invalid_utf8(Vector::position_of(position, start));
    }

    /// Reads every string into `strings`, which holds `size()` of them, at
    /// least one, by checking their UTF-8 as one run, as `to_vector()`
    /// describes. Returns false, for `get()` to find the string that fails,
    /// if an end lies before the end before it or past the bytes, an end
    /// within the run splits a character, or the run isn't valid UTF-8. A
    /// string that a character's continuation byte starts isn't valid on its
    /// own, and if each string is, so is the run.
    [[nodiscard]] bool read_run(std::string_view* strings) const noexcept {
        const auto& vector = vector_;
        const std::size_t count = vector.size_;
        const std::size_t run_start = vector.items_start();
        const std::size_t run_end = vector.end(count - 1);
        if (run_end > vector.length_) {
            return false;
        }

        const auto* const bytes = vector.bytes_;
        std::size_t start = run_start;
        for (std::size_t index = 0; index < count; ++index) {
            const std::size_t end = vector.end(index);
            if (end < start || end > run_end || (end != run_end && (bytes[end] & 0xc0) == 0x80)) {
                return false;
            }

            strings[index] = {reinterpret_cast<const char*>(bytes + start), end - start};
            start = end;
        }

        return detail::valid_utf8(bytes + run_start, run_end - run_start);
    }

    Vector vector_;
};

/// A forward iterator over the strings of a `StringVector`, which reads each
/// one as it's dereferenced, and throws a `DecodeError` as `get()` does: a
/// string that fails doesn't keep the others from being read
class StringVector::iterator {
public:
    using iterator_category = std::input_iterator_tag;
    using iterator_concept = std::forward_iterator_tag;
    using value_type = std::string_view;
    using difference_type = std::ptrdiff_t;
    using reference = std::string_view;
    using pointer = void;

    iterator() noexcept = default;

    [[nodiscard]] TAFLA_ALWAYS_INLINE std::string_view operator*() const {
        return StringVector::string(vector_, start_, end_);
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
    friend class StringVector;

    TAFLA_ALWAYS_INLINE iterator(const Vector& vector, std::size_t index) noexcept : vector_(vector), index_(index) {
        if (index < vector.size_) {
            start_ = vector.start(index);
            end_ = vector.end(index);
        }
    }

    /// A copy, so that the iterator outlives the view it came from
    Vector vector_;
    std::size_t index_ = 0;
    /// Where the string starts and ends, from the vector's position
    std::size_t start_ = 0;
    std::size_t end_ = 0;
};

TAFLA_ALWAYS_INLINE StringVector::iterator StringVector::begin() const noexcept {
    return {vector_, 0};
}

TAFLA_ALWAYS_INLINE StringVector::iterator StringVector::end() const noexcept {
    return {vector_, vector_.size()};
}

}  // namespace tafla

#endif
