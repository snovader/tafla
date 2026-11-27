// Times every benchmark of the suite of lists of strings, its variants
// interleaved, and prints each one's fastest round per call and its ratio to
// tafla-gen's vector of strings, and the median round, and then each list's
// encoded size. It takes the comparison's arguments. See README.md for the
// suite.

#include <cstdio>
#include <vector>

#include "lists.hpp"
#include "runner.hpp"

int main(int argc, char** argv) {
    using namespace tafla::benchmarks;

    const auto arguments = parse(argc, argv);
    print_load("before");

    const auto shapes = lists_of_strings();
    run(arguments, list_benchmarks(shapes));

    std::printf("\nEncoded size (bytes)\n  %-30s%20s%20s\n", "", TaflaGenStrings::name.data(),
                TaflaGenNames::name.data());
    for (const auto& shape : shapes) {
        std::printf("  %-30s%20zu%20zu\n", shape.name.c_str(), TaflaGenStrings::serialize(shape.views).size(),
                    TaflaGenNames::serialize(shape.views).size());
    }
    return 0;
}
