// The rows that the harness times and counts, each with a variant of every
// builder, after a check that each writes main's builder's bytes twice
// through the same state.

#ifndef TAFLA_BUILDER_REWRITE_CATALOG_HPP
#define TAFLA_BUILDER_REWRITE_CATALOG_HPP

#include <cstddef>
#include <cstdint>
#include <memory>
#include <span>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <vector>

#include "builders.hpp"

namespace builder_rewrite {

/// What the rows read
struct Inputs {
    Record record = tafla::benchmarks::record();
    Document small = tafla::benchmarks::document(100);
    Document large = tafla::benchmarks::document(100'000);
    Document padded_small = padded(tafla::benchmarks::document(100));
    Document padded_large = padded(tafla::benchmarks::document(100'000));
    RoomyDocument roomy{tafla::benchmarks::document(100'000), 0};
    List list = tafla::benchmarks::list(100);
    Medium medium_values = medium("table of 32 medium values", 128, 48);
    Medium short_values = medium("table of 32 short values", 64, 24);
    std::vector<StringsRow> strings = strings_rows();
    std::vector<Nesting> nestings{{"tables nested in place, padded, depth 1", 1},
                                  {"tables nested in place, padded, depth 10", 10},
                                  {"tables nested in place, padded, depth 100", 100},
                                  {"tables nested in place, padded, depth 1000", 1000}};
    std::vector<Arrays> arrays = arrays_rows();
    std::vector<Columns> columns{{"table of 5 float64 columns of 100 rows", 5},
                                 {"table of 8 float64 columns of 100 rows", 8},
                                 {"table of 16 float64 columns of 100 rows", 16},
                                 {"table of 33 float64 columns of 100 rows", 33}};
    std::vector<Scalars> by_reference{{"vector of 100,000 builders of 1 uint32, by reference", 100'000, 1},
                                      {"vector of 100,000 builders of 4 uint32, by reference", 100'000, 4},
                                      {"vector of 100,000 builders of 32 uint32, by reference", 100'000, 32}};
    std::vector<Scalars> owned{{"vector of 100,000 builders of 4 uint32, owned", 100'000, 4}};
    std::vector<Repeated> repeated{
        repeat("2,000 strings of 255 bytes", 2'000, 255),   repeat("2,000 strings of 256 bytes", 2'000, 256),
        repeat("20,000 strings of 100 bytes", 20'000, 100), repeat("20,000 strings of 8 bytes", 20'000, 8),
        repeat("20,000 strings of 63 bytes", 20'000, 63),   repeat("20,000 strings of 64 bytes", 20'000, 64)};
    std::vector<Repeated> struct_fields{repeat("2,000 structs of 255 bytes", 2'000, 255),
                                        repeat("20,000 structs of 63 bytes", 20'000, 63)};
    Repeated uint32s = repeat("20,000 uint32s", 20'000, 0);
    Repeated string_items = repeat("vector in place of 10,000 strings of 255 bytes", 10'000, 255);
    Repeated string_builders = repeat("vector of 2,000 builders of a 255-byte string", 2'000, 255);
    Repeated shared = repeat("5,000 fields of a 10 KB builder by reference", 5'000, 250);
    Repeated chain = repeat("chain of 1,000 builders of 100 bytes", 1'000, 100);
    Repeated deep = repeat("vector of 1,000 builders 3 deep", 1'000, 30);
    Repeated rejected = repeat("uint16 vector of 2,000 items of 60 KB, rejected", 2'000, 250);

    Inputs() {
        State<Main> state;
        roomy.length = Rows<Main, 0>::document(roomy.document, state).size();
    }

    /// The document with a name of 17 bytes rather than 16, so that its
    /// table of columns, which follows the name, is padded by 3 bytes
    static Document padded(Document document) {
        document.name = tafla::benchmarks::Text::owned("tafla-benchmarks!");
        return document;
    }

    static std::vector<StringsRow> strings_rows() {
        const auto workloads = tafla::benchmarks::string_workloads();
        std::vector<StringsRow> rows;
        for (const char* shape : {"empty", "8 chars of ASCII", "16 chars of ASCII", "64 chars of ASCII",
                                  "256 chars of ASCII", "1,024 chars of ASCII", "8 chars of 3 bytes"}) {
            for (const auto& workload : workloads) {
                if (workload.name == shape) {
                    rows.push_back({"16 × " + workload.name, workload.strings});
                }
            }
        }
        return rows;
    }

    static std::vector<Arrays> arrays_rows() {
        std::vector<Arrays> rows;
        for (const auto [count, length] : {std::pair<std::size_t, std::size_t>{9, 256},
                                           {16, 256},
                                           {32, 256},
                                           {4, 255},
                                           {8, 255},
                                           {100'000, 255},
                                           {4, 127},
                                           {8, 127},
                                           {4, 63},
                                           {8, 63},
                                           {100'000, 63},
                                           // About 16 KB and 1 MB
                                           {157, 100},
                                           {9'615, 100}}) {
            rows.push_back({"table of " + with_commas(count) + " arrays of " + std::to_string(length) + " B", count,
                            std::vector<std::uint8_t>(length, 7)});
        }
        return rows;
    }

    static Repeated repeat(std::string name, std::size_t count, std::size_t length) {
        return {std::move(name), count, std::string(length, 'x')};
    }

    static std::string with_commas(std::size_t value) {
        auto digits = std::to_string(value);
        for (auto at = static_cast<std::ptrdiff_t>(digits.size()) - 3; at > 0; at -= 3) {
            digits.insert(static_cast<std::size_t>(at), ",");
        }
        return digits;
    }
};

/// A row's variants, one for each builder
struct Row {
    std::string name;
    std::vector<tafla::benchmarks::Variant> variants;
};

/// The bytes that a builder's call returned, or the length of its error
template <class Out>
std::vector<std::uint8_t> bytes_of(const Out& out) {
    if constexpr (std::is_same_v<Out, std::uint64_t>) {
        std::vector<std::uint8_t> bytes(8);
        for (std::size_t index = 0; index < 8; ++index) {
            bytes[index] = static_cast<std::uint8_t>(out >> (8 * index));
        }
        return bytes;
    } else {
        const std::span<const std::uint8_t> bytes(out.data(), out.size());
        return {bytes.begin(), bytes.end()};
    }
}

template <class R>
using Of = std::type_identity<R>;

/// Adds builder `V`'s variant, after checking that two calls through the
/// same state return `expected`, which the first builder's first call sets
template <class V, int Tag, class Input, class Call>
void add(Row& row, const char* name, const Input& input, Call call, std::vector<std::uint8_t>& expected) {
    using R = Rows<V, Tag>;
    auto state = std::make_shared<State<V>>();
    for (int check = 0; check < 2; ++check) {
        const auto bytes = bytes_of(call(Of<R>{}, input, *state));
        if (expected.empty()) {
            expected = bytes;
        } else if (bytes != expected) {
            throw std::logic_error(std::string(name) + " wrote other bytes than main's builder for " + row.name);
        }
    }

    const auto* value = &input;
    row.variants.push_back(tafla::benchmarks::variant(
        name, [call, value, state] { return call(Of<R>{}, *tafla::benchmarks::opaque(value), *state); }));
}

/// The names of the builders' variants, in the order that each row holds
/// them
inline const std::vector<std::string> kVariants{"main",
                                                "main A/A",
                                                "#212",
                                                "buffer, into_buffer()",
                                                "buffer A/A",
                                                "buffer, to_bytes()",
                                                "buffer, reused, view",
                                                "declared alignment",
                                                "raw storage"};

template <class Input, class Call>
Row row(std::string name, const Input& input, Call call) {
    Row result{std::move(name), {}};
    std::vector<std::uint8_t> expected;
    add<Main, 0>(result, kVariants[0].c_str(), input, call, expected);
    add<Main, 1>(result, kVariants[1].c_str(), input, call, expected);
    add<Rewrite212, 0>(result, kVariants[2].c_str(), input, call, expected);
    add<Buffer, 0>(result, kVariants[3].c_str(), input, call, expected);
    add<Buffer, 1>(result, kVariants[4].c_str(), input, call, expected);
    add<BufferCopy, 0>(result, kVariants[5].c_str(), input, call, expected);
    add<BufferReused, 0>(result, kVariants[6].c_str(), input, call, expected);
    add<Declared, 0>(result, kVariants[7].c_str(), input, call, expected);
    add<Raw, 0>(result, kVariants[8].c_str(), input, call, expected);
    return result;
}

#define BUILDER_ROW(function) \
    [](auto rows, const auto& input, auto& state) { return decltype(rows)::type::function(input, state); }

/// Calls `visit(row)` for each row whose name holds one of `filters`, or for
/// every row without filters, making only the rows that it visits
template <class Visit>
void for_each_row(const Inputs& in, const std::vector<std::string>& filters, Visit visit) {
    const auto selected = [&](const std::string& name) {
        if (filters.empty()) {
            return true;
        }
        for (const auto& filter : filters) {
            if (name.find(filter) != std::string::npos) {
                return true;
            }
        }
        return false;
    };
    const auto add_row = [&](const std::string& name, const auto& input, auto call) {
        if (selected(name)) {
            visit(row(name, input, call));
        }
    };

    add_row("record", in.record, BUILDER_ROW(record));
    add_row("document of 100 rows", in.small, BUILDER_ROW(document));
    add_row("document of 100,000 rows", in.large, BUILDER_ROW(document));
    add_row("document of 100 rows, its columns padded", in.padded_small, BUILDER_ROW(document));
    add_row("document of 100,000 rows, its columns padded", in.padded_large, BUILDER_ROW(document));
    add_row("document of 100,000 rows, into a buffer with room for it", in.roomy, BUILDER_ROW(roomy_document));
    add_row("vector of 100 nested tables", in.list, BUILDER_ROW(list));
    add_row("array of 100 structs", in.list, BUILDER_ROW(structs));
    add_row("vector of 100 nested tables from builders", in.list, BUILDER_ROW(builders));
    add_row(in.medium_values.name, in.medium_values, BUILDER_ROW(medium));
    add_row(in.short_values.name, in.short_values, BUILDER_ROW(medium));
    for (const auto& input : in.strings) {
        add_row(input.name, input, BUILDER_ROW(strings));
    }
    for (const auto& input : in.nestings) {
        add_row(input.name, input, BUILDER_ROW(nested));
    }
    for (const auto& input : in.arrays) {
        add_row(input.name, input, BUILDER_ROW(arrays));
    }
    for (const auto& input : in.columns) {
        add_row(input.name, input, BUILDER_ROW(columns));
    }
    for (const auto& input : in.by_reference) {
        add_row(input.name, input, BUILDER_ROW(by_reference));
    }
    for (const auto& input : in.owned) {
        add_row(input.name, input, BUILDER_ROW(owned));
    }
    for (const auto& input : in.repeated) {
        add_row(input.name, input, BUILDER_ROW(repeated));
    }
    for (const auto& input : in.struct_fields) {
        add_row(input.name, input, BUILDER_ROW(struct_fields));
    }
    add_row(in.uint32s.name, in.uint32s, BUILDER_ROW(uint32s));
    add_row(in.string_items.name, in.string_items, BUILDER_ROW(string_items));
    add_row(in.string_builders.name, in.string_builders, BUILDER_ROW(string_builders));
    add_row(in.shared.name, in.shared, BUILDER_ROW(shared_builder));
    add_row(in.chain.name, in.chain, BUILDER_ROW(chain));
    add_row(in.deep.name, in.deep, BUILDER_ROW(deep_builders));
    add_row(in.rejected.name, in.rejected, BUILDER_ROW(rejected));
}

#undef BUILDER_ROW

}  // namespace builder_rewrite

#endif
