// The bools of an array of bools, read from its bytes.

#ifndef TAFLA_BOOLS_HPP
#define TAFLA_BOOLS_HPP

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <initializer_list>
#include <memory>
#include <span>
#include <utility>

namespace tafla {

/// The bools of an array of bools, which own them, one after another in
/// memory, as `TableReader::get_bool_array()` reads them.
///
/// An array of bools is a `uint8` array, whose bytes are true where they
/// aren't 0. A byte other than 0 or 1 is no `bool`, so the bytes are read
/// into bools, as `from_bytes()` reads them, rather than viewed as bools.
///
/// They're a `bool` each, so they convert to the `std::span<const bool>`
/// that `set_bool_array()` writes, as a `std::vector` of numbers converts to
/// a span of them. A `std::vector<bool>` packs its bools into bits instead,
/// which no span views: in a Release build, filling one from 1,000 bytes
/// took 2.0 µs with Apple Clang 21 or GCC 16, where `from_bytes()` took 38
/// and 45 ns.
class Bools {
public:
    using value_type = bool;
    using size_type = std::size_t;
    using difference_type = std::ptrdiff_t;
    using iterator = bool*;
    using const_iterator = const bool*;

    /// No bools
    Bools() noexcept = default;

    /// `size` bools, each false
    explicit Bools(std::size_t size) : data_(size == 0 ? nullptr : std::make_unique<bool[]>(size)), size_(size) {}

    /// A copy of `bools`
    explicit Bools(std::span<const bool> bools) : Bools(for_overwrite(bools.size())) {
        std::ranges::copy(bools, data_.get());
    }

    Bools(std::initializer_list<bool> bools) : Bools(std::span<const bool>(bools.begin(), bools.size())) {}

    Bools(const Bools& other) : Bools(other.span()) {}

    Bools& operator=(const Bools& other) {
        if (this != &other) {
            *this = Bools(other);
        }
        return *this;
    }

    /// Moves `other`'s bools, without copying them, and leaves it empty
    Bools(Bools&& other) noexcept : data_(std::move(other.data_)), size_(std::exchange(other.size_, 0)) {}

    Bools& operator=(Bools&& other) noexcept {
        if (this != &other) {
            data_ = std::move(other.data_);
            size_ = std::exchange(other.size_, 0);
        }
        return *this;
    }

    ~Bools() = default;

    /// Reads the bytes of an array of bools, after its count, as bools, each
    /// true where its byte isn't 0, as `TableReader::get_bool_array()` reads
    /// them. A generated getter of an array of bools returns its bytes,
    /// which this reads too.
    [[nodiscard]] static Bools from_bytes(std::span<const std::uint8_t> bytes) {
        auto bools = for_overwrite(bytes.size());
        bool* const values = bools.data_.get();
        for (std::size_t index = 0; index < bytes.size(); ++index) {
            values[index] = bytes[index] != 0;
        }
        return bools;
    }

    [[nodiscard]] const bool* data() const noexcept {
        return data_.get();
    }

    [[nodiscard]] bool* data() noexcept {
        return data_.get();
    }

    [[nodiscard]] std::size_t size() const noexcept {
        return size_;
    }

    [[nodiscard]] bool empty() const noexcept {
        return size_ == 0;
    }

    [[nodiscard]] bool operator[](std::size_t index) const noexcept {
        return data_[index];
    }

    [[nodiscard]] bool& operator[](std::size_t index) noexcept {
        return data_[index];
    }

    [[nodiscard]] const bool* begin() const noexcept {
        return data_.get();
    }

    [[nodiscard]] bool* begin() noexcept {
        return data_.get();
    }

    [[nodiscard]] const bool* end() const noexcept {
        return data_.get() + size_;
    }

    [[nodiscard]] bool* end() noexcept {
        return data_.get() + size_;
    }

    [[nodiscard]] std::span<const bool> span() const noexcept {
        return {data_.get(), size_};
    }

    friend bool operator==(const Bools& left, const Bools& right) noexcept {
        return std::ranges::equal(left.span(), right.span());
    }

private:
    /// `size` bools that aren't initialized, which are written before
    /// they're read
    static Bools for_overwrite(std::size_t size) {
        Bools bools;
        if (size != 0) {
            bools.data_ = std::make_unique_for_overwrite<bool[]>(size);
            bools.size_ = size;
        }
        return bools;
    }

    std::unique_ptr<bool[]> data_;
    std::size_t size_ = 0;
};

}  // namespace tafla

#endif
