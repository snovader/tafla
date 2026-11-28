"""The lists of strings that the lists suite writes and reads: for each shape,
10, 100 and 10,000 distinct strings of the same length and mix of UTF-8
widths, 16 and 256 characters of ASCII and 8 of 3 bytes.

They're three of the kinds of the TypeScript suite's
``strings/lists/shapes.ts``, made from its seeded generator with its seeds,
so that the languages' lists are the same strings, which
``tests/test_lists.py`` checks against values that it printed.
"""

from collections.abc import Callable, Iterator
from dataclasses import dataclass
from functools import cache
from math import floor

from tafla_benchmarks.workloads import xorshift

_ASCII = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"

# The first of the CJK ideographs, of three bytes in UTF-8
_THREE_BYTES = 0x4E00

COUNTS = (10, 100, 10_000)

# The TypeScript suite's kinds of strings, whose indices into them seed its
# shapes, of which these are three
_TYPESCRIPT_KINDS = 8


def _ascii(random: Iterator[float]) -> str:
    return _ASCII[floor(next(random) * len(_ASCII))]


def _three_bytes(random: Iterator[float]) -> str:
    return chr(_THREE_BYTES + floor(next(random) * 0x5000))


@dataclass(frozen=True, slots=True)
class _Kind:
    """A kind of string: ``length`` characters, each of which ``pick`` picks
    from the generator. ``index`` is the kind's in the TypeScript suite."""

    name: str
    length: int
    index: int
    pick: Callable[[Iterator[float]], str]


_KINDS = (
    _Kind("16 chars of ASCII", 16, 1, _ascii),
    _Kind("256 chars of ASCII", 256, 2, _ascii),
    _Kind("8 chars of 3 bytes", 8, 6, _three_bytes),
)


@dataclass(frozen=True, slots=True)
class ListShape:
    """``count`` strings of a kind, from the generator seeded with ``seed``"""

    name: str
    count: int
    length: int
    seed: int
    pick: Callable[[Iterator[float]], str]

    def strings(self) -> tuple[str, ...]:
        """The shape's strings, made once and then the same tuple"""
        return _strings(self)


LIST_SHAPES: tuple[ListShape, ...] = tuple(
    ListShape(
        f"{count:,} × {kind.name}",
        count,
        kind.length,
        # A seed of its own for each shape, as the TypeScript suite's
        1 + kind.index + _TYPESCRIPT_KINDS * count_index,
        kind.pick,
    )
    for count_index, count in enumerate(COUNTS)
    for kind in _KINDS
)


@cache
def _strings(shape: ListShape) -> tuple[str, ...]:
    """``shape.count`` distinct strings, made in order, each of
    ``shape.length`` characters picked one after another"""
    random = xorshift(shape.seed)
    pick = shape.pick
    strings = tuple(
        "".join([pick(random) for _ in range(shape.length)]) for _ in range(shape.count)
    )

    if len(set(strings)) != shape.count:
        raise ValueError(f"The strings of {shape.name} aren't distinct")

    return strings
