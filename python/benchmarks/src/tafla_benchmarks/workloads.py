"""Generic table shapes, sized to bracket how tafla is used: small tables of
scalars and a string, documents whose header nests a table of numeric columns,
vectors of small nested tables, arrays of structs, and fixed-size records.

The values are the TypeScript comparison's, from a port of its seeded
generator, so that the languages' numbers compare.

Every deserialization creates these classes, which every codec pays for
alike, so they're made cheap to create: dataclasses with slots that aren't
frozen, which the codecs create from positional arguments. Creating a
``SampleInput`` that way took 65 ns, against 190 ns from keywords, and about
twice that from keywords when it was frozen too.
"""

from array import array
from collections.abc import Iterator, Sequence
from dataclasses import dataclass, replace
from math import floor
from struct import Struct
from typing import Protocol


@dataclass(slots=True)
class RecordInput:
    """A small table: two float64s, an int64 and a 16-byte string"""

    x: float
    y: float
    id: int
    name: str


@dataclass(slots=True)
class DocumentInput:
    """A document: a sized, versioned table of two float64s and a string, with
    a nested table of an int64 and three columns of the same length, a uint32
    and two float32. The columns are arrays, or lists as parsed JSON would have
    them."""

    x: float
    y: float
    name: str
    base: int
    keys: Sequence[int]
    a: Sequence[float]
    b: Sequence[float]


class Column[T](Protocol):
    """A column as a library reads it back: a memoryview, a numpy array, a
    protobuf repeated field or an array"""

    def __len__(self) -> int: ...

    def __getitem__(self, index: int, /) -> T: ...

    def __iter__(self) -> Iterator[T]: ...


@dataclass(slots=True)
class DocumentOutput:
    """A document read back, with its columns as the library returns them"""

    x: float
    y: float
    name: str
    base: int
    keys: Column[int]
    a: Column[float]
    b: Column[float]


@dataclass(slots=True)
class ItemInput:
    """A table of a float32 and a nested table of two float32s, or a struct of
    the three float32s"""

    value: float
    first: float
    second: float


@dataclass(slots=True)
class ListInput:
    """A table holding a vector of item tables, or an array of item structs"""

    items: Sequence[ItemInput]


@dataclass(slots=True)
class SampleInput:
    """A fixed-size record of a uint32 and two float32s"""

    key: int
    a: float
    b: float


DOCUMENT_ROWS = (100, 100_000)
"""Rows per document: a small one and a large one"""

LIST_ITEMS = 100

RECORD = RecordInput(x=59.3293, y=18.0686, id=-631_152_000, name="tafla-benchmarks")

SAMPLE = SampleInput(key=2_743_826_400, a=21.75, b=3.25)

_FLOAT32 = Struct("<f")


def document_input(rows: int) -> DocumentInput:
    """Returns a document of ``rows`` rows, with its columns as arrays"""
    random = xorshift(rows)

    # Keys below 2^24 take 4 bytes in protobuf's varints, like the floats.
    # array("f") rounds each float64 to float32 as Math.fround() does.
    keys = array("I", bytes(4 * rows))
    a = array("f", bytes(4 * rows))
    b = array("f", bytes(4 * rows))
    for i in range(rows):
        keys[i] = floor(next(random) * 2**24)
        a[i] = (next(random) - 0.5) * 100
        b[i] = (next(random) - 0.5) * 100

    return DocumentInput(
        x=RECORD.x, y=RECORD.y, name=RECORD.name, base=RECORD.id, keys=keys, a=a, b=b
    )


def document_lists(document: DocumentInput) -> DocumentInput:
    """Returns the same document with list columns, as parsed JSON would have"""
    return replace(
        document, keys=list(document.keys), a=list(document.a), b=list(document.b)
    )


def list_input(items: int) -> ListInput:
    random = xorshift(items)

    # Float32 values, so that every format reads them back exactly
    return ListInput(
        items=[
            ItemInput(
                value=_round_to_float32(next(random) * 30),
                first=_round_to_float32(next(random) * 4 - 2),
                second=_round_to_float32(next(random) * 50),
            )
            for _ in range(items)
        ]
    )


def xorshift(seed: int) -> Iterator[float]:
    """A deterministic generator in [0, 1), so every run sees the same values:
    the TypeScript suites' generator, whose values the workloads and the
    strings of ``list_shapes.py`` take. Its state is the 32 bits that the
    TypeScript generator's shifts leave."""
    state = 0x2545F491 ^ seed

    while True:
        state ^= (state << 13) & 0xFFFF_FFFF
        state ^= state >> 17
        state ^= (state << 5) & 0xFFFF_FFFF

        yield state / 2**32


def _round_to_float32(value: float) -> float:
    """Rounds as Math.fround() does"""
    return _FLOAT32.unpack(_FLOAT32.pack(value))[0]
