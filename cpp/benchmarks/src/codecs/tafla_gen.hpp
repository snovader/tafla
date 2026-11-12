// The code that tafla-gen generates from workloads.tafla

#ifndef TAFLA_BENCHMARKS_CODECS_TAFLA_GEN_HPP
#define TAFLA_BENCHMARKS_CODECS_TAFLA_GEN_HPP

#include <array>
#include <cstdint>
#include <span>
#include <vector>

#include "../codec.hpp"
#include "../codecs/tafla.hpp"
#include "tafla/workloads.hpp"

namespace tafla::benchmarks {

/// The code that tafla-gen generates from `workloads.tafla`, in
/// `generated/tafla/`. A message's `serialize()` counts the table's length
/// with a `TableSizer`, and then writes every field, nested tables included,
/// into bytes of that length with a `TableWriter`, in one allocation. Its
/// readers view strings and columns, as tafla's do.
///
/// It takes the workloads' types, as the other codecs do, so it collects the
/// generated fields of the vector's items and the generated structs of the
/// array into a vector in each call, as the protobuf codec builds its
/// messages, since the generated code writes them from spans.
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
        const ::workloads::DocumentFields fields{
            .x = input.x,
            .y = input.y,
            .name = input.name.view(),
            .columns = {.base = input.base, .keys = input.keys.span(), .a = input.a.span(), .b = input.b.span()},
        };
        return fields.serialize();
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
        std::vector<::workloads::ItemFields> items(input.items.size());
        for (std::size_t index = 0; index < items.size(); ++index) {
            const auto& item = input.items[index];
            items[index] = {.value = item.value, .pair = {.first = item.first, .second = item.second}};
        }
        return ::workloads::ItemListFields{.items = items}.serialize();
    }

    /// Reads the items through the generated getter, which reads every item's
    /// reader into a vector first
    [[nodiscard]] List deserialize(ListShape, std::span<const std::uint8_t> bytes) const {
        const auto items = ::workloads::ItemList(bytes).items();

        List list{std::vector<Item>(items.size())};
        for (std::size_t index = 0; index < items.size(); ++index) {
            const auto pair = items[index].pair();
            list.items[index] = {items[index].value(), pair.first(), pair.second()};
        }
        return list;
    }

    [[nodiscard]] Bytes serialize(StructsShape, const List& input) const {
        std::vector<::workloads::Triple> items(input.items.size());
        for (std::size_t index = 0; index < items.size(); ++index) {
            const auto& item = input.items[index];
            items[index] = {.value = item.value, .first = item.first, .second = item.second};
        }
        return ::workloads::TriplesFields{.items = items}.serialize();
    }

    [[nodiscard]] List deserialize(StructsShape, std::span<const std::uint8_t> bytes) const {
        const auto triples = ::workloads::Triples(bytes).items_view();

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
};

}  // namespace tafla::benchmarks

#endif
