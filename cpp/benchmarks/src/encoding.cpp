// Prints each codec's encoded sizes of the workloads, and what it does with a
// truncated document, as the TypeScript comparison does

#include <cstdio>
#include <iostream>
#include <string>
#include <vector>

#include "suite.hpp"

namespace {

using namespace tafla::benchmarks;

template <class Shape, class Codec>
std::string size_of(const Codec& codec, const typename Shape::Value& input) {
    if constexpr (Encodes<Codec, Shape>) {
        return std::to_string(bytes_of(codec.serialize(Shape{}, input)).size());
    } else {
        return "";
    }
}

void cell(const std::string& text, int width = 12) {
    std::printf("%*s", width, text.c_str());
}

/// Reads a truncated document, and prints what the codec did
template <class Codec>
void read_truncated(const Codec& codec, const Document& document, std::size_t length) {
    const auto bytes = codec.serialize(DocumentShape{}, document);
    const auto all = bytes_of(bytes);
    // A copy, so that no read can reach the bytes after the cut
    const std::vector<std::uint8_t> truncated(all.begin(), all.begin() + static_cast<std::ptrdiff_t>(length));

    std::string outcome;
    try {
        static_cast<void>(codec.deserialize(DocumentShape{}, truncated));
        outcome = "read a document";
    } catch (const std::exception& error) {
        outcome = std::string("threw: ") + error.what();
    }
    if (outcome.size() > 90) {
        outcome = outcome.substr(0, 90);
    }
    std::printf("  %-12s %8zu B  %s\n", std::string(Codec::name).c_str(), truncated.size(), outcome.c_str());
}

}  // namespace

int main() {
    const Inputs inputs;

    std::printf("Encoded size (bytes)\n%-14s", "");
    for (const auto* name : {"record", "doc 100", "doc 100000", "vector", "structs", "fixed"}) {
        cell(name);
    }
    std::printf("\n");

    for_each_codec([&](const auto& codec) {
        std::printf("  %-12s", std::string(codec.name).c_str());
        cell(size_of<RecordShape>(codec, inputs.record));
        cell(size_of<DocumentShape>(codec, inputs.documents[0]));
        cell(size_of<DocumentShape>(codec, inputs.documents[1]));
        cell(size_of<ListShape>(codec, inputs.list));
        cell(size_of<StructsShape>(codec, inputs.list));
        cell(size_of<SampleShape>(codec, inputs.sample));
        std::printf("\n");
    });

    std::printf("\nTruncated document of 100000 rows: deserialize()\n");
    for_each_codec([&](const auto& codec) {
        using Codec = std::decay_t<decltype(codec)>;
        if constexpr (Encodes<Codec, DocumentShape>) {
            const auto length = bytes_of(codec.serialize(DocumentShape{}, inputs.documents[1])).size();
            read_truncated(codec, inputs.documents[1], length / 2);
            read_truncated(codec, inputs.documents[1], 24);
        }
    });
    return 0;
}
