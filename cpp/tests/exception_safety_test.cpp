// A setter that throws leaves its builder as it was, even when an allocation
// fails partway through it: growing its bytes, keeping a value where it is or
// owning it, or moving a table written in place by its padding. The test
// replaces the global `operator new`, so that it can fail each allocation of
// a setter in turn, and every other unaligned form of `new` and `delete`, so
// that memory from any of them is freed by its own, as AddressSanitizer
// checks.

#include <gtest/gtest.h>

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <new>
#include <span>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include <tafla/tafla.hpp>

namespace {

/// The allocations left before one fails, or -1 while none is to fail
long allocations_left = -1;

/// The most bytes that an allocation may ask for before it fails
std::size_t largest_allocation = SIZE_MAX;

}  // namespace

void* operator new(std::size_t size) {
    if (size > largest_allocation) {
        throw std::bad_alloc();
    }
    if (allocations_left == 0) {
        allocations_left = -1;
        throw std::bad_alloc();
    }
    if (allocations_left > 0) {
        --allocations_left;
    }

    // NOLINTNEXTLINE(cppcoreguidelines-no-malloc): this is the allocator
    if (void* allocated = std::malloc(size == 0 ? 1 : size)) {
        return allocated;
    }
    throw std::bad_alloc();
}

// GCC sees the pointers that `new` expressions return, which this allocator
// takes from `std::malloc()`, freed with `std::free()`, as a mismatch
#if defined(__GNUC__) && !defined(__clang__)
#pragma GCC diagnostic ignored "-Wmismatched-new-delete"
#endif

void* operator new[](std::size_t size) {
    return ::operator new(size);
}

void* operator new(std::size_t size, const std::nothrow_t& /*nothrow*/) noexcept {
    try {
        return ::operator new(size);
    } catch (const std::bad_alloc&) {
        return nullptr;
    }
}

void* operator new[](std::size_t size, const std::nothrow_t& nothrow) noexcept {
    return ::operator new(size, nothrow);
}

void operator delete(void* allocated) noexcept {
    std::free(allocated);  // NOLINT(cppcoreguidelines-no-malloc)
}

void operator delete(void* allocated, std::size_t /*size*/) noexcept {
    ::operator delete(allocated);
}

void operator delete(void* allocated, const std::nothrow_t& /*nothrow*/) noexcept {
    ::operator delete(allocated);
}

void operator delete[](void* allocated) noexcept {
    ::operator delete(allocated);
}

void operator delete[](void* allocated, std::size_t /*size*/) noexcept {
    ::operator delete(allocated);
}

void operator delete[](void* allocated, const std::nothrow_t& /*nothrow*/) noexcept {
    ::operator delete(allocated);
}

namespace tafla::test {
namespace {

/// Fails the allocation after `allocations` more, when it's called
class Arm {
public:
    explicit Arm(long allocations) noexcept : allocations_(allocations) {}

