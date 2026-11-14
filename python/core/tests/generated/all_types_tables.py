"""The code generated from ``schemas/all_types.tafla``: how tests read its
messages back and describe them to build their tables by hand, and strategies
of their fields' values."""

from collections.abc import Callable, Iterator
from typing import Any, Protocol

from hypothesis.strategies import (
    SearchStrategy,
    booleans,
    builds,
    just,
    lists,
    recursive,
)

from tests.descriptions import SCALAR_VALUES, STRINGS, scalar_values
from tests.generated.all_types.all_types import (
    Arrays,
    ArraysFields,
    Documented,
    DocumentedFields,
    Inner,
    InnerFields,
    Keywords,
    KeywordsFields,
    Nested,
    NestedFields,
    Outer,
    OuterFields,
    Packed,
    PathFields,
    Point,
    PointFields,
    Reserved,
    ReservedFields,
    Scalars,
    ScalarsFields,
    Segment,
    SegmentFields,
    Structs,
    StructsFields,
    Tree,
    TreeFields,
)
from tests.generated.descriptions import (
    boolean,
    message,
    scalar,
    string,
    struct,
    struct_array,
    struct_bytes,
    table,
    typed_array,
    vector,
)
from tests.tables import DEFAULT_LAYOUT, Description

# Reading every field, with arrays read from their views into lists to compare
# with the fields that were written


def read_scalars(scalars: Scalars) -> ScalarsFields:
    return {
        "int8": scalars.int8(),
        "uint8": scalars.uint8(),
        "int16": scalars.int16(),
        "uint16": scalars.uint16(),
        "int32": scalars.int32(),
        "uint32": scalars.uint32(),
        "int64": scalars.int64(),
        "uint64": scalars.uint64(),
        "float32": scalars.float32(),
        "float64": scalars.float64(),
        "bool_": scalars.bool_(),
    }


def read_arrays(arrays: Arrays) -> ArraysFields:
    return {
        "int8": arrays.int8().tolist(),
        "uint8": arrays.uint8().tolist(),
        "int16": arrays.int16().tolist(),
        "uint16": arrays.uint16().tolist(),
        "int32": arrays.int32().tolist(),
        "uint32": arrays.uint32().tolist(),
        "int64": arrays.int64().tolist(),
        "uint64": arrays.uint64().tolist(),
        "float32": arrays.float32().tolist(),
        "float64": arrays.float64().tolist(),
    }


# The getters of vectors and arrays of structs return views, which read each
# item when it's asked for. A function reads a view's items into a list: by
# iterating it, or by index.


class View[Item](Protocol):
    def __len__(self) -> int: ...

    def __getitem__(self, index: int) -> Item: ...

    def __iter__(self) -> Iterator[Item]: ...


type ViewItems = Callable[[View[Any]], list[Any]]


def in_order[Item](view: View[Item]) -> list[Item]:
    """Reads a view's items by iterating it"""
    return list(view)


def from_last[Item](view: View[Item]) -> list[Item]:
    """Reads a view's items by negative indices, from the last to the first, as
    a reader that skips items would, into a list in order"""
    items = [view[index] for index in range(-1, -len(view) - 1, -1)]
    items.reverse()
    return items


def read_nested(nested: Nested, items: ViewItems = in_order) -> NestedFields:
    return {
        "label": nested.label(),
        "outer": _read_outer(nested.outer()),
        "trees": [_read_tree(tree, items) for tree in items(nested.trees())],
    }


def _read_outer(outer: Outer) -> OuterFields:
    return {"count": outer.count(), "inner": _read_inner(outer.inner())}


def _read_inner(inner: Inner) -> InnerFields:
    return {"name": inner.name(), "values": inner.values().tolist()}


def _read_tree(tree: Tree, items: ViewItems) -> TreeFields:
    return {
        "value": tree.value(),
        "children": [_read_tree(child, items) for child in items(tree.children())],
    }


def read_reserved(reserved: Reserved) -> ReservedFields:
    return {"keywords": _read_keywords(reserved.keywords())}


def _read_keywords(keywords: Keywords) -> KeywordsFields:
    return {
        "class_": keywords.class_(),
        "default": keywords.default(),
        "in_": keywords.in_(),
        "match_": keywords.match_(),
        "self": keywords.self(),
        "type_": keywords.type_(),
    }


