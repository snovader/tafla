from array import array
from collections.abc import Buffer, Callable, Sequence
from typing import Any, Protocol

import msgpack
import msgpack.fallback

from tafla_benchmarks.codecs.codec import Codec, DocumentWorkload, Workload
from tafla_benchmarks.workloads import (
    DocumentInput,
    DocumentOutput,
    ItemInput,
    ListInput,
    RecordInput,
    SampleInput,
)

# The extension types of the arrays that columns pack as. Their elements are in
# the host's byte order, as msgpackr packs typed arrays.
_UINT32_ARRAY = 1
_FLOAT32_ARRAY = 2


class _Packer(Protocol):
    """A ``Packer`` of either of msgpack's builds"""

    def pack(self, obj: object, /) -> bytes: ...


class _Unpackb(Protocol):
    """``unpackb()`` of either of msgpack's builds"""

    def __call__(
        self, packed: Buffer, /, *, ext_hook: Callable[[int, bytes], object]
    ) -> Any: ...


def _pack_array(value: object) -> msgpack.ExtType:
    """Packs an array of a column's type as an extension of its bytes, as
    msgpack's documentation packs an array"""
    if isinstance(value, array):
        if value.typecode == "I":
            return msgpack.ExtType(_UINT32_ARRAY, value.tobytes())
        if value.typecode == "f":
            return msgpack.ExtType(_FLOAT32_ARRAY, value.tobytes())

    raise TypeError(f"Can't pack {type(value).__name__}")


def _unpack_array(ext_type: int, data: bytes) -> memoryview[Any] | msgpack.ExtType:
    """Reads a column's extension as a view of its bytes, which the unpacker
    copied out of the message. That took half the time of copying them again
    into an array, as msgpack's documentation does, for 100,000 rows."""
    if ext_type == _UINT32_ARRAY:
        return memoryview(data).cast("I")
    if ext_type == _FLOAT32_ARRAY:
        return memoryview(data).cast("f")

    return msgpack.ExtType(ext_type, data)


def _as_array[T: (int, float)](type_code: str, values: Sequence[T]) -> array[T]:
    if isinstance(values, array) and values.typecode == type_code:
        return values

    return array(type_code, values)


def _item_map(input: ItemInput) -> dict[str, float]:
    return {"value": input.value, "first": input.first, "second": input.second}


class _Encoding:
    """The workloads' encoding with one build of msgpack: its packer and its
    ``unpackb()``"""

    __slots__ = ("_pack", "_unpackb")

    def __init__(self, packer: Callable[..., _Packer], unpackb: _Unpackb) -> None:
        # One packer serves every message, as msgpackr's does. It returns each
        # message as new bytes.
        self._pack = packer(default=_pack_array).pack
        self._unpackb = unpackb

    def _unpack(self, data: Buffer) -> Any:
        return self._unpackb(data, ext_hook=_unpack_array)

    def serialize_record(self, input: RecordInput) -> bytes:
        return self._pack(
            {"x": input.x, "y": input.y, "id": input.id, "name": input.name}
        )

    def deserialize_record(self, data: Buffer) -> RecordInput:
        record = self._unpack(data)
        return RecordInput(record["x"], record["y"], record["id"], record["name"])

    def serialize_document(self, input: DocumentInput) -> bytes:
        """Packs the document as one map, with list columns converted to arrays
        first, as a caller who wants float32s on the wire would"""
        return self._pack(
            {
                "x": input.x,
                "y": input.y,
                "name": input.name,
                "base": input.base,
                "keys": _as_array("I", input.keys),
                "a": _as_array("f", input.a),
                "b": _as_array("f", input.b),
            }
        )

    def deserialize_document(self, data: Buffer) -> DocumentOutput:
        """Reads the columns as the views that the extension hook returns"""
        document = self._unpack(data)

        return DocumentOutput(
            document["x"],
            document["y"],
            document["name"],
            document["base"],
            document["keys"],
            document["a"],
            document["b"],
        )

    def sum_a(self, data: Buffer) -> float:
        a = self._unpack(data)["a"]

        total = 0.0
        for i in range(len(a)):
            total += a[i]

        return total

    def serialize_list(self, input: ListInput) -> bytes:
        return self._pack({"items": [_item_map(item) for item in input.items]})

    def deserialize_list(self, data: Buffer) -> ListInput:
        items = self._unpack(data)["items"]
        return ListInput(
            [ItemInput(i["value"], i["first"], i["second"]) for i in items]
        )

    def serialize_structs(self, input: ListInput) -> bytes:
        """Packs each struct as an array of its fields in order, without their
        keys, and the table that holds them as an array too, as rmp-serde's
        ``to_vec()`` packs both"""
        return self._pack([[(i.value, i.first, i.second) for i in input.items]])

    def deserialize_structs(self, data: Buffer) -> ListInput:
        [items] = self._unpack(data)
        return ListInput([ItemInput(*fields) for fields in items])

    def serialize_sample(self, input: SampleInput) -> bytes:
        return self._pack({"key": input.key, "a": input.a, "b": input.b})

    def deserialize_sample(self, data: Buffer) -> SampleInput:
        sample = self._unpack(data)
        return SampleInput(sample["key"], sample["a"], sample["b"])


def _codec(name: str, packer: Callable[..., _Packer], unpackb: _Unpackb) -> Codec:
    """Returns the codec of one build of msgpack, from its ``Packer`` class and
    its ``unpackb()``"""
    encoding = _Encoding(packer, unpackb)

    return Codec(
        name=name,
        record=Workload(encoding.serialize_record, encoding.deserialize_record),
        document=DocumentWorkload(
            encoding.serialize_document, encoding.deserialize_document, encoding.sum_a
        ),
        list=Workload(encoding.serialize_list, encoding.deserialize_list),
        structs=Workload(encoding.serialize_structs, encoding.deserialize_structs),
        sample=Workload(encoding.serialize_sample, encoding.deserialize_sample),
    )


MSGPACK = _codec("msgpack", msgpack.Packer, msgpack.unpackb)
"""msgpack's packer and ``unpackb()``, in its C extension. MessagePack is
schemaless: a table packs as a map with its keys, as msgpackr packs an object,
and floats as float64s, which is how msgpack packs Python's floats. Columns
pack as extensions of their arrays' bytes, as msgpackr packs typed arrays, and
read back as views of the bytes that the unpacker copies out."""

MSGPACK_PURE = _codec(
    "msgpack (pure)", msgpack.fallback.Packer, msgpack.fallback.unpackb
)
"""The same encoding in msgpack's pure-Python build, its ``fallback`` module,
which msgpack imports where its C extension isn't built. It packs the same
bytes as the C extension."""
