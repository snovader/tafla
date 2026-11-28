// The builders that the harness compares, and the rows that each writes: the
// comparison's rows that build tables, Rust's harness's rows of #378, and the
// shapes of #212's reviews. Every builder makes the same calls, in a function
// of its own, which a loop of its own calls.

#ifndef TAFLA_BUILDER_REWRITE_BUILDERS_HPP
#define TAFLA_BUILDER_REWRITE_BUILDERS_HPP

#include <array>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <functional>
#include <memory>
#include <optional>
#include <span>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include <tafla/tafla.hpp>
#include "tafla_212/tafla.hpp"
#include "tafla_buf2/tafla.hpp"
#include "tafla_main/tafla.hpp"
#include "tafla_raw/tafla.hpp"

#include "../src/harness.hpp"
#include "../src/string_workloads.hpp"
#include "../src/workloads.hpp"

namespace builder_rewrite {

using tafla::benchmarks::Document;
using tafla::benchmarks::Item;
using tafla::benchmarks::List;
using tafla::benchmarks::Record;

/// How a builder takes its table
enum class Take : std::uint8_t {
    /// `to_bytes()`, a copy
    copy,
    /// `std::move(builder).into_buffer()`, its buffer
    buffer,
    /// `finish()` of a builder that the loop keeps and resets, a view
    reused,
};

/// A builder of one of the namespaces, and how it's used
template <class Namespace, Take How, bool Declares>
struct Version {
    using Ns = Namespace;
    using Builder = typename Namespace::TableBuilder;
    using Item = typename Builder::Item;
    using Nested = typename Namespace::NestedTable;
    using Error = typename Namespace::BuildError;
    using Alignment = typename Namespace::Alignment;

    static constexpr Take take = How;
    /// Whether tables in place are given their alignment
    static constexpr bool declares = Declares;
    static constexpr bool raw = Namespace::raw;
};

#define BUILDER_NAMESPACE(Name, ns, is_raw)                                                        \
    struct Name {                                                                                  \
        using TableBuilder = ns::TableBuilder;                                                     \
        using NestedTable = ns::NestedTable;                                                       \
        using BuildError = ns::BuildError;                                                         \
        using Alignment = ns::Alignment;                                                           \
        static constexpr bool raw = is_raw;                                                        \
        static constexpr auto layout = ns::TableLayout();                                          \
        static constexpr auto document = ns::TableLayout().sized().with_version(1);                \
        static constexpr auto uint16 = ns::TableLayout().with_offset_type(ns::OffsetType::uint16); \
    };

BUILDER_NAMESPACE(MainNs, tafla_main, false)
BUILDER_NAMESPACE(Rewrite212Ns, tafla_212, false)
BUILDER_NAMESPACE(BufferNs, tafla, false)
BUILDER_NAMESPACE(Buffer2Ns, tafla_buf2, false)
BUILDER_NAMESPACE(RawNs, tafla_raw, true)

#undef BUILDER_NAMESPACE

using Main = Version<MainNs, Take::copy, false>;
using Rewrite212 = Version<Rewrite212Ns, Take::copy, false>;
using Buffer = Version<BufferNs, Take::buffer, false>;
using Buffer2 = Version<Buffer2Ns, Take::buffer, false>;
using BufferCopy = Version<BufferNs, Take::copy, false>;
using BufferReused = Version<BufferNs, Take::reused, false>;
using Declared = Version<BufferNs, Take::buffer, true>;
using Raw = Version<RawNs, Take::buffer, false>;

/// A table's values of 32 fields: 16 strings and 16 float32 arrays
struct Medium {
    std::string name;
    std::vector<std::string> strings;
    std::vector<std::vector<float>> arrays;
};

inline Medium medium(std::string name, std::size_t characters, std::size_t elements) {
    Medium values{std::move(name), {}, {}};
    for (std::size_t index = 0; index < 16; ++index) {
        std::string repeated;
        char digits[8];
        std::snprintf(digits, sizeof digits, "%03zu", index);
        while (repeated.size() < characters) {
            repeated += digits;
        }
        repeated.resize(characters);
        values.strings.push_back(repeated);

        std::vector<float> array(elements);
        for (std::size_t element = 0; element < elements; ++element) {
            array[element] = static_cast<float>(element) + static_cast<float>(index);
        }
        values.arrays.push_back(std::move(array));
    }
    return values;
}

/// Tables nested in place, `depth` in one another, each after a `uint8`,
/// around a table of an array of four float64 and 500 arrays of 200 bytes,
/// so that each is padded
struct Nesting {
    std::string name;
    std::size_t depth;
    std::vector<double> column = std::vector<double>(4, 1.5);
    std::vector<std::uint8_t> small = std::vector<std::uint8_t>(200, 7);
};

inline constexpr std::size_t kNestedArrays = 500;

/// A table of `count` arrays of the same bytes
struct Arrays {
    std::string name;
    std::size_t count;
    std::vector<std::uint8_t> value;
};

/// A table of `count` columns of 100 float64
struct Columns {
    std::string name;
    std::size_t count;
    std::vector<double> column = std::vector<double>(100, 1.5);
};

/// `builders` builders of `fields` uint32 each, nested in a vector
struct Scalars {
    std::string name;
    std::size_t builders;
    std::size_t fields;
};

/// A table of 16 strings of one of the strings comparison's shapes
struct StringsRow {
    std::string name;
    tafla::benchmarks::Strings strings;
};

/// The document of 100,000 rows, and its table's length, which a builder
/// that takes a buffer is given room for
struct RoomyDocument {
    Document document;
    std::size_t length;
};

/// `count` fields of one string of `size` bytes, or a vector in place of as
/// many items of one such string
struct Repeated {
    std::string name;
    std::size_t count;
    std::string value;
};

struct Inputs;

/// A builder's state across calls: the builder that a reused one resets
template <class V>
struct State {
    std::optional<typename V::Builder> builder;
};

/// The rows that builder `V` writes, in functions of their own, compiled
/// again for each `Tag`, so that an A/A pair runs code of its own
template <class V, int Tag>
struct Rows {
    using Builder = typename V::Builder;
    using Item = typename V::Item;
    using Nested = typename V::Nested;
    using Alignment = typename V::Alignment;

