// The code that tafla-gen generates from workloads.tafla

#ifndef TAFLA_BENCHMARKS_CODECS_TAFLA_GEN_HPP
#define TAFLA_BENCHMARKS_CODECS_TAFLA_GEN_HPP

#include <array>
#include <cstddef>
#include <cstdint>
#include <ranges>
#include <span>
#include <vector>

#include "../codec.hpp"
#include "../codecs/tafla.hpp"
#include "tafla/workloads.hpp"

namespace tafla::benchmarks {

/// The code that tafla-gen generates from `workloads.tafla`, in
/// `generated/tafla/`. A message's `serialize()` counts the table's length
/// with a `TableSizer`, and then writes every field, nested tables included,
/// into bytes of that length with a `TableWriter`, in one allocation. The
/// record, whose fields' sizes are fixed but for a string after them, it
/// writes whole instead, each field at a constant position in bytes of its
/// length. Its readers view strings and columns, as tafla's do.
///
/// It takes the workloads' types, as the other codecs do, and writes the
/// vector's items and the array's structs from views of the workload's items
/// with the generated generic fields structs, which convert each item as they
/// write it, rather than collecting the items into the generated types first.
struct TaflaGen {
    static constexpr std::string_view name = "tafla-gen";

    [[nodiscard]] Bytes serialize(RecordShape, const Record& input) const {
        const ::workloads::RecordFields fields{.x = input.x, .y = input.y, .id = input.id, .name = input.name.view()};
        return fields.serialize();
    }

    [[nodiscard]] Record deserialize(RecordShape, std::span<const std::uint8_t> bytes) const {
        const ::workloads::Record record(bytes);
        return {record.x(), record.y(), record.id(), Text::borrowed(record.name())};
    }

    [[nodiscard]] Bytes serialize(DocumentShape, const Document& input) const {
        return fields_of(input).serialize();
    }

    /// Writes the document into the start of `bytes`, which the caller keeps
    /// and writes the next document into, and returns its length
    [[nodiscard]] std::size_t serialize_into(DocumentShape, const Document& input,
                                             std::span<std::uint8_t> bytes) const {
        return fields_of(input).serialize_into(bytes);
    }

    [[nodiscard]] Document deserialize(DocumentShape, std::span<const std::uint8_t> bytes) const {
        const ::workloads::Document document(bytes);
        const auto columns = document.columns();

        return {
            document.x(),
            document.y(),
            Text::borrowed(document.name()),
            columns.base(),
            Tafla::column_of(columns.keys()),
            Tafla::column_of(columns.a()),
            Tafla::column_of(columns.b()),
        };
    }

    [[nodiscard]] double sum_a(std::span<const std::uint8_t> bytes) const {
        return sum_column(::workloads::Document(bytes).columns().a());
    }

    [[nodiscard]] Bytes serialize(ListShape, const List& input) const {
        const auto items = input.items | std::views::transform(item_fields_of);
        return ::workloads::generic::ItemListFields{.items = items}.serialize();
    }

    /// Reads the items through the generated getter's view, which reads each
    /// item's end once as it iterates
    [[nodiscard]] List deserialize(ListShape, std::span<const std::uint8_t> bytes) const {
        const auto items = ::workloads::ItemList(bytes).items();

        List list{std::vector<Item>(items.size())};
        auto into = list.items.begin();
        for (const auto item : items) {
            const auto pair = item.pair();
            *into++ = {item.value(), pair.first(), pair.second()};
        }
        return list;
    }

    [[nodiscard]] Bytes serialize(StructsShape, const List& input) const {
        const auto items = input.items | std::views::transform(triple_of);
        return ::workloads::generic::TriplesFields{.items = items}.serialize();
    }

    [[nodiscard]] List deserialize(StructsShape, std::span<const std::uint8_t> bytes) const {
        const auto triples = ::workloads::Triples(bytes).items();

        List list{std::vector<Item>(triples.size())};
        for (std::size_t index = 0; index < triples.size(); ++index) {
            const auto triple = triples[index];
            list.items[index] = {triple.value, triple.first, triple.second};
        }
        return list;
    }

    [[nodiscard]] std::array<std::uint8_t, 12> serialize(SampleShape, const Sample& input) const {
        std::array<std::uint8_t, ::workloads::Sample::SIZE> bytes{};
        ::workloads::Sample{.key = input.key, .a = input.a, .b = input.b}.to_bytes(bytes.data());
        return bytes;
    }

    [[nodiscard]] Sample deserialize(SampleShape, std::span<const std::uint8_t> bytes) const {
        const auto sample = ::workloads::Sample::read(bytes);
        return {sample.key, sample.a, sample.b};
    }

private:
    /// The generated fields of an item of the vector
    static constexpr auto item_fields_of = [](const Item& item) noexcept -> ::workloads::ItemFields {
        return {.value = item.value, .pair = {.first = item.first, .second = item.second}};
    };

    /// The generated struct of an item of the array
    static constexpr auto triple_of = [](const Item& item) noexcept -> ::workloads::Triple {
        return {.value = item.value, .first = item.first, .second = item.second};
    };

    /// The generated fields of a document, which view its name and columns
    static ::workloads::DocumentFields fields_of(const Document& input) {
        return {
            .x = input.x,
            .y = input.y,
            .name = input.name.view(),
            .columns = {.base = input.base, .keys = input.keys.span(), .a = input.a.span(), .b = input.b.span()},
        };
    }
};

}  // namespace tafla::benchmarks

#endif
