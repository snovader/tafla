"""Hypothesis strategies of table descriptions in the shape of the conformance
suite's ``cases.json``, which ``tests/tables.py`` builds and reads.

They cover every layout, element type and kind of field: integers at their
limits, 64-bit ones included; floats of both widths with -0.0, NaN and the
infinities, and float32s that are exact; strings with characters outside the
Basic Multilingual Plane and U+FEFF, but no lone surrogates, which
``set_string()`` rejects; structs of one to four scalars of any type, alone and
in arrays, empty too; nested tables from builders and from bytes, with and
without an alignment; and vectors of tables of any layout.

They follow the format's rules: a table nested with ``set_table()`` has no
header, as the builder's docstring asks, and is read in place when it has its
parent's offset type; versions fit their slot; and every table fits its offset
type.
"""

import struct
import sys
from functools import cache, partial
from math import inf, nan
from typing import Any, Literal

from hypothesis import reject
from hypothesis.strategies import (
    DrawFn,
    SearchStrategy,
    booleans,
    characters,
    composite,
    fixed_dictionaries,
    floats,
    integers,
    just,
    lists,
    one_of,
    sampled_from,
    text,
    tuples,
)

from tests.tables import (
    ELEMENT_SIZES,
    LONGEST,
    Description,
    OffsetType,
    from_json,
    nested_in_place,
    offset_size_of,
    struct_alignment,
    struct_of,
    to_json_number,
)

OFFSET_TYPES: tuple[OffsetType, ...] = ("uint8", "uint16", "uint32")

# Levels of tables nested below the root
MAX_DEPTH = 2
MAX_FIELDS = 6
MAX_ELEMENTS = 8
MAX_ITEMS = 3
MAX_STRUCT_SCALARS = 4
MAX_STRUCTS = 4

# The most padding before a nested table, which aligns it to at most 8
_MAX_TABLE_PADDING = 7

_FLOAT32_MAX: float = struct.unpack("<f", b"\xff\xff\x7f\x7f")[0]


def _integers(bits: int, *, signed: bool) -> SearchStrategy[int]:
    """Integers of ``bits``, half of them at its limits, which Hypothesis
    otherwise draws once in a hundred"""
    low, high = (
        (-(2 ** (bits - 1)), 2 ** (bits - 1) - 1) if signed else (0, 2**bits - 1)
    )
    return one_of(integers(low, high), sampled_from([low, high]))


def _floats(width: Literal[32, 64]) -> SearchStrategy[float | str]:
    """Floats exact at ``width``, as JSON holds them, half of them the edge
    cases that Hypothesis otherwise draws less often: the zeros, the
    infinities, NaN, and the largest and smallest finite magnitudes. Any NaN is
    named, and written as the quiet NaN."""
    largest = _FLOAT32_MAX if width == 32 else sys.float_info.max
    smallest = 2.0**-149 if width == 32 else 2.0**-1074
    edges = [0.0, -0.0, inf, -inf, nan, largest, -largest, smallest, -smallest]

    return one_of(floats(width=width), sampled_from(edges)).map(to_json_number)


# The values of each scalar type, as JSON holds them
SCALAR_VALUES: dict[str, SearchStrategy[Any]] = {
    "int8": _integers(8, signed=True),
    "uint8": _integers(8, signed=False),
    "int16": _integers(16, signed=True),
    "uint16": _integers(16, signed=False),
    "int32": _integers(32, signed=True),
    "uint32": _integers(32, signed=False),
    "int64": _integers(64, signed=True).map(str),
    "uint64": _integers(64, signed=False).map(str),
    "float32": _floats(32),
    "float64": _floats(64),
}


def scalar_values(kind: str) -> SearchStrategy[Any]:
    """The values of a scalar type as tafla takes them, rather than as JSON
    holds them: ints, and floats with -0.0, NaN and the infinities, such as
    the fields of generated code take"""
    return SCALAR_VALUES[kind].map(partial(from_json, kind))


# Characters drawn more often than the default alphabet would: U+FEFF, which is
# part of a string rather than a byte order mark, and ones outside the Basic
# Multilingual Plane, which take 4 bytes of UTF-8
_RARE_CHARACTERS = ["\ufeff", "\U00010000", "\U0001f9ea", "\U0010ffff"]

