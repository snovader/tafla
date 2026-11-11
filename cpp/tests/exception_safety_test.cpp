// A setter that throws leaves its builder as it was, even when an allocation
// fails partway through it. The test replaces the global `operator new`, so
// that it can fail each allocation of a setter in turn, and every other
// unaligned form of `new` and `delete`, so that memory from any of them is
// freed by its own, as AddressSanitizer checks.

#include <gtest/gtest.h>

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <new>
#include <string>
#include <utility>
#include <vector>

#include <tafla/tafla.hpp>

namespace {

/// The allocations left before one fails, or -1 while none is to fail
long allocations_left = -1;

}  // namespace

void* operator new(std::size_t size) {
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

/// A builder of a table without a header, which owns a string and an array
TableBuilder owning() {
    TableBuilder builder(2, TableLayout());
    builder.set_string(std::string(40, 's'));
    builder.set_float64_array(std::vector<double>{1.5, -0.0});
    return builder;
}

/// A builder of a sized table, which is nested as its own bytes
TableBuilder sized() {
    TableBuilder builder(1, TableLayout().sized());
    builder.set_string(std::string(40, 'z'));
    return builder;
}

const std::vector<Setter> setters{
    [](TableBuilder& parent, const Arm& arm) {
        std::string value(40, 's');
        arm();
        parent.set_string(std::move(value));
    },
    [](TableBuilder& parent, const Arm& arm) {
        std::vector<std::uint16_t> values{1, 2, 3};
        arm();
        parent.set_uint16_array(std::move(values));
    },
    [](TableBuilder& parent, const Arm& arm) {
        std::vector<std::uint8_t> bytes{1, 2, 3, 4};
        arm();
        parent.set_struct_array(std::move(bytes), 2, Alignment::two);
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
        auto bytes = owning().to_bytes();
        arm();
        parent.set_table(std::move(bytes));
    },
    [](TableBuilder& parent, const Arm& arm) {
        const auto borrowed = owning();
        std::vector<NestedTable> items;
        items.emplace_back(owning());
        items.emplace_back(borrowed);
        items.emplace_back(sized());
        items.emplace_back(owning().to_bytes());
        items.emplace_back(owning().to_bytes().to_vector());
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

}  // namespace
}  // namespace tafla::test
