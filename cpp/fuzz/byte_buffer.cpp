// Reads arbitrary bytes with every getter of `ByteBuffer` at a position, in an
// offset type and header, and reads the arrays, struct arrays and vector that
// it finds there.
//
// A crash, a sanitizer's report or a failed check is a finding: each read
// returns the bytes or value that lie where the format puts them, viewed in
// the input, exactly when they lie within it, and the decode error otherwise.
//
// Unlike Rust's target, which reads at every position of every input, this
// one reads where the input's first four bytes say: the offset type and
// header, a position within the rest or far past it, and a field's index.
// Most reads of arbitrary bytes fail, and a C++ exception takes about 4.5 µs
// to unwind on macOS, so reading at every position ran 6 inputs a second.

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <optional>
#include <span>
#include <string_view>

#include <tafla/tafla.hpp>

namespace {

using tafla::ByteBuffer;
using tafla::DecodeError;
using tafla::OffsetType;
using tafla::TableLayout;
using Bytes = std::span<const std::uint8_t>;

/// The elements of an array, and items of a vector, at its start and end that
/// are read and compared with the bytes
constexpr std::size_t kChecked = 8;

constexpr std::array kOffsetTypes = {OffsetType::uint8, OffsetType::uint16, OffsetType::uint32};

/// Positions past any bytes, where a position plus a length overflows
constexpr std::array<std::size_t, 5> kFarPositions = {SIZE_MAX, SIZE_MAX - 1, SIZE_MAX - 3, SIZE_MAX - 7, SIZE_MAX / 2};

void check(bool condition, const char* what) {
    if (!condition) {
        std::fprintf(stderr, "Check failed: %s\n", what);
        std::abort();
    }
}

#define CHECK(condition) check(condition, #condition)

std::uint64_t plus(std::uint64_t left, std::uint64_t right) {
    return right > UINT64_MAX - left ? UINT64_MAX : left + right;
}

std::uint64_t times(std::uint64_t left, std::uint64_t right) {
    return left != 0 && right > UINT64_MAX / left ? UINT64_MAX : left * right;
}

/// The `byte_length` bytes at `position`, or nothing when they don't lie
/// within the bytes
std::optional<Bytes> range(Bytes bytes, std::uint64_t position, std::uint64_t byte_length) {
    if (position > bytes.size() || byte_length > bytes.size() - position) {
        return std::nullopt;
    }
    return bytes.subspan(position, byte_length);
}

/// The value of the slot as wide as `offset_type` at `position`, read a byte at
/// a time
std::optional<std::uint64_t> slot(Bytes bytes, std::uint64_t position, OffsetType offset_type) {
    const auto found = range(bytes, position, tafla::offset_size(offset_type));
    if (!found) {
        return std::nullopt;
    }

    std::uint64_t value = 0;
    for (std::size_t byte = found->size(); byte > 0; --byte) {
        value = value << 8 | (*found)[byte - 1];
    }
    return value;
}

/// The bytes of the elements of `element_size` bytes after the count at
/// `position`
std::optional<Bytes> array(Bytes bytes, OffsetType offset_type, std::size_t position, std::size_t element_size) {
    const auto count = slot(bytes, position, offset_type);
    if (!count) {
        return std::nullopt;
    }
    return range(bytes, plus(position, tafla::offset_size(offset_type)), times(*count, element_size));
}

/// Checks that a read failed where it should have, with an error that
/// describes the bytes
void check_error(Bytes bytes, const DecodeError& error, bool expected) {
    CHECK(expected);
    CHECK(std::strlen(error.what()) > 0);

    switch (error.kind()) {
        case DecodeError::Kind::out_of_range:
            CHECK(error.available() == bytes.size());
            break;
        case DecodeError::Kind::invalid_utf8:
            CHECK(error.position() < bytes.size());
            break;
        case DecodeError::Kind::item_ends_before_start:
            CHECK(error.end() < error.position());
            break;
        case DecodeError::Kind::unsupported_version:
            CHECK(false);
            break;
    }
}

/// Checks that a read returns the expected value, or fails with the decode
/// error when there's none
template <class Read, class T>
void check_result(Bytes bytes, Read read, const std::optional<T>& expected) {
    try {
        const auto value = read();
        CHECK(expected.has_value());
        CHECK(static_cast<std::uint64_t>(value) == static_cast<std::uint64_t>(*expected));
    } catch (const DecodeError& error) {
        check_error(bytes, error, !expected.has_value());
    }
}

/// Checks that a read returns the expected bytes, viewed in the input, or
/// fails with the decode error when there are none
template <class Read>
void check_view(Bytes bytes, Read read, const std::optional<Bytes>& expected) {
    try {
        const Bytes view = read();
        CHECK(expected.has_value());
        CHECK(view.data() == expected->data() && view.size() == expected->size());
    } catch (const DecodeError& error) {
        check_error(bytes, error, !expected.has_value());
    }
}

/// Reads the table's size and the offsets of its root fields 0, 1 and
/// `index`
template <class Buffer>
void check_header(const Buffer& buffer, std::size_t index) {
    const auto bytes = buffer.bytes();
    const auto layout = buffer.layout();
    const auto width = layout.offset_size();

    if (layout.is_sized()) {
        check_result(bytes, [&] { return buffer.size(); }, slot(bytes, 0, layout.offset_type()));
    } else {
        CHECK(buffer.size() == bytes.size());
    }

    for (const auto at : {std::size_t{0}, std::size_t{1}, index}) {
        const auto position = plus(times(at, width), layout.header_size());
        check_result(bytes, [&] { return buffer.root_offset(at); }, slot(bytes, position, layout.offset_type()));
    }
}

void check_version(const ByteBuffer<true>& buffer, std::size_t index) {
    const auto layout = buffer.layout();
    const std::size_t position = layout.is_sized() ? layout.offset_size() : 0;
    check_result(
        buffer.bytes(), [&] { return buffer.version(); }, slot(buffer.bytes(), position, layout.offset_type()));
    check_header(buffer, index);
}

template <class T, class Get>
void check_scalar(Bytes bytes, std::size_t position, Get get) {
    const auto expected = range(bytes, position, sizeof(T));
    try {
        const T value = get(position);
        CHECK(expected.has_value());

        // Compared by their bytes, which are the bits of floats
        std::array<std::uint8_t, sizeof(T)> read{};
        tafla::detail::store(read.data(), value);
        CHECK(std::ranges::equal(read, *expected));
    } catch (const DecodeError& error) {
        check_error(bytes, error, !expected.has_value());
    }
}

/// Checks that every way of reading an array's elements reads the elements of
/// its bytes, comparing the first and last `kChecked` by their little-endian
/// bytes
template <class T, class Get>
void check_array(Bytes bytes, OffsetType offset_type, std::size_t position, Get get) {
    const auto expected = array(bytes, offset_type, position, sizeof(T));
    try {
        const auto view = get(position);
        CHECK(expected.has_value());
        CHECK(view.bytes().data() == expected->data() && view.bytes().size() == expected->size());

        const auto size = view.size();
        CHECK(size * sizeof(T) == expected->size());
        CHECK(view.empty() == (size == 0));

        const auto element_bytes = [&](std::size_t index) { return expected->subspan(index * sizeof(T), sizeof(T)); };
        const auto same = [&](T value, std::size_t index) {
            std::array<std::uint8_t, sizeof(T)> read{};
            tafla::detail::store(read.data(), value);
            return std::ranges::equal(read, element_bytes(index));
        };

        auto it = view.begin();
        for (std::size_t index = 0; index < std::min(size, kChecked); ++index, ++it) {
            CHECK(same(*it, index));
            CHECK(same(view[index], index));
        }
        auto back = view.end();
        for (std::size_t index = size; index > size - std::min(size, kChecked); --index) {
            CHECK(same(*--back, index - 1));
        }
        if (size != 0) {
            CHECK(same(view.at(size / 2), size / 2));
        }

        bool past_the_end = false;
        try {
            static_cast<void>(view.at(size));
        } catch (const std::out_of_range&) {
            past_the_end = true;
        }
        CHECK(past_the_end);

        if (size <= kChecked) {
            const auto copy = view.to_vector();
            for (std::size_t index = 0; index < size; ++index) {
                CHECK(same(copy[index], index));
            }
        }

        if (const auto span = view.as_span()) {
            CHECK(static_cast<const void*>(span->data()) == static_cast<const void*>(expected->data()));
            CHECK(span->size() == size);
        }
    } catch (const DecodeError& error) {
        check_error(bytes, error, !expected.has_value());
    }
}

/// Reads the first `kChecked` items of the vector at `position`, by iterating,
/// by index and in place, and the last by index
void check_vector(const ByteBuffer<false>& buffer, std::size_t position) {
    const auto bytes = buffer.bytes();
    const auto offset_type = buffer.layout().offset_type();
    const std::uint64_t width = tafla::offset_size(offset_type);

    // A count, and an end for each item after it, within the bytes
    const auto count = slot(bytes, position, offset_type);
    std::optional<std::uint64_t> ends;
    if (count && range(bytes, plus(position, width), times(*count, width))) {
        ends = count;
    }

    std::optional<tafla::Vector> read;
    try {
        read = buffer.get_vector(position);
    } catch (const DecodeError& error) {
        check_error(bytes, error, !ends.has_value());
        return;
    }
    CHECK(ends.has_value() && read->size() == *ends);

    const auto& vector = *read;
    const auto end_of = [&](std::size_t index) { return *slot(bytes, position + (index + 1) * width, offset_type); };
    const auto check_item = [&](std::size_t index, auto get, auto get_in_place) {
        const auto start = index == 0 ? (*count + 1) * width : end_of(index - 1);
        const auto end = end_of(index);
        const auto rest = bytes.subspan(position);
        std::optional<Bytes> expected;
        if (start <= end) {
            expected = range(rest, start, end - start);
        }

        try {
            const Bytes item = get();
            CHECK(expected.has_value());
            CHECK(item.data() == expected->data() && item.size() == expected->size());
        } catch (const DecodeError& error) {
            if (error.kind() == DecodeError::Kind::item_ends_before_start) {
                CHECK(start > end);
            } else {
                check_error(bytes, error, !expected.has_value());
            }
        }

        try {
            const Bytes item = get_in_place();
            CHECK(expected.has_value());
            CHECK(item.data() == rest.data() + start && item.data() + item.size() == bytes.data() + bytes.size());
        } catch (const DecodeError&) {
            CHECK(!expected.has_value());
        }
    };

    std::size_t index = 0;
    auto in_place = vector.items_in_place().begin();
    for (auto it = vector.items().begin(); it != vector.items().end() && index < kChecked; ++it, ++in_place) {
        check_item(index, [&] { return *it; }, [&] { return *in_place; });
        check_item(index, [&] { return vector.get(index); }, [&] { return vector.get_in_place(index); });
        ++index;
    }

    if (vector.size() != 0) {
        const auto last = vector.size() - 1;
        check_item(last, [&] { return vector.get(last); }, [&] { return vector.get_in_place(last); });
    }

    for (const auto past : {vector.size(), SIZE_MAX}) {
        bool thrown = false;
        try {
            static_cast<void>(vector.get(past));
        } catch (const std::out_of_range&) {
            thrown = true;
        }
        CHECK(thrown);
    }
}

/// Reads every type at `position`, a field of a table there at `index`, and
/// structs of `struct_size` bytes and others
void check_position(const ByteBuffer<false>& buffer, std::size_t position, std::size_t index, std::size_t struct_size) {
    const auto bytes = buffer.bytes();
    const auto offset_type = buffer.layout().offset_type();

    check_scalar<std::int8_t>(bytes, position, [&](std::size_t at) { return buffer.get_int8(at); });
    check_scalar<std::uint8_t>(bytes, position, [&](std::size_t at) { return buffer.get_uint8(at); });
    check_scalar<std::int16_t>(bytes, position, [&](std::size_t at) { return buffer.get_int16(at); });
    check_scalar<std::uint16_t>(bytes, position, [&](std::size_t at) { return buffer.get_uint16(at); });
    check_scalar<std::int32_t>(bytes, position, [&](std::size_t at) { return buffer.get_int32(at); });
    check_scalar<std::uint32_t>(bytes, position, [&](std::size_t at) { return buffer.get_uint32(at); });
    check_scalar<std::int64_t>(bytes, position, [&](std::size_t at) { return buffer.get_int64(at); });
    check_scalar<std::uint64_t>(bytes, position, [&](std::size_t at) { return buffer.get_uint64(at); });
    check_scalar<float>(bytes, position, [&](std::size_t at) { return buffer.get_float32(at); });
    check_scalar<double>(bytes, position, [&](std::size_t at) { return buffer.get_float64(at); });

    const auto at = [&](auto get) { return [&buffer, get](std::size_t where) { return (buffer.*get)(where); }; };
    check_array<std::int8_t>(bytes, offset_type, position, at(&ByteBuffer<false>::get_int8_array));
    check_array<std::int16_t>(bytes, offset_type, position, at(&ByteBuffer<false>::get_int16_array));
    check_array<std::uint16_t>(bytes, offset_type, position, at(&ByteBuffer<false>::get_uint16_array));
    check_array<std::int32_t>(bytes, offset_type, position, at(&ByteBuffer<false>::get_int32_array));
    check_array<std::uint32_t>(bytes, offset_type, position, at(&ByteBuffer<false>::get_uint32_array));
    check_array<std::int64_t>(bytes, offset_type, position, at(&ByteBuffer<false>::get_int64_array));
    check_array<std::uint64_t>(bytes, offset_type, position, at(&ByteBuffer<false>::get_uint64_array));
    check_array<float>(bytes, offset_type, position, at(&ByteBuffer<false>::get_float32_array));
    check_array<double>(bytes, offset_type, position, at(&ByteBuffer<false>::get_float64_array));

    const auto string_bytes = array(bytes, offset_type, position, 1);
    check_view(bytes, [&] { return buffer.get_uint8_array(position); }, string_bytes);
    try {
        const auto string = buffer.get_string(position);
        CHECK(string_bytes.has_value());
        CHECK(static_cast<const void*>(string.data()) == static_cast<const void*>(string_bytes->data()));
        CHECK(tafla::detail::valid_utf8(string_bytes->data(), string_bytes->size()));
    } catch (const DecodeError& error) {
        CHECK(!string_bytes || !tafla::detail::valid_utf8(string_bytes->data(), string_bytes->size()));
        check_error(bytes, error, true);
    }

    const std::size_t remaining = bytes.size() > position ? bytes.size() - position : 0;
    for (const std::size_t size : {std::size_t{0}, std::size_t{1}, struct_size, remaining, remaining + 1, SIZE_MAX}) {
        check_view(bytes, [&] { return buffer.get_struct(position, size); }, range(bytes, position, size));
        check_view(
            bytes, [&] { return buffer.get_struct_array(position, size); }, array(bytes, offset_type, position, size));
    }

    for (const std::size_t field : {std::size_t{0}, index, SIZE_MAX}) {
        const auto slot_position = plus(position, times(field, tafla::offset_size(offset_type)));
        auto expected = slot(bytes, slot_position, offset_type);
        if (expected) {
            expected = plus(position, *expected);
        }
        check_result(bytes, [&] { return buffer.field_offset(position, field); }, expected);
    }

    check_vector(buffer, position);
}

}  // namespace

extern "C" int LLVMFuzzerTestOneInput(const std::uint8_t* data, std::size_t size) {
    if (size < 4) {
        return 0;
    }

    // The first bytes choose what to read, and the rest are the table
    const auto offset_type = kOffsetTypes[data[0] % kOffsetTypes.size()];
    const bool sized = (data[0] & 0x10) != 0;
    const bool far = (data[0] & 0x20) != 0;
    const Bytes bytes(data + 4, size - 4);
    const std::size_t position = far ? kFarPositions[data[1] % kFarPositions.size()] : data[1];
    const std::size_t index = data[2] == 0xff ? SIZE_MAX : data[2];

    auto layout = TableLayout().with_offset_type(offset_type);
    if (sized) {
        layout = layout.sized();
    }

    check_header(ByteBuffer(bytes, layout), index);
    check_version(ByteBuffer(bytes, layout.versioned()), index);
    check_position(ByteBuffer(bytes, layout), position, index, data[3]);

    return 0;
}
