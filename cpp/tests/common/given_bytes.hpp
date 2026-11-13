// Checks of a generated message's `serialize_into()`, which writes it into the
// start of bytes that the caller gives

#ifndef TAFLA_TEST_GIVEN_BYTES_HPP
#define TAFLA_TEST_GIVEN_BYTES_HPP

#include <rapidcheck.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <span>
#include <stdexcept>
#include <vector>

namespace tafla::test {

/// Whether `fields.serialize_into()` writes `expected`, the bytes that its
/// `serialize()` writes, into the start of bytes `before` bytes into a
/// buffer, which runs on past the table, and leaves the buffer's other bytes
/// as they were. Every byte of the buffer is `fill` first, so a table with
/// padding is written whole only if the writer writes its padding too.
template <class Fields>
bool written_into(const Fields& fields, std::span<const std::uint8_t> expected, std::size_t before, std::uint8_t fill) {
    constexpr std::size_t after = 3;
    std::vector<std::uint8_t> buffer(before + expected.size() + after, fill);

    const auto length = fields.serialize_into(std::span(buffer).subspan(before));
    if (length != expected.size()) {
        return false;
    }

    const auto table = std::span(buffer).subspan(before, length);
    const auto is_fill = [fill](std::uint8_t byte) { return byte == fill; };
    return std::ranges::equal(table, expected) && std::ranges::all_of(std::span(buffer).first(before), is_fill) &&
           std::ranges::all_of(std::span(buffer).last(after), is_fill);
}

/// Whether `fields.serialize_into()` throws `std::invalid_argument` for bytes
/// one shorter than its table of `length` bytes, before writing any of them
template <class Fields>
bool rejects_too_short(const Fields& fields, std::size_t length, std::uint8_t fill) {
    if (length == 0) {
        return true;
    }

    std::vector<std::uint8_t> buffer(length - 1, fill);
    try {
        static_cast<void>(fields.serialize_into(buffer));
        return false;
    } catch (const std::invalid_argument&) {
        // As documented, before writing
    }
    return std::ranges::all_of(buffer, [fill](std::uint8_t byte) { return byte == fill; });
}

/// Whether `fields.serialize_into()` writes `expected`, the bytes that its
/// `serialize()` writes, as `written_into()` checks, at any byte offset in
/// a buffer of any bytes, and rejects bytes too short, as
/// `rejects_too_short()` checks
template <class Fields>
bool written_into_any_bytes(const Fields& fields, std::span<const std::uint8_t> expected) {
    const auto before = *rc::gen::inRange<std::size_t>(0, 8);
    const auto fill = *rc::gen::arbitrary<std::uint8_t>();
    return written_into(fields, expected, before, fill) && rejects_too_short(fields, expected.size(), fill);
}

}  // namespace tafla::test

#endif
