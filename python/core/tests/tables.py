"""Builds tables from descriptions in the shape of the conformance suite's
``cases.json``, and reads tables back as descriptions. ``conformance/README.md``
at the root of the repository describes them.

The conformance test reads and rebuilds the suite's tables with them, and the
property tests build and read the descriptions that ``tests/descriptions.py``
generates, so that both test one model of tables.

Values are as JSON holds them: 64-bit integers as decimal strings, and the
floats that JSON has no number for as ``"NaN"``, ``"Infinity"``,
``"-Infinity"`` and ``"-0"``. A reader returns them as tafla reads them, as
ints and floats, and ``comparable_table()`` compares the two forms exactly.
"""

import struct
from collections.abc import Buffer, Callable, Iterable
from dataclasses import dataclass
from math import copysign, isinf, isnan
from typing import Any, Literal, cast

from tafla import OffsetType, TableBuilder, TableLayout, TableReader, TableWriter

# A table, field, layout or vector item as cases.json describes it
type Description = dict[str, Any]

type Alignment = Literal[1, 2, 4, 8]

# The struct format of each scalar type, which is also the type code of the
# array module and of the memoryviews that array getters return
FORMATS: dict[str, str] = {
    "int8": "b",
    "uint8": "B",
    "int16": "h",
    "uint16": "H",
    "int32": "i",
    "uint32": "I",
    "int64": "q",
    "uint64": "Q",
    "float32": "f",
    "float64": "d",
}

ELEMENT_SIZES: dict[str, int] = {
    kind: struct.calcsize(f"<{code}") for kind, code in FORMATS.items()
}

FLOAT_TYPES = ("float32", "float64")
INT64_TYPES = ("int64", "uint64")

# TableLayout.DEFAULT, as cases.json describes it
DEFAULT_LAYOUT: Description = {
    "sized": False,
    "versioned": False,
    "offsetType": "uint32",
}

# The longest table that the offsets of each offset type address
LONGEST: dict[OffsetType, int] = {
    "uint8": 0xFF,
    "uint16": 0xFFFF,
    "uint32": 0xFFFF_FFFF,
}

# The alignment that set_table() gives a table nested as bytes by default
BYTES_ALIGNMENT: Alignment = 4


def from_json(kind: str, value: Any) -> Any:
    """Returns a scalar as tafla takes it, from a description's value or a
    value that was read. ``float()`` parses the names of floats."""
    if kind in INT64_TYPES:
        return int(value)

    if kind in FLOAT_TYPES:
        return float(value)

    return value


def to_json_number(value: float) -> float | str:
    """Returns a float as JSON holds it, with the floats that JSON has no
    number for as their names"""
    if isnan(value):
        return "NaN"

    if isinf(value):
        return "Infinity" if value > 0 else "-Infinity"

    return "-0" if value == 0 and copysign(1, value) < 0 else value


def to_json(kind: str, value: Any) -> Any:
    """Returns a scalar as a description holds it, from a value as tafla takes
    it or reads it: the inverse of ``from_json()``"""
    if kind in INT64_TYPES:
        return str(value)

    if kind in FLOAT_TYPES:
        return to_json_number(value)

    return value


def offset_size_of(offset_type: OffsetType) -> int:
    """The offset size of an offset type, its width in bytes, which is the size
    of the unsigned integer type of its name"""
    return ELEMENT_SIZES[offset_type]


def layout_of(layout: Description, version: int = 0) -> TableLayout:
    """The layout that a description sets out, with ``version`` when it's
    versioned. A reader reads tables of any version, so it needs none."""
    return TableLayout(
        sized=layout["sized"],
        version=version if layout["versioned"] else None,
        offset_type=layout["offsetType"],
    )


def struct_of(types: Iterable[str]) -> struct.Struct:
    """The ``struct`` of a struct of scalars of these types, little-endian
    and without padding, whose ``size`` is the struct's"""
    return struct.Struct("<" + "".join(FORMATS[kind] for kind in types))


