"""The code generated from ``typescript/core/test/presence.tafla``: how tests
read its messages back, describe them to build their tables by hand, and
strategies of their fields, which leave each optional field out, give it as
``None``, or give it a value."""

from collections.abc import Buffer, Iterable, Sequence
from struct import pack, unpack

from hypothesis.strategies import (
    DrawFn,
    SearchStrategy,
    booleans,
    builds,
    composite,
    lists,
    none,
    one_of,
)

from tests.descriptions import STRINGS, scalar_values
from tests.generated.descriptions import (
    boolean,
    layout,
    message,
    scalar,
    string,
    struct,
    table,
    typed_array,
    vector,
)
from tests.generated.presence.presence import (
    EntryFields,
    EntryV1Fields,
    EntryV1Vector,
    EntryVector,
    Extent,
    Point,
    PointFields,
    Record,
    RecordFields,
    RecordV1,
    RecordV1Fields,
)
from tests.tables import Description

# The defaults that the schema gives the scalars that may be missing, a
# float32's as the float32 nearest to it
FLAG_DEFAULT = True
TOTAL_DEFAULT = -1
Z_DEFAULT = -0.5
WEIGHT_DEFAULT: float = unpack("<f", pack("<f", 0.1))[0]
COUNT_DEFAULT = 7

RECORD_LAYOUT: Description = layout(sized=True, versioned=True)
"""The layout of a ``Record``, from its attributes"""

# Reading every field through its getters, with arrays as lists, None for
# each field but a scalar that the table doesn't hold, and each scalar that
# may be missing as its getter's value and whether has_<field>() finds it


def read_record(record: Record) -> dict[str, object]:
    extent = record.extent()

    return {
        **read_first_version(record),
        "origin": _read_point(record.origin()),
        "entries": _read_entries(record.entries()),
        "label": record.label(),
        "extent": None if extent is None else (extent.width, extent.height),
    }


def read_first_version(record: Record | RecordV1) -> dict[str, object]:
    """The fields of the first version, as either version's reader reads them,
    with a point's ``x`` and ``y`` and an entry's ``key`` and ``note``"""
    values = record.values()
    origin = record.origin()
    entries = record.entries()

    return {
        "id": record.id(),
        "name": record.name(),
        "values": None if values is None else values.tolist(),
        "origin": None if origin is None else {"x": origin.x(), "y": origin.y()},
        "flag": (record.flag(), record.has_flag()),
        "entries": None if entries is None else _read_first_entries(entries),
        "total": (record.total(), record.has_total()),
    }


def _read_point(point: Point | None) -> dict[str, object] | None:
    if point is None:
        return None

    return {
        "x": point.x(),
        "y": point.y(),
        "z": (point.z(), point.has_z()),
        "label": point.label(),
    }


def _read_entries(entries: EntryVector | None) -> list[dict[str, object]] | None:
    if entries is None:
        return None

    return [
        {
            "key": entry.key(),
            "note": entry.note(),
            "weight": (entry.weight(), entry.has_weight()),
        }
        for entry in entries
    ]


def _read_first_entries(
    entries: EntryVector | EntryV1Vector,
) -> list[dict[str, object]]:
    return [{"key": entry.key(), "note": entry.note()} for entry in entries]


# What the getters return for the fields that were written


def expected_record(fields: RecordFields) -> dict[str, object]:
    """A record's fields as its getters return them, ``None`` for each that
    was left out but a scalar, which reads as its default"""
    origin = fields.get("origin")
    entries = fields.get("entries")
    extent = fields.get("extent")

    return {
        **_expected_first_version(first_version_of(fields)),
        "origin": None if origin is None else _expected_point(origin),
        "entries": None if entries is None else [_expected_entry(e) for e in entries],
        "label": fields["label"],
        "extent": None if extent is None else (extent.width, extent.height),
    }


def expected_from_first_version(fields: RecordV1Fields) -> dict[str, object]:
    """The fields of the first version, as the second version's reader reads
    them: those that version 2 added are ``None``, and each scalar that a
    later revision of a nested table added reads as its default"""
    origin = fields.get("origin")
    entries = fields.get("entries")

    first = _expected_first_version(fields)
    if origin is not None:
        first["origin"] = {
            "x": origin["x"],
            "y": origin["y"],
            "z": (Z_DEFAULT, False),
            "label": None,
        }
    if entries is not None:
        first["entries"] = [
            {
                "key": entry["key"],
                "note": entry.get("note"),
                "weight": (WEIGHT_DEFAULT, False),
            }
            for entry in entries
        ]

    return {**first, "label": None, "extent": None}


def _expected_first_version(fields: RecordV1Fields) -> dict[str, object]:
    values = fields.get("values")
    origin = fields.get("origin")
    entries = fields.get("entries")

    return {
        "id": fields["id"],
        "name": fields.get("name"),
        "values": None if values is None else _elements(values),
        "origin": None if origin is None else {"x": origin["x"], "y": origin["y"]},
        "flag": _scalar(fields.get("flag"), FLAG_DEFAULT),
        "entries": None
        if entries is None
        else [{"key": entry["key"], "note": entry.get("note")} for entry in entries],
        "total": _scalar(fields.get("total"), TOTAL_DEFAULT),
    }


