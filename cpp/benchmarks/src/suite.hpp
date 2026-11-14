// The comparison's benchmarks: each workload's serialize and deserialize, and
// a document's column summed per element, through every codec that encodes
// it, and the ways to read tafla's items. Every codec is checked to read back
// exactly what it wrote before anything is timed.

#ifndef TAFLA_BENCHMARKS_SUITE_HPP
#define TAFLA_BENCHMARKS_SUITE_HPP

#include <array>
#include <bit>
#include <memory>
#include <stdexcept>
#include <string>
#include <string_view>
#include <tuple>
#include <type_traits>
#include <vector>

#include "codec.hpp"
#include "codecs/bit_cast.hpp"
#include "codecs/flatbuffers.hpp"
#include "codecs/msgpack.hpp"
#include "codecs/protobuf.hpp"
#include "codecs/tafla.hpp"
#include "codecs/tafla_gen.hpp"
#include "harness.hpp"
#include "item_access.hpp"
#include "workloads.hpp"

namespace tafla::benchmarks {

/// The codecs, in the order that results list them, tafla's first, which the
/// others' ratios compare with. `bit_cast` writes the format's bytes only on a
/// little-endian host.
using Codecs = std::conditional_t<std::endian::native == std::endian::little,
                                  std::tuple<Tafla, TaflaGen, FlatBuffers, Protobuf, Msgpack, BitCast>,
                                  std::tuple<Tafla, TaflaGen, FlatBuffers, Protobuf, Msgpack>>;

/// The name of tafla's codec again, timed beside itself, an A/A pair whose
/// ratio shows the harness's noise
inline constexpr std::string_view kTaflaAgain = "tafla A/A";

/// The values that the codecs write and read, which outlive the benchmarks
struct Inputs {
    Record record = benchmarks::record();
    std::array<Document, 2> documents = {document(kDocumentRows[0]), document(kDocumentRows[1])};
    List list = benchmarks::list(kListItems);
    Sample sample = kSample;
};

struct Benchmark {
    std::string name;
    std::vector<Variant> variants;
};

template <class Function>
void for_each_codec(Function function) {
    std::apply([&](const auto&... codec) { (function(codec), ...); }, Codecs{});
}

/// Checks that a codec reads back exactly what it wrote
template <class Shape, class Codec>
void check_round_trip(const Codec& codec, const typename Shape::Value& input) {
    const auto bytes = codec.serialize(Shape{}, input);
    if (!same(value_of(codec.deserialize(Shape{}, bytes_of(bytes))), input)) {
        throw std::logic_error(std::string(Codec::name) + " didn't read back its " + std::string(Shape::name));
    }
}

/// Adds a codec's variants, named `name`, to a workload's serialize and
/// deserialize benchmarks, after checking that it reads back what it wrote
template <class Shape, class Codec>
void add_variants(std::array<Benchmark, 2>& benchmarks, const Codec& codec, std::string_view name,
                  const typename Shape::Value& input) {
    check_round_trip<Shape>(codec, input);

    const auto* value = &input;
    benchmarks[0].variants.push_back(
        variant(std::string(name), [codec, value] { return codec.serialize(Shape{}, *opaque(value)); }));

    using Bytes = decltype(codec.serialize(Shape{}, input));
    const auto bytes = std::make_shared<const Bytes>(codec.serialize(Shape{}, input));
    benchmarks[1].variants.push_back(variant(
        std::string(name), [codec, bytes] { return codec.deserialize(Shape{}, bytes_of(*opaque(bytes.get()))); }));
}

/// The serialize and deserialize benchmarks of a workload, with a variant for
/// each codec that encodes its shape, and tafla's A/A pair
template <class Shape>
std::array<Benchmark, 2> workload(const std::string& name, const typename Shape::Value& input) {
    std::array<Benchmark, 2> benchmarks{Benchmark{name + ": serialize", {}}, Benchmark{name + ": deserialize", {}}};

    for_each_codec([&](const auto& codec) {
        using Codec = std::decay_t<decltype(codec)>;
        if constexpr (Encodes<Codec, Shape>) {
            add_variants<Shape>(benchmarks, codec, Codec::name, input);
        }
    });
    add_variants<Shape>(benchmarks, Tafla(), kTaflaAgain, input);

    return benchmarks;
}

/// Sums a document's column `a` through each codec's per-element accessor
inline Benchmark sum_column_benchmark(const std::string& name, const Document& input) {
    Benchmark sum{name + ": sum a column per element", {}};
    const auto expected = sum_column(input.a.span());

    for_each_codec([&](const auto& codec) {
        using Codec = std::decay_t<decltype(codec)>;
        if constexpr (SumsColumns<Codec>) {
            using Bytes = decltype(codec.serialize(DocumentShape{}, input));
            const auto bytes = std::make_shared<const Bytes>(codec.serialize(DocumentShape{}, input));
            if (!same_bits(codec.sum_a(bytes_of(*bytes)), expected)) {
                throw std::logic_error(std::string(Codec::name) + " summed the column otherwise");
            }

            sum.variants.push_back(variant(std::string(Codec::name),
                                           [codec, bytes] { return codec.sum_a(bytes_of(*opaque(bytes.get()))); }));
        }
    });

    const auto bytes = std::make_shared<const Bytes>(Tafla().serialize(DocumentShape{}, input));
    sum.variants.push_back(
        variant(std::string(kTaflaAgain), [bytes] { return Tafla().sum_a(bytes_of(*opaque(bytes.get()))); }));
    return sum;
}

/// The groups that read tafla's items: the last item and every item of the
/// vector of nested tables and of the array of structs, by hand and through
/// the generated getters twice, an A/A pair that shows the harness's noise
inline std::array<Benchmark, 4> item_access(const List& list) {
    const auto tables = std::make_shared<const Bytes>(Tafla().serialize(ListShape{}, list));
    const auto structs = std::make_shared<const Bytes>(Tafla().serialize(StructsShape{}, list));

    std::array<Benchmark, 4> groups{
        Benchmark{"vector of 100 nested tables: read the last item", {}},
        Benchmark{"vector of 100 nested tables: sum every item", {}},
        Benchmark{"array of 100 structs: read the last item", {}},
        Benchmark{"array of 100 structs: sum every item", {}},
    };

    const auto add = [&](const auto& access, const std::string& name) {
        check_items(access, list, *tables, *structs);
        groups[0].variants.push_back(
            variant(name, [access, tables] { return access.last_table(opaque(tables.get())->span()); }));
        groups[1].variants.push_back(
            variant(name, [access, tables] { return access.every_table(opaque(tables.get())->span()); }));
        groups[2].variants.push_back(
            variant(name, [access, structs] { return access.last_struct(opaque(structs.get())->span()); }));
        groups[3].variants.push_back(
            variant(name, [access, structs] { return access.every_struct(opaque(structs.get())->span()); }));
    };
    add(TaflaItems(), std::string(TaflaItems::name));
    add(TaflaGenItems(), std::string(TaflaGenItems::name));
    add(TaflaGenItems(), "tafla-gen A/A");

    return groups;
}

/// Every benchmark, in the order that results list them
inline std::vector<Benchmark> benchmarks(const Inputs& inputs) {
    std::vector<Benchmark> all;
    const auto append = [&all](auto group) {
        for (auto& benchmark : group) {
            all.push_back(std::move(benchmark));
        }
    };

    append(workload<RecordShape>("record", inputs.record));
    for (std::size_t index = 0; index < inputs.documents.size(); ++index) {
        const auto name = index == 0 ? std::string("document of 100 rows") : std::string("document of 100,000 rows");
        append(workload<DocumentShape>(name, inputs.documents[index]));
        all.push_back(sum_column_benchmark(name, inputs.documents[index]));
    }
    append(workload<ListShape>("vector of 100 nested tables", inputs.list));
    append(workload<StructsShape>("array of 100 structs", inputs.list));
    append(workload<SampleShape>("fixed-size record", inputs.sample));
    append(item_access(inputs.list));
    return all;
}

}  // namespace tafla::benchmarks

#endif
