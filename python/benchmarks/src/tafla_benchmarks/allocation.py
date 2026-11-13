"""Prints how much memory each benchmark's call allocates, as tracemalloc
traces it: the most bytes that the call held at once, its result included, above
what was allocated before it. Run it with ``python -m tafla_benchmarks.allocation``.

tracemalloc traces every block that Python's allocators hand out, objects and
buffers alike, and slows each allocation down, so this runs apart from the
timings. It traces what is held, not every allocation: a call that frees a
buffer before allocating the next counts only the larger.
"""

import argparse
import tracemalloc
from typing import Any

from tafla_benchmarks.suite import Call, selected_groups

_WARMUP_CALLS = 20
"""Calls before tracing, so that CPython has specialized the codec's code and
filled its caches"""

_TRACED_CALLS = 5


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--filter",
        default="",
        metavar="REGEX",
        help="measure only the groups whose name matches",
    )
    parser.add_argument(
        "--codec",
        default="",
        metavar="REGEX",
        help="measure only the codecs whose name matches",
    )
    args = parser.parse_args()

    print("Peak bytes allocated per call, as tracemalloc traces them")
    for group in selected_groups(group=args.filter, codec=args.codec):
        print(f"\n{group.name}")
        for variant in group.variants:
            peak = peak_bytes(variant.prepare())
            print(f"  {variant.codec:<18} {peak:>12,} B")


def peak_bytes(call: Call[Any]) -> int:
    """Returns the fewest bytes above the baseline that a call of the operation
    held at its peak, over a few calls after warming up, less what reading the
    traced memory allocates"""
    return _traced_peak(call) - _traced_peak(Call(_nothing, None))


def _traced_peak(call: Call[Any]) -> int:
    operation = call.operation
    argument = call.argument
    for _ in range(_WARMUP_CALLS):
        operation(argument)

    fewest = None
    tracemalloc.start()
    try:
        for _ in range(_TRACED_CALLS):
            tracemalloc.reset_peak()
            before, _ = tracemalloc.get_traced_memory()
            result = operation(argument)
            _, peak = tracemalloc.get_traced_memory()
            del result

            fewest = peak - before if fewest is None else min(fewest, peak - before)
    finally:
        tracemalloc.stop()

    assert fewest is not None
    return fewest


def _nothing(argument: None) -> None:
    return argument


if __name__ == "__main__":
    main()
