// Generic table shapes, with the TypeScript comparison's values.
//
// The shapes are sized to bracket how tafla is used: small tables of scalars
// and a string, documents whose header nests a table of numeric columns,
// vectors of small nested tables, arrays of structs, and fixed-size records.
// The values come from a port of the TypeScript comparison's seeded
// generator, so that the languages' numbers compare.
//
// Deserialized values view strings and columns in their bytes where a codec
// reads them in place, and own copies otherwise. Equality compares floats by
// their bits, so that a codec reads back exactly what it wrote.

#ifndef TAFLA_BENCHMARKS_WORKLOADS_HPP
#define TAFLA_BENCHMARKS_WORKLOADS_HPP

#include <bit>
#include <cstddef>
#include <cstdint>
#include <span>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace tafla::benchmarks {

/// Rows per document: a small one and a large one
inline constexpr std::size_t kDocumentRows[] = {100, 100'000};

inline constexpr std::size_t kListItems = 100;

/// A column viewed in the bytes it was read from, or owned, as Rust's `Cow`
template <class T>
class Column {
public:
    Column() = default;

    static Column borrowed(std::span<const T> values) {
        Column column;
        column.view_ = values;
        return column;
    }

    static Column owned(std::vector<T> values) {
        Column column;
        column.owned_ = std::move(values);
        column.owning_ = true;
        return column;
    }

    [[nodiscard]] std::span<const T> span() const {
        return owning_ ? std::span<const T>(owned_) : view_;
    }

    [[nodiscard]] bool is_borrowed() const {
        return !owning_;
    }

private:
    std::span<const T> view_;
    std::vector<T> owned_;
    bool owning_ = false;
};

/// A string viewed in the bytes it was read from, or owned
class Text {
public:
    Text() = default;

    static Text borrowed(std::string_view text) {
        Text result;
        result.view_ = text;
        return result;
    }

    static Text owned(std::string text) {
        Text result;
        result.owned_ = std::move(text);
        result.owning_ = true;
        return result;
    }

    [[nodiscard]] std::string_view view() const {
        return owning_ ? std::string_view(owned_) : view_;
    }

private:
    std::string_view view_;
    std::string owned_;
    bool owning_ = false;
};

/// A small table: two float64s, an int64 and a 16-byte string
struct Record {
    double x = 0;
    double y = 0;
    std::int64_t id = 0;
    Text name;
};

/// A document: a sized, versioned table of two float64s and a string, with a
/// nested table of an int64 and three columns of the same length
struct Document {
    double x = 0;
    double y = 0;
    Text name;
    std::int64_t base = 0;
    Column<std::uint32_t> keys;
    Column<float> a;
    Column<float> b;
};

/// A table of a float32 and a nested table of two float32s, `first` and
/// `second`, or a struct of the three float32s
struct Item {
    float value = 0;
    float first = 0;
    float second = 0;
};

/// A table holding a vector of item tables, or an array of item structs
struct List {
    std::vector<Item> items;
};

/// A fixed-size record of a uint32 and two float32s
struct Sample {
    std::uint32_t key = 0;
    float a = 0;
    float b = 0;
};

/// The shapes, which name the values that codecs serialize and read back
struct RecordShape {
    using Value = Record;
    static constexpr std::string_view name = "record";
};

struct DocumentShape {
    using Value = Document;
    static constexpr std::string_view name = "document";
};

struct ListShape {
    using Value = List;
    static constexpr std::string_view name = "vector of nested tables";
};

struct StructsShape {
    using Value = List;
    static constexpr std::string_view name = "array of structs";
};

struct SampleShape {
    using Value = Sample;
    static constexpr std::string_view name = "fixed-size record";
};

inline const Record& record() {
    static const Record record{59.3293, 18.0686, -631'152'000, Text::borrowed("tafla-benchmarks")};
    return record;
}

inline constexpr Sample kSample{2'743'826'400, 21.75F, 3.25F};

/// The TypeScript generator's xorshift, in [0, 1), so every run sees the same
/// values. Its state is a `uint32`, whose bits JavaScript's 32-bit shifts keep
/// the same.
class Xorshift {
public:
    // A seed wraps to 32 bits, as JavaScript's `^` wraps its operands
    explicit Xorshift(std::size_t seed) : state_(0x2545'f491U ^ static_cast<std::uint32_t>(seed)) {}

    double next() {
        state_ ^= state_ << 13;
        state_ ^= state_ >> 17;
        state_ ^= state_ << 5;
        return static_cast<double>(state_) / 4'294'967'296.0;
    }

private:
    std::uint32_t state_;
};

inline Document document(std::size_t rows) {
    Xorshift random(rows);

    // Keys below 2^24 take 4 bytes in protobuf's varints, like the floats.
    // `Math.floor()` of a value in [0, 2^32), and `Math.fround()`, as typed
    // arrays store them
    std::vector<std::uint32_t> keys;
    std::vector<float> a;
    std::vector<float> b;
    keys.reserve(rows);
    a.reserve(rows);
    b.reserve(rows);
    for (std::size_t row = 0; row < rows; ++row) {
        keys.push_back(static_cast<std::uint32_t>(random.next() * double{1 << 24}));
        a.push_back(static_cast<float>((random.next() - 0.5) * 100.0));
        b.push_back(static_cast<float>((random.next() - 0.5) * 100.0));
    }

    const auto& base = record();
    return {base.x,
            base.y,
            Text::borrowed(base.name.view()),
            base.id,
            Column<std::uint32_t>::owned(std::move(keys)),
            Column<float>::owned(std::move(a)),
            Column<float>::owned(std::move(b))};
}

inline List list(std::size_t count) {
    Xorshift random(count);

    // Float32 values, so that every format reads them back exactly. Fields
    // are evaluated in the order they're written, as in JavaScript.
    List list;
    list.items.reserve(count);
    for (std::size_t item = 0; item < count; ++item) {
        const auto value = static_cast<float>(random.next() * 30.0);
        const auto first = static_cast<float>(random.next() * 4.0 - 2.0);
        const auto second = static_cast<float>(random.next() * 50.0);
        list.items.push_back({value, first, second});
    }
    return list;
}

/// The sum of `values` in their order, in a `double`, as the TypeScript
/// comparison adds them to a `number`. Every codec sums its column with this,
/// so their sums have the same bits.
template <class Values>
double sum_column(const Values& values) {
    double sum = 0;
    for (const float value : values) {
        sum += static_cast<double>(value);
    }
    return sum;
}

/// The sum of an item's three float32s, in the order that every way of
/// reading items adds them
inline double sum_item(float value, float first, float second) {
    return static_cast<double>(value) + static_cast<double>(first) + static_cast<double>(second);
}

inline bool same_bits(float left, float right) {
    return std::bit_cast<std::uint32_t>(left) == std::bit_cast<std::uint32_t>(right);
}

inline bool same_bits(double left, double right) {
    return std::bit_cast<std::uint64_t>(left) == std::bit_cast<std::uint64_t>(right);
}

template <class T>
bool same_column(std::span<const T> left, std::span<const T> right) {
    if (left.size() != right.size()) {
        return false;
    }
    for (std::size_t index = 0; index < left.size(); ++index) {
        if constexpr (std::is_floating_point_v<T>) {
            if (!same_bits(left[index], right[index])) {
                return false;
            }
        } else if (left[index] != right[index]) {
            return false;
        }
    }
    return true;
}

inline bool same(const Record& left, const Record& right) {
    return same_bits(left.x, right.x) && same_bits(left.y, right.y) && left.id == right.id &&
           left.name.view() == right.name.view();
}

inline bool same(const Document& left, const Document& right) {
    return same_bits(left.x, right.x) && same_bits(left.y, right.y) && left.name.view() == right.name.view() &&
           left.base == right.base && same_column(left.keys.span(), right.keys.span()) &&
           same_column(left.a.span(), right.a.span()) && same_column(left.b.span(), right.b.span());
}

inline bool same(const Item& left, const Item& right) {
    return same_bits(left.value, right.value) && same_bits(left.first, right.first) &&
           same_bits(left.second, right.second);
}

inline bool same(const List& left, const List& right) {
    if (left.items.size() != right.items.size()) {
        return false;
    }
    for (std::size_t index = 0; index < left.items.size(); ++index) {
        if (!same(left.items[index], right.items[index])) {
            return false;
        }
    }
    return true;
}

inline bool same(const Sample& left, const Sample& right) {
    return left.key == right.key && same_bits(left.a, right.a) && same_bits(left.b, right.b);
}

}  // namespace tafla::benchmarks

#endif