    /// Starts a builder of `fields` fields in `layout`, or resets the one
    /// that the loop keeps
    template <class Layout>
    static Builder& start(State<V>& state, std::size_t fields, Layout layout) {
        if constexpr (V::take == Take::reused) {
            if (state.builder) {
                state.builder->reset(fields, layout);
                return *state.builder;
            }
        }
        state.builder.emplace(fields, layout);
        return *state.builder;
    }

    static auto take(Builder& builder) {
        if constexpr (V::take == Take::copy) {
            return builder.to_bytes();
        } else if constexpr (V::take == Take::buffer) {
            return std::move(builder).into_buffer();
        } else {
            return builder.finish();
        }
    }

    /// Nests a table of `fields` in place, with its alignment where the
    /// builder is given it
    template <class Target, class Fill>
    [[gnu::always_inline]] static void nest(Target& target, std::size_t fields, Alignment alignment, Fill&& fill) {
        if constexpr (V::declares) {
            target.set_table(fields, alignment, std::forward<Fill>(fill));
        } else {
            static_cast<void>(alignment);
            target.set_table(fields, std::forward<Fill>(fill));
        }
    }

    [[gnu::noinline]] static auto record(const Record& input, State<V>& state) {
        auto& builder = start(state, 4, V::Ns::layout);
        builder.set_float64(input.x);
        builder.set_float64(input.y);
        builder.set_int64(input.id);
        builder.set_string(input.name.view());
        return take(builder);
    }

    template <class Target>
    [[gnu::always_inline]] static void document_fields(Target& document, const Document& input) {
        document.set_float64(input.x);
        document.set_float64(input.y);
        document.set_string(input.name.view());
        nest(document, 4, Alignment::four, [&](Item& columns) {
            columns.set_int64(input.base);
            columns.set_uint32_array(input.keys.span());
            columns.set_float32_array(input.a.span());
            columns.set_float32_array(input.b.span());
        });
    }

    [[gnu::noinline]] static auto document(const Document& input, State<V>& state) {
        auto& builder = start(state, 4, V::Ns::document);
        document_fields(builder, input);
        return take(builder);
    }

    /// The document into a buffer with room for it, where the builder takes
    /// one, and the reused builder has the room of the tables before
    [[gnu::noinline]] static auto roomy_document(const RoomyDocument& input, State<V>& state) {
        if constexpr (V::take == Take::buffer) {
            if constexpr (V::raw) {
                state.builder.emplace(Builder::with_capacity(input.length, 4, V::Ns::document));
            } else {
                std::vector<std::uint8_t> buffer;
                buffer.reserve(input.length);
                state.builder.emplace(Builder::with_buffer(std::move(buffer), 4, V::Ns::document));
            }
            document_fields(*state.builder, input.document);
            return take(*state.builder);
        } else {
            auto& builder = start(state, 4, V::Ns::document);
            document_fields(builder, input.document);
            return take(builder);
        }
    }

    [[gnu::noinline]] static auto list(const List& input, State<V>& state) {
        const auto& items = input.items;
        auto& builder = start(state, 1, V::Ns::layout);
        builder.set_vector(items.size(), 2, [&](std::size_t index, Item& table) {
            const auto& item = items[index];
            table.set_float32(item.value);
            nest(table, 2, Alignment::four, [&](Item& pair) {
                pair.set_float32(item.first);
                pair.set_float32(item.second);
            });
        });
        return take(builder);
    }

