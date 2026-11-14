"""Builders that write each field into a ``bytearray`` when it's set, with
``tafla.TableBuilder``'s API, bytes and errors, as prototypes beside it.
``TableBuilder`` records each field and writes them all through a
``TableWriter`` in ``bytes()``.

Two growth strategies, each with its setters written out for it, since a
helper's call for each write is what the comparison would time:

- ``DirectTableBuilder`` appends to a ``bytearray``, which ``+=`` grows in
  place with CPython's over-allocation, and ``bytes()`` returns a view of it.
- ``PreallocatedTableBuilder`` writes with ``Struct.pack_into()`` and slice
  assignment into room that it allocates ahead, 128 bytes and then at least
  twice as many, copied into a new ``bytearray`` each time, and ``bytes()``
  truncates it to the table's length.

Both write the same bytes, in the same way:

- A field's offset is written when it's set, from the bytes written so far.
- A vector writes its count and zeros for its ends, and each item's end once
  the item is written.
- A table nested in place is written where its parent ends, unpadded. Once
  ``fill`` has set its fields, its alignment is known, and it's moved forward
  by its padding. Its offsets count from its own start, so none changes.
- Runs of ``SEGMENT_BYTES`` bytes or more, an array's elements, a string's
  UTF-8, a struct array's bytes or a table nested as bytes, are kept as
  segments: references to the caller's buffer, or to the converted copy,
  which ``bytes()`` joins with the bytes between them, so that a large array
  is copied once, into the table. The ``Copying`` variants keep none.
- A nested builder is copied into its parent as its bytes and segments.
- A setter checks its field before it writes anything. A nested table or
  vector whose ``fill`` raises, or that is incomplete or too long, is undone
  by truncating the bytes and segments and restoring the counters.

What differs from ``TableBuilder``:

- A scalar that its type can't hold raises ``ValueError`` from its setter,
  rather than from ``bytes()``.
- A buffer shorter than ``SEGMENT_BYTES`` is copied when it's set, so a later
  change to it isn't written. A longer one is written as it is when the table
  is, and a change of its length raises ``ValueError`` from ``bytes()``.
- ``bytes()`` or ``finalize()`` within a ``fill`` raises the ``ValueError``
  that the in-place setter would raise once ``fill`` returned.
"""

import sys
from array import array
from collections.abc import Buffer, Callable, Collection, Iterable, Sequence
from struct import Struct, error as StructError
from typing import Any, ClassVar, Literal, overload

from tafla import ByteBuffer, TableLayout, elements as _elements
from tafla.constants import SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64
from tafla.elements import as_elements, byte_view, little_endian_bytes, struct_count
from tafla.layout import MAX_LENGTHS, header_length, padding_to
from tafla.structs import (
    OFFSET_STRUCTS,
    float32,
    float64,
    int8,
    int16,
    int32,
    int64,
    uint8,
    uint16,
    uint32,
    uint64,
)
from tafla.types import ElementSize, TypeCode

SEGMENT_BYTES = 256
"""The shortest run that a builder keeps as a segment rather than copying it
when it's set, as the C++ prototype keeps"""

# Zeros by their number, for padding and for slots and ends until they're
# written, as TableWriter appends them
_ZEROS = tuple(bytes(length) for length in range(16 * SIZEOF_32 + 1))

# typeshed's bytearray takes bytes for a slice, where it takes any buffer, so
# ty is told to ignore the buffers that the preallocated builder assigns,
# rather than a cast() adding a call to each

# Comparisons rather than max() for alignments, and counted indices rather
# than enumerate(), as in tafla.TableBuilder, which found them faster

# What Struct.pack() raises for a value that its format can't hold
_VALUE_ERRORS = (StructError, OverflowError)

# The alignment of a table before any field needs one
_UNALIGNED: ElementSize = SIZEOF_8

type _Segment = tuple[int, Buffer, int]
"""A run that bytes() copies in: where it goes among the builder's own bytes,
the buffer that holds it, and its length when it was set"""


