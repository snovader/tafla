// Compares how tafla and the other libraries write and read strings, as the
// TypeScript strings suite does. Each table holds 16 distinct strings of one
// shape and nothing else, so that a call spends its time on their UTF-8, and
// the 16 empty strings time what a call takes without any. Prints each
// benchmark's fastest round per call and its ratio to tafla's, and the
// median round, and then each codec's encoded sizes. See README.md for the
// shapes, the method and the options.

#include <cstdio>
#include <string>
#include <type_traits>
#include <vector>

#include "runner.hpp"
#include "suite.hpp"

namespace {

using namespace tafla::benchmarks;

void print_sizes(const std::vector<StringsWorkload>& workloads) {
    std::printf("\nEncoded size (bytes)\n  %-28s", "");
    for_each_codec([](const auto& codec) {
        if constexpr (Encodes<std::decay_t<decltype(codec)>, StringsShape>) {
            std::printf("%12s", std::string(codec.name).c_str());
        }
    });
    std::printf("\n");

    for (const auto& [name, strings] : workloads) {
        std::printf("  %-28s", name.c_str());
        for_each_codec([&strings](const auto& codec) {
            if constexpr (Encodes<std::decay_t<decltype(codec)>, StringsShape>) {
                std::printf("%12zu", bytes_of(codec.serialize(StringsShape{}, strings)).size());
            }
        });
        std::printf("\n");
    }
}

}  // namespace

int main(int argc, char** argv) {
    const auto arguments = parse(argc, argv);
    print_load("before");

    const auto workloads = string_workloads();
    run(arguments, string_benchmarks(workloads));
    print_sizes(workloads);
    return 0;
}
