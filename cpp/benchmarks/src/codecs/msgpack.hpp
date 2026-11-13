// MessagePack, through msgpack-cxx's packer and unpacker

#ifndef TAFLA_BENCHMARKS_CODECS_MSGPACK_HPP
#define TAFLA_BENCHMARKS_CODECS_MSGPACK_HPP

#include <cstdint>
#include <span>
#include <string_view>
#include <vector>

#include <msgpack.hpp>

#include "../codec.hpp"

namespace tafla::benchmarks {

/// msgpack-cxx, which packs the workloads' values into an `sbuffer`, as its
/// documentation shows, and unpacks them into an object, which the codec
/// reads its fields from.
///
/// The msgpack format is schemaless: a table packs as a map with its field
/// names, as msgpackr packs an object and rmp-serde's `to_vec_named()` packs a
/// struct, and a document packs as one map. Columns pack as arrays of
/// float32s and uint32s, element by element, since msgpack has no typed
/// arrays. Unpacking references strings in the bytes, so reading views them,
/// and copies columns into vectors.
///
/// Structs have a fixed shape, so an array of them packs each struct, and the
/// table that holds them, as an array of its fields in order, without their
/// names, as rmp-serde's `to_vec()` does.
struct Msgpack {
    static constexpr std::string_view name = "msgpack";

    using Packer = msgpack::packer<msgpack::sbuffer>;

    [[nodiscard]] msgpack::sbuffer serialize(RecordShape, const Record& input) const {
        msgpack::sbuffer buffer;
        Packer packer(buffer);
        packer.pack_map(4);
        packer.pack("x");
        packer.pack(input.x);
        packer.pack("y");
        packer.pack(input.y);
        packer.pack("id");
        packer.pack(input.id);
        packer.pack("name");
        packer.pack(input.name.view());
        return buffer;
    }

    [[nodiscard]] Record deserialize(RecordShape, std::span<const std::uint8_t> bytes) const {
        const auto handle = unpack(bytes);
        Record record;
        for (const auto& [key, value] : map_of(handle.get())) {
            const auto field = key.as<std::string_view>();
            if (field == "x") {
                record.x = value.as<double>();
            } else if (field == "y") {
                record.y = value.as<double>();
            } else if (field == "id") {
                record.id = value.as<std::int64_t>();
            } else if (field == "name") {
                record.name = Text::borrowed(value.as<std::string_view>());
            }
        }
        return record;
    }

    [[nodiscard]] msgpack::sbuffer serialize(DocumentShape, const Document& input) const {
        msgpack::sbuffer buffer;
        Packer packer(buffer);
        packer.pack_map(7);
        packer.pack("x");
        packer.pack(input.x);
        packer.pack("y");
        packer.pack(input.y);
        packer.pack("name");
        packer.pack(input.name.view());
        packer.pack("base");
        packer.pack(input.base);
        packer.pack("keys");
        pack_column(packer, input.keys.span());
        packer.pack("a");
        pack_column(packer, input.a.span());
        packer.pack("b");
        pack_column(packer, input.b.span());
        return buffer;
    }

    [[nodiscard]] Document deserialize(DocumentShape, std::span<const std::uint8_t> bytes) const {
        const auto handle = unpack(bytes);
        Document document;
        for (const auto& [key, value] : map_of(handle.get())) {
            const auto field = key.as<std::string_view>();
            if (field == "x") {
                document.x = value.as<double>();
            } else if (field == "y") {
                document.y = value.as<double>();
            } else if (field == "name") {
                document.name = Text::borrowed(value.as<std::string_view>());
            } else if (field == "base") {
                document.base = value.as<std::int64_t>();
            } else if (field == "keys") {
                document.keys = Column<std::uint32_t>::owned(value.as<std::vector<std::uint32_t>>());
            } else if (field == "a") {
                document.a = Column<float>::owned(value.as<std::vector<float>>());
            } else if (field == "b") {
                document.b = Column<float>::owned(value.as<std::vector<float>>());
            }
        }
        return document;
    }

    /// Unpacks the whole document, and sums its column
    [[nodiscard]] double sum_a(std::span<const std::uint8_t> bytes) const {
        return sum_column(deserialize(DocumentShape{}, bytes).a.span());
    }

