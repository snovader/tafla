// protobuf, through the code that `protoc --cpp_out` generates from
// schemas/messages.proto

#ifndef TAFLA_BENCHMARKS_CODECS_PROTOBUF_HPP
#define TAFLA_BENCHMARKS_CODECS_PROTOBUF_HPP

#include <cstddef>
#include <cstdint>
#include <memory>
#include <span>
#include <string>
#include <vector>

#include "../codec.hpp"
#include "messages.pb.h"

namespace tafla::benchmarks {

/// protobuf's generated messages, used as its documentation shows.
///
/// Messages are built from the input and written with
/// `SerializeAsString()`, which sizes the bytes first, and read with
/// `ParseFromArray()`. Messages own their strings and repeated fields, so
/// building one copies its columns, as a caller that keeps its values would.
/// Reading decodes the columns into the message's repeated fields, which the
/// output views, keeping the message, as tafla's and FlatBuffers' outputs view
/// theirs in the bytes, and moves its strings.
/// protobuf has no structs, so the array of them is a repeated message of the
/// three floats.
struct Protobuf {
    static constexpr std::string_view name = "protobuf";

    [[nodiscard]] std::string serialize(RecordShape, const Record& input) const {
        tafla_proto::Record message;
        message.set_x(input.x);
        message.set_y(input.y);
        message.set_id(input.id);
        message.set_name(input.name.view());
        return message.SerializeAsString();
    }

    [[nodiscard]] Record deserialize(RecordShape, std::span<const std::uint8_t> bytes) const {
        auto message = parse<tafla_proto::Record>(bytes);
        return {message.x(), message.y(), message.id(), Text::owned(std::move(*message.mutable_name()))};
    }

    [[nodiscard]] std::string serialize(DocumentShape, const Document& input) const {
        tafla_proto::Document message;
        message.set_x(input.x);
        message.set_y(input.y);
        message.set_name(input.name.view());

        auto& columns = *message.mutable_columns();
        columns.set_base(input.base);
        columns.mutable_keys()->Add(input.keys.span().begin(), input.keys.span().end());
        columns.mutable_a()->Add(input.a.span().begin(), input.a.span().end());
        columns.mutable_b()->Add(input.b.span().begin(), input.b.span().end());
        return message.SerializeAsString();
    }

    [[nodiscard]] Document deserialize(DocumentShape, std::span<const std::uint8_t> bytes) const {
        const auto message = std::make_shared<tafla_proto::Document>(parse<tafla_proto::Document>(bytes));
        const auto& columns = message->columns();

        return {
            message->x(),
            message->y(),
            Text::owned(std::move(*message->mutable_name())),
            columns.base(),
            Column<std::uint32_t>::kept(view_of(columns.keys()), message),
            Column<float>::kept(view_of(columns.a()), message),
            Column<float>::kept(view_of(columns.b()), message),
        };
    }

    [[nodiscard]] double sum_a(std::span<const std::uint8_t> bytes) const {
        const auto message = parse<tafla_proto::Document>(bytes);
        return sum_column(message.columns().a());
    }

    [[nodiscard]] std::string serialize(ListShape, const List& input) const {
        tafla_proto::List message;
        message.mutable_items()->Reserve(static_cast<int>(input.items.size()));
        for (const auto& item : input.items) {
            auto& added = *message.add_items();
            added.set_value(item.value);
            added.mutable_pair()->set_first(item.first);
            added.mutable_pair()->set_second(item.second);
        }
        return message.SerializeAsString();
    }

    [[nodiscard]] List deserialize(ListShape, std::span<const std::uint8_t> bytes) const {
        const auto message = parse<tafla_proto::List>(bytes);

        List list{std::vector<Item>(static_cast<std::size_t>(message.items_size()))};
        auto into = list.items.begin();
        for (const auto& item : message.items()) {
            *into++ = {item.value(), item.pair().first(), item.pair().second()};
        }
        return list;
    }

    [[nodiscard]] std::string serialize(StructsShape, const List& input) const {
        tafla_proto::Triples message;
        message.mutable_items()->Reserve(static_cast<int>(input.items.size()));
        for (const auto& item : input.items) {
            auto& added = *message.add_items();
            added.set_value(item.value);
            added.set_first(item.first);
            added.set_second(item.second);
        }
        return message.SerializeAsString();
    }

    [[nodiscard]] List deserialize(StructsShape, std::span<const std::uint8_t> bytes) const {
        const auto message = parse<tafla_proto::Triples>(bytes);

        List list{std::vector<Item>(static_cast<std::size_t>(message.items_size()))};
        auto into = list.items.begin();
        for (const auto& item : message.items()) {
            *into++ = {item.value(), item.first(), item.second()};
        }
        return list;
    }

    [[nodiscard]] std::string serialize(SampleShape, const Sample& input) const {
        tafla_proto::Sample message;
        message.set_key(input.key);
        message.set_a(input.a);
        message.set_b(input.b);
        return message.SerializeAsString();
    }

    [[nodiscard]] Sample deserialize(SampleShape, std::span<const std::uint8_t> bytes) const {
        const auto message = parse<tafla_proto::Sample>(bytes);
        return {message.key(), message.a(), message.b()};
    }

    /// The values of a repeated field, in place
    template <class T>
    static std::span<const T> view_of(const ::google::protobuf::RepeatedField<T>& field) {
        return {field.data(), static_cast<std::size_t>(field.size())};
    }

    template <class Message>
    static Message parse(std::span<const std::uint8_t> bytes) {
        Message message;
        if (!message.ParseFromArray(bytes.data(), static_cast<int>(bytes.size()))) {
            throw CodecError("The protobuf message isn't valid");
        }
        return message;
    }
};

}  // namespace tafla::benchmarks

#endif
