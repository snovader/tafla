"""The fields of generated messages, tables and structs in the conformance
suite's description model of ``tests/tables.py``, so that ``build_table()``
writes the same table by hand with ``TableBuilder``, as the suite builds its
tables, and ``comparable_table()`` compares what the generated code reads with
what it wrote."""

from collections.abc import Buffer, Iterable
from struct import pack

from tests.tables import (
    DEFAULT_LAYOUT,
    FORMATS,
    Description,
    OffsetType,
    from_json,
    to_json,
)


def layout(
    *,
    sized: bool = False,
    versioned: bool = False,
    offset_type: OffsetType = "uint32",
) -> Description:
    """A layout, as a message's attributes set it"""
    return {"sized": sized, "versioned": versioned, "offsetType": offset_type}


def scalar(kind: str, value: float) -> Description:
    return {"type": kind, "value": to_json(kind, value)}


def boolean(value: bool) -> Description:
    """A bool, which tables hold as the uint8 1 or 0"""
    return {"type": "bool", "value": value}


def typed_array(kind: str, values: Buffer | Iterable[float]) -> Description:
    """An array of ``kind``'s elements, from any values that the generated code
    takes for it"""
    elements = memoryview(values).tolist() if isinstance(values, Buffer) else values
    return {"type": f"{kind}[]", "value": [to_json(kind, value) for value in elements]}


def bool_array(values: Iterable[bool]) -> Description:
    """An array of bools, which tables hold as a uint8 array of 1 and 0"""
    return {"type": "bool[]", "value": list(values)}


def string(value: str) -> Description:
    return {"type": "string", "value": value}


def struct(fields: list[Description]) -> Description:
    """A struct of scalar fields, which lie one after another in its bytes"""
    return {"type": "struct", "fields": fields}


def struct_array(types: list[str], items: Iterable[list[Description]]) -> Description:
    """An array of structs of scalars of these types, from each struct's scalar
    fields, as ``struct()`` takes them"""
    return {
        "type": "struct[]",
        "fields": types,
        "items": [[field["value"] for field in fields] for fields in items],
    }


def table(fields: list[Description]) -> Description:
    """A table nested from its builder at its default alignment, as the
    generated code nests it"""
    return {
        "type": "table",
        "from": "builder",
        "table": {"layout": DEFAULT_LAYOUT, "fields": fields},
    }


def vector(items: Iterable[list[Description]]) -> Description:
    """A vector of tables from their builders, as the generated code writes it"""
    return {
        "type": "vector",
        "items": [
            {"from": "builder", "table": {"layout": DEFAULT_LAYOUT, "fields": fields}}
            for fields in items
        ],
    }


def message(
    layout: Description, fields: list[Description], version: int | None = None
) -> Description:
    """A message's table, with its version when its layout is versioned"""
    if version is None:
        return {"layout": layout, "fields": fields}

    return {"layout": layout, "version": version, "fields": fields}


def struct_bytes(fields: list[Description]) -> bytes:
    """The bytes of a struct of scalar fields, each little-endian right after
    the one before, packed one by one rather than with the generated code's
    format"""
    packed = bytearray()
    for field in fields:
        kind = field["type"]
        packed += pack(f"<{FORMATS[kind]}", from_json(kind, field["value"]))

    return bytes(packed)
