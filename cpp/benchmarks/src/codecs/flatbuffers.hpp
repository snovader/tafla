// FlatBuffers, through the code that `flatc --cpp` generates from
// schemas/workloads.fbs

#ifndef TAFLA_BENCHMARKS_CODECS_FLATBUFFERS_HPP
#define TAFLA_BENCHMARKS_CODECS_FLATBUFFERS_HPP

#include <cstdint>
#include <span>
#include <vector>

#include <flatbuffers/flatbuffers.h>

#include "../codec.hpp"
#include "workloads_generated.h"

namespace tafla::benchmarks {

/// flatc's generated code, used as its documentation shows.
///
/// Messages are written with a builder of the default size and the generated
/// `Create*()` functions with each table's fields, and read with
/// `GetRoot()` after a `Verifier` verifies them, as the generated `Get*()` and
/// `Verify*Buffer()` of a schema's root type do.
/// Vectors are written from spans with `CreateVector()`, and a vector of
/// structs with `CreateVectorOfStructs()` from flatc's structs.
///
/// A message is the builder's buffer from where the builder finished it,
/// released without a copy. Strings and columns are viewed in place, through
/// `string_view()` and `Vector::data()`, as C++'s FlatBuffers reads them on a
/// little-endian host.
///
/// Fields that a table leaves out read as null, which is an error here, since
/// every workload has them.
struct FlatBuffers {
    static constexpr std::string_view name = "flatbuffers";

    [[nodiscard]] ::flatbuffers::DetachedBuffer serialize(RecordShape, const Record& input) const {
        ::flatbuffers::FlatBufferBuilder builder;
        const auto name_offset = builder.CreateString(input.name.view());
        builder.Finish(tafla_fbs::CreateRecord(builder, input.x, input.y, input.id, name_offset));
        return builder.Release();
    }

    [[nodiscard]] Record deserialize(RecordShape, std::span<const std::uint8_t> bytes) const {
        const auto* record = verified<tafla_fbs::Record>(bytes);
        return {record->x(), record->y(), record->id(), Text::borrowed(present(record->name())->string_view())};
    }

    [[nodiscard]] ::flatbuffers::DetachedBuffer serialize(DocumentShape, const Document& input) const {
        ::flatbuffers::FlatBufferBuilder builder;
        const auto keys = builder.CreateVector(input.keys.span().data(), input.keys.span().size());
        const auto a = builder.CreateVector(input.a.span().data(), input.a.span().size());
        const auto b = builder.CreateVector(input.b.span().data(), input.b.span().size());
        const auto columns = tafla_fbs::CreateColumns(builder, input.base, keys, a, b);
        const auto name_offset = builder.CreateString(input.name.view());
        builder.Finish(tafla_fbs::CreateDocument(builder, input.x, input.y, name_offset, columns));
        return builder.Release();
    }

    [[nodiscard]] Document deserialize(DocumentShape, std::span<const std::uint8_t> bytes) const {
        const auto* document = verified<tafla_fbs::Document>(bytes);
        const auto* columns = present(document->columns());

        return {
            document->x(),
            document->y(),
            Text::borrowed(present(document->name())->string_view()),
            columns->base(),
            column_of(present(columns->keys())),
            column_of(present(columns->a())),
            column_of(present(columns->b())),
        };
    }

    /// Sums the column through `Vector::Get()`, the per-element accessor
    [[nodiscard]] double sum_a(std::span<const std::uint8_t> bytes) const {
        const auto* document = verified<tafla_fbs::Document>(bytes);
        const auto* a = present(present(document->columns())->a());

        double sum = 0;
        for (::flatbuffers::uoffset_t index = 0; index < a->size(); ++index) {
            sum += static_cast<double>(a->Get(index));
        }
        return sum;
    }

    [[nodiscard]] ::flatbuffers::DetachedBuffer serialize(ListShape, const List& input) const {
        ::flatbuffers::FlatBufferBuilder builder;
        std::vector<::flatbuffers::Offset<tafla_fbs::Item>> items(input.items.size());
        for (std::size_t index = 0; index < items.size(); ++index) {
            const auto& item = input.items[index];
            const auto pair = tafla_fbs::CreatePair(builder, item.first, item.second);
            items[index] = tafla_fbs::CreateItem(builder, item.value, pair);
        }
        builder.Finish(tafla_fbs::CreateList(builder, builder.CreateVector(items)));
        return builder.Release();
    }

    [[nodiscard]] List deserialize(ListShape, std::span<const std::uint8_t> bytes) const {
        const auto* items = present(verified<tafla_fbs::List>(bytes)->items());

        List list{std::vector<Item>(items->size())};
        auto into = list.items.begin();
        for (const auto* item : *items) {
            const auto* pair = present(item->pair());
            *into++ = {item->value(), pair->first(), pair->second()};
        }
        return list;
    }

    [[nodiscard]] ::flatbuffers::DetachedBuffer serialize(StructsShape, const List& input) const {
        ::flatbuffers::FlatBufferBuilder builder;
        std::vector<tafla_fbs::Triple> items(input.items.size());
        for (std::size_t index = 0; index < items.size(); ++index) {
            const auto& item = input.items[index];
            items[index] = tafla_fbs::Triple(item.value, item.first, item.second);
        }
        builder.Finish(tafla_fbs::CreateTriples(builder, builder.CreateVectorOfStructs(items)));
        return builder.Release();
    }

    [[nodiscard]] List deserialize(StructsShape, std::span<const std::uint8_t> bytes) const {
        const auto* items = present(verified<tafla_fbs::Triples>(bytes)->items());

        List list{std::vector<Item>(items->size())};
        auto into = list.items.begin();
        for (const auto* triple : *items) {
            *into++ = {triple->value(), triple->first(), triple->second()};
        }
        return list;
    }

    [[nodiscard]] ::flatbuffers::DetachedBuffer serialize(SampleShape, const Sample& input) const {
        ::flatbuffers::FlatBufferBuilder builder;
        builder.Finish(tafla_fbs::CreateSample(builder, input.key, input.a, input.b));
        return builder.Release();
    }

    [[nodiscard]] Sample deserialize(SampleShape, std::span<const std::uint8_t> bytes) const {
        const auto* sample = verified<tafla_fbs::Sample>(bytes);
        return {sample->key(), sample->a(), sample->b()};
    }

    /// The root of verified bytes, as the generated `Verify*Buffer()` and
    /// `Get*()` read a schema's root type
    template <class Table>
    static const Table* verified(std::span<const std::uint8_t> bytes) {
        ::flatbuffers::Verifier verifier(bytes.data(), bytes.size());
        if (!verifier.VerifyBuffer<Table>(nullptr)) {
            throw CodecError("The FlatBuffers message isn't valid");
        }
        return ::flatbuffers::GetRoot<Table>(bytes.data());
    }

    template <class T>
    static const T* present(const T* field) {
        if (field == nullptr) {
            throw CodecError("The FlatBuffers message lacks a field");
        }
        return field;
    }

    /// A column in place: the vector's elements, which the verifier checked
    /// lie within the bytes and are aligned
    template <class T>
    static Column<T> column_of(const ::flatbuffers::Vector<T>* vector) {
        return Column<T>::borrowed(std::span<const T>(vector->data(), vector->size()));
    }
};

}  // namespace tafla::benchmarks

#endif