    [[gnu::noinline]] static auto structs(const List& input, State<V>& state) {
        constexpr std::size_t size = 12;
        std::vector<std::uint8_t> structs(input.items.size() * size);
        for (std::size_t index = 0; index < input.items.size(); ++index) {
            const auto& item = input.items[index];
            tafla::store_le(structs.data() + index * size, item.value);
            tafla::store_le(structs.data() + index * size + 4, item.first);
            tafla::store_le(structs.data() + index * size + 8, item.second);
        }

        auto& builder = start(state, 1, V::Ns::layout);
        builder.set_struct_array(std::span<const std::uint8_t>(structs), size, Alignment::four);
        return take(builder);
    }

    [[gnu::noinline]] static auto builders(const List& input, State<V>& state) {
        std::vector<Nested> items;
        items.reserve(input.items.size());
        for (const auto& item : input.items) {
            Builder pair(2, V::Ns::layout);
            pair.set_float32(item.first);
            pair.set_float32(item.second);

            Builder table(2, V::Ns::layout);
            table.set_float32(item.value);
            table.set_table(std::move(pair));
            items.emplace_back(std::move(table));
        }

        auto& builder = start(state, 1, V::Ns::layout);
        builder.set_vector(std::move(items));
        return take(builder);
    }

    [[gnu::noinline]] static auto medium(const Medium& input, State<V>& state) {
        auto& builder = start(state, 32, V::Ns::layout);
        for (std::size_t index = 0; index < 16; ++index) {
            builder.set_string(std::string_view(input.strings[index]));
            builder.set_float32_array(std::span<const float>(input.arrays[index]));
        }
        return take(builder);
    }

    [[gnu::noinline]] static auto strings(const StringsRow& input, State<V>& state) {
        auto& builder = start(state, tafla::benchmarks::kStrings, V::Ns::layout);
        for (const auto& string : input.strings) {
            builder.set_string(string.view());
        }
        return take(builder);
    }

    static void level(Item& table, std::size_t depth, const Nesting& input) {
        table.set_uint8(1);
        if (depth > 0) {
            nest(table, 2, Alignment::eight, [&](Item& inner) { level(inner, depth - 1, input); });
            return;
        }
        nest(table, 1 + kNestedArrays, Alignment::eight, [&](Item& inner) {
            inner.set_float64_array(std::span<const double>(input.column));
            for (std::size_t array = 0; array < kNestedArrays; ++array) {
                inner.set_uint8_array(std::span<const std::uint8_t>(input.small));
            }
        });
    }

    [[gnu::noinline]] static auto nested(const Nesting& input, State<V>& state) {
        auto& builder = start(state, 1, V::Ns::layout);
        nest(builder, 2, Alignment::eight, [&](Item& table) { level(table, input.depth, input); });
        return take(builder);
    }

    [[gnu::noinline]] static auto arrays(const Arrays& input, State<V>& state) {
        auto& builder = start(state, input.count, V::Ns::layout);
        for (std::size_t index = 0; index < input.count; ++index) {
            builder.set_uint8_array(std::span<const std::uint8_t>(input.value));
        }
        return take(builder);
    }

    [[gnu::noinline]] static auto columns(const Columns& input, State<V>& state) {
        auto& builder = start(state, input.count, V::Ns::layout);
        for (std::size_t index = 0; index < input.count; ++index) {
            builder.set_float64_array(std::span<const double>(input.column));
        }
        return take(builder);
    }

    static Builder scalars(std::size_t index, std::size_t fields) {
        Builder builder(fields, V::Ns::layout);
        for (std::size_t field = 0; field < fields; ++field) {
            builder.set_uint32(static_cast<std::uint32_t>(index));
        }
        return builder;
    }

    [[gnu::noinline]] static auto by_reference(const Scalars& input, State<V>& state) {
        std::vector<Builder> builders;
        builders.reserve(input.builders);
        for (std::size_t index = 0; index < input.builders; ++index) {
            builders.push_back(scalars(index, input.fields));
        }
        std::vector<Nested> items;
        items.reserve(input.builders);
        for (const auto& item : builders) {
            items.emplace_back(item);
        }

        auto& builder = start(state, 1, V::Ns::layout);
        builder.set_vector(std::move(items));
        return take(builder);
    }

    [[gnu::noinline]] static auto owned(const Scalars& input, State<V>& state) {
        std::vector<Nested> items;
        items.reserve(input.builders);
        for (std::size_t index = 0; index < input.builders; ++index) {
            items.emplace_back(scalars(index, input.fields));
        }

        auto& builder = start(state, 1, V::Ns::layout);
        builder.set_vector(std::move(items));
        return take(builder);
    }

