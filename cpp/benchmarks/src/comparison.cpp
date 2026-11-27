// Times every benchmark of the comparison, its codecs interleaved, and prints
// each one's fastest round per call and its ratio to tafla's, and the median
// round. See README.md for the method and options.

#include "runner.hpp"
#include "suite.hpp"

int main(int argc, char** argv) {
    using namespace tafla::benchmarks;

    const auto arguments = parse(argc, argv);
    print_load("before");

    const Inputs inputs;
    run(arguments, benchmarks(inputs));
    return 0;
}
