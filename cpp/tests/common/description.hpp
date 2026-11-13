// Tables as the conformance suite's `cases.json` describes them, which the
// conformance test parses and `strategy` generates: the tests' one model of
// tables. See `conformance/README.md` at the root of the repository.

#ifndef TAFLA_TEST_DESCRIPTION_HPP
#define TAFLA_TEST_DESCRIPTION_HPP

#include <cstddef>
#include <cstdint>
#include <optional>
#include <ostream>
#include <string>
#include <variant>
#include <vector>

#include <tafla/tafla.hpp>

namespace tafla::test {

struct Field;

/// The type of a scalar, of which structs are made
enum class ScalarType : std::uint8_t { int8, uint8, int16, uint16, int32, uint32, int64, uint64, float32, float64 };

inline constexpr ScalarType kScalarTypes[] = {
    ScalarType::int8,   ScalarType::uint8, ScalarType::int16,  ScalarType::uint16,  ScalarType::int32,
    ScalarType::uint32, ScalarType::int64, ScalarType::uint64, ScalarType::float32, ScalarType::float64,
};

/// The scalar's size in bytes, which is also its alignment in an array
std::size_t size_of(ScalarType type);

Alignment alignment_of(ScalarType type);

struct Layout {
    bool sized = false;
    bool versioned = false;
    OffsetType offset_type = OffsetType::uint32;

    /// A layout without header slots, which tables nested with `set_table()`
    /// have
    static Layout headerless(OffsetType offset_type) {
        return {false, false, offset_type};
    }

    /// The layout without its version slot, which `versioned()` adds
    [[nodiscard]] TableLayout<false> unversioned() const;

    /// The byte length of the header slots before the field offsets
    [[nodiscard]] std::size_t header_size() const;

    friend bool operator==(const Layout&, const Layout&) = default;
};

/// A table: its layout, its version and its fields, in the order that
/// they're set
struct Table {
    Layout layout;
    /// The version in the header, present only when the layout is versioned
    std::optional<std::uint32_t> version;
    std::vector<Field> fields;

    /// The alignment that `set_table()` pads the table to when given its
    /// builder: the largest alignment of its arrays, of scalars and of
    /// structs, and of its nested tables, or 1 if it has none
    [[nodiscard]] Alignment alignment() const;
};

enum class Source : std::uint8_t { builder, bytes };

/// A table nested from its builder or from the bytes that its builder writes
struct Nested {
    Source from = Source::builder;
    Table table;
};

/// A bool, which the runtimes have no API for: it's written as the uint8 1
/// or 0, and read as true when its byte isn't 0. It's a field only, rather
/// than a scalar of structs.
struct Bool {
    bool value;
};

/// A struct of scalar fields, one after another
struct Struct {
    std::vector<Field> scalars;
};

/// An array of structs of scalars of `types`, as the scalar fields of each
/// struct
struct StructArray {
    std::vector<ScalarType> types;
    std::vector<std::vector<Field>> items;
};

/// A table nested with `set_table()`, with the alignment that the
/// description gives, if any
struct TableField {
    Nested nested;
    std::optional<Alignment> alignment;
};

struct VectorField {
    std::vector<Nested> items;
};

/// A field of each type that a table holds, with its value. Equal fields
/// hold floats of the same bits, so that NaN equals itself and -0 doesn't
/// equal 0.
struct Field {
    using Value = std::variant<std::int8_t, std::uint8_t, std::int16_t, std::uint16_t, std::int32_t, std::uint32_t,
                               std::int64_t, std::uint64_t, float, double, Bool, std::vector<std::int8_t>,
                               std::vector<std::uint8_t>, std::vector<std::int16_t>, std::vector<std::uint16_t>,
                               std::vector<std::int32_t>, std::vector<std::uint32_t>, std::vector<std::int64_t>,
                               std::vector<std::uint64_t>, std::vector<float>, std::vector<double>, std::string, Struct,
                               StructArray, TableField, VectorField>;

    Value value;

    /// The type of a scalar field, or nothing for a field of any other type
    [[nodiscard]] std::optional<ScalarType> scalar_type() const;

    /// The little-endian bytes of a scalar field, which must be one
    [[nodiscard]] std::vector<std::uint8_t> scalar_bytes() const;

    /// The alignment that the field needs from its table's start: an array's
    /// element size, an array of structs' largest scalar's, the alignment
    /// that a nested table is set with, or else 1
    [[nodiscard]] Alignment alignment() const;
};

bool operator==(const Field& left, const Field& right);
bool operator==(const Table& left, const Table& right);
bool operator==(const Nested& left, const Nested& right);

std::ostream& operator<<(std::ostream& stream, const Field& field);
std::ostream& operator<<(std::ostream& stream, const Table& table);

/// The size of a struct of scalars of `types`: the sum of theirs
std::size_t struct_size(const std::vector<ScalarType>& types);

/// The alignment of an array of structs of scalars of `types`: the largest
/// of their sizes
Alignment struct_alignment(const std::vector<ScalarType>& types);

/// The types of a struct's scalar fields
std::vector<ScalarType> struct_types(const std::vector<Field>& scalars);

/// The bytes of a struct of scalar fields: theirs, one after another
std::vector<std::uint8_t> struct_bytes(const std::vector<Field>& scalars);

}  // namespace tafla::test

#endif
