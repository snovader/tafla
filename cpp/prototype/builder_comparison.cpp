// Times the comparison's builder rows through `tafla::TableBuilder` and the
// prototype's builders, interleaved, and prints each variant's fastest round
// per call, its ratio to `TableBuilder`'s, and its median round. Arguments
// select rows by a part of their name; `--rounds`, `--warmup-rounds` and
// `--sample-ms` set the harness's numbers, as for `tafla_comparison`.

#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <stdexcept>
#include <string>
#include <vector>

#include "builder_suite.hpp"

namespace {

void print_load(const char* when) {
    double load[3] = {};
    if (getloadavg(load, 3) == 3) {
        std::printf("Load average %s: %.2f %.2f %.2f\n", when, load[0], load[1], load[2]);
    }
}

}  // namespace

int main(int argc, char** argv) {
    using namespace tafla::benchmarks;

    Options options;
    std::vector<std::string> filters;
    for (int index = 1; index < argc; ++index) {
        const std::string argument = argv[index];
        const auto value = [&] {
            if (index + 1 >= argc) {
                throw std::invalid_argument(argument + " needs a value");
            }
            return std::string(argv[++index]);
        };

        if (argument == "--rounds") {
            options.rounds = std::stoul(value());
        } else if (argument == "--warmup-rounds") {
            options.warmup_rounds = std::stoul(value());
        } else if (argument == "--sample-ms") {
            options.sample_time = std::chrono::microseconds(static_cast<std::int64_t>(std::stod(value()) * 1000));
        } else if (argument == "--quick") {
            options = {3, 1, std::chrono::milliseconds(1)};
        } else {
            filters.push_back(argument);
        }
    }

    print_load("before");
    const tafla::prototype::Inputs inputs;
    for (const auto& benchmark : tafla::prototype::benchmarks(inputs)) {
        bool selected = filters.empty();
        for (const auto& filter : filters) {
            selected = selected || benchmark.name.find(filter) != std::string::npos;
        }
        if (!selected) {
            continue;
        }

        const auto measurements = measure(benchmark.variants, options);
        std::printf("%s\n", benchmark.name.c_str());
        for (const auto& measurement : measurements) {
            std::printf("  %-18s %10.4f µs (%.3f×), median %10.4f µs (+%ld%%)\n", measurement.name.c_str(),
                        measurement.fastest / 1000, measurement.fastest / measurements.front().fastest,
                        measurement.median / 1000, std::lround((measurement.median / measurement.fastest - 1) * 100));
        }
        std::fflush(stdout);
    }
    print_load("after");
    return 0;
}
