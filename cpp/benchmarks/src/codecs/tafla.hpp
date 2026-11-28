// tafla's runtime, used by hand, as its README shows

#ifndef TAFLA_BENCHMARKS_CODECS_TAFLA_HPP
#define TAFLA_BENCHMARKS_CODECS_TAFLA_HPP

#include <array>
#include <cstdint>
#include <span>
#include <utility>
#include <vector>

#include <tafla/tafla.hpp>

#include "../codec.hpp"

namespace tafla::benchmarks {

/// tafla's runtime, as its README shows.
///
/// Nested tables are written in place into their parent builder's buffer,
/// with `set_table(fields, fill)` and `set_vector(count, fields, fill)`. The
/// builder copies the columns when they're set, and hands over its buffer,
/// as long as the table, with `into_buffer()`. Arrays are read in place as
/// `ArrayView`s, whose `as_span()` views them when they're aligned on a
/// little-endian host.
///
/// The runtime writes tables, and leaves fixed-size records to
/// `tafla::store_le()` and `load_le()`. The fixed-size record is written with
/// `store_le()` and read with `TableReader`'s getters, and an array of structs
/// with `store_le()` and `load_le()`, as tafla-gen's code writes and reads an
/// array of them.
///
/// The strings suite's 16 strings are a table of 16 string fields, which it
/// times, rather than a vector of strings, written with `set_string()` and
/// read with `get_string()`, which checks that each is UTF-8 and views it.
struct Tafla {
    static constexpr std::string_view name = "tafla";

    static constexpr std::uint32_t kDocumentVersion = 1;
    static constexpr auto kDocumentLayout = TableLayout().sized().with_version(kDocumentVersion);

    /// An item as a struct: its three float32s, aligned to 4 in an array
    static constexpr std::size_t kItemSize = 12;
    static constexpr auto kItemAlignment = Alignment::four;

    [[nodiscard]] std::vector<std::uint8_t> serialize(RecordShape, const Record& input) const {
        TableBuilder builder(4, TableLayout());
        builder.set_float64(input.x);
        builder.set_float64(input.y);
        builder.set_int64(input.id);
        builder.set_string(input.name.view());
        return std::move(builder).into_buffer();
    }

    [[nodiscard]] Record deserialize(RecordShape, std::span<const std::uint8_t> bytes) const {
        const TableReader reader(bytes, TableLayout());
        return {
            reader.get_float64(reader.root_offset(0)),
            reader.get_float64(reader.root_offset(1)),
            reader.get_int64(reader.root_offset(2)),
            Text::borrowed(reader.get_string(reader.root_offset(3))),
        };
    }

    [[nodiscard]] std::vector<std::uint8_t> serialize(DocumentShape, const Document& input) const {
        TableBuilder document(4, kDocumentLayout);
        document.set_float64(input.x);
        document.set_float64(input.y);
        document.set_string(input.name.view());
        document.set_table(4, [&](TableBuilder::Item& columns) {
            columns.set_int64(input.base);
            columns.set_uint32_array(input.keys.span());
            columns.set_float32_array(input.a.span());
            columns.set_float32_array(input.b.span());
        });
        return std::move(document).into_buffer();
    }

    [[nodiscard]] Document deserialize(DocumentShape, std::span<const std::uint8_t> bytes) const {
        const auto reader = open_document(bytes);
        const auto columns = reader.root_offset(3);

        return {
            reader.get_float64(reader.root_offset(0)),
            reader.get_float64(reader.root_offset(1)),
            Text::borrowed(reader.get_string(reader.root_offset(2))),
            reader.get_int64(reader.field_offset(columns, 0)),
            column_of(reader.get_uint32_array(reader.field_offset(columns, 1))),
            column_of(reader.get_float32_array(reader.field_offset(columns, 2))),
            column_of(reader.get_float32_array(reader.field_offset(columns, 3))),
        };
    }

    [[nodiscard]] double sum_a(std::span<const std::uint8_t> bytes) const {
        const auto reader = open_document(bytes);
        return sum_column(reader.get_float32_array(reader.field_offset(reader.root_offset(3), 2)));
    }

    [[nodiscard]] std::vector<std::uint8_t> serialize(ListShape, const List& input) const {
        const auto& items = input.items;

        TableBuilder builder(1, TableLayout());
        builder.set_vector(items.size(), 2,
                           [&](std::size_t index, TableBuilder::Item& item) { set_item(item, items[index]); });
        return std::move(builder).into_buffer();
    }

    [[nodiscard]] List deserialize(ListShape, std::span<const std::uint8_t> bytes) const {
        const TableReader reader(bytes, TableLayout());
        const auto vector = reader.get_vector(reader.root_offset(0));

        List list{std::vector<Item>(vector.size())};
        auto into = list.items.begin();
        for (const auto item : vector.items_in_place()) {
            *into++ = read_item(item);
        }
        return list;
    }

    /// Writes the structs one after another into a vector, which the builder
    /// views until it writes the table. tafla-gen's code writes them straight
    /// into the table, with `TableWriter::set_structs()`.
    [[nodiscard]] std::vector<std::uint8_t> serialize(StructsShape, const List& input) const {
        std::vector<std::uint8_t> structs(input.items.size() * kItemSize);
        for (std::size_t index = 0; index < input.items.size(); ++index) {
            write_item_struct(input.items[index], structs.data() + index * kItemSize);
        }

        TableBuilder builder(1, TableLayout());
        builder.set_struct_array(std::span<const std::uint8_t>(structs), kItemSize, kItemAlignment);
        return std::move(builder).into_buffer();
    }

    [[nodiscard]] List deserialize(StructsShape, std::span<const std::uint8_t> bytes) const {
        const TableReader reader(bytes, TableLayout());
        const auto structs = reader.get_struct_array(reader.root_offset(0), kItemSize);

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
        const TableReader reader(bytes, TableLayout());
        return {reader.get_uint32(0), reader.get_float32(4), reader.get_float32(8)};
    }

    [[nodiscard]] std::vector<std::uint8_t> serialize(StringsShape, const Strings& input) const {
        TableBuilder builder(kStrings, TableLayout());
        for (const auto& string : input) {
            builder.set_string(string.view());
        }
        return std::move(builder).into_buffer();
    }

    [[nodiscard]] Strings deserialize(StringsShape, std::span<const std::uint8_t> bytes) const {
        const TableReader reader(bytes, TableLayout());

        Strings strings;
        for (std::size_t index = 0; index < kStrings; ++index) {
            strings[index] = Text::borrowed(reader.get_string(reader.root_offset(index)));
        }
        return strings;
    }

    /// Reads a document's header, which must carry the version it's written in
    static TableReader<true> open_document(std::span<const std::uint8_t> bytes) {
        const TableReader reader(bytes, kDocumentLayout);
        const auto version = reader.version();
        if (version != kDocumentVersion) {
            throw DecodeError::unsupported_version(version, kDocumentVersion);
        }
        return reader;
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

    static void set_item(TableBuilder::Item& table, const Item& item) {
        table.set_float32(item.value);
        table.set_table(2, [&](TableBuilder::Item& pair) {
            pair.set_float32(item.first);
            pair.set_float32(item.second);
        });
    }

    /// Reads an item of the vector in place, a table without a header, whose
    /// fields `root_offset()` finds from its start
    static Item read_item(std::span<const std::uint8_t> bytes) {
        const TableReader item(bytes, TableLayout());
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
