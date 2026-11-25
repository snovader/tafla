"""The message of enums that the code generated from
``schemas/all_types.tafla`` writes: how tests read it back and describe it to
build its table by hand, of the integer types that store its enums, and
strategies of its fields' values."""

from collections.abc import Callable
from enum import IntEnum
from typing import Any

from hypothesis.strategies import SearchStrategy, builds, lists, sampled_from

from tests.descriptions import STRINGS, scalar_values
from tests.generated.all_types.all_types import (
    Change,
    Direction,
    Enums,
    EnumsFields,
    Extent,
    Level,
    Mark,
    MarkFields,
    Mask,
    Sign,
    Status,
    Step,
    StepFields,
    Turn,
    Unit,
    Word,
)
from tests.generated.all_types_tables import ViewItems, in_order
from tests.generated.descriptions import (
    message,
    scalar,
    string,
    struct,
    struct_array,
    struct_bytes,
    typed_array,
    vector,
)
from tests.tables import DEFAULT_LAYOUT, Description

ENUM_VALUES: list[tuple[type[IntEnum], str, dict[str, int]]] = [
    (Level, "uint8", {"LOW": 0, "MEDIUM": 1, "HIGH": 2}),
    (
        Direction,
        "int8",
        {"BACKWARD": -1, "NONE": 0, "FORWARD": 1, "FURTHEST_BACK": -128},
    ),
    (Unit, "uint16", {"LAST": 65_535, "FIRST": 0, "V_2": 2}),
    (Change, "int16", {"LEAST": -32_768, "GREATEST": 32_767}),
    (Status, "uint32", {"OK": 0, "NOT_FOUND": 404, "GREATEST": 4_294_967_295}),
    (Sign, "int32", {"LEAST": -2_147_483_648, "GREATEST": 2_147_483_647}),
    (
        Mask,
        "uint64",
        {
            "NONE": 0,
            "ONE": 1,
            "BEYOND_FLOAT64": 9_007_199_254_740_993,
            "ALL": 2**64 - 1,
        },
    ),
    (Extent, "int64", {"LEAST": -(2**63), "ZERO": 0, "GREATEST": 2**63 - 1}),
    (
        Word,
        "uint8",
        {
            "CLASS": 0,
            "DEFAULT": 1,
            "IN": 2,
            "MATCH": 3,
            "SELF": 4,
            "TYPE": 5,
            "TRUE": 6,
            "NULL": 7,
            "NONE": 8,
            "INIT": 9,
            "NEW": 10,
            "NAME": 11,
            "VALUE": 12,
        },
    ),
]
"""The enums of ``all_types.tafla``, with the integer type that stores each
and the values that the schema names, by their names in upper case"""

# Reading every field, with arrays read from their views into lists to compare
# with the fields that were written


def read_enums(enums: Enums, items: ViewItems = in_order) -> EnumsFields:
    return {
        "level": enums.level(),
        "levels": enums.levels().tolist(),
        "direction": enums.direction(),
        "directions": enums.directions().tolist(),
        "unit": enums.unit(),
        "units": enums.units().tolist(),
        "change": enums.change(),
        "changes": enums.changes().tolist(),
        "status": enums.status(),
        "statuses": enums.statuses().tolist(),
        "sign": enums.sign(),
        "signs": enums.signs().tolist(),
        "mask": enums.mask(),
        "masks": enums.masks().tolist(),
        "extent": enums.extent(),
        "extents": enums.extents().tolist(),
        "word": enums.word(),
        "mark": enums.mark(),
        "marks": items(enums.marks()),
        "steps": [read_step(step) for step in items(enums.steps())],
    }


def read_step(step: Step) -> StepFields:
    return {"level": step.level(), "mark": step.mark(), "name": step.name()}


# The table that the generated code writes, in the conformance suite's model,
# where an enum is the integer type that stores it

# The scalar types of a mark, in the order that they lie in its bytes, the
# last two of its turn
MARK_TYPES = ["uint8", "int16", "float32", "int64", "int8", "uint64"]