    void operator()() const noexcept {
        allocations_left = allocations_;
    }

private:
    long allocations_;
};

/// Sets a field of `parent`, after preparing its value and then calling
/// `arm()`, so that only the setter's allocations fail
using Setter = void (*)(TableBuilder& parent, const Arm& arm);

/// Values that the builders borrow, long enough that a builder keeps them
/// where they are rather than copying them
const std::vector<double> kDoubles(100, 2.5);
const std::vector<std::uint8_t> kBytes(300, 7);

/// A builder of a table without a header, which owns a string and an array
TableBuilder owning() {
    TableBuilder builder(2, TableLayout());
    builder.set_string(std::string(40, 's'));
    builder.set_float64_array(std::vector<double>{1.5, -0.0});
    return builder;
}

/// A builder of a sized table, which borrows a long array
TableBuilder sized() {
    TableBuilder builder(2, TableLayout().sized());
    builder.set_string(std::string(40, 'z'));
    builder.set_uint8_array(std::span<const std::uint8_t>(kBytes));
    return builder;
}

/// Setters of every value that a builder copies, keeps or owns, and of
/// tables, nested and written in place, whose bytes grow and move
const std::vector<Setter> setters{
    [](TableBuilder& parent, const Arm& arm) {
        std::string value(40, 's');
        arm();
        parent.set_string(std::move(value));
    },
    [](TableBuilder& parent, const Arm& arm) {
        std::string value(400, 'l');
        arm();
        parent.set_string(std::move(value));
    },
    [](TableBuilder& parent, const Arm& arm) {
        std::vector<std::uint16_t> values{1, 2, 3};
        arm();
        parent.set_uint16_array(std::move(values));
    },
    [](TableBuilder& parent, const Arm& arm) {
        std::vector<double> values(kDoubles);
        arm();
        parent.set_float64_array(std::move(values));
    },
    [](TableBuilder& parent, const Arm& arm) {
        arm();
        parent.set_float64_array(std::span<const double>(kDoubles));
    },
    [](TableBuilder& parent, const Arm& arm) {
        Bools values{true, false, true};
        arm();
        parent.set_bool_array(std::move(values));
    },
    [](TableBuilder& parent, const Arm& arm) {
        std::vector<std::uint8_t> bytes{1, 2, 3, 4};
        arm();
        parent.set_struct_array(std::move(bytes), 2, Alignment::two);
    },
    [](TableBuilder& parent, const Arm& arm) {
        std::vector<std::string> strings{"a", std::string(40, 's'), ""};
        arm();
        parent.set_string_vector(std::move(strings));
    },
    [](TableBuilder& parent, const Arm& arm) {
        std::vector<std::string> strings{"a", std::string(400, 'l'), ""};
        arm();
        parent.set_string_vector(std::move(strings));
    },
    [](TableBuilder& parent, const Arm& arm) {
        auto nested = owning();
        arm();
        parent.set_table(std::move(nested));
    },
    [](TableBuilder& parent, const Arm& arm) {
        const auto nested = owning();
        arm();
        parent.set_table(nested);
    },
    [](TableBuilder& parent, const Arm& arm) {
        const auto nested = sized();
        arm();
        parent.set_table(nested);
    },
    [](TableBuilder& parent, const Arm& arm) {
        auto bytes = sized().to_bytes();
        arm();
        parent.set_table(std::move(bytes));
    },
    [](TableBuilder& parent, const Arm& arm) {
        arm();
        parent.set_table(3, [](TableBuilder::Item& nested) {
            nested.set_uint8(1);
            nested.set_string(std::string(200, 'n'));
            nested.set_float64_array(std::span<const double>(kDoubles));
        });
    },
    [](TableBuilder& parent, const Arm& arm) {
        arm();
        parent.set_vector(20, 2, [](std::size_t index, TableBuilder::Item& item) {
            item.set_uint32(static_cast<std::uint32_t>(index));
            item.set_table(
                1, [&](TableBuilder::Item& pair) { pair.set_uint16_array(std::vector<std::uint16_t>(index, 3)); });
        });
    },
    [](TableBuilder& parent, const Arm& arm) {
        const auto borrowed = owning();
        std::vector<NestedTable> items;
        items.emplace_back(owning());
        items.emplace_back(borrowed);
        items.emplace_back(sized());
        items.emplace_back(sized().to_bytes());
        items.emplace_back(sized().to_bytes().to_vector());
        items.emplace_back(std::span<const std::uint8_t>(kBytes));
        items.emplace_back(owning());
        arm();
        parent.set_vector(std::move(items));
    },
};

/// The parent of the field that `set` sets, between two others, the first
/// of which leaves the field's padding to it
TableBuilder parent_before() {
    TableBuilder parent(3, TableLayout());
    parent.set_uint8(5);
    return parent;
}

Bytes parent_after(TableBuilder& parent, Setter set) {
    set(parent, Arm(-1));
    parent.set_string(std::string(30, 'a'));
    return parent.to_bytes();
}

TEST(ExceptionSafety, ASetterWhoseAllocationFailsLeavesTheBuilderAsItWas) {
    for (std::size_t setter = 0; setter < setters.size(); ++setter) {
        const auto set = setters[setter];
        auto unfailed = parent_before();
        const auto expected = parent_after(unfailed, set);

        // Each allocation in turn, until the setter makes no more
        for (long allocations = 0;; ++allocations) {
            auto parent = parent_before();
            const auto length = parent.byte_length();
            const auto alignment = parent.alignment();

            bool failed = false;
            try {
                set(parent, Arm(allocations));
            } catch (const std::bad_alloc&) {
                failed = true;
            }
            allocations_left = -1;

            if (!failed) {
                parent.set_string(std::string(30, 'a'));
                EXPECT_EQ(parent.to_bytes(), expected) << "setter " << setter;
                break;
            }

            EXPECT_EQ(parent.byte_length(), length) << "setter " << setter << ", allocation " << allocations;
            EXPECT_EQ(parent.alignment(), alignment) << "setter " << setter << ", allocation " << allocations;
            EXPECT_EQ(parent_after(parent, set), expected) << "setter " << setter << ", allocation " << allocations;
        }
    }
}

/// Fails the allocations of more than `bytes` while it lives
class AllocationsUpTo {
public:
    explicit AllocationsUpTo(std::size_t bytes) noexcept {
        largest_allocation = bytes;
    }

