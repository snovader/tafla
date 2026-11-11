// The bytes of a table that a builder or writer wrote.

#ifndef TAFLA_BYTES_HPP
#define TAFLA_BYTES_HPP

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <span>
#include <vector>

namespace tafla {

/// The bytes of a written table, which own them.
///
/// Writers allocate a table's bytes at its exact length, and write every
/// byte once, without zeroing them first, as a `std::vector` would. They
/// convert to a `std::span` of their bytes, which `ByteBuffer` reads, and
/// `to_vector()` copies them.
class Bytes {
public:
    using value_type = std::uint8_t;
    using size_type = std::size_t;
    using iterator = const std::uint8_t*;
    using const_iterator = const std::uint8_t*;

    /// No bytes
    Bytes() noexcept = default;

    /// `size` bytes that aren't initialized, which the caller writes
    static Bytes for_overwrite(std::size_t size) {
        Bytes bytes;
        if (size != 0) {
            bytes.data_ = std::make_unique_for_overwrite<std::uint8_t[]>(size);
            bytes.size_ = size;
        }
        return bytes;
    }

    /// A copy of `bytes`
    static Bytes copy_of(std::span<const std::uint8_t> bytes) {
        auto copy = for_overwrite(bytes.size());
        std::copy(bytes.begin(), bytes.end(), copy.data_.get());
        return copy;
    }

    Bytes(Bytes&& other) noexcept : data_(std::move(other.data_)), size_(other.size_) {
        other.size_ = 0;
    }

    Bytes& operator=(Bytes&& other) noexcept {
        data_ = std::move(other.data_);
        size_ = other.size_;
        other.size_ = 0;
        return *this;
    }

    Bytes(const Bytes&) = delete;
    Bytes& operator=(const Bytes&) = delete;
    ~Bytes() = default;

    [[nodiscard]] const std::uint8_t* data() const noexcept {
        return data_.get();
    }

    [[nodiscard]] std::uint8_t* data() noexcept {
        return data_.get();
    }

    [[nodiscard]] std::size_t size() const noexcept {
        return size_;
    }

    [[nodiscard]] bool empty() const noexcept {
        return size_ == 0;
    }

    [[nodiscard]] std::uint8_t operator[](std::size_t index) const noexcept {
        return data_[index];
    }

    [[nodiscard]] const std::uint8_t* begin() const noexcept {
        return data_.get();
    }

    [[nodiscard]] const std::uint8_t* end() const noexcept {
        return data_.get() + size_;
    }

    [[nodiscard]] std::span<const std::uint8_t> span() const noexcept {
        return {data_.get(), size_};
    }

    // NOLINTNEXTLINE(google-explicit-constructor): bytes are read as spans
    operator std::span<const std::uint8_t>() const noexcept {
        return span();
    }

    /// Copies the bytes into a `std::vector`
    [[nodiscard]] std::vector<std::uint8_t> to_vector() const {
        return {begin(), end()};
    }

    friend bool operator==(const Bytes& left, const Bytes& right) noexcept {
        return std::ranges::equal(left.span(), right.span());
    }

private:
    std::unique_ptr<std::uint8_t[]> data_;
    std::size_t size_ = 0;
};

}  // namespace tafla

#endif