def describe_enums(fields: EnumsFields) -> Description:
    return message(
        DEFAULT_LAYOUT,
        [
            scalar("uint8", fields["level"]),
            typed_array("uint8", fields["levels"]),
            scalar("int8", fields["direction"]),
            typed_array("int8", fields["directions"]),
            scalar("uint16", fields["unit"]),
            typed_array("uint16", fields["units"]),
            scalar("int16", fields["change"]),
            typed_array("int16", fields["changes"]),
            scalar("uint32", fields["status"]),
            typed_array("uint32", fields["statuses"]),
            scalar("int32", fields["sign"]),
            typed_array("int32", fields["signs"]),
            scalar("uint64", fields["mask"]),
            typed_array("uint64", fields["masks"]),
            scalar("int64", fields["extent"]),
            typed_array("int64", fields["extents"]),
            scalar("uint8", fields["word"]),
            struct(describe_mark(fields["mark"])),
            struct_array(MARK_TYPES, map(describe_mark, fields["marks"])),
            vector(_describe_step(step) for step in fields["steps"]),
        ],
    )


def _describe_step(step: StepFields) -> list[Description]:
    return [
        scalar("uint8", step["level"]),
        struct(describe_mark(step["mark"])),
        string(step["name"]),
    ]


def describe_mark(mark: MarkFields) -> list[Description]:
    """The fields of a ``Mark``, in the order that they lie in its bytes,
    those of its turn in its place"""
    return [
        scalar("uint8", mark.level),
        scalar("int16", mark.change),
        scalar("float32", mark.weight),
        scalar("int64", mark.extent),
        scalar("int8", mark.turn.direction),
        scalar("uint64", mark.turn.mask),
    ]


def mark_bytes(mark: Mark) -> bytes:
    """A ``Mark`` as its fields lie at fixed offsets, little-endian, without
    padding"""
    return struct_bytes(describe_mark(mark))


# Values of the fields: any integer of the type that stores each enum, most of
# which the enum doesn't name, or the enum's members


def _arrays(values: SearchStrategy[Any]) -> SearchStrategy[list[Any]]:
    return lists(values, max_size=8)


def _marks_of(
    values: Callable[[type[IntEnum]], SearchStrategy[int]],
) -> SearchStrategy[Mark]:
    return builds(
        Mark,
        level=values(Level),
        change=values(Change),
        weight=scalar_values("float32"),
        extent=values(Extent),
        turn=builds(Turn, direction=values(Direction), mask=values(Mask)),
    )


def _fields_of(
    values: Callable[[type[IntEnum]], SearchStrategy[int]],
) -> SearchStrategy[EnumsFields]:
    """The fields of an ``Enums``, with the values of each enum that
    ``values`` gives"""
    marks = _marks_of(values)

    return builds(
        EnumsFields,
        level=values(Level),
        levels=_arrays(values(Level)),
        direction=values(Direction),
        directions=_arrays(values(Direction)),
        unit=values(Unit),
        units=_arrays(values(Unit)),
        change=values(Change),
        changes=_arrays(values(Change)),
        status=values(Status),
        statuses=_arrays(values(Status)),
        sign=values(Sign),
        signs=_arrays(values(Sign)),
        mask=values(Mask),
        masks=_arrays(values(Mask)),
        extent=values(Extent),
        extents=_arrays(values(Extent)),
        word=values(Word),
        mark=marks,
        marks=_arrays(marks),
        steps=_arrays(
            builds(StepFields, level=values(Level), mark=marks, name=STRINGS)
        ),
    )


_UNDERLYING = {enum: kind for enum, kind, _ in ENUM_VALUES}


def _integers(enum: type[IntEnum]) -> SearchStrategy[int]:
    """Any integer of the type that stores an enum"""
    return scalar_values(_UNDERLYING[enum])


mark_structs: SearchStrategy[Mark] = _marks_of(_integers)
enums_fields: SearchStrategy[EnumsFields] = _fields_of(_integers)

member_marks: SearchStrategy[Mark] = _marks_of(sampled_from)
"""Marks of the enums' members, rather than of plain ints"""

member_fields: SearchStrategy[EnumsFields] = _fields_of(sampled_from)
"""The fields of an ``Enums`` with the enums' members, rather than plain ints"""
