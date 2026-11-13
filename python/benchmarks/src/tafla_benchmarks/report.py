"""Prints the fastest value of each benchmark in pyperf's results, and its ratio
to tafla's, as a Markdown table, after the machine and load they were measured
with. Run it with ``python -m tafla_benchmarks.report results.json``."""

import argparse
from dataclasses import dataclass
from statistics import fmean, median

import pyperf

from tafla_benchmarks.codecs import CODEC_NAMES, STRUCT, TAFLA
from tafla_benchmarks.item_access import ACCESSES
from tafla_benchmarks.suite import groups

# The codecs, protobuf's pure-Python implementation included, which runs in
# processes of its own, and the ways of reading tafla's items that aren't codecs
_CODEC_NAMES = list(
    dict.fromkeys([*CODEC_NAMES, STRUCT, *(access.name for access in ACCESSES)])
)


@dataclass(frozen=True, slots=True)
class Timing:
    """A benchmark's values, in seconds per call"""

    fastest: float
    mean: float


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", help="pyperf's JSON results")
    args = parser.parse_args()

    suite = pyperf.BenchmarkSuite.load(args.results)
    timings = _timings(suite)

    _print_environment(suite)
    print()
    _print_table(timings)
    print()
    _print_mean_spread(timings)


def _timings(suite: pyperf.BenchmarkSuite) -> dict[tuple[str, str], Timing]:
    """Returns each benchmark's timing by its group and codec"""
    timings: dict[tuple[str, str], Timing] = {}
    for benchmark in suite.get_benchmarks():
        metadata = benchmark.get_metadata()
        values = benchmark.get_values()
        timings[metadata["group"], metadata["codec"]] = Timing(
            fastest=min(values), mean=fmean(values)
        )

    return timings


def _print_environment(suite: pyperf.BenchmarkSuite) -> None:
    metadata = suite.get_metadata()
    runs = [run for benchmark in suite.get_benchmarks() for run in benchmark.get_runs()]
    loads = [run.get_metadata()["load_avg_1min"] for run in runs]
    start, end = suite.get_dates()

    print(
        f"Python {metadata.get('python_version')}, {metadata.get('python_build')} "
        f"build, on {metadata.get('platform')} with {metadata.get('cpu_count')} CPUs"
    )
    print(
        f"{len(runs)} worker runs from {start:%Y-%m-%d %H:%M} to {end:%H:%M}; "
        f"1-minute load average {min(loads):.1f} to {max(loads):.1f}, "
        f"median {median(loads):.1f}"
    )


def _print_table(timings: dict[tuple[str, str], Timing]) -> None:
    """Prints the fastest value of each benchmark in µs, and its ratio to
    tafla's"""
    codecs = [name for name in _CODEC_NAMES if any(c == name for _, c in timings)]
    print("| Benchmark | " + " | ".join(codecs) + " |")
    print("| --- | " + " | ".join("---:" for _ in codecs) + " |")

    for group in groups():
        reference = timings.get((group.name, TAFLA.name))
        if not any((group.name, codec) in timings for codec in codecs):
            continue

        cells = [_cell(timings.get((group.name, codec)), reference) for codec in codecs]
        print(f"| {group.name} | " + " | ".join(cells) + " |")


def _cell(timing: Timing | None, reference: Timing | None) -> str:
    if timing is None:
        return ""

    value = _format_microseconds(timing.fastest)
    if reference is None or timing is reference:
        return value

    return f"{value} ({timing.fastest / reference.fastest:.2f}×)"


def _format_microseconds(seconds: float) -> str:
    microseconds = seconds * 1e6
    return f"{microseconds:.{3 if microseconds < 10 else 1}f}"


def _print_mean_spread(timings: dict[tuple[str, str], Timing]) -> None:
    """Prints how far the means lie above the fastest values, which load from
    other processes widens"""
    spreads = sorted(
        (timing.mean / timing.fastest - 1, group, codec)
        for (group, codec), timing in timings.items()
    )
    within = sum(1 for spread, _, _ in spreads if spread <= 0.05)
    widest, group, codec = spreads[-1]

    print(
        f"The mean was within 5% of the fastest value for {within} of the "
        f"{len(spreads)} benchmarks, and at most {widest:.0%} above it, for "
        f"{codec}: {group}."
    )


if __name__ == "__main__":
    main()