class DirectTableBuilder:
    """Builds a table as ``tafla.TableBuilder`` does, writing each field into a
    ``bytearray`` that grows in place when it's set; see the module's
    docstring"""

    __slots__ = [
        "__alignment",
        "__base",
        "__data",
        "__fields",
        "__header_size",
        "__index",
        "__layout",
        "__length",
        "__max_length",
        "__segments",
        "__shift",
        "__size",
        "__sized",
        "__slots",
        "__written",
    ]

    _segment_bytes: ClassVar[int] = SEGMENT_BYTES

    # The bytes written so far, without the segments
    __data: bytearray
    # The segments' bytes so far, which come before the end of the bytes, so
    # that a position in the table is a position in the bytes plus those of
    # the segments before it
    __shift: int
    __segments: list[_Segment] | None
    # Where the table being written starts in the table, less the segments'
    # bytes so far, so that a field that starts at the end of the bytes has
    # the offset ``len(data) - base``
    __base: int
    # The position in the bytes of its first slot, its number of fields, the
    # number set so far, and the largest alignment that a field needs
    __slots: int
    __fields: int
    __index: int
    __alignment: ElementSize
    __written: memoryview[int] | ByteBuffer | None

    @overload
    def __init__(
        self, fields: int, layout: TableLayout[Literal[False]] = TableLayout.DEFAULT
    ) -> None: ...

    @overload
    def __init__(
        self, fields: int, layout: TableLayout[Literal[True]], version: int
    ) -> None: ...

    def __init__(
        self,
        fields: int,
        layout: TableLayout = TableLayout.DEFAULT,
        version: int | None = None,
    ) -> None:
        length = header_length(fields, layout, version)
        size = layout.offset_size()
        sized = layout.sized()
        header_size = layout.header_size()
        data = bytearray(length)

        self.__size = size
        self.__length = OFFSET_STRUCTS[size]
        self.__max_length = MAX_LENGTHS[size]
        self.__layout = layout
        self.__sized = sized
        self.__header_size = header_size
        self.__data = data
        self.__shift = 0
        self.__segments = None
        self.__base = 0
        self.__slots = header_size
        self.__fields = fields
        self.__index = 0
        self.__alignment = SIZEOF_8
        self.__written = None

        if version is not None:
            self.__length.pack_into(data, size if sized else 0, version)

    def finalize(self) -> ByteBuffer:
        written = self.__written
        if written is None:
            written = self.__write()
        elif isinstance(written, ByteBuffer):
            return written

        buffer = ByteBuffer(written, self.__layout)
        self.__written = buffer

        return buffer

    def bytes(self) -> memoryview[int]:
        written = self.__written
        if written is None:
            return self.__write()

        return written.bytes() if isinstance(written, ByteBuffer) else written

    def __write(self) -> memoryview[int]:
        """Writes the sized header's length, and joins the bytes with the
        segments, if there are any"""
        # Only a table nested in place has its slots elsewhere than after the
        # root's header
        if self.__slots != self.__header_size:
            raise _written_in_place_error()
        if self.__index != self.__fields:
            raise _unset_fields_error(self.__index, self.__fields, "the table's")

        data = self.__data
        if self.__sized:
            self.__length.pack_into(data, 0, len(data) + self.__shift)

        segments = self.__segments
        written = memoryview(data) if segments is None else self.__join(segments)
        self.__segments = None
        self.__written = written

        return written

    def __join(self, segments: list[_Segment]) -> memoryview[int]:
        """Copies the bytes and the segments between them into the table.
        Raises ``ValueError`` if a segment's length changed since it was set.
        """
        data = self.__data
        view = memoryview(data)
        pieces: list[Buffer] = []
        changed = 0
        position = 0
        for at, buffer, byte_length in segments:
            segment = memoryview(buffer)
            changed += segment.nbytes - byte_length
            pieces.append(view[position:at])
            pieces.append(segment)
            position = at
        pieces.append(view[position:])

        if changed:
            counted = len(data) + self.__shift
            raise ValueError(
                f"The fields wrote {counted + changed} bytes rather than the "
                f"{counted} they counted when they were set"
            )

        return memoryview(bytearray().join(pieces))

    # Each scalar setter is written out, from a closure over its Struct, since
    # a helper's call would add to every field. It packs its value before it
    # writes the offset, so that a setter that raises writes nothing.

    def _scalar_setter(packing: Struct) -> Callable[[DirectTableBuilder, Any], None]:
        pack = packing.pack
        size = packing.size

        def set_scalar(self: DirectTableBuilder, value: Any) -> None:
            index = self.__index
            if index == self.__fields:
                raise self.__full()

            data = self.__data
            offset = len(data) - self.__base
            if offset + size > self.__max_length:
                raise self.__too_long(index, offset + size)

            try:
                packed = pack(value)
            except _VALUE_ERRORS as error:
                raise self.__value_error(value, error) from error

            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
            data += packed
            self.__index = index + 1

        return set_scalar

    set_int8 = _scalar_setter(int8)
    set_uint8 = _scalar_setter(uint8)
    set_int16 = _scalar_setter(int16)
    set_uint16 = _scalar_setter(uint16)
    set_int32 = _scalar_setter(int32)
    set_uint32 = _scalar_setter(uint32)
    set_int64 = _scalar_setter(int64)
    set_uint64 = _scalar_setter(uint64)
    set_float32 = _scalar_setter(float32)
    set_float64 = _scalar_setter(float64)
    del _scalar_setter

    def set_int8_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("b", SIZEOF_8, values)

    def set_uint8_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("B", SIZEOF_8, values)

    def set_int16_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("h", SIZEOF_16, values)

    def set_uint16_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("H", SIZEOF_16, values)

    def set_int32_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("i", SIZEOF_32, values)

    def set_uint32_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("I", SIZEOF_32, values)

    def set_int64_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("q", SIZEOF_64, values)

    def set_uint64_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("Q", SIZEOF_64, values)

    def set_float32_array(self, values: Buffer | Iterable[float]) -> None:
        self.__set_array("f", SIZEOF_32, values)

    def set_float64_array(self, values: Buffer | Iterable[float]) -> None:
        self.__set_array("d", SIZEOF_64, values)

    def set_string(self, string: str) -> None:
        encoded = string.encode()
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        data = self.__data
        size = self.__size
        byte_length = len(encoded)
        offset = len(data) - self.__base
        length = offset + size + byte_length
        if length > self.__max_length:
            raise self.__too_long(index, length)

        self.__length.pack_into(data, self.__slots + index * size, offset)
        data += self.__length.pack(byte_length)
        if byte_length >= self._segment_bytes:
            self.__hold(encoded, byte_length)
        else:
            data += encoded
        self.__index = index + 1

    def set_struct(self, data: Buffer) -> None:
        view = byte_view(data)
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        buffer = self.__data
        offset = len(buffer) - self.__base
        length = offset + view.nbytes
        if length > self.__max_length:
            raise self.__too_long(index, length)

        self.__length.pack_into(buffer, self.__slots + index * self.__size, offset)
        buffer += view
        self.__index = index + 1

    def set_struct_array(self, data: Buffer, size: int, alignment: ElementSize) -> None:
        byte_length = byte_view(data).nbytes
        count = struct_count(byte_length, size)
        buffer = self.__data
        offset_size = self.__size
        offset = len(buffer) - self.__base
        padding = padding_to(offset + offset_size, alignment)

        index = self.__index
        if index == self.__fields:
            raise self.__full()

        length = offset + padding + offset_size + byte_length
        if length > self.__max_length:
            raise self.__too_long(index, length)

        if padding:
            buffer += _ZEROS[padding]
        self.__length.pack_into(
            buffer, self.__slots + index * offset_size, offset + padding
        )
        buffer += self.__length.pack(count)
        if byte_length >= self._segment_bytes:
            self.__hold(data, byte_length)
        else:
            buffer += data
        self.__index = index + 1
        if alignment > self.__alignment:  # noqa: PLR1730
            self.__alignment = alignment

    def set_table(
        self, table: DirectTableBuilder | Buffer, alignment: ElementSize | None = None
    ) -> None:
        if isinstance(table, DirectTableBuilder):
            byte_length = DirectTableBuilder.__length_of(table)
            default_alignment = table.__alignment
        else:
            byte_length = byte_view(table).nbytes
            default_alignment = SIZEOF_32

        if alignment is None:
            alignment = default_alignment

        data = self.__data
        offset = len(data) - self.__base
        padding = padding_to(offset, alignment)
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        length = offset + padding + byte_length
        if length > self.__max_length:
            raise self.__too_long(index, length)

        if padding:
            data += _ZEROS[padding]
        self.__length.pack_into(
            data, self.__slots + index * self.__size, offset + padding
        )
        self.__append_table(table, byte_length)
        self.__index = index + 1
        if alignment > self.__alignment:  # noqa: PLR1730
            self.__alignment = alignment

    def set_vector(self, vector: Sequence[DirectTableBuilder | Buffer]) -> None:
        items = tuple(vector)
        size = self.__size
        lengths = [DirectTableBuilder.__length_of(item) for item in items]
        byte_length = size * (1 + len(items)) + sum(lengths)

        index = self.__index
        if index == self.__fields:
            raise self.__full()

        data = self.__data
        offset = len(data) - self.__base
        length = offset + byte_length
        if length > self.__max_length:
            raise self.__too_long(index, length)

        packing = self.__length
        packing.pack_into(data, self.__slots + index * size, offset)
        vector_at = len(data)
        start = vector_at + self.__shift
        ends = len(items) * size
        data += packing.pack(len(items))
        data += _ZEROS[ends] if ends < len(_ZEROS) else bytes(ends)

        item = 0
        for table in items:
            self.__append_table(table, lengths[item])
            item += 1  # noqa: SIM113
            packing.pack_into(
                data, vector_at + item * size, len(data) + self.__shift - start
            )
        self.__index = index + 1

    # The in-place setters write the nested table, or each item, at the end
    # of the bytes, with the builder's counters set to it, and restore its
    # parent's once fill has set its fields. A nested table's segments move
    # the parent's base, which is restored from where the parent starts.

    def set_table_in_place[*Args](
        self,
        fill: Callable[[DirectTableBuilder, *Args], object],
        *args: *Args,
        fields: int,
    ) -> None:
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        data = self.__data
        start = len(data)
        shift = self.__shift
        segments = self.__segments
        held = 0 if segments is None else len(segments)
        base = self.__base
        slots = self.__slots
        parent_fields = self.__fields
        parent_alignment = self.__alignment
        size = self.__size

        self.__base = start
        self.__slots = start
        self.__fields = fields
        self.__index = 0
        self.__alignment = _UNALIGNED
        try:
            slot_bytes = fields * size
            if slot_bytes < len(_ZEROS):
                data += _ZEROS[slot_bytes]
            else:
                self.__start_slots(slot_bytes)
            fill(self, *args)
            if self.__index != fields:
                raise _unset_fields_error(self.__index, fields, "the nested table's")
        except BaseException:
            self.__roll_back(
                start, shift, held, base, slots, parent_fields, index, parent_alignment
            )
            raise

        alignment = self.__alignment
        shift_now = self.__shift
        # Padded from its parent's start, as a nested builder is
        offset = start - base
        padding = -offset % alignment
        length = offset + padding + len(data) + shift_now - start - shift
        if length > self.__max_length:
            self.__roll_back(
                start, shift, held, base, slots, parent_fields, index, parent_alignment
            )
            raise self.__too_long(index, length)

        if padding:
            data[start:start] = _ZEROS[padding]
            if self.__segments is not None:
                self.__move_segments(held, padding)
        self.__length.pack_into(data, slots + index * size, offset + padding)

        self.__base = base + shift - shift_now
        self.__slots = slots
        self.__fields = parent_fields
        self.__index = index + 1
        self.__alignment = max(alignment, parent_alignment)

    def set_vector_in_place[T](
        self,
        values: Collection[T],
        fill: Callable[[DirectTableBuilder, T], object],
        *,
        fields: int,
    ) -> None:
        count = len(values)
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        data = self.__data
        size = self.__size
        max_length = self.__max_length
        offset = len(data) - self.__base
        length = offset + size * (1 + count)
        if length > max_length:
            raise self.__too_long(index, length)

        start = len(data)
        shift = self.__shift
        segments = self.__segments
        held = 0 if segments is None else len(segments)
        base = self.__base
        slots = self.__slots
        parent_fields = self.__fields
        alignment = self.__alignment
        packing = self.__length
        vector = start + shift
        ends = count * size
        slot_bytes = fields * size

        data += packing.pack(count)
        data += _ZEROS[ends] if ends < len(_ZEROS) else bytes(ends)
        self.__fields = fields
        try:
            # Each item counts its length and alignment from its own start,
            # as a builder does
            item = 0
            for value in values:
                if item == count:
                    raise _count_error(count)

                item_start = len(data)
                self.__base = item_start
                self.__slots = item_start
                self.__index = 0
                self.__alignment = _UNALIGNED
                if slot_bytes < len(_ZEROS):
                    data += _ZEROS[slot_bytes]
                else:
                    self.__start_slots(slot_bytes)
                fill(self, value)
                if self.__index != fields:
                    raise _unset_fields_error(self.__index, fields, f"item {item}'s")

                item += 1
                # An end that its slot can't hold makes the table too long,
                # which is raised once every item is written
                end = len(data) + self.__shift - vector
                if end <= max_length:
                    packing.pack_into(data, start + item * size, end)

            if item != count:
                raise _count_error(count)
        except BaseException:
            self.__roll_back(
                start, shift, held, base, slots, parent_fields, index, alignment
            )
            raise

        shift_now = self.__shift
        length = len(data) + shift_now - base - shift
        if length > max_length:
            self.__roll_back(
                start, shift, held, base, slots, parent_fields, index, alignment
            )
            raise self.__too_long(index, length)

        packing.pack_into(data, slots + index * size, offset)
        self.__base = base + shift - shift_now
        self.__slots = slots
        self.__fields = parent_fields
        self.__index = index + 1
        self.__alignment = alignment

    def __start_slots(self, slot_bytes: int) -> None:
        """Writes zeros for a nested table's slots. Slots longer than the
        offset type addresses make every field too long, so they're counted
        as bytes of the table without being written."""
        if slot_bytes <= self.__max_length:
            self.__data += (
                _ZEROS[slot_bytes] if slot_bytes < len(_ZEROS) else bytes(slot_bytes)
            )
        else:
            self.__shift += slot_bytes
            self.__base -= slot_bytes

    def __roll_back(
        self,
        start: int,
        shift: int,
        held: int,
        base: int,
        slots: int,
        fields: int,
        index: int,
        alignment: ElementSize,
    ) -> None:
        """Truncates the bytes to ``start`` and the segments to ``held``, and
        restores the counters"""
        del self.__data[start:]
        segments = self.__segments
        if segments is not None:
            del segments[held:]
        self.__shift = shift
        self.__base = base
        self.__slots = slots
        self.__fields = fields
        self.__index = index
        self.__alignment = alignment

    def __move_segments(self, held: int, padding: int) -> None:
        """Moves the segments from ``held`` on, a nested table's, forward by
        its padding"""
        segments = self.__segments
        assert segments is not None
        for i in range(held, len(segments)):
            at, buffer, byte_length = segments[i]
            segments[i] = (at + padding, buffer, byte_length)

    def __set_array(
        self,
        type_code: TypeCode,
        item_size: ElementSize,
        values: Buffer | Iterable[float],
    ) -> None:
        """Pads the array so that its elements, after the length, are aligned"""
        elements = as_elements(type_code, values)
        if type(elements) is array:
            byte_length = len(elements) * elements.itemsize
        else:
            byte_length = memoryview(elements).nbytes

        index = self.__index
        if index == self.__fields:
            raise self.__full()

        data = self.__data
        size = self.__size
        offset = len(data) - self.__base
        padding = -(offset + size) % item_size
        length = offset + padding + size + byte_length
        if length > self.__max_length:
            raise self.__too_long(index, length)

        if not _elements.IS_LITTLE_ENDIAN:
            elements = little_endian_bytes(type_code, elements)

        if padding:
            data += _ZEROS[padding]
        self.__length.pack_into(data, self.__slots + index * size, offset + padding)
        data += self.__length.pack(byte_length // item_size)
        if byte_length >= self._segment_bytes:
            self.__hold(elements, byte_length)
        else:
            data += elements
        self.__index = index + 1
        if item_size > self.__alignment:  # noqa: PLR1730
            self.__alignment = item_size

    def __hold(self, buffer: Buffer, byte_length: int) -> None:
        """Keeps a run as a segment at the end of the bytes"""
        segments = self.__segments
        if segments is None:
            segments = self.__segments = []
        segments.append((len(self.__data), buffer, byte_length))
        self.__shift += byte_length
        self.__base -= byte_length

    def __append_table(
        self, table: DirectTableBuilder | Buffer, byte_length: int
    ) -> None:
        """Appends a nested table of ``byte_length`` bytes: a builder's bytes
        and segments, or bytes"""
        if isinstance(table, DirectTableBuilder):
            written = table.__written
            if written is None:
                self.__append_builder(table)
                return
            table = written.bytes() if isinstance(written, ByteBuffer) else written

        if byte_length >= self._segment_bytes:
            self.__hold(table, byte_length)
        else:
            self.__data += table

    def __append_builder(self, nested: DirectTableBuilder) -> None:
        """Copies a complete builder's bytes, with the length in a sized
        header, and its segments"""
        data = self.__data
        start = len(data)
        data += nested.__data
        if nested.__sized:
            nested.__length.pack_into(data, start, len(nested.__data) + nested.__shift)

        held = nested.__segments
        if held is None:
            return

        segments = self.__segments
        if segments is None:
            segments = self.__segments = []
        for at, buffer, byte_length in held:
            segments.append((start + at, buffer, byte_length))
        self.__shift += nested.__shift
        self.__base -= nested.__shift

    @staticmethod
    def __length_of(table: DirectTableBuilder | Buffer) -> int:
        """The byte length of a table to nest, which a builder knows once all
        its fields are set. Raises ``ValueError`` if it has fields left to
        set."""
        if not isinstance(table, DirectTableBuilder):
            return byte_view(table).nbytes

        written = table.__written
        if written is not None:
            return len(written.bytes() if isinstance(written, ByteBuffer) else written)

        if table.__index != table.__fields:
            raise _unset_fields_error(table.__index, table.__fields, "the table's")
        return len(table.__data) + table.__shift

    def __full(self) -> Exception:
        """The error of a field that the table has no room for: it's
        finalized, or holds all its fields"""
        if self.__written is not None:
            return _finalized_error()
        return _field_count_error(self.__index)

    def __too_long(self, index: int, length: int) -> ValueError:
        return ValueError(
            f"Field {index + 1} makes the table {length} bytes, "
            f"too long for {self.__layout.offset_type()} offsets"
        )

    def __value_error(self, value: object, error: Exception) -> ValueError:
        """The error that ``TableWriter`` raises for a scalar that its type
        can't hold"""
        position = len(self.__data) + self.__shift
        return ValueError(f"Can't write {value!r} at {position}: {error}")


class CopyingTableBuilder(DirectTableBuilder):
    """``DirectTableBuilder``, copying every run when it's set rather than
    keeping any as a segment"""

    __slots__ = ()

    _segment_bytes: ClassVar[int] = sys.maxsize


def _finalized_error() -> TypeError:
    return TypeError("The table was already finalized")


def _field_count_error(fields: int) -> ValueError:
    return ValueError(f"Can't add field {fields + 1} to a table of {fields}")


def _unset_fields_error(set_fields: int, fields: int, whose: str) -> ValueError:
    return ValueError(f"Only {set_fields} of {whose} {fields} fields were set")


def _count_error(count: int) -> ValueError:
    return ValueError(f"Iterating the values gave other than the {count} of len()")


def _written_in_place_error() -> ValueError:
    return ValueError("The builder was written while it recorded a nested table")


_INITIAL_CAPACITY = 128
"""The room that ``PreallocatedTableBuilder`` allocates first, as the C++
prototype holds within its builder"""

_LEAST_GROWTH = 512
"""The least room that ``PreallocatedTableBuilder`` grows to, as the C++
prototype's first heap buffer"""


class PreallocatedTableBuilder:
    """Builds a table as ``tafla.TableBuilder`` does, writing each field when
    it's set into room allocated ahead, which is copied into a new
    ``bytearray`` of at least twice its length when it runs out; see the
    module's docstring. Its counters are ``DirectTableBuilder``'s, with the
    end of the bytes written kept as ``end``, since ``len(data)`` is the room.
    Bytes past the end are scratch, so every zero is written."""

    __slots__ = [
        "__alignment",
        "__base",
        "__data",
        "__end",
        "__fields",
        "__header_size",
        "__index",
        "__layout",
        "__length",
        "__max_length",
        "__segments",
        "__shift",
        "__size",
        "__sized",
        "__slots",
        "__written",
    ]

    _segment_bytes: ClassVar[int] = SEGMENT_BYTES

    __data: bytearray
    __end: int
    __shift: int
    __segments: list[_Segment] | None
    __base: int
    __slots: int
    __fields: int
    __index: int
    __alignment: ElementSize
    __written: memoryview[int] | ByteBuffer | None

    @overload
    def __init__(
        self, fields: int, layout: TableLayout[Literal[False]] = TableLayout.DEFAULT
    ) -> None: ...

    @overload
    def __init__(
        self, fields: int, layout: TableLayout[Literal[True]], version: int
    ) -> None: ...

    def __init__(
        self,
        fields: int,
        layout: TableLayout = TableLayout.DEFAULT,
        version: int | None = None,
    ) -> None:
        length = header_length(fields, layout, version)
        size = layout.offset_size()
        sized = layout.sized()
        header_size = layout.header_size()
        data = bytearray(max(length, _INITIAL_CAPACITY))

        self.__size = size
        self.__length = OFFSET_STRUCTS[size]
        self.__max_length = MAX_LENGTHS[size]
        self.__layout = layout
        self.__sized = sized
        self.__header_size = header_size
        self.__data = data
        self.__end = length
        self.__shift = 0
        self.__segments = None
        self.__base = 0
        self.__slots = header_size
        self.__fields = fields
        self.__index = 0
        self.__alignment = SIZEOF_8
        self.__written = None

        if version is not None:
            self.__length.pack_into(data, size if sized else 0, version)

    def finalize(self) -> ByteBuffer:
        written = self.__written
        if written is None:
            written = self.__write()
        elif isinstance(written, ByteBuffer):
            return written

        buffer = ByteBuffer(written, self.__layout)
        self.__written = buffer

        return buffer

    def bytes(self) -> memoryview[int]:
        written = self.__written
        if written is None:
            return self.__write()

        return written.bytes() if isinstance(written, ByteBuffer) else written

    def __write(self) -> memoryview[int]:
        if self.__slots != self.__header_size:
            raise _written_in_place_error()
        if self.__index != self.__fields:
            raise _unset_fields_error(self.__index, self.__fields, "the table's")

        data = self.__data
        end = self.__end
        if self.__sized:
            self.__length.pack_into(data, 0, end + self.__shift)

        segments = self.__segments
        if segments is None:
            # Truncated in place, which reallocates the bytes to their length
            # only if that's less than half the room
            del data[end:]
            written = memoryview(data)
        else:
            written = self.__join(segments)
        self.__segments = None
        self.__written = written

        return written

    def __join(self, segments: list[_Segment]) -> memoryview[int]:
        data = self.__data
        end = self.__end
        view = memoryview(data)
        pieces: list[Buffer] = []
        changed = 0
        position = 0
        for at, buffer, byte_length in segments:
            segment = memoryview(buffer)
            changed += segment.nbytes - byte_length
            pieces.append(view[position:at])
            pieces.append(segment)
            position = at
        pieces.append(view[position:end])

        if changed:
            counted = end + self.__shift
            raise ValueError(
                f"The fields wrote {counted + changed} bytes rather than the "
                f"{counted} they counted when they were set"
            )

        return memoryview(bytearray().join(pieces))

    def __grow(self, required: int) -> bytearray:
        """Copies the bytes into a new ``bytearray`` of at least ``required``
        bytes, and at least twice the room"""
        data = self.__data
        end = self.__end
        grown = bytearray(max(required, 2 * len(data), _LEAST_GROWTH))
        grown[:end] = memoryview(data)[:end]
        self.__data = grown

        return grown

    # A scalar is packed straight into the room, which pack_into() raises
    # for if it's too short, as it raises for a value that the type can't
    # hold, so that a scalar that fits costs no check of the room

    def _scalar_setter(
        packing: Struct,
    ) -> Callable[[PreallocatedTableBuilder, Any], None]:
        pack_into = packing.pack_into
        size = packing.size

        def set_scalar(self: PreallocatedTableBuilder, value: Any) -> None:
            index = self.__index
            if index == self.__fields:
                raise self.__full()

            end = self.__end
            offset = end - self.__base
            if offset + size > self.__max_length:
                raise self.__too_long(index, offset + size)

            data = self.__data
            try:
                pack_into(data, end, value)
            except _VALUE_ERRORS as error:
                data = self.__pack_after_growing(pack_into, size, value, error)

            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
            self.__end = end + size
            self.__index = index + 1

        return set_scalar

    set_int8 = _scalar_setter(int8)
    set_uint8 = _scalar_setter(uint8)
    set_int16 = _scalar_setter(int16)
    set_uint16 = _scalar_setter(uint16)
    set_int32 = _scalar_setter(int32)
    set_uint32 = _scalar_setter(uint32)
    set_int64 = _scalar_setter(int64)
    set_uint64 = _scalar_setter(uint64)
    set_float32 = _scalar_setter(float32)
    set_float64 = _scalar_setter(float64)
    del _scalar_setter

    def __pack_after_growing(
        self,
        pack_into: Callable[[Buffer, int, Any], None],
        size: int,
        value: Any,
        error: Exception,
    ) -> bytearray:
        """Packs a scalar that ``pack_into()`` refused at the end, after
        growing the room if that's what it lacked, or raises the error of a
        value that its type can't hold"""
        end = self.__end
        data = self.__data
        if end + size <= len(data):
            raise self.__value_error(value, error) from error

        data = self.__grow(end + size)
        try:
            pack_into(data, end, value)
        except _VALUE_ERRORS as retried:
            raise self.__value_error(value, retried) from retried

        return data

    def __room(self, byte_length: int) -> bytearray:
        """Returns the bytes, with room for ``byte_length`` more at the end"""
        data = self.__data
        if self.__end + byte_length > len(data):
            data = self.__grow(self.__end + byte_length)
        return data

    def set_int8_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("b", SIZEOF_8, values)

    def set_uint8_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("B", SIZEOF_8, values)

    def set_int16_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("h", SIZEOF_16, values)

    def set_uint16_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("H", SIZEOF_16, values)

    def set_int32_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("i", SIZEOF_32, values)

    def set_uint32_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("I", SIZEOF_32, values)

    def set_int64_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("q", SIZEOF_64, values)

    def set_uint64_array(self, values: Buffer | Iterable[int]) -> None:
        self.__set_array("Q", SIZEOF_64, values)

    def set_float32_array(self, values: Buffer | Iterable[float]) -> None:
        self.__set_array("f", SIZEOF_32, values)

    def set_float64_array(self, values: Buffer | Iterable[float]) -> None:
        self.__set_array("d", SIZEOF_64, values)

    def set_string(self, string: str) -> None:
        encoded = string.encode()
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        size = self.__size
        byte_length = len(encoded)
        end = self.__end
        offset = end - self.__base
        length = offset + size + byte_length
        if length > self.__max_length:
            raise self.__too_long(index, length)

        held = byte_length >= self._segment_bytes
        data = self.__room(size + (0 if held else byte_length))
        packing = self.__length
        packing.pack_into(data, self.__slots + index * size, offset)
        packing.pack_into(data, end, byte_length)
        end += size
        if held:
            self.__end = end
            self.__hold(encoded, byte_length)
        else:
            data[end : end + byte_length] = encoded
            self.__end = end + byte_length
        self.__index = index + 1

    def set_struct(self, data: Buffer) -> None:
        view = byte_view(data)
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        byte_length = view.nbytes
        end = self.__end
        offset = end - self.__base
        length = offset + byte_length
        if length > self.__max_length:
            raise self.__too_long(index, length)

        buffer = self.__room(byte_length)
        self.__length.pack_into(buffer, self.__slots + index * self.__size, offset)
        buffer[end : end + byte_length] = view
        self.__end = end + byte_length
        self.__index = index + 1

    def set_struct_array(self, data: Buffer, size: int, alignment: ElementSize) -> None:
        byte_length = byte_view(data).nbytes
        count = struct_count(byte_length, size)
        offset_size = self.__size
        end = self.__end
        offset = end - self.__base
        padding = padding_to(offset + offset_size, alignment)

        index = self.__index
        if index == self.__fields:
            raise self.__full()

        length = offset + padding + offset_size + byte_length
        if length > self.__max_length:
            raise self.__too_long(index, length)

        held = byte_length >= self._segment_bytes
        buffer = self.__room(padding + offset_size + (0 if held else byte_length))
        if padding:
            buffer[end : end + padding] = _ZEROS[padding]
            end += padding
        self.__length.pack_into(
            buffer, self.__slots + index * offset_size, offset + padding
        )
        self.__length.pack_into(buffer, end, count)
        end += offset_size
        if held:
            self.__end = end
            self.__hold(data, byte_length)
        else:
            buffer[end : end + byte_length] = data  # ty: ignore[invalid-assignment]
            self.__end = end + byte_length
        self.__index = index + 1
        if alignment > self.__alignment:  # noqa: PLR1730
            self.__alignment = alignment

    def set_table(
        self,
        table: PreallocatedTableBuilder | Buffer,
        alignment: ElementSize | None = None,
    ) -> None:
        if isinstance(table, PreallocatedTableBuilder):
            byte_length = PreallocatedTableBuilder.__length_of(table)
            default_alignment = table.__alignment
        else:
            byte_length = byte_view(table).nbytes
            default_alignment = SIZEOF_32

        if alignment is None:
            alignment = default_alignment

        end = self.__end
        offset = end - self.__base
        padding = padding_to(offset, alignment)
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        length = offset + padding + byte_length
        if length > self.__max_length:
            raise self.__too_long(index, length)

        if padding:
            data = self.__room(padding)
            data[end : end + padding] = _ZEROS[padding]
            self.__end = end + padding
        self.__length.pack_into(
            self.__data, self.__slots + index * self.__size, offset + padding
        )
        self.__append_table(table, byte_length)
        self.__index = index + 1
        if alignment > self.__alignment:  # noqa: PLR1730
            self.__alignment = alignment

    def set_vector(self, vector: Sequence[PreallocatedTableBuilder | Buffer]) -> None:
        items = tuple(vector)
        size = self.__size
        lengths = [PreallocatedTableBuilder.__length_of(item) for item in items]
        byte_length = size * (1 + len(items)) + sum(lengths)

        index = self.__index
        if index == self.__fields:
            raise self.__full()

        end = self.__end
        offset = end - self.__base
        length = offset + byte_length
        if length > self.__max_length:
            raise self.__too_long(index, length)

        packing = self.__length
        ends = len(items) * size
        data = self.__room(size + ends)
        packing.pack_into(data, self.__slots + index * size, offset)
        vector_at = end
        start = vector_at + self.__shift
        packing.pack_into(data, end, len(items))
        end += size
        data[end : end + ends] = _ZEROS[ends] if ends < len(_ZEROS) else bytes(ends)
        self.__end = end + ends

        item = 0
        for table in items:
            self.__append_table(table, lengths[item])
            item += 1  # noqa: SIM113
            packing.pack_into(
                self.__data,
                vector_at + item * size,
                self.__end + self.__shift - start,
            )
        self.__index = index + 1

    def set_table_in_place[*Args](
        self,
        fill: Callable[[PreallocatedTableBuilder, *Args], object],
        *args: *Args,
        fields: int,
    ) -> None:
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        start = self.__end
        shift = self.__shift
        segments = self.__segments
        held = 0 if segments is None else len(segments)
        base = self.__base
        slots = self.__slots
        parent_fields = self.__fields
        parent_alignment = self.__alignment
        size = self.__size

        self.__base = start
        self.__slots = start
        self.__fields = fields
        self.__index = 0
        self.__alignment = _UNALIGNED
        try:
            slot_bytes = fields * size
            if slot_bytes < len(_ZEROS) and start + slot_bytes <= len(self.__data):
                self.__data[start : start + slot_bytes] = _ZEROS[slot_bytes]
                self.__end = start + slot_bytes
            else:
                self.__start_slots(slot_bytes)
            fill(self, *args)
            if self.__index != fields:
                raise _unset_fields_error(self.__index, fields, "the nested table's")
        except BaseException:
            self.__roll_back(
                start, shift, held, base, slots, parent_fields, index, parent_alignment
            )
            raise

        alignment = self.__alignment
        shift_now = self.__shift
        end = self.__end
        offset = start - base
        padding = -offset % alignment
        length = offset + padding + end + shift_now - start - shift
        if length > self.__max_length:
            self.__roll_back(
                start, shift, held, base, slots, parent_fields, index, parent_alignment
            )
            raise self.__too_long(index, length)

        data = self.__data
        if padding:
            data = self.__room(padding)
            # Copied out first, since a slice of the bytes assigned over
            # bytes that it overlaps would be copied with memcpy()
            data[start + padding : end + padding] = data[start:end]
            data[start : start + padding] = _ZEROS[padding]
            self.__end = end + padding
            if self.__segments is not None:
                self.__move_segments(held, padding)
        self.__length.pack_into(data, slots + index * size, offset + padding)

        self.__base = base + shift - shift_now
        self.__slots = slots
        self.__fields = parent_fields
        self.__index = index + 1
        self.__alignment = max(alignment, parent_alignment)

    def set_vector_in_place[T](
        self,
        values: Collection[T],
        fill: Callable[[PreallocatedTableBuilder, T], object],
        *,
        fields: int,
    ) -> None:
        count = len(values)
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        size = self.__size
        max_length = self.__max_length
        start = self.__end
        offset = start - self.__base
        length = offset + size * (1 + count)
        if length > max_length:
            raise self.__too_long(index, length)

        shift = self.__shift
        segments = self.__segments
        held = 0 if segments is None else len(segments)
        base = self.__base
        slots = self.__slots
        parent_fields = self.__fields
        alignment = self.__alignment
        packing = self.__length
        vector = start + shift
        ends = count * size
        slot_bytes = fields * size

        data = self.__room(size + ends)
        packing.pack_into(data, start, count)
        data[start + size : start + size + ends] = (
            _ZEROS[ends] if ends < len(_ZEROS) else bytes(ends)
        )
        self.__end = start + size + ends
        self.__fields = fields
        try:
            item = 0
            for value in values:
                if item == count:
                    raise _count_error(count)

                item_start = self.__end
                self.__base = item_start
                self.__slots = item_start
                self.__index = 0
                self.__alignment = _UNALIGNED
                end = self.__end
                if slot_bytes < len(_ZEROS) and end + slot_bytes <= len(self.__data):
                    self.__data[end : end + slot_bytes] = _ZEROS[slot_bytes]
                    self.__end = end + slot_bytes
                else:
                    self.__start_slots(slot_bytes)
                fill(self, value)
                if self.__index != fields:
                    raise _unset_fields_error(self.__index, fields, f"item {item}'s")

                item += 1
                end = self.__end
                if item == 1 and count > 1:
                    # Room for the other items, each as long as the first, up
                    # to what the offset type addresses
                    self.__room(min((count - 1) * (end - item_start), max_length))
                last = end + self.__shift - vector
                if last <= max_length:
                    packing.pack_into(self.__data, start + item * size, last)

            if item != count:
                raise _count_error(count)
        except BaseException:
            self.__roll_back(
                start, shift, held, base, slots, parent_fields, index, alignment
            )
            raise

        shift_now = self.__shift
        length = self.__end + shift_now - base - shift
        if length > max_length:
            self.__roll_back(
                start, shift, held, base, slots, parent_fields, index, alignment
            )
            raise self.__too_long(index, length)

        packing.pack_into(self.__data, slots + index * size, offset)
        self.__base = base + shift - shift_now
        self.__slots = slots
        self.__fields = parent_fields
        self.__index = index + 1
        self.__alignment = alignment

    def __start_slots(self, slot_bytes: int) -> None:
        """Writes zeros for a nested table's slots, or counts them, as
        ``DirectTableBuilder`` does"""
        if slot_bytes <= self.__max_length:
            end = self.__end
            data = self.__room(slot_bytes)
            data[end : end + slot_bytes] = (
                _ZEROS[slot_bytes] if slot_bytes < len(_ZEROS) else bytes(slot_bytes)
            )
            self.__end = end + slot_bytes
        else:
            self.__shift += slot_bytes
            self.__base -= slot_bytes

    def __roll_back(
        self,
        start: int,
        shift: int,
        held: int,
        base: int,
        slots: int,
        fields: int,
        index: int,
        alignment: ElementSize,
    ) -> None:
        self.__end = start
        segments = self.__segments
        if segments is not None:
            del segments[held:]
        self.__shift = shift
        self.__base = base
        self.__slots = slots
        self.__fields = fields
        self.__index = index
        self.__alignment = alignment

    def __move_segments(self, held: int, padding: int) -> None:
        segments = self.__segments
        assert segments is not None
        for i in range(held, len(segments)):
            at, buffer, byte_length = segments[i]
            segments[i] = (at + padding, buffer, byte_length)

    def __set_array(
        self,
        type_code: TypeCode,
        item_size: ElementSize,
        values: Buffer | Iterable[float],
    ) -> None:
        elements = as_elements(type_code, values)
        if type(elements) is array:
            byte_length = len(elements) * elements.itemsize
        else:
            byte_length = memoryview(elements).nbytes

        index = self.__index
        if index == self.__fields:
            raise self.__full()

        size = self.__size
        end = self.__end
        offset = end - self.__base
        padding = -(offset + size) % item_size
        length = offset + padding + size + byte_length
        if length > self.__max_length:
            raise self.__too_long(index, length)

        if not _elements.IS_LITTLE_ENDIAN:
            elements = little_endian_bytes(type_code, elements)

        held = byte_length >= self._segment_bytes
        data = self.__room(padding + size + (0 if held else byte_length))
        if padding:
            data[end : end + padding] = _ZEROS[padding]
            end += padding
        self.__length.pack_into(data, self.__slots + index * size, offset + padding)
        self.__length.pack_into(data, end, byte_length // item_size)
        end += size
        if held:
            self.__end = end
            self.__hold(elements, byte_length)
        else:
            data[end : end + byte_length] = elements  # ty: ignore[invalid-assignment]
            self.__end = end + byte_length
        self.__index = index + 1
        if item_size > self.__alignment:  # noqa: PLR1730
            self.__alignment = item_size

    def __hold(self, buffer: Buffer, byte_length: int) -> None:
        segments = self.__segments
        if segments is None:
            segments = self.__segments = []
        segments.append((self.__end, buffer, byte_length))
        self.__shift += byte_length
        self.__base -= byte_length

    def __append_table(
        self, table: PreallocatedTableBuilder | Buffer, byte_length: int
    ) -> None:
        if isinstance(table, PreallocatedTableBuilder):
            written = table.__written
            if written is None:
                self.__append_builder(table)
                return
            table = written.bytes() if isinstance(written, ByteBuffer) else written

        if byte_length >= self._segment_bytes:
            self.__hold(table, byte_length)
        else:
            end = self.__end
            data = self.__room(byte_length)
            data[end : end + byte_length] = table  # ty: ignore[invalid-assignment]
            self.__end = end + byte_length

    def __append_builder(self, nested: PreallocatedTableBuilder) -> None:
        nested_end = nested.__end
        start = self.__end
        data = self.__room(nested_end)
        data[start : start + nested_end] = memoryview(nested.__data)[:nested_end]
        self.__end = start + nested_end
        if nested.__sized:
            nested.__length.pack_into(data, start, nested_end + nested.__shift)

        held = nested.__segments
        if held is None:
            return

        segments = self.__segments
        if segments is None:
            segments = self.__segments = []
        for at, buffer, byte_length in held:
            segments.append((start + at, buffer, byte_length))
        self.__shift += nested.__shift
        self.__base -= nested.__shift

    @staticmethod
    def __length_of(table: PreallocatedTableBuilder | Buffer) -> int:
        if not isinstance(table, PreallocatedTableBuilder):
            return byte_view(table).nbytes

        written = table.__written
        if written is not None:
            return len(written.bytes() if isinstance(written, ByteBuffer) else written)

        if table.__index != table.__fields:
            raise _unset_fields_error(table.__index, table.__fields, "the table's")
        return table.__end + table.__shift

    def __full(self) -> Exception:
        if self.__written is not None:
            return _finalized_error()
        return _field_count_error(self.__index)

    def __too_long(self, index: int, length: int) -> ValueError:
        return ValueError(
            f"Field {index + 1} makes the table {length} bytes, "
            f"too long for {self.__layout.offset_type()} offsets"
        )

    def __value_error(self, value: object, error: Exception) -> ValueError:
        position = self.__end + self.__shift
        return ValueError(f"Can't write {value!r} at {position}: {error}")


class CopyingPreallocatedTableBuilder(PreallocatedTableBuilder):
    """``PreallocatedTableBuilder``, copying every run when it's set"""

    __slots__ = ()

    _segment_bytes: ClassVar[int] = sys.maxsize
