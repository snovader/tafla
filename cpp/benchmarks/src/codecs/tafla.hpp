// tafla's runtime, used by hand, as its README shows

#ifndef TAFLA_BENCHMARKS_CODECS_TAFLA_HPP
#define TAFLA_BENCHMARKS_CODECS_TAFLA_HPP

#include <array>
#include <cstdint>
#include <span>
#include <vector>

#include <tafla/tafla.hpp>

#include "../codec.hpp"

namespace tafla::benchmarks {

/// tafla's runtime, as its README shows.
///
/// Tables are nested from their builders, which view the columns, and arrays
/// are read in place as `ArrayView`s, whose `as_span()` views them when
/// they're aligned on a little-endian host.
///
/// The runtime writes tables, and leaves fixed-size records to
/// `tafla::store_le()` and `load_le()`. The fixed-size record is written with
/// `store_le()` and read with `ByteBuffer`'s getters, and an array of structs
/// with `store_le()` and `load_le()`, as tafla-gen's code writes and reads an
/// array of them.
struct Tafla {
    static constexpr std::string_view name = "tafla";

    static constexpr auto kDocumentLayout = TableLayout().sized().versioned();
    static constexpr std::uint32_t kDocumentVersion = 1;

    /// An item as a struct: its three float32s, aligned to 4 in an array
    static constexpr std::size_t kItemSize = 12;
    static constexpr auto kItemAlignment = Alignment::four;

    [[nodiscard]] Bytes serialize(RecordShape, const Record& input) const {
        TableBuilder builder(4, TableLayout());
        builder.set_float64(input.x);
        builder.set_float64(input.y);
        builder.set_int64(input.id);
        builder.set_string(input.name.view());
        return builder.to_bytes();
    }

    [[nodiscard]] Record deserialize(RecordShape, std::span<const std::uint8_t> bytes) const {
        const ByteBuffer buffer(bytes, TableLayout());
        return {
            buffer.get_float64(buffer.root_offset(0)),
            buffer.get_float64(buffer.root_offset(1)),
            buffer.get_int64(buffer.root_offset(2)),
            Text::borrowed(buffer.get_string(buffer.root_offset(3))),
        };
    }

    [[nodiscard]] Bytes serialize(DocumentShape, const Document& input) const {
        TableBuilder columns(4, TableLayout());
        columns.set_int64(input.base);
        columns.set_uint32_array(input.keys.span());
        columns.set_float32_array(input.a.span());
        columns.set_float32_array(input.b.span());

        TableBuilder document(4, kDocumentLayout, kDocumentVersion);
        document.set_float64(input.x);
        document.set_float64(input.y);
        document.set_string(input.name.view());
        document.set_table(std::move(columns));
        return document.to_bytes();
    }

    [[nodiscard]] Document deserialize(DocumentShape, std::span<const std::uint8_t> bytes) const {
        const auto buffer = open_document(bytes);
        const auto columns = buffer.root_offset(3);

        return {
            buffer.get_float64(buffer.root_offset(0)),
            buffer.get_float64(buffer.root_offset(1)),
            Text::borrowed(buffer.get_string(buffer.root_offset(2))),
            buffer.get_int64(buffer.field_offset(columns, 0)),
            column_of(buffer.get_uint32_array(buffer.field_offset(columns, 1))),
            column_of(buffer.get_float32_array(buffer.field_offset(columns, 2))),
            column_of(buffer.get_float32_array(buffer.field_offset(columns, 3))),
        };
    }

    [[nodiscard]] double sum_a(std::span<const std::uint8_t> bytes) const {
        const auto buffer = open_document(bytes);
        return sum_column(buffer.get_float32_array(buffer.field_offset(buffer.root_offset(3), 2)));
    }

    /// Nests each item from its builder, collected first
    [[nodiscard]] Bytes serialize(ListShape, const List& input) const {
        std::vector<NestedTable> items;
        items.reserve(input.items.size());
        for (const auto& item : input.items) {
            items.emplace_back(item_builder(item));
        }

        TableBuilder builder(1, TableLayout());
        builder.set_vector(std::move(items));
        return builder.to_bytes();
    }