def struct_alignment(types: Iterable[str]) -> Alignment:
    """The alignment of an array of structs of scalars of these types: the
    size of the largest"""
    return cast(Alignment, max(ELEMENT_SIZES[kind] for kind in types))


def create_builder(table: Description) -> TableBuilder:
    """Returns the builder of a table's description, with none of its fields
    set"""
    layout = table["layout"]
    sized, offset_type = layout["sized"], layout["offsetType"]
    fields = len(table["fields"])

    if layout["versioned"]:
        return TableBuilder(fields, layout_of(layout, table["version"]))

    return TableBuilder(fields, TableLayout(sized=sized, offset_type=offset_type))


def build_table(table: Description) -> TableBuilder:
    """Builds a table as its description sets it out"""
    builder = create_builder(table)
    for field in table["fields"]:
        set_field(builder, field)

    return builder


def set_field(builder: TableBuilder, field: Description) -> None:
    kind = field["type"]

    if kind == "absent":
        builder.set_absent()
    elif kind == "table":
        builder.set_table(_build_nested(field), field.get("alignment"))
    elif kind == "vector":
        builder.set_vector([_build_nested(item) for item in field["items"]])
    elif kind == "struct":
        scalars = field["fields"]
        packing = struct_of(scalar["type"] for scalar in scalars)
        builder.set_struct(packing.pack(*_struct_values(scalars)))
    elif kind == "struct[]":
        types = field["fields"]
        packing = struct_of(types)
        data = b"".join(
            packing.pack(*_item_values(types, item)) for item in field["items"]
        )
        builder.set_struct_array(data, packing.size, struct_alignment(types))
    elif kind.endswith("[]"):
        element = kind.removesuffix("[]")
        getattr(builder, f"set_{element}_array")(_array_values(element, field))
    elif kind == "bool":
        builder.set_uint8(_bool_byte(field))
    else:
        getattr(builder, f"set_{kind}")(from_json(kind, field["value"]))


def _bool_byte(field: Description) -> int:
    """The byte that writers write for a bool, which tafla sets as a uint8: 1
    for true and 0 for false. Readers read any byte other than 0 as true."""
    return 1 if field["value"] else 0


def _build_nested(nested: Description) -> TableBuilder | memoryview[int]:
    builder = build_table(nested["table"])
    return builder if nested["from"] == "builder" else builder.bytes()


def build_table_in_place(table: Description) -> TableBuilder:
    """Builds a table as ``build_table()`` does, but records each table nested
    from a builder that the builder writes in place with
    ``set_table_in_place()``, and each vector of them whose items hold as
    many fields with ``set_vector_in_place()``, rather than from their
    builders. A table nested at an alignment of its own is nested from its
    builder still."""
    builder = create_builder(table)
    offset_type = table["layout"]["offsetType"]
    for field in table["fields"]:
        set_field_in_place(builder, field, offset_type)

    return builder


def set_field_in_place(
    builder: TableBuilder, field: Description, offset_type: OffsetType
) -> None:
    """Sets a field as ``set_field()`` does, recording the tables nested in it
    in place as ``build_table_in_place()`` does, in a builder of
    ``offset_type``"""
    kind = field["type"]

    if (
        kind == "table"
        and "alignment" not in field
        and records_in_place(field, offset_type)
    ):
        fields = field["table"]["fields"]
        builder.set_table_in_place(
            _set_fields_in_place, fields, offset_type, fields=len(fields)
        )
    elif kind == "vector" and all(
        records_in_place(item, offset_type) for item in field["items"]
    ):
        tables = [item["table"]["fields"] for item in field["items"]]
        counts = {len(fields) for fields in tables}
        if len(counts) > 1:
            set_field(builder, field)
            return

        def set_item(item: TableBuilder, fields: list[Description]) -> None:
            _set_fields_in_place(item, fields, offset_type)

        builder.set_vector_in_place(
            tables, set_item, fields=counts.pop() if counts else 0
        )
    else:
        set_field(builder, field)


