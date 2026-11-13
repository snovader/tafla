// Sizes and writes tables as their descriptions set them out, with
// `TableSizer` and `TableWriter`, which write tables nested in them in place

#ifndef TAFLA_TEST_WRITE_HPP
#define TAFLA_TEST_WRITE_HPP

#include <cstddef>
#include <cstdint>
#include <span>

#include <tafla/tafla.hpp>

#include "common/description.hpp"

namespace tafla::test {

/// Starts a sizer of the table's layout, version and number of fields, with
/// no field added
TableSizer sizer_of(const Table& table);

/// Sizes and writes the table. The tables nested in it are written in place,
/// so they must have no header and its offset type, as `nested_in_place()`
/// makes them.
Bytes write(const Table& table);

/// Sizes the table `sized`, and writes the fields of `written` into the bytes
/// that it sized, which fails unless they're as long as its own
Bytes write_as(const Table& sized, const Table& written);

/// Sizes and writes the table as `write()` does, but into the start of
/// `bytes`, and returns its length
std::size_t write_into(const Table& table, std::span<std::uint8_t> bytes);

/// The table with every table nested in it made one that a `TableWriter`
/// writes in place: from its builder, without a header, and with the root's
/// offset type
Table nested_in_place(const Table& table);

/// Adds the field to the sizer, with what decides its length
void add_field(const Field& field, TableSizer& sizer);

/// Writes the field as field `index` of the writer's table
void write_field(const Field& field, TableWriter& writer, std::size_t index);

}  // namespace tafla::test

#endif
