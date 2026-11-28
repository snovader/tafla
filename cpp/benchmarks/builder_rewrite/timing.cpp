// Times every builder's variant of each row, interleaved: each round runs
// every variant once, starting from the next variant each round and in
// reverse order every other round, after warm-up rounds. Prints a line for
// each variant: the row, the variant, its fastest round's and its median
// round's time per call in nanoseconds, separated by tabs. Arguments select
// rows by a part of their name; `--rounds` (20), `--warm-up` (5) and
// `--sample-ms` (5) set the rounds.

#include <algorithm>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <stdexcept>
#include <string>
#include <vector>

#include "catalog.hpp"

namespace {

using tafla::benchmarks::Variant;

struct Options {
    std::size_t rounds = 20;
    std::size_t warm_up = 5;
    std::chrono::nanoseconds sample = std::chrono::milliseconds(5);
};

/// Times the variants, rotating the order of each round
void time_row(const builder_rewrite::Row& row, const Options& options) {
    const auto& variants = row.variants;
    const auto count = variants.size();

    std::vector<std::size_t> repetitions;
    for (const auto& variant : variants) {
        repetitions.push_back(tafla::benchmarks::detail::calibrate(variant, options.sample));
    }

    const auto run_rounds = [&](std::size_t rounds) {
        std::vector<std::vector<double>> times(count);
        for (std::size_t round = 0; round < rounds; ++round) {
            for (std::size_t position = 0; position < count; ++position) {
                const auto step = round % 2 == 0 ? position : count - 1 - position;
                const auto index = (round / 2 + step) % count;
                times[index].push_back(tafla::benchmarks::detail::time_round(variants[index], repetitions[index]));
            }
        }
        return times;
    };

    if (options.warm_up > 0) {
        const auto warm = run_rounds(options.warm_up);
        for (std::size_t index = 0; index < count; ++index) {
            const auto fastest = *std::min_element(warm[index].begin(), warm[index].end());
            repetitions[index] = tafla::benchmarks::detail::repetitions_of(fastest, options.sample);
        }
    }

    auto times = run_rounds(options.rounds);
    for (std::size_t index = 0; index < count; ++index) {
        auto& rounds = times[index];
        std::sort(rounds.begin(), rounds.end());
        std::printf("%s\t%s\t%.3f\t%.3f\n", row.name.c_str(), variants[index].name.c_str(), rounds.front(),
                    rounds[rounds.size() / 2]);
    }
    std::fflush(stdout);
}

}  // namespace

int main(int argc, char** argv) {
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
        } else if (argument == "--warm-up") {
            options.warm_up = std::stoul(value());
        } else if (argument == "--sample-ms") {
            options.sample = std::chrono::microseconds(static_cast<std::int64_t>(std::stod(value()) * 1000));
        } else {
            filters.push_back(argument);
        }
    }

    double load[3] = {};
    if (getloadavg(load, 3) == 3) {
        std::printf("# load %.2f %.2f %.2f\n", load[0], load[1], load[2]);
        std::fflush(stdout);
    }

    const builder_rewrite::Inputs inputs;
    builder_rewrite::for_each_row(inputs, filters, [&](const builder_rewrite::Row& row) { time_row(row, options); });
    return 0;
}
