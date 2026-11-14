// Generators of table descriptions in the conformance suite's shape: tables
// of every layout, with fields of every type holding values across their
// range, and tables nested in them, from builders and from bytes, to a
// bounded depth.
//
// Tables follow the rules of the format and the builder's documentation: a
// version fits in its slot, a table nested with `set_table()` has no header,
// and a table and the tables in it fit in their offset types.

#ifndef TAFLA_TEST_STRATEGY_HPP
#define TAFLA_TEST_STRATEGY_HPP

#include <rapidcheck.h>

#include <cstddef>
#include <string>

#include "common/description.hpp"

namespace tafla::test::strategy {

/// Tables of any layout
rc::Gen<Table> table();

/// Tables of the layouts that `layouts` generates
rc::Gen<Table> tables(rc::Gen<Layout> layouts);

/// Layouts of any header and offset type
rc::Gen<Layout> layout();

/// Layouts of any header, of the offset types that `offset_types` generates
rc::Gen<Layout> layouts(rc::Gen<OffsetType> offset_types);

rc::Gen<OffsetType> offset_type();

/// Floats of every class and sign, NaNs with any payload among them, and -0,
/// NaN, the infinities and the limits more often than chance
rc::Gen<float> float32();

/// Floats like `float32()`'s, as doubles
rc::Gen<double> float64();

/// Strings of up to `characters` characters of ASCII, of two, three and four
/// bytes in UTF-8, the last outside the Basic Multilingual Plane, and U+FEFF,
/// which isn't a byte order mark within a table, leading or not
rc::Gen<std::string> strings(std::size_t characters);

}  // namespace tafla::test::strategy

#endif