def records_in_place(nested: Description, offset_type: OffsetType) -> bool:
    """Whether a nested table is one that a builder of ``offset_type`` writes
    in place: from a builder without a header and of that offset type"""
    layout = nested["table"]["layout"]
    return (
        nested["from"] == "builder"
        and not layout["sized"]
        and not layout["versioned"]
        and layout["offsetType"] == offset_type
    )


def _set_fields_in_place(
    builder: TableBuilder, fields: list[Description], offset_type: OffsetType
) -> None:
    for field in fields:
        set_field_in_place(builder, field, offset_type)


def _struct_values(scalars: list[Description]) -> list[Any]:
    """The values of a struct's scalars, as tafla takes them"""
    return [from_json(scalar["type"], scalar["value"]) for scalar in scalars]


def _item_values(types: list[str], item: list[Any]) -> list[Any]:
    """The values of a struct in an array of structs, as tafla takes them"""
    return [from_json(kind, value) for kind, value in zip(types, item, strict=True)]


def _array_values(element: str, field: Description) -> list[Any]:
    values = field["value"]
    if element in INT64_TYPES or element in FLOAT_TYPES:
        return [from_json(element, value) for value in values]

    return values


def write_table(
    table: Description, *, wrap: Callable[[TableWriter], Any] | None = None
) -> memoryview[int]:
    """Writes a table with a ``TableWriter`` as its description sets it out.
    The writer writes the tables nested in it in place, so they must have no
    header and its offset type, as ``nested_in_place()`` makes them. ``wrap``
    returns what the fields are written through instead of the writer, which
    can change the calls."""
    layout = table["layout"]
    sized, offset_type = layout["sized"], layout["offsetType"]
    fields = len(table["fields"])

    if layout["versioned"]:
        writer = TableWriter(fields, layout_of(layout, table["version"]))
    else:
        writer = TableWriter(fields, TableLayout(sized=sized, offset_type=offset_type))

    target = writer if wrap is None else wrap(writer)
    for index, field in enumerate(table["fields"]):
        write_field(target, index, field)

    return writer.bytes()


def write_field(writer: TableWriter, index: int, field: Description) -> None:
    kind = field["type"]

    if kind == "absent":
        writer.set_absent(index)
    elif kind == "table":
        writer.set_table(index, alignment_of(field))
        _write_nested(writer, field["table"])
    elif kind == "vector":
        vector = writer.set_vector(index, len(field["items"]))
        for item, nested in enumerate(field["items"]):
            _write_nested(writer, nested["table"])
            writer.set_item_end(vector, item)
    elif kind == "struct":
        scalars = field["fields"]
        packing = struct_of(scalar["type"] for scalar in scalars)
        writer.set_struct(index, packing.pack(*_struct_values(scalars)))
    elif kind == "struct[]":
        types = field["fields"]
        packing = struct_of(types)
        data = b"".join(
            packing.pack(*_item_values(types, item)) for item in field["items"]
        )
        writer.set_struct_array(index, data, packing.size, struct_alignment(types))
    elif kind.endswith("[]"):
        element = kind.removesuffix("[]")
        getattr(writer, f"set_{element}_array")(index, _array_values(element, field))
    elif kind == "bool":
        writer.set_uint8(index, _bool_byte(field))
    else:
        getattr(writer, f"set_{kind}")(index, from_json(kind, field["value"]))


def _write_nested(writer: TableWriter, table: Description) -> None:
    parent = writer.start_table(len(table["fields"]))
    for index, field in enumerate(table["fields"]):
        write_field(writer, index, field)
    writer.end_table(parent)


def nested_in_place(table: Description) -> Description:
    """Returns a table's description with every table nested in it made one
    that a ``TableWriter`` writes in place: from its builder, without a header,
    and with the root's offset type"""
    offset_type = table["layout"]["offsetType"]

    def in_place(nested: Description) -> Description:
        layout = {"sized": False, "versioned": False, "offsetType": offset_type}
        table = {"layout": layout, "fields": [field(each) for each in nested["fields"]]}
        if "missing" in nested:
            table["missing"] = nested["missing"]

        return table

    def field(description: Description) -> Description:
        if description["type"] == "table":
            return {
                **description,
                "from": "builder",
                "table": in_place(description["table"]),
            }

        if description["type"] == "vector":
            items = [
                {"from": "builder", "table": in_place(item["table"])}
                for item in description["items"]
            ]
            return {**description, "items": items}

        return description

    return {**table, "fields": [field(each) for each in table["fields"]]}


