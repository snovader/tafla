"""Prints the memory that each builder's call allocates on each of
``tafla_prototype.bench``'s rows: the most bytes that ``tracemalloc`` traced
at once, as the comparison's ``tafla_benchmarks.allocation`` measures them,
and, in a native module built with the feature ``count-allocations``, the
most bytes and the allocations of the module's Rust heap, which tracemalloc
doesn't trace, and which holds the native prototype's ``Vec``. Run it with
``python -m tafla_prototype.allocation``."""

from typing import Any

import tafla
from tafla_benchmarks.allocation import peak_bytes
from tafla_benchmarks.suite import Call
from tafla_prototype.bench import ROWS, prepare, variants

_WARMUP_CALLS = 20
_COUNTED_CALLS = 5

_reset_rust_allocations: Any = None
_rust_allocations: Any = None
if tafla.IMPLEMENTATION == "native":
    from tafla import _native

    _reset_rust_allocations = getattr(_native, "_reset_rust_allocations", None)
    _rust_allocations = getattr(_native, "_rust_allocations", None)


def main() -> None:
    print(
        f"Peak bytes per call that tracemalloc traced, with tafla's "
        f"{tafla.IMPLEMENTATION} implementation"
        + (
            ", and the Rust heap's peak bytes and allocations"
            if _rust_allocations is not None
            else ""
        )
    )
    chosen = [variant for variant in variants() if not variant.name.endswith("A/A")]
    for row in ROWS:
        print(f"\n{row.name}")
        for variant in chosen:
            call = prepare(variant, row)
            traced = peak_bytes(call)
            line = f"  {variant.name:<32} {traced:>12,} B"
            if _rust_allocations is not None:
                rust_peak, allocations = _rust_heap(call)
                line += f"  Rust {rust_peak:>12,} B in {allocations}"
            print(line)


def _rust_heap(call: Call[Any]) -> tuple[int, int]:
    """The fewest peak bytes of the Rust heap over a few calls, after warming
    up, and the allocations of that call"""
    operation = call.operation
    argument = call.argument
    for _ in range(_WARMUP_CALLS):
        operation(argument)

    fewest: tuple[int, int] | None = None
    for _ in range(_COUNTED_CALLS):
        _reset_rust_allocations()
        result = operation(argument)
        allocations, peak = _rust_allocations()
        del result
        if fewest is None or peak < fewest[0]:
            fewest = (peak, allocations)

    assert fewest is not None
    return fewest


if __name__ == "__main__":
    main()
