"""The code generated from ``schemas/shapes.tafla``: how tests read its
messages back and describe them to build their tables by hand, and strategies
of their fields' values."""

from typing import Any

from hypothesis.strategies import SearchStrategy, builds, lists

from tests.descriptions import STRINGS, scalar_values
from tests.generated.descriptions import (
    layout,
    message,
    scalar,
    string,
    struct,
    struct_bytes,
    table,
    typed_array,
    vector,
)
from tests.generated.shapes.shapes import (
    Columns,
    ColumnsFields,
    Document,
    DocumentFields,
    Item,
    ItemFields,
    ItemList,
    ItemListFields,
    Pair,
    PairFields,
    Point,
    PointFields,
    Sample,
    Step,
    Task,
    TaskFields,
)
from tests.tables import DEFAULT_LAYOUT, Description

# Reading every field, with arrays read from their views into lists to compare
# with the fields that were written


def read_point(point: Point) -> PointFields:
    return {"x": point.x(), "y": point.y(), "id": point.id(), "name": point.name()}


def read_document(document: Document) -> DocumentFields:
    return {
        "x": document.x(),
        "y": document.y(),
        "name": document.name(),
        "columns": _read_columns(document.columns()),
    }


def _read_columns(columns: Columns) -> ColumnsFields:
    return {
        "base": columns.base(),
        "keys": columns.keys().tolist(),
        "a": columns.a().tolist(),
        "b": columns.b().tolist(),
    }


def read_item_list(item_list: ItemList) -> ItemListFields:
    return {"items": [read_item(item) for item in item_list.items()]}


def read_item(item: Item) -> ItemFields:
    return {"value": item.value(), "pair": _read_pair(item.pair())}


def _read_pair(pair: Pair) -> PairFields:
    return {"first": pair.first(), "second": pair.second()}


def read_task(task: Task) -> TaskFields:
    return {
        "priority": task.priority(),
        "history": task.history().tolist(),
        "step": task.step(),
        "name": task.name(),
    }


# The tables that the generated code writes, in the conformance suite's model

DOCUMENT_LAYOUT: Description = layout(sized=True, versioned=True)
"""The layout of a ``Document``, from its attributes"""

DOCUMENT_VERSION = 1
"""The version of a ``Document``, from its attributes"""


def describe_point(fields: PointFields) -> Description:
    return message(
        DEFAULT_LAYOUT,
        [
            scalar("float64", fields["x"]),
            scalar("float64", fields["y"]),
            scalar("int64", fields["id"]),
            string(fields["name"]),
        ],
    )


def describe_document(
    fields: DocumentFields, version: int = DOCUMENT_VERSION
) -> Description:
    columns = fields["columns"]

    return message(
        DOCUMENT_LAYOUT,
        [
            scalar("float64", fields["x"]),
            scalar("float64", fields["y"]),
            string(fields["name"]),
            table(
                [
                    scalar("int64", columns["base"]),
                    typed_array("uint32", columns["keys"]),
                    typed_array("float32", columns["a"]),
                    typed_array("float32", columns["b"]),
                ]
            ),
        ],
        version,
    )


def describe_item_list(fields: ItemListFields) -> Description:
    return message(
        DEFAULT_LAYOUT, [vector(_describe_item(item) for item in fields["items"])]
    )


def _describe_item(item: ItemFields) -> list[Description]:
    pair = item["pair"]

    return [
        scalar("float32", item["value"]),
        table([scalar("float32", pair["first"]), scalar("float32", pair["second"])]),
    ]


def describe_sample(sample: Sample) -> list[Description]:
    """The fields of a ``Sample``, in the order that they lie in its bytes"""
    return [
        scalar("uint32", sample.key),
        scalar("float32", sample.a),
        scalar("float32", sample.b),
    ]


def describe_task(fields: TaskFields) -> Description:
    """A task, whose enums are the integer types that store them"""
    return message(
        DEFAULT_LAYOUT,
        [
            scalar("int8", fields["priority"]),
            typed_array("uint16", fields["history"]),
            struct(describe_step(fields["step"])),
            string(fields["name"]),
        ],
    )


def describe_step(step: Step) -> list[Description]:
    """The fields of a ``Step``, in the order that they lie in its bytes"""
    return [
        scalar("uint16", step.state),
        scalar("int8", step.priority),
        scalar("uint32", step.seconds),
    ]


def sample_bytes(sample: Sample) -> bytes:
    """A ``Sample`` as its fields lie at fixed offsets, little-endian"""
    return struct_bytes(describe_sample(sample))


# Values of the fields over each type's full range, with -0.0, NaN and the
# infinities, strings outside the Basic Multilingual Plane, and empty arrays
# and vectors


def _arrays(values: SearchStrategy[Any]) -> SearchStrategy[list[Any]]:
    return lists(values, max_size=8)


point_fields: SearchStrategy[PointFields] = builds(
    PointFields,
    x=scalar_values("float64"),
    y=scalar_values("float64"),
    id=scalar_values("int64"),
    name=STRINGS,
)

document_fields: SearchStrategy[DocumentFields] = builds(
    DocumentFields,
    x=scalar_values("float64"),
    y=scalar_values("float64"),
    name=STRINGS,
    columns=builds(
        ColumnsFields,
        base=scalar_values("int64"),
        keys=_arrays(scalar_values("uint32")),
        a=_arrays(scalar_values("float32")),
        b=_arrays(scalar_values("float32")),
    ),
)

_pair_fields: SearchStrategy[PairFields] = builds(
    PairFields, first=scalar_values("float32"), second=scalar_values("float32")
)

item_list_fields: SearchStrategy[ItemListFields] = builds(
    ItemListFields,
    items=_arrays(
        builds(ItemFields, value=scalar_values("float32"), pair=_pair_fields)
    ),
)

samples: SearchStrategy[Sample] = builds(
    Sample,
    key=scalar_values("uint32"),
    a=scalar_values("float32"),
    b=scalar_values("float32"),
)