def alignment_of(field: Description) -> Alignment:
    """The alignment that ``set_table()`` pads a nested table's field to: the
    one passed, or by default 4 for bytes, and for a builder the largest
    alignment of its arrays, of scalars and of structs, and of its nested
    tables, or 1 if it has neither"""
    if "alignment" in field:
        return field["alignment"]

    if field["from"] == "bytes":
        return BYTES_ALIGNMENT

    alignments = [1]
    for nested in field["table"]["fields"]:
        kind = nested["type"]
        if kind == "table":
            alignments.append(alignment_of(nested))
        elif kind == "struct[]":
            alignments.append(struct_alignment(nested["fields"]))
        elif kind.endswith("[]"):
            alignments.append(ELEMENT_SIZES[kind.removesuffix("[]")])

    return cast(Alignment, max(alignments))


def comparable_table(table: Description) -> Description:
    """Returns a table's description, or one that a reader returned, with
    every value in a form that compares exactly: 64-bit integers as ints, and
    floats as the bits of their float64, which a float32 widens to exactly, so
    that -0.0 differs from 0.0 and NaN equals the same NaN. Other values are
    bools, ints and strings in both forms already."""
    return {**table, "fields": [comparable_field(field) for field in table["fields"]]}


def comparable_field(field: Description) -> Description:
    """Returns a field as ``comparable_table()`` returns its fields"""
    kind = field["type"]

    if kind == "table":
        return {**field, "table": comparable_table(field["table"])}

    if kind == "vector":
        items = [
            {**item, "table": comparable_table(item["table"])}
            for item in field["items"]
        ]
        return {**field, "items": items}

    if kind == "struct":
        return {
            **field,
            "fields": [comparable_field(scalar) for scalar in field["fields"]],
        }

    if kind == "struct[]":
        types = field["fields"]
        items = [_comparable_values(types, item) for item in field["items"]]
        return {**field, "items": items}

    if kind.endswith("[]"):
        element = kind.removesuffix("[]")
        convert = _COMPARABLE.get(element)
        if convert is None:
            return field

        return {**field, "value": [convert(value) for value in field["value"]]}

    convert = _COMPARABLE.get(kind)
    return field if convert is None else {**field, "value": convert(field["value"])}


def _comparable_values(types: list[str], values: list[Any]) -> list[Any]:
    """Returns the values of a struct in an array of structs as
    ``comparable_field()`` returns a scalar's"""
    return [
        comparable_field({"type": kind, "value": value})["value"]
        for kind, value in zip(types, values, strict=True)
    ]


def _float_bits(value: Any) -> str:
    """The bits of a float, or of the float that JSON names, as a float64 in
    hex, such as 7ff8000000000000 for NaN"""
    return struct.pack(">d", float(value)).hex()


# The conversions of the values that a description and a reader hold in
# different forms
_COMPARABLE: dict[str, Callable[[Any], int | str]] = {
    "int64": int,
    "uint64": int,
    "float32": _float_bits,
    "float64": _float_bits,
}


class MismatchError(AssertionError):
    """Raised by ``DescriptionReader`` when the bytes hold a table of another shape
    than its description: a size slot that doesn't hold the table's length, or
    a vector of another number of items. It's the description's mismatch,
    rather than tafla's, which raises ``DecodeError``."""


@dataclass(frozen=True, slots=True)
class Placement:
    """Where a reader found a field: at ``position`` in ``data``, in a table
    that starts at ``table`` in ``data`` and whose offsets, counts and lengths
    are of the offset size ``offset_size``"""

    field: Description
    data: memoryview[int]
    table: int
    position: int
    offset_size: int


type Visit = Callable[[Placement], None]