    /// `count` fields of the same string, viewed
    [[gnu::noinline]] static auto repeated(const Repeated& input, State<V>& state) {
        auto& builder = start(state, input.count, V::Ns::layout);
        for (std::size_t index = 0; index < input.count; ++index) {
            builder.set_string(std::string_view(input.value));
        }
        return take(builder);
    }

    /// `count` struct fields of the same bytes
    [[gnu::noinline]] static auto struct_fields(const Repeated& input, State<V>& state) {
        const std::span<const std::uint8_t> bytes(reinterpret_cast<const std::uint8_t*>(input.value.data()),
                                                  input.value.size());
        auto& builder = start(state, input.count, V::Ns::layout);
        for (std::size_t index = 0; index < input.count; ++index) {
            builder.set_struct(bytes);
        }
        return take(builder);
    }

    [[gnu::noinline]] static auto uint32s(const Repeated& input, State<V>& state) {
        auto& builder = start(state, input.count, V::Ns::layout);
        for (std::size_t index = 0; index < input.count; ++index) {
            builder.set_uint32(static_cast<std::uint32_t>(index));
        }
        return take(builder);
    }

    /// A vector in place of `count` items of one string
    [[gnu::noinline]] static auto string_items(const Repeated& input, State<V>& state) {
        const std::string_view string(input.value);
        auto& builder = start(state, 1, V::Ns::layout);
        builder.set_vector(input.count, 1, [&](std::size_t, Item& item) { item.set_string(string); });
        return take(builder);
    }

    /// A vector of `count` builders of one string
    [[gnu::noinline]] static auto string_builders(const Repeated& input, State<V>& state) {
        const std::string_view string(input.value);
        std::vector<Nested> items;
        items.reserve(input.count);
        for (std::size_t index = 0; index < input.count; ++index) {
            Builder item(1, V::Ns::layout);
            item.set_string(string);
            items.emplace_back(std::move(item));
        }
        auto& builder = start(state, 1, V::Ns::layout);
        builder.set_vector(std::move(items));
        return take(builder);
    }

    /// 5,000 fields of one builder of 40 strings of 250 bytes, nested by
    /// `const` reference
    [[gnu::noinline]] static auto shared_builder(const Repeated& input, State<V>& state) {
        const std::string_view string(input.value);
        Builder shared(40, V::Ns::layout);
        for (int index = 0; index < 40; ++index) {
            shared.set_string(string);
        }
        auto& builder = start(state, input.count, V::Ns::layout);
        for (std::size_t index = 0; index < input.count; ++index) {
            builder.set_table(shared);
        }
        return take(builder);
    }

    /// A chain of `count` builders, each of a string and the one before it
    [[gnu::noinline]] static auto chain(const Repeated& input, State<V>& state) {
        const std::string_view string(input.value);
        Builder current(1, V::Ns::layout);
        current.set_string(string);
        for (std::size_t index = 0; index < input.count; ++index) {
            Builder next(2, V::Ns::layout);
            next.set_string(string);
            next.set_table(std::move(current));
            current = std::move(next);
        }
        auto& builder = start(state, 1, V::Ns::layout);
        builder.set_table(std::move(current));
        return take(builder);
    }

    /// A vector of `count` builders, each nesting a builder nesting a builder
    [[gnu::noinline]] static auto deep_builders(const Repeated& input, State<V>& state) {
        const std::string_view string(input.value);
        std::vector<Nested> items;
        items.reserve(input.count);
        for (std::size_t index = 0; index < input.count; ++index) {
            Builder a(2, V::Ns::layout);
            a.set_uint32(1);
            a.set_string(string);
            Builder c(2, V::Ns::layout);
            c.set_uint32(2);
            c.set_table(std::move(a));
            Builder d(2, V::Ns::layout);
            d.set_uint32(3);
            d.set_table(std::move(c));
            items.emplace_back(std::move(d));
        }
        auto& builder = start(state, 1, V::Ns::layout);
        builder.set_vector(std::move(items));
        return take(builder);
    }

    /// A vector in place, in a table of uint16 offsets, of `count` items of
    /// 240 strings, which is rejected as too long: the error's length
    [[gnu::noinline]] static std::uint64_t rejected(const Repeated& input, State<V>& state) {
        const std::string_view string(input.value);
        try {
            auto& builder = start(state, 1, V::Ns::uint16);
            builder.set_vector(input.count, 240, [&](std::size_t, Item& item) {
                for (int index = 0; index < 240; ++index) {
                    item.set_string(string);
                }
            });
            return 0;
        } catch (const typename V::Error& error) {
            return error.length();
        }
    }
};

}  // namespace builder_rewrite

#endif