_CHARACTERS = one_of(characters(codec="utf-8"), sampled_from(_RARE_CHARACTERS))

# Strings, some of them after a leading U+FEFF
STRINGS: SearchStrategy[str] = one_of(
    text(_CHARACTERS, max_size=MAX_ELEMENTS),
    text(_CHARACTERS, max_size=MAX_ELEMENTS).map(lambda string: f"\ufeff{string}"),
)

SCALAR_FIELDS: SearchStrategy[Description] = one_of(
    [
        fixed_dictionaries({"type": just(kind), "value": values})
        for kind, values in SCALAR_VALUES.items()
    ]
)

ARRAY_FIELDS: SearchStrategy[Description] = one_of(
    [
        fixed_dictionaries(
            {"type": just(f"{kind}[]"), "value": lists(values, max_size=MAX_ELEMENTS)}
        )
        for kind, values in SCALAR_VALUES.items()
    ]
)

STRING_FIELDS: SearchStrategy[Description] = fixed_dictionaries(
    {"type": just("string"), "value": STRINGS}
)

# A struct's scalars are described as scalar fields are
STRUCT_FIELDS: SearchStrategy[Description] = lists(
    SCALAR_FIELDS, min_size=1, max_size=MAX_STRUCT_SCALARS
).map(lambda scalars: {"type": "struct", "fields": scalars})

# The scalar types of a struct
STRUCT_TYPES: SearchStrategy[list[str]] = lists(
    sampled_from(list(SCALAR_VALUES)), min_size=1, max_size=MAX_STRUCT_SCALARS
)


def _struct_arrays(types: list[str]) -> SearchStrategy[Description]:
    """Arrays of structs of scalars of these types, empty too"""
    struct_values = tuples(*(SCALAR_VALUES[kind] for kind in types)).map(list)
    return fixed_dictionaries(
        {
            "type": just("struct[]"),
            "fields": just(types),
            "items": lists(struct_values, max_size=MAX_STRUCTS),
        }
    )


STRUCT_ARRAY_FIELDS: SearchStrategy[Description] = STRUCT_TYPES.flatmap(_struct_arrays)


LAYOUTS: SearchStrategy[Description] = fixed_dictionaries(
    {
        "sized": booleans(),
        "versioned": booleans(),
        "offsetType": sampled_from(OFFSET_TYPES),
    }
)


@composite
def tables(
    draw: DrawFn,
    depth: int = MAX_DEPTH,
    *,
    header: bool = True,
    offset_types: tuple[OffsetType, ...] = OFFSET_TYPES,
    parent_offset_type: OffsetType | None = None,
) -> Description:
    """Descriptions of tables with up to ``depth`` levels of tables nested
    below them. ``header=False`` leaves out the size and version slots.

    The offset type is drawn first, from ``offset_types`` and more often the
    parent's, so that the table's own nested tables can share it and be read in
    place. It's then widened to the next wider of ``offset_types`` if the
    fields don't fit, and the example is rejected if none fits.
    """
    sized = header and draw(booleans())
    versioned = header and draw(booleans())

    if parent_offset_type is None:
        drawn = draw(sampled_from(offset_types))
    else:
        drawn = draw(sampled_from((parent_offset_type, *offset_types)))

    fields = draw(_field_lists(depth, drawn))

    drawn_size = offset_size_of(drawn)
    wider = [each for each in offset_types if offset_size_of(each) >= drawn_size]
    layout = _fitting_layout(
        sized, versioned, sorted(wider, key=offset_size_of), fields
    )
    if layout is None:
        reject()

    table: Description = {"layout": layout}
    if versioned:
        table["version"] = draw(integers(0, LONGEST[layout["offsetType"]]))

    table["fields"] = fields
    return table


def _fitting_layout(
    sized: bool,
    versioned: bool,
    offset_types: list[OffsetType],
    fields: list[Description],
) -> Description | None:
    """Returns the layout of the first of ``offset_types`` whose longest table
    of ``fields`` fits, or ``None`` if none fits"""
    for offset_type in offset_types:
        layout = {"sized": sized, "versioned": versioned, "offsetType": offset_type}
        if longest_length({"layout": layout, "fields": fields}) <= LONGEST[offset_type]:
            return layout

    return None


