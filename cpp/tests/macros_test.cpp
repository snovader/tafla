// The C++ that tafla-gen generates, and the runtime, compile beside the
// macros that compilers predefine outside strict ISO modes, such as GNU's
// `linux` and `unix`, and those of C's, glibc's and Windows' headers, such as
// `EOF`, `major()` and `min()`, which this test defines as those would.
// The generated code renames fields, types and namespaces of their names,
// and the runtime calls `(std::min)()` rather than `std::min()`.

#include <gtest/gtest.h>

// The standard headers that the runtime includes, which only the runtime and
// the generated code must then compile beside the macros
#include <algorithm>
#include <array>
#include <bit>
#include <cerrno>
#include <compare>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iterator>
#include <limits>
#include <memory>
#include <optional>
#include <ranges>
#include <span>
#include <stdexcept>
#include <string>
#include <string_view>
#include <type_traits>
#include <utility>
#include <variant>
#include <vector>

// NOLINTBEGIN(cppcoreguidelines-macro-usage): they're what's tested
#define linux 1
#define unix 1
#define i386 1
#define sun 1
#define min(a, b) ((a) < (b) ? (a) : (b))
#define max(a, b) ((a) < (b) ? (b) : (a))
#define major(device) 0
#define minor(device) 0
#define near
#define far
#define small char
#define interface struct
// NOLINTEND(cppcoreguidelines-macro-usage)

#include "macros/macros.hpp"

namespace tafla::test {
namespace {

namespace generated = linux_::EOF_::schema_Upper::b_c;

// <cstdio> and <cstddef> define the macros that the schema's types are named
// after, so that the test fails to compile if the generated code keeps them
#if !defined(EOF) || !defined(NULL)
#error "EOF and NULL are defined by the C headers that this test includes"
#endif

TEST(Macros, FieldsAndTypesNamedAfterMacrosAreWrittenAndRead) {
    const std::array<float, 2> low{1.5F, -2.0F};
    const std::array<float, 1> high{3.0F};
    const std::array<generated::NULL_, 2> nulls{{{.x = 0.5}, {.x = 2.5}}};
    const auto bytes =
        generated::MacrosFields{
            .linux_ = 1,
            .unix_ = 2,
            .i386_ = 3,
            .sun_ = 4,
            .min_ = 5,
            .max_ = 6,
            .major_ = 7,
            .minor_ = 8,
            .errno_ = 9,
            .near_ = 10,
            .far_ = 11,
            .small_ = 12,
            .interface_ = {.min_ = low, .max_ = high},
            .eof = {.x = 13},
            .nulls = nulls,
        }
            .serialize();

    const generated::Macros macros(bytes);
    EXPECT_EQ(macros.linux_(), 1);
    EXPECT_EQ(macros.unix_(), 2);
    EXPECT_EQ(macros.i386_(), 3);
    EXPECT_EQ(macros.sun_(), 4);
    EXPECT_EQ(macros.min_(), 5);
    EXPECT_EQ(macros.max_(), 6);
    EXPECT_EQ(macros.major_(), 7);
    EXPECT_EQ(macros.minor_(), 8);
    EXPECT_EQ(macros.errno_(), 9);
    EXPECT_EQ(macros.near_(), 10);
    EXPECT_EQ(macros.far_(), 11);
    EXPECT_EQ(macros.small_(), 12);
    EXPECT_EQ(macros.interface_().min_().to_vector(), (std::vector<float>{1.5F, -2.0F}));
    EXPECT_EQ(macros.interface_().max_().to_vector(), (std::vector<float>{3.0F}));
    EXPECT_EQ(macros.eof().x(), 13);
    EXPECT_EQ(macros.nulls(), (std::vector<generated::NULL_>{{.x = 0.5}, {.x = 2.5}}));
}

}  // namespace
}  // namespace tafla::test
