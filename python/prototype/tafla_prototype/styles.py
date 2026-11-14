"""Times ways of writing a field into a ``bytearray`` in pure Python: its
4-byte offset into its slot, and its 4-byte value at the end, 100 fields to a
table that each call creates. Each way is a function of its own, timed in a
loop of its own, compiled for it; the ways run interleaved, in alternating
order, for ``ROUNDS`` rounds of at least ``ROUND_SECONDS`` after a warm-up,
and the fastest round is reported, with an A/A pair of the first way. Run it
with ``python -m tafla_prototype.styles``."""

import os
import struct
import sys
from collections.abc import Callable
from struct import Struct
from time import perf_counter_ns
from types import FunctionType
from typing import Any

FIELDS = 100
ROUNDS = int(os.environ.get("STYLES_ROUNDS", "30"))
ROUND_NANOSECONDS = int(float(os.environ.get("STYLES_ROUND_SECONDS", "0.02")) * 1e9)

UINT32 = Struct("<I")
FLOAT32 = Struct("<f")
HEADER = FIELDS * 4
END = HEADER + FIELDS * 4


def struct_pack_append() -> bytearray:
    """The pure ``DirectTableBuilder``'s: the value packed and appended with
    ``+=``, the offset packed into its slot"""
    data = bytearray(HEADER)
    pack_into = UINT32.pack_into
    pack = FLOAT32.pack
    for index in range(FIELDS):
        pack_into(data, index * 4, len(data))
        data += pack(1.5)
    return data


def struct_pack_append_again() -> bytearray:
    """``struct_pack_append()`` again, as an A/A pair"""
    data = bytearray(HEADER)
    pack_into = UINT32.pack_into
    pack = FLOAT32.pack
    for index in range(FIELDS):
        pack_into(data, index * 4, len(data))
        data += pack(1.5)
    return data


def struct_pack_extend() -> bytearray:
    """``bytearray.extend()`` rather than ``+=``"""
    data = bytearray(HEADER)
    pack_into = UINT32.pack_into
    pack = FLOAT32.pack
    for index in range(FIELDS):
        pack_into(data, index * 4, len(data))
        data.extend(pack(1.5))
    return data


def struct_pack_into_room() -> bytearray:
    """``PreallocatedTableBuilder``'s: the value packed into room allocated
    ahead, at the end that it tracks"""
    data = bytearray(END)
    pack_into = UINT32.pack_into
    value_into = FLOAT32.pack_into
    end = HEADER
    for index in range(FIELDS):
        value_into(data, end, 1.5)
        pack_into(data, index * 4, end)
        end += 4
    return data


def int_to_bytes_offset() -> bytearray:
    """The offset converted with ``int.to_bytes()`` and assigned to its
    slot's slice"""
    data = bytearray(HEADER)
    pack = FLOAT32.pack
    for index in range(FIELDS):
        data[index * 4 : index * 4 + 4] = len(data).to_bytes(4, "little")
        data += pack(1.5)
    return data


def int_to_bytes_value() -> bytearray:
    """An integer value converted with ``int.to_bytes()``, which a float has
    no counterpart of"""
    data = bytearray(HEADER)
    pack_into = UINT32.pack_into
    for index in range(FIELDS):
        pack_into(data, index * 4, len(data))
        data += (1234).to_bytes(4, "little")
    return data


def struct_pack_value() -> bytearray:
    """The same integer value packed with a precompiled ``Struct``"""
    data = bytearray(HEADER)
    pack_into = UINT32.pack_into
    pack = UINT32.pack
    for index in range(FIELDS):
        pack_into(data, index * 4, len(data))
        data += pack(1234)
    return data


def module_functions() -> bytearray:
    """``struct``'s module functions with a format string, which look its
    Struct up in a cache, rather than a precompiled ``Struct``"""
    data = bytearray(HEADER)
    for index in range(FIELDS):
        struct.pack_into("<I", data, index * 4, len(data))
        data += struct.pack("<f", 1.5)
    return data


STYLES: tuple[FunctionType, ...] = (
    struct_pack_append,
    struct_pack_append_again,
    struct_pack_extend,
    struct_pack_into_room,
    int_to_bytes_offset,
    int_to_bytes_value,
    struct_pack_value,
    module_functions,
)

_LOOP = """
def timed(write, loops):
    start = perf_counter_ns()
    for _ in range(loops):
        result = write()
    return perf_counter_ns() - start, result
"""


def timing_loop() -> Callable[..., tuple[int, Any]]:
    """A timing loop of its own, so that no two ways share its code"""
    namespace: dict[str, Any] = {"perf_counter_ns": perf_counter_ns}
    exec(compile(_LOOP, "<timing loop>", "exec"), namespace)  # noqa: S102
    return namespace["timed"]


def main() -> None:
    expected = bytes(struct_pack_append())
    for style in STYLES:
        written = bytes(style())
        if style not in (int_to_bytes_value, struct_pack_value):
            assert written == expected, style.__name__

    loops: dict[str, int] = {}
    timers = {style.__name__: timing_loop() for style in STYLES}
    kept: dict[str, Any] = {}
    for style in STYLES:
        name = style.__name__
        count = 1
        while True:
            elapsed, kept[name] = timers[name](style, count)
            if elapsed >= ROUND_NANOSECONDS:
                break
            count *= 2
        loops[name] = count

    for _ in range(2):
        for style in STYLES:
            _, kept[style.__name__] = timers[style.__name__](
                style, loops[style.__name__]
            )

    fastest = {style.__name__: float("inf") for style in STYLES}
    for round in range(ROUNDS):
        for style in STYLES if round % 2 == 0 else reversed(STYLES):
            name = style.__name__
            elapsed, kept[name] = timers[name](style, loops[name])
            fastest[name] = min(fastest[name], elapsed / loops[name] / FIELDS)

    print(f"Python {sys.version.split()[0]}, {os.getloadavg()[0]:.1f} load")
    print("| Way of writing a field | ns per field |")
    print("| --- | ---: |")
    for style in STYLES:
        print(f"| {style.__name__} | {fastest[style.__name__]:.1f} |")


if __name__ == "__main__":
    main()
