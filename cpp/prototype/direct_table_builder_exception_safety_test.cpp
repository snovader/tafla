// A setter of the prototype that throws leaves its builder as it was, even
// when an allocation fails partway through it: growing its bytes, keeping a
// segment or an owned value, or moving a table written in place by its
// padding. It replaces the global `operator new` as
// `tests/exception_safety_test.cpp` does, whose setters it ports.

#include <gtest/gtest.h>

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <new>
#include <span>
#include <string>
#include <utility>
#include <vector>

#include "direct_table_builder.hpp"

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

namespace tafla::prototype {
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

/// Values that the builders borrow, long enough to be kept as segments
const std::vector<double> kDoubles(100, 2.5);
const std::vector<std::uint8_t> kBytes(300, 7);

template <class Builder>
Builder owning() {
    Builder builder(2, TableLayout());
    builder.set_string(std::string(40, 's'));
    builder.set_float64_array(std::vector<double>{1.5, -0.0});
    return builder;
}

template <class Builder>
Builder sized() {
    Builder builder(2, TableLayout().sized());
    builder.set_string(std::string(40, 'z'));
    builder.set_uint8_array(std::span<const std::uint8_t>(kBytes));
    return builder;
}

template <class Builder>
using Setter = void (*)(Builder& parent, const Arm& arm);

template <class Builder>
std::vector<Setter<Builder>> setters() {
    using Nested = typename Builder::NestedTable;
    return {
        [](Builder& parent, const Arm& arm) {
            std::string value(40, 's');
            arm();
            parent.set_string(std::move(value));
        },
        [](Builder& parent, const Arm& arm) {
            std::string value(400, 'l');
            arm();
            parent.set_string(std::move(value));
        },
        [](Builder& parent, const Arm& arm) {
            std::vector<std::uint16_t> values{1, 2, 3};
            arm();
            parent.set_uint16_array(std::move(values));
        },
        [](Builder& parent, const Arm& arm) {
            std::vector<double> values(kDoubles);
            arm();
            parent.set_float64_array(std::move(values));
        },
        [](Builder& parent, const Arm& arm) {
            arm();
            parent.set_float64_array(std::span<const double>(kDoubles));
        },
        [](Builder& parent, const Arm& arm) {
            std::vector<std::uint8_t> bytes{1, 2, 3, 4};
            arm();
            parent.set_struct_array(std::move(bytes), 2, Alignment::two);
        },
        [](Builder& parent, const Arm& arm) {
            auto nested = owning<Builder>();
            arm();
            parent.set_table(std::move(nested));
        },
        [](Builder& parent, const Arm& arm) {
            const auto nested = owning<Builder>();
            arm();
            parent.set_table(nested);
        },
        [](Builder& parent, const Arm& arm) {
            const auto nested = sized<Builder>();
            arm();
            parent.set_table(nested);
        },
        [](Builder& parent, const Arm& arm) {
            auto bytes = sized<Builder>().to_bytes();
            arm();
            parent.set_table(std::move(bytes));
        },
        [](Builder& parent, const Arm& arm) {
            arm();
            parent.set_table(3, [](auto& nested) {
                nested.set_uint8(1);
                nested.set_string(std::string(200, 'n'));
                nested.set_float64_array(std::span<const double>(kDoubles));
            });
        },
        [](Builder& parent, const Arm& arm) {
            arm();
            parent.set_vector(20, 2, [](std::size_t index, auto& item) {
                item.set_uint32(static_cast<std::uint32_t>(index));
                item.set_table(1, [&](auto& pair) { pair.set_uint16_array(std::vector<std::uint16_t>(index, 3)); });
            });
        },
        [](Builder& parent, const Arm& arm) {
            const auto borrowed = owning<Builder>();
            std::vector<Nested> items;
            items.emplace_back(owning<Builder>());
            items.emplace_back(borrowed);
            items.emplace_back(sized<Builder>());
            items.emplace_back(sized<Builder>().to_bytes());
            items.emplace_back(sized<Builder>().to_bytes().to_vector());
            items.emplace_back(std::span<const std::uint8_t>(kBytes));
            items.emplace_back(owning<Builder>());
            arm();
            parent.set_vector(std::move(items));
        },
    };
}

/// The parent of the field that a setter sets, between two others, the
/// first of which leaves the field's padding to it
template <class Builder>
Builder parent_before() {
    Builder parent(3, TableLayout());
    parent.set_uint8(5);
    return parent;
}

template <class Builder>
Bytes parent_after(Builder& parent, Setter<Builder> set) {
    set(parent, Arm(-1));
    parent.set_string(std::string(30, 'a'));
    return parent.to_bytes();
}

template <class Builder>
void check_every_allocation() {
    const auto all = setters<Builder>();
    for (std::size_t setter = 0; setter < all.size(); ++setter) {
        const auto set = all[setter];
        auto unfailed = parent_before<Builder>();
        const auto expected = parent_after(unfailed, set);

        for (long allocations = 0;; ++allocations) {
            auto parent = parent_before<Builder>();
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

TEST(DirectTableBuilderExceptionSafety, ASetterWhoseAllocationFailsLeavesTheBuilderAsItWas) {
    check_every_allocation<DirectTableBuilder>();
    check_every_allocation<CopyingTableBuilder>();
}

}  // namespace
}  // namespace tafla::prototype
