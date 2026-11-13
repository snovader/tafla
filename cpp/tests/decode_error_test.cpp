// Readers decode untrusted bytes: any bytes, and valid tables cut short or
// overwritten, read as values or fail with a `DecodeError`, and never crash.
// The examples document the errors.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <cstddef>
#include <cstdint>
#include <limits>
#include <sstream>
#include <string>
#include <vector>

#include "common/at_byte_offset.hpp"
#include "common/build.hpp"
#include "common/read.hpp"
#include "common/strategy.hpp"

namespace tafla::test {
namespace {

/// Bytes, mostly small, so that offsets, counts and lengths often lie within
/// them, and some 0xff
rc::Gen<std::uint8_t> byte() {
    return rc::gen::weightedOneOf<std::uint8_t>({
        {4, rc::gen::just(std::uint8_t{0})},
        {4, rc::gen::inRange<std::uint8_t>(1, 16)},
        {1, rc::gen::just(std::uint8_t{0xff})},
        {1, rc::gen::arbitrary<std::uint8_t>()},
    });
}

rc::Gen<std::vector<std::uint8_t>> bytes(std::size_t min, std::size_t max) {
    return rc::gen::mapcat(rc::gen::inRange(min, max), [](std::size_t count) {
        return rc::gen::container<std::vector<std::uint8_t>>(count, byte());
    });
}

/// Positions or indexes within small bytes, up to `small`, and up to the
/// largest
rc::Gen<std::size_t> up_to_largest(std::size_t small) {
    return rc::gen::resize(rc::kNominalSize, rc::gen::weightedOneOf<std::size_t>({
                                                 {8, rc::gen::inRange<std::size_t>(0, small)},
                                                 {1, rc::gen::just(SIZE_MAX)},
                                                 {1, rc::gen::arbitrary<std::size_t>()},
                                             }));
}

/// What a read returned, its value or its error, as text to compare between
/// copies of bytes
template <class Read>
std::string result_of(Read read) {
    std::ostringstream text;
    try {
        if constexpr (std::is_same_v<decltype(read()), std::span<const std::uint8_t>>) {
            const auto bytes = read();
            for (auto byte : bytes) {
                text << static_cast<int>(byte) << " ";
            }
        } else {
            text << read();
        }
    } catch (const DecodeError& error) {
        text << "DecodeError: " << error.what();
    } catch (const ReadMismatch& error) {
        text << "ReadMismatch: " << error.what();
    }
    return text.str();
}

/// The errors of each read, which must be decode errors of reads that would
/// leave the bytes, or read mismatches other than of finding vector items
struct Errors {
    std::vector<DecodeError> decode;
    std::vector<std::string> mismatches;
};

template <class Read>
void collect(Errors& errors, Read read) {
    try {
        static_cast<void>(read());
    } catch (const DecodeError& error) {
        errors.decode.push_back(error);
    } catch (const ReadMismatch& error) {
        errors.mismatches.emplace_back(error.what());
    }
}

/// Reads `bytes` as `table`, and reads each of its root fields at its own root
/// offset, at root offset `index` and at `position`, and a struct and an
/// array of structs of `size` bytes at `position`, as text of each read
std::vector<std::string> reads_of(std::span<const std::uint8_t> bytes, const Table& table, std::size_t position,
                                  std::size_t index, std::size_t size, Errors* errors) {
    const auto& layout = table.layout;
    const ByteBuffer buffer(bytes, layout.unversioned());
    const ByteBuffer versioned(bytes, layout.unversioned().versioned());

    std::vector<std::string> reads;
    const auto read = [&](auto function) {
        reads.push_back(result_of(function));
        if (errors != nullptr) {
            collect(*errors, function);
        }
    };

    read([&] { return read_table(bytes, table); });
    for (std::size_t root_index = 0; root_index < table.fields.size(); ++root_index) {
        const auto& field = table.fields[root_index];
        read([&] { return read_root_field(bytes, layout, root_index, field); });
        read([&] { return read_root_field(bytes, layout, index, field); });
        read([&] { return read_field(bytes, layout, position, field); });
    }

    read([&] { return buffer.size(); });
    read([&] { return versioned.version(); });
    read([&] { return buffer.field_offset(position, index); });
    read([&] { return buffer.get_struct(position, size); });
    read([&] { return buffer.get_struct_array(position, size); });
    return reads;
}

/// A field that lies before the cut reads the value that the table was built
/// with, and any other fails with `out_of_range`
RC_GTEST_PROP(DecodeError, TruncatedTablesReadTheirValuesOrOutOfRange, ()) {
    const auto table = *strategy::table();
    const auto cuts = *rc::gen::resize(
        rc::kNominalSize, rc::gen::mapcat(rc::gen::inRange<std::size_t>(1, 17), [](std::size_t count) {
            return rc::gen::container<std::vector<std::uint64_t>>(count, rc::gen::arbitrary<std::uint64_t>());
        }));

    const auto bytes = to_bytes(table).to_vector();
    RC_PRE(!bytes.empty());

    for (const auto cut : cuts) {
        // A copy, so that no read can reach the bytes after the cut
        const std::vector<std::uint8_t> truncated(bytes.begin(),
                                                  bytes.begin() + static_cast<std::ptrdiff_t>(cut % bytes.size()));

        for (std::size_t index = 0; index < table.fields.size(); ++index) {
            try {
                RC_ASSERT(read_root_field(truncated, table.layout, index, table.fields[index]) == table.fields[index]);
            } catch (const tafla::DecodeError& error) {
                RC_ASSERT(error.kind() == tafla::DecodeError::Kind::out_of_range);
            }
        }
    }
}

/// Overwritten bytes can hold other values, offsets, counts and item ends,
/// which read as other values, as vectors of another number of items or as a
/// size other than the table's length, or fail with a `DecodeError`. Finding a
/// vector item from its ends finds what iterating finds, whatever they hold.
RC_GTEST_PROP(DecodeError, OverwrittenTablesReadValuesOrDecodeErrors, ()) {
    const auto table = *strategy::table();
    const auto overwrites =
        *rc::gen::resize(rc::kNominalSize, rc::gen::mapcat(rc::gen::inRange<std::size_t>(1, 5), [](std::size_t count) {
                             return rc::gen::container<std::vector<std::pair<std::uint64_t, std::uint8_t>>>(
                                 count, rc::gen::pair(rc::gen::arbitrary<std::uint64_t>(), byte()));
                         }));

    auto bytes = to_bytes(table).to_vector();
    RC_PRE(!bytes.empty());

    for (const auto& [index, byte] : overwrites) {
        // Less than the size, so within a size_t
        bytes[static_cast<std::size_t>(index % bytes.size())] = byte;
    }

    Errors errors;
    collect(errors, [&] { return read_table(bytes, table); });
    for (std::size_t index = 0; index < table.fields.size(); ++index) {
        collect(errors, [&] { return read_root_field(bytes, table.layout, index, table.fields[index]); });
    }

    for (const auto& mismatch : errors.mismatches) {
        RC_ASSERT(mismatch.find("Finding item") == std::string::npos);
    }
}

/// Every getter, at the root offsets of a table of any layout, in the tables
/// nested in it and in its vectors, and at any position and field offset, and
/// structs of any size, returns a value or a `DecodeError`. Bytes read the
/// same within larger bytes as on their own, at any address, and an
/// `out_of_range` error describes a read that would leave them. Finding a
/// vector item from its ends finds what iterating finds.
RC_GTEST_PROP(DecodeError, ArbitraryBytesReadAsValuesOrDecodeErrors, ()) {
    const auto table_bytes = *bytes(0, 64);
    const auto before = *bytes(0, 8);
    const auto after = *bytes(0, 8);
    const auto byte_offset = *rc::gen::inRange<std::size_t>(0, 8);
    const auto table = *strategy::table();
    const auto position = *up_to_largest(80);
    const auto index = *up_to_largest(12);
    const auto size = *up_to_largest(16);

    auto backing = before;
    backing.insert(backing.end(), table_bytes.begin(), table_bytes.end());
    backing.insert(backing.end(), after.begin(), after.end());
    const std::span<const std::uint8_t> within(backing.data() + before.size(), table_bytes.size());
    const AtByteOffset copy(table_bytes, byte_offset);

    Errors errors;
    const auto reads = reads_of(copy.bytes(), table, position, index, size, &errors);
    RC_ASSERT(reads_of(within, table, position, index, size, nullptr) == reads);

    for (const auto& mismatch : errors.mismatches) {
        RC_ASSERT(mismatch.find("Finding item") == std::string::npos);
    }

    for (const auto& error : errors.decode) {
        if (error.kind() == tafla::DecodeError::Kind::out_of_range) {
            // The read ends past the bytes, without overflowing
            RC_ASSERT(error.position() > error.available() ||
                      error.byte_length() > error.available() - error.position());
            RC_ASSERT(error.available() <= table_bytes.size());
        }
    }
}

/// Throws the decode error of `read`
template <class Read>
tafla::DecodeError error_of(Read read) {
    try {
        static_cast<void>(read());
    } catch (const tafla::DecodeError& error) {
        return error;
    }
    throw std::logic_error("The read didn't fail");
}

void expect_outside(const tafla::DecodeError& error, std::size_t position, std::uint64_t byte_length,
                    std::size_t available = 8) {
    EXPECT_EQ(error.kind(), tafla::DecodeError::Kind::out_of_range);
    EXPECT_EQ(error.position(), position);
    EXPECT_EQ(error.byte_length(), byte_length);
    EXPECT_EQ(error.available(), available);
}

TEST(DecodeError, ThrownOutsideTheBytes) {
    const std::vector<std::uint8_t> backing(32, 0);
    const ByteBuffer buffer{std::span(backing).first(8), TableLayout()};

    expect_outside(error_of([&] { return buffer.get_float64(4); }), 4, 8);
    EXPECT_EQ(std::string(error_of([&] { return buffer.get_int64(SIZE_MAX); }).what()),
              "Read exceeds the supplied buffer: 8 bytes at " + std::to_string(SIZE_MAX) + " of 8");
    EXPECT_THROW(static_cast<void>(buffer.get_uint32(5)), tafla::DecodeError);
    EXPECT_THROW(static_cast<void>(buffer.get_int8(8)), tafla::DecodeError);
    EXPECT_EQ(buffer.get_uint32(4), 0U);

    EXPECT_THROW(static_cast<void>(buffer.root_offset(SIZE_MAX)), tafla::DecodeError);
    EXPECT_THROW(static_cast<void>(buffer.field_offset(SIZE_MAX, SIZE_MAX)), tafla::DecodeError);
    EXPECT_THROW(static_cast<void>(buffer.field_offset(4, 1)), tafla::DecodeError);

    expect_outside(error_of([&] { return buffer.get_struct(4, 5); }), 4, 5);
    EXPECT_EQ(buffer.get_struct(4, 4).size(), 4U);
}

/// 8 bytes holding `uint32s` after 4 bytes, followed by bytes that a read
/// past their end would return
std::vector<std::uint8_t> slice_of(std::initializer_list<std::uint32_t> uint32s) {
    std::vector<std::uint8_t> backing(32, 65);
    std::size_t position = 4;
    for (auto uint32 : uint32s) {
        detail::store(backing.data() + position, uint32);
        position += 4;
    }
    return backing;
}

// The slice holds a vector's count of 1 and its item's end at 13, which is
// also read as the count of an array, string or array of structs
TEST(DecodeError, ThrownForAnArrayStringStructOrVectorItemOutsideTheBytes) {
    const auto backing = slice_of({1, 13});
    const ByteBuffer buffer{std::span(backing).subspan(4, 8), TableLayout()};

    expect_outside(error_of([&] { return buffer.get_uint8_array(4); }), 8, 13);
    expect_outside(error_of([&] { return buffer.get_string(4); }), 8, 13);
    expect_outside(error_of([&] { return buffer.get_float32_array(4); }), 8, 52);
    expect_outside(error_of([&] { return buffer.get_struct_array(4, 2); }), 8, 26);

    // The vector's end lies within the slice, and its item doesn't
    const auto vector = buffer.get_vector(0);
    expect_outside(error_of([&] { return vector.get_in_place(0); }), 8, 5);
    expect_outside(error_of([&] { return *vector.items_in_place().begin(); }), 8, 5);
    expect_outside(error_of([&] { return vector.get(0); }), 8, 5);
    expect_outside(error_of([&] { return *vector.items().begin(); }), 8, 5);
}

TEST(DecodeError, ThrownForAVectorWhoseEndsLieOutsideTheBytes) {
    const auto backing = slice_of({3});
    const ByteBuffer buffer{std::span(backing).subspan(4, 8), TableLayout()};

    // The count of 3, at 0, has three ends, of 12 bytes from 4. The count is
    // checked before any item is found, so an index past it fails with the
    // decode error too, as in every runtime.
    expect_outside(error_of([&] { return buffer.get_vector(0); }), 4, 12);
    expect_outside(error_of([&] { return buffer.get_vector(0).get(3); }), 4, 12);
}

TEST(DecodeError, ThrownForAVectorItemThatEndsBeforeItStarts) {
    // A count of 2 puts the first item after the ends, at 12, but it ends at
    // 11
    const std::vector<std::uint8_t> bytes{2, 0, 0, 0, 11, 0, 0, 0, 12, 0, 0, 0};
    const auto vector = ByteBuffer(bytes, TableLayout()).get_vector(0);

    const auto expect_ends_before_start = [](const tafla::DecodeError& error) {
        EXPECT_EQ(error.kind(), tafla::DecodeError::Kind::item_ends_before_start);
        EXPECT_EQ(error.position(), 12U);
        EXPECT_EQ(error.end(), 11U);
        EXPECT_STREQ(error.what(), "The vector item at 12 ends before it starts, at 11");
    };
    expect_ends_before_start(error_of([&] { return vector.get_in_place(0); }));
    expect_ends_before_start(error_of([&] { return *vector.items_in_place().begin(); }));
    expect_ends_before_start(error_of([&] { return vector.get(0); }));
    expect_ends_before_start(error_of([&] { return *vector.items().begin(); }));

    // An index past the count is the caller's mistake rather than the bytes'
    EXPECT_THROW(static_cast<void>(vector.get_in_place(2)), std::out_of_range);
    EXPECT_THROW(static_cast<void>(vector.get(2)), std::out_of_range);
}

TEST(DecodeError, ThrownForAnArrayWhoseByteLengthOverflows) {
    const std::vector<std::uint8_t> bytes{255, 255, 255, 255, 0, 0, 0, 0};
    const ByteBuffer buffer(bytes, TableLayout());

    expect_outside(error_of([&] { return buffer.get_float64_array(0); }), 4, 8ULL * 0xffff'ffff);

    // Structs as large as a size_t make byte lengths past 2^64 on 64-bit
    // hosts, which saturate
    expect_outside(error_of([&] { return buffer.get_struct_array(0, SIZE_MAX); }), 4,
                   sizeof(std::size_t) == 8 ? UINT64_MAX : 0xffff'ffffULL * SIZE_MAX);
}

TEST(DecodeError, ThrownForInvalidUtf8) {
    const std::vector<std::pair<const char*, std::vector<std::uint8_t>>> cases{
        {"a lone continuation byte", {0x80}},
        {"a truncated sequence", {0xe2, 0x82}},
        {"an overlong encoding", {0xc0, 0xaf}},
        {"an encoded surrogate", {0xed, 0xa0, 0x80}},
    };

    for (const auto& [name, utf8] : cases) {
        std::vector<std::uint8_t> bytes(4);
        detail::store(bytes.data(), static_cast<std::uint32_t>(utf8.size()));
        bytes.insert(bytes.end(), utf8.begin(), utf8.end());

        const auto error = error_of([&] { return ByteBuffer(bytes, TableLayout()).get_string(0); });
        EXPECT_EQ(error.kind(), tafla::DecodeError::Kind::invalid_utf8) << name;
        EXPECT_EQ(error.position(), 0U) << name;
        EXPECT_STREQ(error.what(), "The string at 0 isn't valid UTF-8") << name;
    }
}

TEST(DecodeError, DescribesAnUnsupportedVersion) {
    const auto layout = TableLayout().sized().versioned();
    TableBuilder builder(1, layout, 2);
    builder.set_uint8(42);
    const auto bytes = builder.to_bytes();

    const auto version = ByteBuffer(bytes, layout).version();
    const auto error = tafla::DecodeError::unsupported_version(version, 1);

    EXPECT_EQ(error.kind(), tafla::DecodeError::Kind::unsupported_version);
    EXPECT_EQ(error.version(), 2U);
    EXPECT_EQ(error.supported(), 1U);
    EXPECT_STREQ(error.what(), "Unsupported format version 2; the reader reads version 1");
}

TEST(DecodeError, IsAnErrorOfTafla) {
    const auto error = tafla::DecodeError::invalid_utf8(3);
    const tafla::Error& base = error;
    const std::runtime_error& standard = error;
    EXPECT_STREQ(base.what(), standard.what());
}

}  // namespace
}  // namespace tafla::test
