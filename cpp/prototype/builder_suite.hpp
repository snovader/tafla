// The comparison's builder rows, written through `tafla::TableBuilder` by the
// comparison's own tafla codec, and through the prototype's builders by the
// same calls, timed interleaved with the comparison's harness

#ifndef TAFLA_PROTOTYPE_BUILDER_SUITE_HPP
#define TAFLA_PROTOTYPE_BUILDER_SUITE_HPP

#include <array>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <span>
#include <stdexcept>
#include <string>
#include <vector>

#include "codecs/tafla.hpp"
#include "direct_table_builder.hpp"
#include "harness.hpp"
#include "workloads.hpp"

namespace tafla::prototype {

using benchmarks::Document;
using benchmarks::List;
using benchmarks::Record;
using benchmarks::Tafla;

/// The comparison's tafla codec's serialize calls, through a builder of type
/// `Builder`, whose setters are `tafla::TableBuilder`'s
template <class Builder>
struct HandWritten {
    [[nodiscard]] Bytes record(const Record& input) const {
        Builder builder(4, TableLayout());
        builder.set_float64(input.x);
        builder.set_float64(input.y);
        builder.set_int64(input.id);
        builder.set_string(input.name.view());
        return builder.to_bytes();
    }

    [[nodiscard]] Bytes document(const Document& input) const {
        Builder document(4, Tafla::kDocumentLayout, Tafla::kDocumentVersion);
        document.set_float64(input.x);
        document.set_float64(input.y);
        document.set_string(input.name.view());
        document.set_table(4, [&](auto& columns) {
            columns.set_int64(input.base);
            columns.set_uint32_array(input.keys.span());
            columns.set_float32_array(input.a.span());
            columns.set_float32_array(input.b.span());
        });
        return document.to_bytes();
    }

    [[nodiscard]] Bytes list(const List& input) const {
        const auto& items = input.items;

        Builder builder(1, TableLayout());
        builder.set_vector(items.size(), 2, [&](std::size_t index, auto& item) {
            const auto& value = items[index];
            item.set_float32(value.value);
            item.set_table(2, [&](auto& pair) {
                pair.set_float32(value.first);
                pair.set_float32(value.second);
            });
        });
        return builder.to_bytes();
    }

    [[nodiscard]] Bytes structs(const List& input) const {
        std::vector<std::uint8_t> structs(input.items.size() * Tafla::kItemSize);
        for (std::size_t index = 0; index < input.items.size(); ++index) {
            Tafla::write_item_struct(input.items[index], structs.data() + index * Tafla::kItemSize);
        }

        Builder builder(1, TableLayout());
        builder.set_struct_array(std::span<const std::uint8_t>(structs), Tafla::kItemSize, Tafla::kItemAlignment);
        return builder.to_bytes();
    }
};

/// The values that the builders write, which outlive the benchmarks
struct Inputs {
    Record record = benchmarks::record();
    std::array<Document, 2> documents = {benchmarks::document(benchmarks::kDocumentRows[0]),
                                         benchmarks::document(benchmarks::kDocumentRows[1])};
    List list = benchmarks::list(benchmarks::kListItems);
};

struct Benchmark {
    std::string name;
    std::vector<benchmarks::Variant> variants;
};

/// A row's variants: the comparison's tafla codec and its A/A pair, and the
/// prototype's builders, after checking that each writes the codec's bytes
template <class Input, class Expected, class Write>
Benchmark row(std::string name, const Input& input, Expected expected, Write write) {
    using benchmarks::opaque;
    using benchmarks::variant;

    const auto reference = expected(input);
    const auto check = [&](const Bytes& bytes, const char* builder) {
        if (bytes != reference) {
            throw std::logic_error(std::string(builder) + " wrote other bytes than tafla's builder for " + name);
        }
    };
    check(write(HandWritten<DirectTableBuilder>(), input), "the direct builder");
    check(write(HandWritten<CopyingTableBuilder>(), input), "the copying builder");
    check(write(HandWritten<InliningTableBuilder>(), input), "the inlining builder");

    const auto* value = &input;
    Benchmark benchmark{std::move(name), {}};
    benchmark.variants.push_back(variant("TableBuilder", [expected, value] { return expected(*opaque(value)); }));
    benchmark.variants.push_back(variant("TableBuilder A/A", [expected, value] { return expected(*opaque(value)); }));
    benchmark.variants.push_back(
        variant("direct", [write, value] { return write(HandWritten<DirectTableBuilder>(), *opaque(value)); }));
    benchmark.variants.push_back(
        variant("direct A/A", [write, value] { return write(HandWritten<DirectTableBuilder>(), *opaque(value)); }));
    benchmark.variants.push_back(variant(
        "direct, copying", [write, value] { return write(HandWritten<CopyingTableBuilder>(), *opaque(value)); }));
    benchmark.variants.push_back(variant("direct, fills inlined", [write, value] {
        return write(HandWritten<InliningTableBuilder>(), *opaque(value));
    }));
    return benchmark;
}

/// Every builder row of the comparison, in its order
inline std::vector<Benchmark> benchmarks(const Inputs& inputs) {
    using benchmarks::DocumentShape;
    using benchmarks::ListShape;
    using benchmarks::RecordShape;
    using benchmarks::StructsShape;

    std::vector<Benchmark> all;
    all.push_back(row(
        "record: serialize", inputs.record, [](const Record& input) { return Tafla().serialize(RecordShape{}, input); },
        [](const auto& codec, const Record& input) { return codec.record(input); }));

    const auto document = [](const Document& input) { return Tafla().serialize(DocumentShape{}, input); };
    const auto write_document = [](const auto& codec, const Document& input) { return codec.document(input); };
    all.push_back(row("document of 100 rows: serialize", inputs.documents[0], document, write_document));
    all.push_back(row("document of 100,000 rows: serialize", inputs.documents[1], document, write_document));

    all.push_back(row(
        "vector of 100 nested tables: serialize", inputs.list,
        [](const List& input) { return Tafla().serialize(ListShape{}, input); },
        [](const auto& codec, const List& input) { return codec.list(input); }));
    all.push_back(row(
        "array of 100 structs: serialize", inputs.list,
        [](const List& input) { return Tafla().serialize(StructsShape{}, input); },
        [](const auto& codec, const List& input) { return codec.structs(input); }));
    return all;
}

}  // namespace tafla::prototype

#endif
