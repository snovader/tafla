// Reads tables with `ByteBuffer` as their descriptions set them out, and
// returns the descriptions with the values read in place of theirs, so that
// they equal the descriptions when the bytes hold the described values.
//
// A nested table, from `set_table()` or in a vector, is read in place when it
// has no header and its parent's offset type: a field from its position in
// its parent's bytes, and a vector item from the bytes that
// `Vector::items_in_place()` returns, which end where its parent's do. Any
// other is read as a table of its own: a field from its position to the end
// of its parent, since its length isn't written, and a vector item from the
// bytes that `Vector::items()` returns. The scalars of structs are read at
// their offsets in the bytes that `get_struct()` and `get_struct_array()`
// return.
//
// A read throws the runtime's `DecodeError`, or a `ReadMismatch` when the
// bytes read otherwise than a table's would.

#ifndef TAFLA_TEST_READ_HPP
#define TAFLA_TEST_READ_HPP

#include <cstddef>
#include <cstdint>
#include <span>
#include <stdexcept>

#include "common/description.hpp"

namespace tafla::test {

/// Thrown when bytes read otherwise than the runtime's reads would: a sized
/// table's size isn't its length, a vector holds another number of items
/// than described, or finding an item from its ends finds another than
/// iterating does
class ReadMismatch : public std::runtime_error {
public:
    using std::runtime_error::runtime_error;
};

/// Reads a table from its own bytes, as the root or a vector item is read:
/// its header, whose size must be the table's byte length when it's sized,
/// and then its fields
Table read_table(std::span<const std::uint8_t> bytes, const Table& table);

/// Reads root field `index` of a table of `layout` as `field`, without
/// reading the header
Field read_root_field(std::span<const std::uint8_t> bytes, const Layout& layout, std::size_t index, const Field& field);

/// Reads `field` at `position` of a table of `layout`
Field read_field(std::span<const std::uint8_t> bytes, const Layout& layout, std::size_t position, const Field& field);

}  // namespace tafla::test

#endif
