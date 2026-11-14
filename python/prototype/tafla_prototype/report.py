"""Prints the fastest value of each builder on each row of
``tafla_prototype.bench``'s results, in µs, with its ratio to
``TableBuilder``'s with the same implementation, and the machine and load
that they were measured with, as Markdown tables. Run it with
``python -m tafla_prototype.report results.json [more.json ...]``, one file
per implementation."""

import argparse
from statistics import fmean, median

import pyperf

from tafla_prototype.bench import ROWS


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", nargs="+", help="pyperf's JSON results")
    args = parser.parse_args()

    for path in args.results:
        suite = pyperf.BenchmarkSuite.load(path)
        _print_environment(path, suite)
        print()
        _print_table(suite)
        print()


def _print_environment(path: str, suite: pyperf.BenchmarkSuite) -> None:
    metadata = suite.get_metadata()
    runs = [run for benchmark in suite.get_benchmarks() for run in benchmark.get_runs()]
    loads = [run.get_metadata()["load_avg_1min"] for run in runs]
    start, end = suite.get_dates()
    spreads = [
        fmean(benchmark.get_values()) / min(benchmark.get_values()) - 1
        for benchmark in suite.get_benchmarks()
    ]

    print(
        f"{path}: tafla's {metadata.get('tafla_implementation')} implementation, "
        f"Python {metadata.get('python_version')} on {metadata.get('platform')}"
    )
    print(
        f"{len(runs)} worker runs from {start:%Y-%m-%d %H:%M} to {end:%H:%M}; "
        f"1-minute load average {min(loads):.1f} to {max(loads):.1f}, "
        f"median {median(loads):.1f}; mean within {median(spreads):.1%} of the "
        f"fastest value in the median benchmark, {max(spreads):.1%} at most"
    )


def _print_table(suite: pyperf.BenchmarkSuite) -> None:
    fastest: dict[tuple[str, str], float] = {}
    builders: list[str] = []
    for benchmark in suite.get_benchmarks():
        metadata = benchmark.get_metadata()
        fastest[metadata["group"], metadata["codec"]] = min(benchmark.get_values())
        if metadata["codec"] not in builders:
            builders.append(metadata["codec"])

    reference = next(name for name in builders if name.startswith("TableBuilder"))
    builders.sort(key=_order)
    print("| Row | " + " | ".join(builders) + " |")
    print("| --- | " + " | ".join("---:" for _ in builders) + " |")
    for row in ROWS:
        base = fastest.get((row.name, reference))
        cells = []
        for builder in builders:
            value = fastest.get((row.name, builder))
            if value is None:
                cells.append("")
            elif base is None or builder == reference:
                cells.append(_microseconds(value))
            else:
                cells.append(f"{_microseconds(value)} ({value / base:.2f}×)")
        print(f"| {row.name.removesuffix(': serialize')} | " + " | ".join(cells) + " |")


# The builders in the order of the table's columns, each A/A pair's second
# right after its first
_ORDER = (
    "TableBuilder",
    "pure, +=",
    "pure, +=, copying",
    "pure, preallocated",
    "pure, preallocated, copying",
    "native",
    "native, copying",
    "native, copy growth",
    "tafla-gen",
    "msgpack",
    "protobuf (upb)",
)


def _order(name: str) -> tuple[int, bool]:
    first = name.removesuffix(" A/A")
    for index, ordered in enumerate(_ORDER):
        if first == ordered or (
            ordered == "TableBuilder" and first.startswith(ordered)
        ):
            return index, first != name
    return len(_ORDER), first != name


def _microseconds(seconds: float) -> str:
    microseconds = seconds * 1e6
    return f"{microseconds:.{3 if microseconds < 10 else 1}f}"


if __name__ == "__main__":
    main()
