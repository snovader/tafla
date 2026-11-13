// The bits of a scalar read as a value, which 32-bit x86 may quiet when it's
// a signalling NaN

#ifndef TAFLA_TEST_READ_BITS_HPP
#define TAFLA_TEST_READ_BITS_HPP

#include <type_traits>

#include <tafla/element.hpp>

namespace tafla::test {

/// Whether the host's ABI passes floats through x87 registers, which quiet a
/// signalling NaN as they load it: 32-bit x86, which returns floats and
/// doubles on the x87 stack. Whether a value read passes through them
/// depends on the compiler's code, so it keeps its bits or is quieted.
inline constexpr bool kX87Floats =
#if defined(__i386__) || defined(_M_IX86)
    true;
#else
    false;
#endif

/// Whether `read`, the bits of a `T` read as a value, are `expected`, or, for
/// a float on a host of x87 floats, `expected` quieted where it's a
/// signalling NaN
template <class T>
bool same_read_bits(detail::BitsOf<T> read, detail::BitsOf<T> expected) {
    if (read == expected) {
        return true;
    }

    if constexpr (kX87Floats && std::is_floating_point_v<T>) {
        using Bits = detail::BitsOf<T>;
        constexpr int mantissa = sizeof(T) == 4 ? 23 : 52;
        constexpr Bits quiet = Bits{1} << (mantissa - 1);
        constexpr Bits exponent = ((Bits{1} << (8 * sizeof(T) - 1 - mantissa)) - 1) << mantissa;
        constexpr Bits fraction = (Bits{1} << mantissa) - 1;

        const bool signalling =
            (expected & exponent) == exponent && (expected & fraction) != 0 && (expected & quiet) == 0;
        return signalling && read == (expected | quiet);
    }
    return false;
}

}  // namespace tafla::test

#endif
