// What the codecs share: the bytes of what they write, their errors, and the
// shapes that each one encodes

#ifndef TAFLA_BENCHMARKS_CODEC_HPP
#define TAFLA_BENCHMARKS_CODEC_HPP

#include <array>
#include <cstddef>
#include <cstdint>
#include <span>
#include <stdexcept>
#include <string>
#include <vector>

#include <tafla/tafla.hpp>

#include "workloads.hpp"

namespace tafla::benchmarks {

/// A library's error of a failed read, which the timed calls never have
class CodecError : public std::runtime_error {
public:
    using std::runtime_error::runtime_error;
};

/// The bytes that a library wrote, in whatever holds them: tafla's `Bytes`,
/// a `std::string`, FlatBuffers' `DetachedBuffer` or msgpack's `sbuffer`
template <class Written>
std::span<const std::uint8_t> bytes_of(const Written& written) {
    return {reinterpret_cast<const std::uint8_t*>(written.data()), written.size()};
}

/// A library that encodes the shape `S`, writing its value into bytes that it
/// owns and reading them back
template <class Codec, class S>
concept Encodes = requires(const Codec& codec, const typename S::Value& value, std::span<const std::uint8_t> bytes) {
    bytes_of(codec.serialize(S{}, value));
    { codec.deserialize(S{}, bytes) } -> std::same_as<typename S::Value>;
};

/// A library that also sums a document's column `a` through its own
/// per-element accessor
template <class Codec>
concept SumsColumns = requires(const Codec& codec, std::span<const std::uint8_t> bytes) {
    { codec.sum_a(bytes) } -> std::same_as<double>;
};

}  // namespace tafla::benchmarks

#endif
