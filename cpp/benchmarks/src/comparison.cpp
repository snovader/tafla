// Times every benchmark of the comparison, its codecs interleaved, and prints
// each one's fastest round per call and its ratio to tafla's, and the median
// round. See README.md for the method and options.

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <map>
#include <string>
#include <vector>

#if defined(__unix__) || defined(__APPLE__)
#include <stdlib.h>
#endif

#include "suite.hpp"

namespace {

using namespace tafla::benchmarks;

void print_load(const char* when) {
#if defined(__unix__) || defined(__APPLE__)
    double load[3] = {};
    if (getloadavg(load, 3) == 3) {
        std::printf("Load average %s: %.2f %.2f %.2f\n", when, load[0], load[1], load[2]);
    }
#endif
}

/// Microseconds, with three significant digits or more
std::string microseconds(double nanoseconds) {
    const double value = nanoseconds / 1000.0;
    char text[32];
    if (value >= 100) {
        std::snprintf(text, sizeof text, "%.1f", value);
    } else if (value >= 1) {
        std::snprintf(text, sizeof text, "%.3f", value);
    } else {
        const int digits = std::max(3, 2 - static_cast<int>(std::floor(std::log10(value))));
        std::snprintf(text, sizeof text, "%.*f", digits, value);
    }
    return text;
}

std::string ratio(double value, double reference) {
    const double ratio = value / reference;
    char text[32];
    if (ratio >= 100) {
        std::snprintf(text, sizeof text, "%.0f×", ratio);
    } else if (ratio >= 10) {
        std::snprintf(text, sizeof text, "%.1f×", ratio);
    } else if (ratio >= 0.1) {
        std::snprintf(text, sizeof text, "%.2f×", ratio);
    } else {
        std::snprintf(text, sizeof text, "%.2g×", ratio);
    }
    return text;
}

struct Arguments {
    std::vector<std::string> filters;
    Options options;
    bool markdown = false;
};

Arguments parse(int argc, char** argv) {
    Arguments arguments;
    for (int index = 1; index < argc; ++index) {
        const std::string argument = argv[index];
        const auto value = [&]() -> std::string {
            if (index + 1 >= argc) {
                throw std::invalid_argument(argument + " needs a value");
            }
            return argv[++index];
        };

        if (argument == "--rounds") {
            arguments.options.rounds = std::stoul(value());
        } else if (argument == "--warmup-rounds") {
            arguments.options.warmup_rounds = std::stoul(value());
        } else if (argument == "--sample-ms") {
            arguments.options.sample_time =
                std::chrono::microseconds(static_cast<std::int64_t>(std::stod(value()) * 1000));
        } else if (argument == "--quick") {
            arguments.options = {3, 1, std::chrono::milliseconds(1)};
        } else if (argument == "--markdown") {
            arguments.markdown = true;
        } else {
            arguments.filters.push_back(argument);
        }
    }
    return arguments;
}

bool selected(const Arguments& arguments, const std::string& name) {
    if (arguments.filters.empty()) {
        return true;
    }
    for (const auto& filter : arguments.filters) {
        if (name.find(filter) != std::string::npos) {
            return true;
        }
    }
    return false;
}

}  // namespace

int main(int argc, char** argv) {
    const auto arguments = parse(argc, argv);
    print_load("before");

    const Inputs inputs;
    const auto all = benchmarks(inputs);

    std::vector<std::pair<std::string, std::vector<Measurement>>> results;
    for (const auto& benchmark : all) {
        if (!selected(arguments, benchmark.name)) {
            continue;
        }

        const auto measurements = measure(benchmark.variants, arguments.options);
        std::cout << benchmark.name << "\n";
        for (const auto& measurement : measurements) {
            std::cout << "  " << measurement.name << ": " << microseconds(measurement.fastest) << " µs ("
                      << ratio(measurement.fastest, measurements.front().fastest) << "), median "
                      << microseconds(measurement.median) << " µs (+"
                      << std::lround((measurement.median / measurement.fastest - 1) * 100) << "%)\n";
        }
        std::cout.flush();
        results.emplace_back(benchmark.name, measurements);
    }

    print_load("after");

    if (arguments.markdown) {
        std::cout << "\n";
        for (const auto& [name, measurements] : results) {
            std::cout << "| " << name;
            for (const auto& measurement : measurements) {
                std::cout << " | " << measurement.name << " " << microseconds(measurement.fastest);
                if (&measurement != &measurements.front()) {
                    std::cout << " (" << ratio(measurement.fastest, measurements.front().fastest) << ")";
                }
            }
            std::cout << " |\n";
        }
    }
    return 0;
}