    AllocationsUpTo(const AllocationsUpTo&) = delete;
    AllocationsUpTo& operator=(const AllocationsUpTo&) = delete;

    ~AllocationsUpTo() {
        largest_allocation = SIZE_MAX;
    }
};

/// A vector written in place whose first item is far longer than the
/// others, here a vector of 5,000 strings of 200 bytes before 9,999 items of
/// one byte, allocates in proportion to the table of 1.15 MB, rather than
/// making room for 9,999 more items as long as the first
TEST(Allocation, AVectorOfUnevenItemsAllocatesInProportionToItsTable) {
    const std::string string(200, 's');
    const AllocationsUpTo limit(8 << 20);

    TableBuilder builder(1, TableLayout());
    builder.set_vector(10'000, 1, [&](std::size_t index, TableBuilder::Item& item) {
        if (index == 0) {
            item.set_vector(
                5'000, 1, [&](std::size_t, TableBuilder::Item& inner) { inner.set_string(std::string_view(string)); });
        } else {
            item.set_uint8(1);
        }
    });
    EXPECT_EQ(builder.to_bytes().size(), 1'150'011U);
}

/// A vector written in place that makes its table too long keeps no more
/// than the table could hold, and an item, rather than every item until the
/// last: here 2,000 items of 240 strings of 250 bytes, 122 MB, in a table of
/// uint16 offsets, which hold 64 KiB. Its error has the vector's whole
/// length, as a vector of builders' has, which is checked before they're
/// written, and the builder is left as it was.
TEST(Allocation, AVectorTooLongForItsTableKeepsNoMoreThanItCould) {
    const std::string string(250, 's');
    const AllocationsUpTo limit(1 << 20);

    TableBuilder builder(1, TableLayout().with_offset_type(OffsetType::uint16));
    try {
        builder.set_vector(2'000, 240, [&](std::size_t, TableBuilder::Item& item) {
            for (int field = 0; field < 240; ++field) {
                item.set_string(std::string_view(string));
            }
        });
        FAIL() << "The vector wasn't rejected";
    } catch (const BuildError& error) {
        // The header, the count and the items' ends, and each item's offsets
        // and strings
        EXPECT_EQ(error.kind(), BuildError::Kind::too_long);
        EXPECT_EQ(error.length(), 2U + 2 * 2'001 + 2'000 * (240 * 2 + 240 * (2 + 250)));
    }

    builder.set_vector(1, 1, [&](std::size_t, TableBuilder::Item& item) { item.set_string(std::string_view(string)); });
    EXPECT_EQ(builder.to_bytes().size(), 2U + 2 * 2 + 2 + 2 + 250);
}

}  // namespace
}  // namespace tafla::test
