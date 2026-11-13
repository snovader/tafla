// Times variants of a benchmark interleaved, as the TypeScript and Java
// comparisons do

#ifndef TAFLA_BENCHMARKS_HARNESS_HPP
#define TAFLA_BENCHMARKS_HARNESS_HPP

#include <algorithm>
#include <chrono>
#include <cstddef>
#include <functional>
#include <string>
#include <utility>
#include <vector>

namespace tafla::benchmarks {

/// Keeps a value alive, so that the compiler can't eliminate the work that
/// made it
template <class T>
inline void keep(const T& value) {
#if defined(__GNUC__) || defined(__clang__)
    asm volatile("" : : "r,m"(&value) : "memory");
#else
    static_cast<void>(value);
#endif
}

/// Hides where a pointer points, so that the compiler can't fold a constant
/// input into the code that reads it
template <class T>
inline T* opaque(T* pointer) {
#if defined(__GNUC__) || defined(__clang__)
    asm volatile("" : "+r"(pointer));
#endif
    return pointer;
}

/// A variant of a benchmark: a loop of its calls, compiled for its call alone,
/// that runs a number of repetitions and frees each call's result in the loop,
/// as a caller frees its bytes
struct Variant {
    std::string name;
    std::function<void(std::size_t)> loop;
};

template <class Call>
Variant variant(std::string name, Call call) {
    return {std::move(name), [call](std::size_t repetitions) {
                for (std::size_t repetition = 0; repetition < repetitions; ++repetition) {
                    const auto result = call();
                    keep(result);
                }
            }};
}

struct Options {
    std::size_t rounds = 30;
    std::size_t warmup_rounds = 5;
    /// The time that one round of one variant should take
    std::chrono::nanoseconds sample_time = std::chrono::milliseconds(10);
};

struct Measurement {
    std::string name;
    /// The fastest round's time per call, in nanoseconds
    double fastest;
    /// The median round's time per call, in nanoseconds
    double median;
};

namespace detail {

using Clock = std::chrono::steady_clock;

/// The time per call of one round of `repetitions` calls, in nanoseconds
inline double time_round(const Variant& variant, std::size_t repetitions) {
    const auto start = Clock::now();
    variant.loop(repetitions);
    const auto elapsed = std::chrono::duration<double, std::nano>(Clock::now() - start).count();
    return elapsed / static_cast<double>(repetitions);
}

/// The repetitions of a call of `nanoseconds` that take about `sample_time`
inline std::size_t repetitions_of(double nanoseconds, std::chrono::nanoseconds sample_time) {
    const auto repetitions = static_cast<double>(sample_time.count()) / std::max(nanoseconds, 0.01);
    return std::max<std::size_t>(1, static_cast<std::size_t>(repetitions));
}

/// The repetitions that make one round of the variant's first calls take
/// about `sample_time`, doubling them until a round takes a quarter of it
inline std::size_t calibrate(const Variant& variant, std::chrono::nanoseconds sample_time) {
    for (std::size_t repetitions = 1;; repetitions *= 2) {
        const auto start = Clock::now();
        variant.loop(repetitions);
        const auto elapsed = Clock::now() - start;

        if (elapsed >= sample_time / 4 || repetitions >= (std::size_t{1} << 30)) {
            const auto per_call =
                std::chrono::duration<double, std::nano>(elapsed).count() / static_cast<double>(repetitions);
            return repetitions_of(per_call, sample_time);
        }
    }
}

}  // namespace detail

/// Times the variants interleaved: every round runs each of them once, in
/// alternating order, and the fastest round counts. Load from other processes
/// then slows every variant alike, and only for some rounds. Each variant runs
/// its own loop, compiled for its call.
///
/// A round of a variant repeats its call for about `sample_time`. The
/// repetitions are counted from the variant's first calls, and again from its
/// fastest warm-up round.
inline std::vector<Measurement> measure(const std::vector<Variant>& variants, const Options& options) {
    std::vector<std::size_t> repetitions;
    for (const auto& variant : variants) {
        repetitions.push_back(detail::calibrate(variant, options.sample_time));
    }

    const auto run_rounds = [&](std::size_t rounds) {
        std::vector<std::vector<double>> times(variants.size());
        for (std::size_t round = 0; round < rounds; ++round) {
            for (std::size_t position = 0; position < variants.size(); ++position) {
                const auto index = round % 2 == 0 ? position : variants.size() - 1 - position;
                times[index].push_back(detail::time_round(variants[index], repetitions[index]));
            }
        }
        return times;
    };

    if (options.warmup_rounds > 0) {
        const auto warm = run_rounds(options.warmup_rounds);
        for (std::size_t index = 0; index < variants.size(); ++index) {
            const auto fastest = *std::min_element(warm[index].begin(), warm[index].end());
            repetitions[index] = detail::repetitions_of(fastest, options.sample_time);
        }
    }

    auto times = run_rounds(options.rounds);
    std::vector<Measurement> measurements;
    for (std::size_t index = 0; index < variants.size(); ++index) {
        auto& rounds = times[index];
        std::sort(rounds.begin(), rounds.end());
        measurements.push_back({variants[index].name, rounds.front(), rounds[rounds.size() / 2]});
    }
    return measurements;
}

}  // namespace tafla::benchmarks

#endif