# The strategies of nested tables are cached, since building them on every draw
# took most of the time that generating a table took


@cache
def _field_lists(
    depth: int, offset_type: OffsetType
) -> SearchStrategy[list[Description]]:
    kinds = [
        SCALAR_FIELDS,
        ARRAY_FIELDS,
        STRING_FIELDS,
        STRUCT_FIELDS,
        STRUCT_ARRAY_FIELDS,
    ]
    if depth > 0:
        kinds += [
            _table_fields(depth - 1, offset_type),
            _vector_fields(depth - 1, offset_type),
        ]

    # A kind of field is drawn first, since one_of() would flatten the scalars'
    # and arrays' one_of() into a branch per type, and draw a string, struct,
    # table or vector one time in 25
    fields = sampled_from(kinds).flatmap(lambda kind: kind)

    return lists(fields, max_size=MAX_FIELDS)


def _table_fields(depth: int, offset_type: OffsetType) -> SearchStrategy[Description]:
    """Tables nested with ``set_table()``, without a header"""
    return fixed_dictionaries(
        {
            "type": just("table"),
            "from": sampled_from(["builder", "bytes"]),
            "table": tables(depth, header=False, parent_offset_type=offset_type),
        },
        optional={"alignment": sampled_from([1, 2, 4, 8])},
    )


def _vector_fields(depth: int, offset_type: OffsetType) -> SearchStrategy[Description]:
    """Vectors of tables of any layout"""
    item = fixed_dictionaries(
        {
            "from": sampled_from(["builder", "bytes"]),
            "table": tables(depth, parent_offset_type=offset_type),
        }
    )
    return fixed_dictionaries(
        {"type": just("vector"), "items": lists(item, max_size=MAX_ITEMS)}
    )


def longest_length(table: Description) -> int:
    """The most bytes that a table of the description can take, from the
    format's rules, with the most padding that each array and nested table can
    need. It keeps generated tables within their offset type without building
    them."""
    layout = table["layout"]
    offset_size = offset_size_of(layout["offsetType"])
    fields = table["fields"]

    slots = int(layout["sized"]) + int(layout["versioned"]) + len(fields)
    values = sum(_longest_field_length(field, offset_size) for field in fields)

    return slots * offset_size + values


def _longest_field_length(field: Description, offset_size: int) -> int:
    kind = field["type"]

    if kind == "table":
        return _MAX_TABLE_PADDING + longest_length(field["table"])

    if kind == "vector":
        items = field["items"]
        lengths = sum(longest_length(item["table"]) for item in items)
        return offset_size * (1 + len(items)) + lengths

    if kind == "string":
        return offset_size + len(field["value"].encode())

    if kind == "struct":
        return struct_of(scalar["type"] for scalar in field["fields"]).size

    if kind == "struct[]":
        types = field["fields"]
        padding = struct_alignment(types) - 1
        return padding + offset_size + struct_of(types).size * len(field["items"])

    if kind.endswith("[]"):
        element_size = ELEMENT_SIZES[kind.removesuffix("[]")]
        padding = element_size - 1
        return padding + offset_size + element_size * len(field["value"])

    return ELEMENT_SIZES[kind]


def with_offset_type(table: Description, offset_type: OffsetType) -> Description:
    """Returns a table's description with every table in it, nested ones
    included, of ``offset_type``, and versions that fit its slots, so that a
    table of short offsets may be too long for them"""
    layout = {**table["layout"], "offsetType": offset_type}
    shortened: Description = {**table, "layout": layout}
    if "version" in table:
        shortened["version"] = min(table["version"], LONGEST[offset_type])

    shortened["fields"] = [
        _field_with_offset_type(field, offset_type) for field in table["fields"]
    ]
    return shortened


def _field_with_offset_type(field: Description, offset_type: OffsetType) -> Description:
    if field["type"] == "table":
        return {**field, "table": with_offset_type(field["table"], offset_type)}

    if field["type"] == "vector":
        items = [
            {**item, "table": with_offset_type(item["table"], offset_type)}
            for item in field["items"]
        ]
        return {**field, "items": items}

    return field