    [[nodiscard]] List deserialize(ListShape, std::span<const std::uint8_t> bytes) const {
        const ByteBuffer buffer(bytes, TableLayout());
        const auto vector = buffer.get_vector(buffer.root_offset(0));

        List list{std::vector<Item>(vector.size())};
        auto into = list.items.begin();
        for (const auto item : vector.items_in_place()) {
            *into++ = read_item(item);
        }
        return list;
    }

    /// Writes the structs one after another into a vector, which the builder
    /// owns, as tafla-gen's code writes an array of them
    [[nodiscard]] Bytes serialize(StructsShape, const List& input) const {
        std::vector<std::uint8_t> structs(input.items.size() * kItemSize);
        for (std::size_t index = 0; index < input.items.size(); ++index) {
            write_item_struct(input.items[index], structs.data() + index * kItemSize);
        }

        TableBuilder builder(1, TableLayout());
        builder.set_struct_array(std::move(structs), kItemSize, kItemAlignment);
        return builder.to_bytes();
    }

    [[nodiscard]] List deserialize(StructsShape, std::span<const std::uint8_t> bytes) const {
        const ByteBuffer buffer(bytes, TableLayout());
        const auto structs = buffer.get_struct_array(buffer.root_offset(0), kItemSize);

        List list{std::vector<Item>(structs.size() / kItemSize)};
        for (std::size_t index = 0; index < list.items.size(); ++index) {
            list.items[index] = read_item_struct(structs.data() + index * kItemSize);
        }
        return list;
    }

    [[nodiscard]] std::array<std::uint8_t, 12> serialize(SampleShape, const Sample& input) const {
        std::array<std::uint8_t, 12> bytes{};
        store_le(bytes.data(), input.key);
        store_le(bytes.data() + 4, input.a);
        store_le(bytes.data() + 8, input.b);
        return bytes;
    }

    [[nodiscard]] Sample deserialize(SampleShape, std::span<const std::uint8_t> bytes) const {
        const ByteBuffer buffer(bytes, TableLayout());
        return {buffer.get_uint32(0), buffer.get_float32(4), buffer.get_float32(8)};
    }

    /// Reads a document's header, which must carry the version it's written in
    static ByteBuffer<true> open_document(std::span<const std::uint8_t> bytes) {
        const ByteBuffer buffer(bytes, kDocumentLayout);
        const auto version = buffer.version();
        if (version != kDocumentVersion) {
            throw DecodeError::unsupported_version(version, kDocumentVersion);
        }
        return buffer;
    }

    /// A column in place when it's aligned on a little-endian host, and
    /// copied otherwise
    template <class T>
    static Column<T> column_of(ArrayView<T> view) {
        if (const auto span = view.as_span()) {
            return Column<T>::borrowed(*span);
        }
        return Column<T>::owned(view.to_vector());
    }

    static TableBuilder item_builder(const Item& item) {
        TableBuilder pair(2, TableLayout());
        pair.set_float32(item.first);
        pair.set_float32(item.second);

        TableBuilder builder(2, TableLayout());
        builder.set_float32(item.value);
        builder.set_table(std::move(pair));
        return builder;
    }

    /// Reads an item of the vector in place, a table without a header, whose
    /// fields `root_offset()` finds from its start
    static Item read_item(std::span<const std::uint8_t> bytes) {
        const ByteBuffer item(bytes, TableLayout());
        const auto pair = item.root_offset(1);
        return {
            item.get_float32(item.root_offset(0)),
            item.get_float32(item.field_offset(pair, 0)),
            item.get_float32(item.field_offset(pair, 1)),
        };
    }

    static void write_item_struct(const Item& item, std::uint8_t* bytes) {
        store_le(bytes, item.value);
        store_le(bytes + 4, item.first);
        store_le(bytes + 8, item.second);
    }

    static Item read_item_struct(const std::uint8_t* bytes) {
        return {load_le<float>(bytes), load_le<float>(bytes + 4), load_le<float>(bytes + 8)};
    }
};

}  // namespace tafla::benchmarks

#endif