def _elements(values: Buffer | Iterable[float]) -> Sequence[float]:
    """The elements of an array, from any values that the generated code takes
    for it, as a list"""
    if isinstance(values, Buffer):
        return memoryview(values).tolist()

    return list(values)


def _expected_point(point: PointFields) -> dict[str, object]:
    return {
        "x": point["x"],
        "y": point["y"],
        "z": (point["z"], True),
        "label": point.get("label"),
    }


def _expected_entry(entry: EntryFields) -> dict[str, object]:
    return {
        "key": entry["key"],
        "note": entry.get("note"),
        "weight": (entry["weight"], True),
    }


def _scalar[T](value: T | None, default: T) -> tuple[T, bool]:
    """A scalar that may be missing, as its getter and ``has_<field>()`` read
    it"""
    return (default, False) if value is None else (value, True)


def first_version_of(fields: RecordFields) -> RecordV1Fields:
    """The fields of a record's first version: those of its second, without
    those that version 2 added, and without those that the second revisions
    of its nested tables added. A field left out is ``None``, which writes
    it as absent too."""
    origin = fields.get("origin")
    entries = fields.get("entries")

    return {
        "id": fields["id"],
        "name": fields.get("name"),
        "values": fields.get("values"),
        "origin": None if origin is None else {"x": origin["x"], "y": origin["y"]},
        "flag": fields.get("flag"),
        "entries": None if entries is None else [_first_entry(e) for e in entries],
        "total": fields.get("total"),
    }


def _first_entry(entry: EntryFields) -> EntryV1Fields:
    return {"key": entry["key"], "note": entry.get("note")}


# The tables that the generated code writes, in the conformance suite's
# model, with an absent field for each optional one that was left out


ABSENT: Description = {"type": "absent"}


def describe_record(fields: RecordFields) -> Description:
    values = fields.get("values")
    origin = fields.get("origin")
    flag = fields.get("flag")
    entries = fields.get("entries")
    total = fields.get("total")
    extent = fields.get("extent")
    name = fields.get("name")

    return message(
        RECORD_LAYOUT,
        [
            scalar("uint32", fields["id"]),
            ABSENT if name is None else string(name),
            ABSENT if values is None else typed_array("float64", values),
            ABSENT if origin is None else table(_describe_point(origin)),
            ABSENT if flag is None else boolean(flag),
            ABSENT if entries is None else vector(map(_describe_entry, entries)),
            ABSENT if total is None else scalar("int64", total),
            string(fields["label"]),
            ABSENT if extent is None else _describe_extent(extent),
        ],
        Record.FORMAT_VERSION,
    )


def _describe_point(point: PointFields) -> list[Description]:
    label = point.get("label")

    return [
        *(scalar("float64", point[key]) for key in ("x", "y", "z")),
        ABSENT if label is None else string(label),
    ]


def _describe_entry(entry: EntryFields) -> list[Description]:
    note = entry.get("note")

    return [
        scalar("uint16", entry["key"]),
        ABSENT if note is None else string(note),
        scalar("float32", entry["weight"]),
    ]


def _describe_extent(extent: Extent) -> Description:
    return struct([scalar("float32", extent.width), scalar("float32", extent.height)])


# Values of the fields over each type's full range, with each optional field
# left out, None, or a value


def _optional[T](values: SearchStrategy[T]) -> SearchStrategy[T | None]:
    """A value, or ``None``, which writes the field as absent, as leaving it
    out does"""
    return one_of(none(), values)


_FLOAT32S = scalar_values("float32")
_FLOAT64S = scalar_values("float64")


@composite
def _point_fields(draw: DrawFn) -> PointFields:
    fields: PointFields = {
        "x": draw(_FLOAT64S),
        "y": draw(_FLOAT64S),
        "z": draw(_FLOAT64S),
    }
    if draw(booleans()):
        fields["label"] = draw(_optional(STRINGS))

    return fields


@composite
def _entry_fields(draw: DrawFn) -> EntryFields:
    fields: EntryFields = {
        "key": draw(scalar_values("uint16")),
        "weight": draw(_FLOAT32S),
    }
    if draw(booleans()):
        fields["note"] = draw(_optional(STRINGS))

    return fields


extents: SearchStrategy[Extent] = builds(Extent, _FLOAT32S, _FLOAT32S)


@composite
def record_fields(draw: DrawFn) -> RecordFields:
    """A record's fields, each optional one left out half the time, and
    ``None`` or a value otherwise"""
    fields: RecordFields = {
        "id": draw(scalar_values("uint32")),
        "label": draw(STRINGS),
    }
    if draw(booleans()):
        fields["name"] = draw(_optional(STRINGS))
    if draw(booleans()):
        fields["values"] = draw(_optional(lists(_FLOAT64S, max_size=4)))
    if draw(booleans()):
        fields["origin"] = draw(_optional(_point_fields()))
    if draw(booleans()):
        fields["flag"] = draw(_optional(booleans()))
    if draw(booleans()):
        fields["entries"] = draw(_optional(lists(_entry_fields(), max_size=3)))
    if draw(booleans()):
        fields["total"] = draw(_optional(scalar_values("int64")))
    if draw(booleans()):
        fields["extent"] = draw(_optional(extents))

    return fields