UNSIZED_UINT8: Description = {
    "sized": False,
    "versioned": False,
    "offsetType": "uint8",
}

_PAIR: Description = {
    "layout": UNSIZED_UINT8,
    "fields": [
        {"type": "float32", "value": 1.5},
        {"type": "int16[]", "value": [1, -2]},
    ],
}

KINDS_OF_FIELDS: list[Description] = [
    *({"type": kind, "value": 1} for kind in ("int8", "int16", "int32", "int64")),
    {"type": "float32", "value": 0.5},
    {"type": "float64", "value": -0.0},
    {"type": "string", "value": ""},
    {"type": "string", "value": "tafla"},
    {
        "type": "struct",
        "fields": [{"type": "uint8", "value": 1}, {"type": "int16", "value": -1}],
    },
    {"type": "int32[]", "value": [1, 2, 3]},
    {"type": "float64[]", "value": [0.5, 1.5]},
    {"type": "struct[]", "fields": ["int16", "uint32"], "items": [[1, 2], [3, 4]]},
    # At the alignment of 4 that set_table() pads bytes to by default, which
    # a writer nesting the table in place pads it to as well
    {"type": "table", "from": "bytes", "alignment": 4, "table": _PAIR},
    {"type": "table", "from": "builder", "table": _PAIR},
    {"type": "vector", "items": []},
    {"type": "vector", "items": [{"from": "builder", "table": _PAIR}] * 2},
    {"type": "vector", "items": [{"from": "bytes", "table": _PAIR}] * 2},
]
"""A field of each kind, of a few bytes, in a table of uint8 offsets, with
nested tables and vectors from builders and from bytes"""


@composite
def long_vectors(draw: DrawFn) -> Description:
    """Tables of uint8 offsets whose last field is a vector of many copies of
    one table, which a builder writes in place and which often makes its
    parent too long for its offsets, after fields that start it at any
    offset"""
    item = draw(tables(1, header=False, offset_types=("uint8",)))
    before = draw(tables(0, header=False, offset_types=("uint8",)))
    count = draw(integers(0, 40))
    vector = {"type": "vector", "items": [{"from": "builder", "table": item}] * count}

    return {"layout": UNSIZED_UINT8, "fields": [*before["fields"], vector]}


@composite
def long_tables(draw: DrawFn) -> Description:
    """Tables of uint8 offsets whose last field is a table of the same fields
    several times over, which a builder writes in place and which often makes
    its parent too long for its offsets, though not itself"""
    fields = draw(tables(1, header=False, offset_types=("uint8",)))
    before = draw(tables(0, header=False, offset_types=("uint8",)))
    nested = {
        "layout": fields["layout"],
        "fields": fields["fields"] * draw(integers(1, 8)),
    }
    table = {"type": "table", "from": "builder", "table": nested}

    return {"layout": UNSIZED_UINT8, "fields": [*before["fields"], table]}


@composite
def tables_near_the_limit(draw: DrawFn) -> Description:
    """Tables of uint8 offsets that start with an array of 180 to 250 bytes,
    so that each kind of field after it may be the one that makes the table
    too long, with its nested tables written in place or not"""
    table = draw(tables(1, offset_types=("uint8",)))
    array = {"type": "uint8[]", "value": [0] * draw(integers(180, 250))}
    table = {**table, "fields": [array, *table["fields"]]}

    return nested_in_place(table) if draw(booleans()) else table


def any_tables() -> SearchStrategy[Description]:
    """Tables of any offset type, and the same tables of short offsets, whose
    limits their fields reach, tables whose nested tables have no header,
    which builders write in place more often, long vectors and tables written
    in place, and fields that reach the limit of uint8 offsets. Tables of
    short offsets may be too long for them, unlike those of ``tables()``."""
    return one_of(
        tables(),
        tables().map(lambda table: with_offset_type(table, "uint8")),
        tables().map(lambda table: with_offset_type(table, "uint16")),
        tables().map(nested_in_place),
        tables().map(lambda table: with_offset_type(nested_in_place(table), "uint8")),
        long_vectors(),
        long_tables(),
        tables_near_the_limit(),
    )