def read_structs(structs: Structs, items: ViewItems = in_order) -> StructsFields:
    # In the fields' order, so that corrupted bytes raise the first field's error
    packed, documented = structs.packed(), items(structs.documented())
    segment, segments = structs.segment(), items(structs.segments())
    path = structs.path()

    return {
        "packed": packed,
        "documented": documented,
        "segment": segment,
        "segments": segments,
        "path": {"start": path.start(), "points": items(path.points())},
    }


# The tables that the generated code writes, in the conformance suite's model


def describe_scalars(fields: ScalarsFields) -> Description:
    return message(
        DEFAULT_LAYOUT,
        [
            scalar("int8", fields["int8"]),
            scalar("uint8", fields["uint8"]),
            scalar("int16", fields["int16"]),
            scalar("uint16", fields["uint16"]),
            scalar("int32", fields["int32"]),
            scalar("uint32", fields["uint32"]),
            scalar("int64", fields["int64"]),
            scalar("uint64", fields["uint64"]),
            scalar("float32", fields["float32"]),
            scalar("float64", fields["float64"]),
            boolean(fields["bool_"]),
        ],
    )


def describe_arrays(fields: ArraysFields) -> Description:
    return message(
        DEFAULT_LAYOUT,
        [
            typed_array("int8", fields["int8"]),
            typed_array("uint8", fields["uint8"]),
            typed_array("int16", fields["int16"]),
            typed_array("uint16", fields["uint16"]),
            typed_array("int32", fields["int32"]),
            typed_array("uint32", fields["uint32"]),
            typed_array("int64", fields["int64"]),
            typed_array("uint64", fields["uint64"]),
            typed_array("float32", fields["float32"]),
            typed_array("float64", fields["float64"]),
        ],
    )


def describe_nested(fields: NestedFields) -> Description:
    outer = fields["outer"]
    inner = outer["inner"]

    return message(
        DEFAULT_LAYOUT,
        [
            string(fields["label"]),
            table(
                [
                    scalar("uint32", outer["count"]),
                    table(
                        [
                            string(inner["name"]),
                            typed_array("float64", inner["values"]),
                        ]
                    ),
                ]
            ),
            vector(_describe_tree(tree) for tree in fields["trees"]),
        ],
    )


def _describe_tree(tree: TreeFields) -> list[Description]:
    return [
        scalar("int32", tree["value"]),
        vector(_describe_tree(child) for child in tree["children"]),
    ]


def describe_reserved(fields: ReservedFields) -> Description:
    keywords = fields["keywords"]

    return message(
        DEFAULT_LAYOUT,
        [
            table(
                [
                    scalar("uint8", keywords["class_"]),
                    scalar("uint8", keywords["default"]),
                    scalar("uint8", keywords["in_"]),
                    scalar("uint8", keywords["match_"]),
                    scalar("uint8", keywords["self"]),
                    string(keywords["type_"]),
                ]
            )
        ],
    )


# The scalar types of each struct, in the order that they lie in its bytes
DOCUMENTED_TYPES = ["uint8", "uint16", "uint32"]
POINT_TYPES = ["float32", "float32"]
# A segment's scalars are those of its two points, in their place
SEGMENT_TYPES = POINT_TYPES * 2


def describe_structs(fields: StructsFields) -> Description:
    """Structs as fields and in arrays, each array aligned to its structs'
    largest scalar, and in a table nested from its builder"""
    path = fields["path"]

    return message(
        DEFAULT_LAYOUT,
        [
            struct(describe_packed(fields["packed"])),
            struct_array(
                DOCUMENTED_TYPES, map(describe_documented, fields["documented"])
            ),
            struct(describe_segment(fields["segment"])),
            struct_array(SEGMENT_TYPES, map(describe_segment, fields["segments"])),
            table(
                [
                    struct(describe_point(path["start"])),
                    struct_array(POINT_TYPES, map(describe_point, path["points"])),
                ]
            ),
        ],
    )


def describe_packed(packed: Packed) -> list[Description]:
    """The fields of a ``Packed``, in the order that they lie in its bytes:
    one of each scalar type, in their order and named after them, and then a
    bool, which a struct holds as the uint8 1 or 0"""
    scalars = [scalar(kind, getattr(packed, kind)) for kind in SCALAR_VALUES]
    return [*scalars, scalar("uint8", 1 if packed.bool_ else 0)]


