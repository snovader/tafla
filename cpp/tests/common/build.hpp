// Builds tables as their descriptions set them out, with `TableBuilder`

#ifndef TAFLA_TEST_BUILD_HPP
#define TAFLA_TEST_BUILD_HPP

#include <tafla/tafla.hpp>

#include "common/description.hpp"

namespace tafla::test {

/// Starts a builder of the table's layout, version and number of fields, with
/// no field set
TableBuilder builder_of(const Table& table);

/// Builds the table, borrowing its values, which must outlive the builder
TableBuilder build(const Table& table);

/// Writes the table with `TableBuilder`
Bytes to_bytes(const Table& table);

/// Sets the field on `builder`, which borrows its value, or owns the bytes of
/// structs
void set_field(const Field& field, TableBuilder& builder);

/// Builds the table to nest, as its builder or its bytes
NestedTable build_nested(const Nested& nested);

}  // namespace tafla::test

#endif
