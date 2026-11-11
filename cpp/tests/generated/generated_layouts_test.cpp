// The code generated from `schemas/layouts.tafla` writes a message in each
// layout as `TableBuilder` writes it by hand, with the version in its header,
// reads back what it writes, rejects other versions, reads corrupted bytes as
// values or decode errors, and writes tables of `uint8` and `uint16` offsets up
// to the longest that they address and fails past it

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <optional>
#include <string>
#include <tuple>
#include <vector>

#include "common/build.hpp"
#include "common/values.hpp"
#include "layouts/layouts.hpp"

namespace tafla::test {
namespace {

/// A message of the schema, and its layout and version as its attributes give
/// them, which the tests repeat rather than take from the generated code
template <class Message, class Fields>
struct Spec {
    using MessageType = Message;
    using FieldsType = Fields;

    Layout layout;
    std::optional<std::uint32_t> version;
};

const auto kSpecs = std::make_tuple(
    Spec<layouts::Uint8Offsets, layouts::Uint8OffsetsFields>{{false, false, OffsetType::uint8}, std::nullopt},
    Spec<layouts::Uint8OffsetsSized, layouts::Uint8OffsetsSizedFields>{{true, false, OffsetType::uint8}, std::nullopt},
    Spec<layouts::Uint8OffsetsVersioned, layouts::Uint8OffsetsVersionedFields>{{false, true, OffsetType::uint8}, 255},
    Spec<layouts::Uint8OffsetsSizedVersioned, layouts::Uint8OffsetsSizedVersionedFields>{
        {true, true, OffsetType::uint8}, 0},
    Spec<layouts::Uint16Offsets, layouts::Uint16OffsetsFields>{{false, false, OffsetType::uint16}, std::nullopt},
    Spec<layouts::Uint16OffsetsSized, layouts::Uint16OffsetsSizedFields>{{true, false, OffsetType::uint16},
                                                                         std::nullopt},
    Spec<layouts::Uint16OffsetsVersioned, layouts::Uint16OffsetsVersionedFields>{{false, true, OffsetType::uint16},
                                                                                 65535},
    Spec<layouts::Uint16OffsetsSizedVersioned, layouts::Uint16OffsetsSizedVersionedFields>{
        {true, true, OffsetType::uint16}, 1},
    Spec<layouts::Uint32Offsets, layouts::Uint32OffsetsFields>{{false, false, OffsetType::uint32}, std::nullopt},
    Spec<layouts::Uint32OffsetsSized, layouts::Uint32OffsetsSizedFields>{{true, false, OffsetType::uint32},
                                                                         std::nullopt},
    Spec<layouts::Uint32OffsetsVersioned, layouts::Uint32OffsetsVersionedFields>{{false, true, OffsetType::uint32},
                                                                                 4'294'967'295},
    Spec<layouts::Uint32OffsetsSizedVersioned, layouts::Uint32OffsetsSizedVersionedFields>{
        {true, true, OffsetType::uint32}, 2});

template <class Function>
void for_each_spec(Function function) {
    std::apply([&](const auto&... spec) { (function(spec), ...); }, kSpecs);
}

/// The values of every message's fields: an id, a name and values
struct Owned {
    std::uint32_t id;
    std::string name;
    std::vector<std::uint16_t> values;
};

rc::Gen<Owned> owned() {
    return rc::gen::construct<Owned>(values::integers<std::uint32_t>(), values::strings(),
                                     values::arrays(values::integers<std::uint16_t>(), 40));
}

Table describe(const Layout& layout, std::optional<std::uint32_t> version, const Owned& owned) {
    return {layout, version, {{owned.id}, {owned.name}, {owned.values}}};
}

template <class Message>
Table read(const Layout& layout, std::optional<std::uint32_t> version, const Message& message) {
    return {layout, version, {{message.id()}, {std::string(message.name())}, {message.values().to_vector()}}};
}

template <class Fields>
Fields fields_of(const Owned& owned) {
    return {.id = owned.id, .name = owned.name, .values = owned.values};
}

/// The version slot of a layout's header, after the length when it's sized
std::size_t version_slot(const Layout& layout) {
    return layout.sized ? offset_size(layout.offset_type) : 0;
}

/// Each message is written as `TableBuilder` writes it by hand, reads back as
/// it was written, and fails as it does when it's too long for its offsets
RC_GTEST_PROP(GeneratedLayouts, EveryLayoutIsWrittenAsTableBuilderWritesItAndReadsBack, ()) {
    const auto values = *owned();

    for_each_spec([&](const auto& spec) {
        using Spec = std::decay_t<decltype(spec)>;
        using Message = typename Spec::MessageType;
        const auto expected = describe(spec.layout, spec.version, values);
        const auto fields = fields_of<typename Spec::FieldsType>(values);

        std::optional<Bytes> built;
        try {
            built = to_bytes(expected);
        } catch (const BuildError&) {
            try {
                static_cast<void>(fields.serialize());
                RC_FAIL("A table too long for its offsets was written");
            } catch (const BuildError& error) {
                RC_ASSERT(error.kind() == BuildError::Kind::too_long);
            }
            return;
        }

        const auto bytes = fields.serialize();
        RC_ASSERT(bytes == *built);
        RC_ASSERT(read(spec.layout, spec.version, Message(bytes)) == expected);
        if constexpr (requires { Message::FORMAT_VERSION; }) {
            RC_ASSERT(Message::FORMAT_VERSION == *spec.version);
            RC_ASSERT(Message::version_of(bytes) == *spec.version);
        }
    });
}

/// A versioned message rejects every other version, from the slot that every
/// version of its layout holds it in
RC_GTEST_PROP(GeneratedLayouts, EveryVersionedLayoutRejectsOtherVersions, ()) {
    const auto other = *values::integers<std::uint32_t>();

    for_each_spec([&](const auto& spec) {
        using Spec = std::decay_t<decltype(spec)>;
        using Message = typename Spec::MessageType;
        if constexpr (requires { Message::FORMAT_VERSION; }) {
            const auto version =
                static_cast<std::uint32_t>(other % (std::uint64_t{max_value(spec.layout.offset_type)} + 1));
            if (version == Message::FORMAT_VERSION) {
                return;
            }

            // Every message's header holds the version slot, which `at()`
            // shows GCC, whose warnings otherwise see a write into no bytes
            auto bytes = typename Spec::FieldsType{}.serialize().to_vector();
            const auto slot = version_slot(spec.layout);
            for (std::size_t byte = 0; byte < offset_size(spec.layout.offset_type); ++byte) {
                bytes.at(slot + byte) = static_cast<std::uint8_t>(version >> (8 * byte));
            }

            RC_ASSERT(Message::version_of(bytes) == version);
            try {
                static_cast<void>(Message(bytes));
                RC_FAIL("A message of another version was read");
            } catch (const DecodeError& error) {
                RC_ASSERT(error.kind() == DecodeError::Kind::unsupported_version);
                RC_ASSERT(error.version() == version);
            }
        }
    });
}

/// Corrupted and arbitrary bytes read as values or decode errors, and the
/// header of a versioned message as a version or a decode error too
RC_GTEST_PROP(GeneratedLayouts, CorruptedTablesReadAsValuesOrDecodeErrors, ()) {
    const auto values = *owned();
    const auto corruption = *values::corruptions();

    for_each_spec([&](const auto& spec) {
        using Spec = std::decay_t<decltype(spec)>;
        using Message = typename Spec::MessageType;

        std::vector<std::uint8_t> bytes;
        try {
            bytes = fields_of<typename Spec::FieldsType>(values).serialize().to_vector();
        } catch (const BuildError&) {
            return;
        }

        for (const auto& corrupted : values::corrupted(bytes, corruption)) {
            try {
                static_cast<void>(read(spec.layout, spec.version, Message(corrupted)));
            } catch (const DecodeError&) {
                // Corrupted bytes may fail to read
            }
        }
    });
}

/// Tables of `uint8` and `uint16` offsets hold values up to the longest that
/// their offsets address, and fail with one more
TEST(GeneratedLayouts, EveryLayoutOfShortOffsetsIsWrittenUpToItsLimitAndFailsPastIt) {
    for_each_spec([&](const auto& spec) {
        using Spec = std::decay_t<decltype(spec)>;
        if (spec.layout.offset_type == OffsetType::uint32) {
            return;
        }

        const std::size_t max = max_value(spec.layout.offset_type);
        const auto empty = typename Spec::FieldsType{}.serialize().size();
        // The values are padded to 2 from the end of the empty table, which
        // is where they'd start, so the room is whole values
        const auto count = (max - empty) / 2;

        const std::vector<std::uint16_t> values(count, 7);
        const typename Spec::FieldsType longest{.values = values};
        const auto bytes = longest.serialize();
        EXPECT_GT(bytes.size(), max - 2);
        EXPECT_LE(bytes.size(), max);
        EXPECT_EQ(bytes, to_bytes(describe(spec.layout, spec.version, {0, "", values})));

        const std::vector<std::uint16_t> more(count + 1, 7);
        const typename Spec::FieldsType too_long{.values = more};
        try {
            static_cast<void>(too_long.serialize());
            ADD_FAILURE() << "A table too long for its offsets was written";
        } catch (const BuildError& error) {
            EXPECT_EQ(error.kind(), BuildError::Kind::too_long);
            EXPECT_EQ(error.index(), 2U);
            EXPECT_EQ(error.offset_type(), spec.layout.offset_type);
        }
    });
}

}  // namespace
}  // namespace tafla::test
