// The number types that tables store in arrays, and the little-endian loads
// and stores that every read and write goes through.

#ifndef TAFLA_ELEMENT_HPP
#define TAFLA_ELEMENT_HPP

#include <bit>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <limits>

#if defined(__GNUC__) || defined(__clang__)
#define TAFLA_COLD [[gnu::cold, gnu::noinline]]
#define TAFLA_ALWAYS_INLINE [[gnu::always_inline]] inline
#elif defined(_MSC_VER)
#define TAFLA_COLD __declspec(noinline)
#define TAFLA_ALWAYS_INLINE __forceinline
#else
#define TAFLA_COLD
#define TAFLA_ALWAYS_INLINE inline
#endif

static_assert(std::endian::native == std::endian::little || std::endian::native == std::endian::big,
              "tafla reads and writes on little- and big-endian hosts only");
static_assert(std::numeric_limits<float>::is_iec559 && std::numeric_limits<double>::is_iec559,
              "tafla's floats are IEEE 754 binary32 and binary64");
static_assert(std::numeric_limits<unsigned char>::digits == 8, "tafla's bytes are octets");

namespace tafla {

/// A number type that tables store in arrays: `std::int8_t` … `std::uint64_t`,
/// `float` or `double`. Elements are stored little-endian and aligned to their
/// size from the table's start.
template <class T>
concept Element = std::same_as<T, std::int8_t> || std::same_as<T, std::uint8_t> || std::same_as<T, std::int16_t> ||
                  std::same_as<T, std::uint16_t> || std::same_as<T, std::int32_t> || std::same_as<T, std::uint32_t> ||
                  std::same_as<T, std::int64_t> || std::same_as<T, std::uint64_t> || std::same_as<T, float> ||
                  std::same_as<T, double>;

namespace detail {

/// Whether a little-endian table's arrays are in the host's byte order, so
/// that they're read as views and written with a copy of their bytes.
///
/// Defining `TAFLA_FORCE_BYTE_SWAP` makes a little-endian host take the path
/// of a big-endian one, which converts every element on its own, so that the
/// tests cover it on any host. The values read and written stay the same.
#if defined(TAFLA_FORCE_BYTE_SWAP)
inline constexpr bool kHostOrder = false;
#else
inline constexpr bool kHostOrder = std::endian::native == std::endian::little;
#endif

/// The unsigned integer of a type's size, which holds its bits
template <std::size_t Size>
struct Bits;

template <>
struct Bits<1> {
    using type = std::uint8_t;
};

template <>
struct Bits<2> {
    using type = std::uint16_t;
};

template <>
struct Bits<4> {
    using type = std::uint32_t;
};

template <>
struct Bits<8> {
    using type = std::uint64_t;
};

template <class T>
using BitsOf = typename Bits<sizeof(T)>::type;

/// Reverses the bytes of an unsigned integer, as C++23's `std::byteswap()` does
template <std::unsigned_integral U>
constexpr U byte_swap(U value) noexcept {
    if constexpr (sizeof(U) == 1) {
        return value;
    } else {
#if defined(__GNUC__) || defined(__clang__)
        if constexpr (sizeof(U) == 2) {
            return __builtin_bswap16(value);
        } else if constexpr (sizeof(U) == 4) {
            return __builtin_bswap32(value);
        } else {
            return __builtin_bswap64(value);
        }
#else
        U swapped = 0;
        for (std::size_t byte = 0; byte < sizeof(U); ++byte) {
            swapped = static_cast<U>((swapped << 8) | ((value >> (8 * byte)) & 0xff));
        }
        return swapped;
#endif
    }
}

/// Reads the little-endian `T` at `bytes`, which needn't be aligned
template <class T>
TAFLA_ALWAYS_INLINE T load(const std::uint8_t* bytes) noexcept {
    BitsOf<T> bits;
    std::memcpy(&bits, bytes, sizeof bits);
    if constexpr (std::endian::native == std::endian::big) {
        bits = byte_swap(bits);
    }

    return std::bit_cast<T>(bits);
}

/// Writes `value` little-endian at `bytes`, which needn't be aligned
template <class T>
TAFLA_ALWAYS_INLINE void store(std::uint8_t* bytes, T value) noexcept {
    auto bits = std::bit_cast<BitsOf<T>>(value);
    if constexpr (std::endian::native == std::endian::big) {
        bits = byte_swap(bits);
    }

    std::memcpy(bytes, &bits, sizeof bits);
}

/// Whether `bytes` are aligned in memory for `T`
template <class T>
bool is_aligned(const std::uint8_t* bytes) noexcept {
    return reinterpret_cast<std::uintptr_t>(bytes) % alignof(T) == 0;
}

}  // namespace detail

/// Reads the little-endian `T` at `bytes`, which must hold `sizeof(T)` bytes
/// at any alignment, as a struct's scalars lie at fixed offsets
template <Element T>
[[nodiscard]] TAFLA_ALWAYS_INLINE T load_le(const std::uint8_t* bytes) noexcept {
    return detail::load<T>(bytes);
}

/// Writes `value` little-endian at `bytes`, which must have room for
/// `sizeof(T)` bytes at any alignment, as a struct's scalars lie at fixed
/// offsets
template <Element T>
TAFLA_ALWAYS_INLINE void store_le(std::uint8_t* bytes, T value) noexcept {
    detail::store(bytes, value);
}

}  // namespace tafla

#endif