class DescriptionReader:
    """Reads the bytes of a table as descriptions of its fields set them out,
    and returns those descriptions with the values that were read in place of
    theirs, so that they compare equal with ``comparable_table()`` when the
    bytes hold the values.

    A nested table, from ``set_table()`` or in a vector, is read in place when
    it has no header and its parent's offset type. Any other is read as a table
    of its own: a vector item from its bytes, and a field from its position to
    the end of its parent, since its length isn't written.

    A field of ``"type": "absent"`` must have the offset 0, from its table's
    start, and is returned as it is. A table's ``missing``, which only the
    cases of presence have, is replaced by the number of the fields after its
    own that a reader of a later revision finds missing from it, as
    ``__count_missing()`` counts them.

    ``visit`` is called with the placement of every field before it's read,
    nested ones included, but absent ones, which have no bytes.
    """

    __slots__ = ["__header_size", "__offset_type", "__reader", "__visit"]

    __offset_type: OffsetType

    def __init__(
        self, data: Buffer, layout: Description, visit: Visit | None = None
    ) -> None:
        table_layout = layout_of(layout)
        self.__reader = TableReader(data, table_layout)
        self.__header_size = table_layout.header_size()
        self.__offset_type = layout["offsetType"]
        self.__visit = visit

    def read_table(self, table: Description) -> Description:
        """Reads the header and every field of a table from its own bytes,
        which hold that table alone. Raises ``MismatchError`` if its size slot
        doesn't hold its length."""
        reader = self.__reader
        if table["layout"]["sized"] and reader.get_size() != len(reader.bytes()):
            raise MismatchError(
                f"The size slot holds {reader.get_size()}, "
                f"but the table has {len(reader.bytes())} bytes"
            )

        return self.__read_header_and_fields(table)

    def read_root_field(self, index: int, field: Description) -> Description:
        """Reads a field of the root table. Raises ``MismatchError`` if the
        field is described as absent and its offset isn't 0."""
        return self.__read_field(0, self.__reader.root_offset(index), field)

    def __read_header_and_fields(self, table: Description) -> Description:
        layout = table["layout"]

        read: Description = {"layout": layout}
        if layout["versioned"]:
            read["version"] = self.__reader.get_version()

        read["fields"] = [
            self.read_root_field(index, field)
            for index, field in enumerate(table["fields"])
        ]
        if "missing" in table:
            read["missing"] = self.__count_missing(0, table)

        return read

    def __count_missing(self, position: int, table: Description) -> int:
        """Counts the fields after the table's own that it doesn't hold, of
        the ``missing`` fields that a reader of a later revision knows there,
        as SPEC.md's Revisions finds them: a field whose offset would end
        after the table's first value starts, which isn't read, and one whose
        offset is 0, as padding before the first value reads. A table that
        holds one of them counts fewer than ``missing``."""
        reader = self.__reader
        fields = len(table["fields"])
        offset_size = offset_size_of(self.__offset_type)

        # The root's offsets follow its header, and a nested table's count
        # from its own start, where it has no header
        if position == 0:
            header_size = self.__header_size
            first_value = reader.root_offset(0)
        else:
            header_size = 0
            first_value = reader.field_offset(position, 0) - position

        count = 0
        for index in range(fields, fields + table["missing"]):
            # The offset is read only when it ends before the first value
            beyond = header_size + (index + 1) * offset_size > first_value
            if beyond or self.__offset_of(position, index) == 0:
                count += 1

        return count

    def __offset_of(self, position: int, index: int) -> int:
        """The offset of field ``index`` of the table at ``position``, from
        the table's start"""
        if position == 0:
            return self.__reader.root_offset(index)

        return self.__reader.field_offset(position, index) - position

    def __read_field(
        self, table: int, position: int, field: Description
    ) -> Description:
        kind = field["type"]

        # The offset of an absent field is 0 from its table's start
        if kind == "absent":
            if position != table:
                raise MismatchError(
                    f"A field described as absent has the offset {position - table}"
                )
            return field

        if self.__visit is not None:
            placement = Placement(
                field,
                self.__reader.bytes(),
                table,
                position,
                offset_size_of(self.__offset_type),
            )
            self.__visit(placement)

        if kind == "table":
            return {**field, "table": self.__read_nested(position, field["table"])}

        if kind == "vector":
            return {**field, "items": self.__read_vector(position, field["items"])}

        if kind == "struct":
            return {**field, "fields": self.__read_struct(position, field["fields"])}

        if kind == "struct[]":
            return {
                **field,
                "items": self.__read_struct_array(position, field["fields"]),
            }

        if kind.endswith("[]"):
            element = kind.removesuffix("[]")
            values = getattr(self.__reader, f"get_{element}_array")(position)
            return {**field, "value": values.tolist()}

        if kind == "bool":
            return {**field, "value": self.__reader.get_uint8(position) != 0}

        return {**field, "value": getattr(self.__reader, f"get_{kind}")(position)}

    def __read_vector(
        self, position: int, items: list[Description]
    ) -> list[Description]:
        """Reads the items that the vector and its description both hold, and
        then raises ``MismatchError`` if they hold different numbers of
        items"""
        reader = self.__reader
        offsets = reader.get_vector_offsets(position)
        vector = reader.get_vector(position)

        # Finding each item from its ends alone finds the same item
        for index, offset in enumerate(offsets):
            found = reader.get_vector_offset(position, index)
            if found != offset:
                raise MismatchError(
                    f"Item {index} of the vector is at {found} from its ends, "
                    f"but at {offset} among every item"
                )

        read = [
            {**item, "table": self.__read_item(offset, item_bytes, item["table"])}
            for item, offset, item_bytes in zip(items, offsets, vector, strict=False)
        ]

        if len(vector) != len(items):
            raise MismatchError(
                f"The vector holds {len(vector)} items rather than {len(items)}"
            )

        return read

    def __read_struct(
        self, position: int, scalars: list[Description]
    ) -> list[Description]:
        """Reads the scalars of a struct at their offsets in its bytes"""
        packing = struct_of(scalar["type"] for scalar in scalars)
        values = packing.unpack(self.__reader.get_struct(position, packing.size))

        return [
            {**scalar, "value": value}
            for scalar, value in zip(scalars, values, strict=True)
        ]

    def __read_struct_array(self, position: int, types: list[str]) -> list[list[Any]]:
        """Reads the scalars of every struct of an array at their offsets in
        the structs' bytes"""
        packing = struct_of(types)
        data = self.__reader.get_struct_array(position, packing.size)

        return [list(values) for values in packing.iter_unpack(data)]

    def __read_item(
        self, position: int, item_bytes: memoryview[int], table: Description
    ) -> Description:
        if self.__reads_in_place(table):
            return self.__read_in_place(position, table)

        return DescriptionReader(item_bytes, table["layout"], self.__visit).read_table(
            table
        )

    def __read_nested(self, position: int, table: Description) -> Description:
        """Reads a table nested with ``set_table()``"""
        if self.__reads_in_place(table):
            return self.__read_in_place(position, table)

        rest = self.__reader.bytes()[position:]
        description_reader = DescriptionReader(rest, table["layout"], self.__visit)

        return description_reader.__read_header_and_fields(table)

    def __reads_in_place(self, table: Description) -> bool:
        layout = table["layout"]
        return (
            not layout["sized"]
            and not layout["versioned"]
            and layout["offsetType"] == self.__offset_type
        )

    def __read_in_place(self, position: int, table: Description) -> Description:
        reader = self.__reader
        read: Description = {
            "layout": table["layout"],
            "fields": [
                self.__read_field(position, reader.field_offset(position, index), field)
                for index, field in enumerate(table["fields"])
            ],
        }
        if "missing" in table:
            read["missing"] = self.__count_missing(position, table)

        return read


def read_table(
    data: Buffer, table: Description, visit: Visit | None = None
) -> Description:
    """Reads a table from its own bytes, as ``DescriptionReader.read_table()`` does"""
    return DescriptionReader(data, table["layout"], visit).read_table(table)