    [[nodiscard]] msgpack::sbuffer serialize(ListShape, const List& input) const {
        msgpack::sbuffer buffer;
        Packer packer(buffer);
        packer.pack_map(1);
        packer.pack("items");
        packer.pack_array(static_cast<std::uint32_t>(input.items.size()));
        for (const auto& item : input.items) {
            packer.pack_map(3);
            packer.pack("value");
            packer.pack(item.value);
            packer.pack("first");
            packer.pack(item.first);
            packer.pack("second");
            packer.pack(item.second);
        }
        return buffer;
    }

    [[nodiscard]] List deserialize(ListShape, std::span<const std::uint8_t> bytes) const {
        const auto handle = unpack(bytes);
        List list;
        for (const auto& [key, value] : map_of(handle.get())) {
            if (key.as<std::string_view>() != "items") {
                continue;
            }
            const auto items = array_of(value);
            list.items.resize(items.size());
            auto into = list.items.begin();
            for (const auto& item : items) {
                auto& read = *into++;
                for (const auto& [field, number] : map_of(item)) {
                    const auto name_of_field = field.as<std::string_view>();
                    if (name_of_field == "value") {
                        read.value = number.as<float>();
                    } else if (name_of_field == "first") {
                        read.first = number.as<float>();
                    } else if (name_of_field == "second") {
                        read.second = number.as<float>();
                    }
                }
            }
        }
        return list;
    }

    [[nodiscard]] msgpack::sbuffer serialize(StructsShape, const List& input) const {
        msgpack::sbuffer buffer;
        Packer packer(buffer);
        packer.pack_array(1);
        packer.pack_array(static_cast<std::uint32_t>(input.items.size()));
        for (const auto& item : input.items) {
            packer.pack_array(3);
            packer.pack(item.value);
            packer.pack(item.first);
            packer.pack(item.second);
        }
        return buffer;
    }

    [[nodiscard]] List deserialize(StructsShape, std::span<const std::uint8_t> bytes) const {
        const auto handle = unpack(bytes);
        const auto table = array_of(handle.get());
        if (table.size() != 1) {
            throw CodecError("The msgpack array of structs isn't a table of one field");
        }

        const auto items = array_of(table[0]);
        List list{std::vector<Item>(items.size())};
        auto into = list.items.begin();
        for (const auto& item : items) {
            const auto fields = array_of(item);
            if (fields.size() != 3) {
                throw CodecError("A msgpack struct isn't three floats");
            }
            *into++ = {fields[0].as<float>(), fields[1].as<float>(), fields[2].as<float>()};
        }
        return list;
    }

    [[nodiscard]] msgpack::sbuffer serialize(SampleShape, const Sample& input) const {
        msgpack::sbuffer buffer;
        Packer packer(buffer);
        packer.pack_map(3);
        packer.pack("key");
        packer.pack(input.key);
        packer.pack("a");
        packer.pack(input.a);
        packer.pack("b");
        packer.pack(input.b);
        return buffer;
    }

    [[nodiscard]] Sample deserialize(SampleShape, std::span<const std::uint8_t> bytes) const {
        const auto handle = unpack(bytes);
        Sample sample;
        for (const auto& [key, value] : map_of(handle.get())) {
            const auto field = key.as<std::string_view>();
            if (field == "key") {
                sample.key = value.as<std::uint32_t>();
            } else if (field == "a") {
                sample.a = value.as<float>();
            } else if (field == "b") {
                sample.b = value.as<float>();
            }
        }
        return sample;
    }

    /// Packs a column as an array, element by element
    template <class T>
    static void pack_column(Packer& packer, std::span<const T> values) {
        packer.pack_array(static_cast<std::uint32_t>(values.size()));
        for (const T value : values) {
            packer.pack(value);
        }
    }

    /// Unpacks bytes, referencing their strings rather than copying them
    static msgpack::object_handle unpack(std::span<const std::uint8_t> bytes) {
        const auto reference = [](msgpack::type::object_type type, std::size_t, void*) {
            return type == msgpack::type::STR;
        };
        return msgpack::unpack(reinterpret_cast<const char*>(bytes.data()), bytes.size(), reference);
    }

    static std::span<const msgpack::object_kv> map_of(const msgpack::object& object) {
        if (object.type != msgpack::type::MAP) {
            throw CodecError("The msgpack object isn't a map");
        }
        return {object.via.map.ptr, object.via.map.size};
    }

    static std::span<const msgpack::object> array_of(const msgpack::object& object) {
        if (object.type != msgpack::type::ARRAY) {
            throw CodecError("The msgpack object isn't an array");
        }
        return {object.via.array.ptr, object.via.array.size};
    }
};

}  // namespace tafla::benchmarks

#endif