def packed_bytes(packed: Packed) -> bytes:
    """A ``Packed`` as its fields lie at fixed offsets, little-endian, without
    padding"""
    return struct_bytes(describe_packed(packed))


def describe_documented(documented: DocumentedFields) -> list[Description]:
    """The fields of a ``Documented``, in the order that they lie in its
    bytes"""
    return [
        scalar("uint8", documented.one),
        scalar("uint16", documented.two),
        scalar("uint32", documented.undocumented),
    ]


def documented_bytes(documented: Documented) -> bytes:
    """A ``Documented`` as its fields lie at fixed offsets, little-endian,
    without padding"""
    return struct_bytes(describe_documented(documented))


def describe_point(point: PointFields) -> list[Description]:
    return [scalar("float32", point.x), scalar("float32", point.y)]


def describe_segment(segment: SegmentFields) -> list[Description]:
    """The fields of a ``Segment``: those of its points, one after the
    other"""
    return [*describe_point(segment.start), *describe_point(segment.end)]


def segment_bytes(segment: Segment) -> bytes:
    """A ``Segment`` as its points' fields lie at fixed offsets,
    little-endian, without padding"""
    return struct_bytes(describe_segment(segment))


# Values of the fields over each type's full range, with -0.0, NaN and the
# infinities, strings outside the Basic Multilingual Plane, empty arrays,
# vectors and arrays of structs, and trees a few levels deep. The fields of
# Scalars, Arrays and Packed are named after their types, as SCALAR_VALUES
# names its strategies, and the bools of Scalars and Packed are bool_.

_SCALARS: dict[str, SearchStrategy[Any]] = {
    kind: scalar_values(kind) for kind in SCALAR_VALUES
}


def _arrays(values: SearchStrategy[Any]) -> SearchStrategy[list[Any]]:
    return lists(values, max_size=8)


scalars_fields: SearchStrategy[ScalarsFields] = builds(
    ScalarsFields, **_SCALARS, bool_=booleans()
)

arrays_fields: SearchStrategy[ArraysFields] = builds(
    ArraysFields,
    **{kind: _arrays(values) for kind, values in _SCALARS.items()},
)


def _trees_of(children: SearchStrategy[TreeFields]) -> SearchStrategy[TreeFields]:
    return builds(
        TreeFields, value=_SCALARS["int32"], children=lists(children, max_size=3)
    )


_leaves: SearchStrategy[TreeFields] = builds(
    TreeFields, value=_SCALARS["int32"], children=just([])
)
_trees: SearchStrategy[TreeFields] = recursive(_leaves, _trees_of, max_leaves=8)

nested_fields: SearchStrategy[NestedFields] = builds(
    NestedFields,
    label=STRINGS,
    outer=builds(
        OuterFields,
        count=_SCALARS["uint32"],
        inner=builds(InnerFields, name=STRINGS, values=_arrays(_SCALARS["float64"])),
    ),
    trees=lists(_trees, max_size=3),
)

reserved_fields: SearchStrategy[ReservedFields] = builds(
    ReservedFields,
    keywords=builds(
        KeywordsFields,
        class_=_SCALARS["uint8"],
        default=_SCALARS["uint8"],
        in_=_SCALARS["uint8"],
        match_=_SCALARS["uint8"],
        self=_SCALARS["uint8"],
        type_=STRINGS,
    ),
)

packed_structs: SearchStrategy[Packed] = builds(Packed, **_SCALARS, bool_=booleans())

documented_structs: SearchStrategy[Documented] = builds(
    Documented,
    one=_SCALARS["uint8"],
    two=_SCALARS["uint16"],
    undocumented=_SCALARS["uint32"],
)

point_structs: SearchStrategy[Point] = builds(
    Point, x=_SCALARS["float32"], y=_SCALARS["float32"]
)

segment_structs: SearchStrategy[Segment] = builds(
    Segment, start=point_structs, end=point_structs
)

structs_fields: SearchStrategy[StructsFields] = builds(
    StructsFields,
    packed=packed_structs,
    documented=_arrays(documented_structs),
    segment=segment_structs,
    segments=_arrays(segment_structs),
    path=builds(PathFields, start=point_structs, points=_arrays(point_structs)),
)
