from array import array
from collections.abc import Buffer, Callable, Collection, Iterable, Sequence
from struct import Struct, error as StructError
from typing import TYPE_CHECKING, Any, Literal, overload

from . import elements
from .byte_buffer import ByteBuffer
from .constants import SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64
from .elements import as_elements, byte_view, little_endian_bytes, struct_count
from .errors import describe_int
from .layout import MAX_LENGTHS, TableLayout, header_length, padding_to
from .structs import (
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
from .types import ElementSize, TypeCode

# Zeros by their number: those that pad a field to its alignment, and those
# that hold the offsets of a nested table of up to 16 fields, or a vector's
# item ends, until they're written, as TableWriter appends them
_ZEROS = tuple(bytes(length) for length in range(16 * SIZEOF_32 + 1))

# What Struct.pack() raises for a value that its format can't hold: struct.error
# for an integer out of range or of another type, and OverflowError for a float
# too large for a float32
_VALUE_ERRORS = (StructError, OverflowError)

# The alignment of a table before any field needs one, as a parent pads a
# table without arrays, which is 1 in effect, since every offset is a
# multiple of 1. It's declared rather than inferred, so that a type checker
# takes a table's alignment after its fields are set for any of them.
_UNALIGNED: ElementSize = SIZEOF_8

# Comparisons rather than max() for alignments, and counted indices rather
# than enumerate(), whose iteration Python 3.14 doesn't specialize: each made
# writing the vector of 100 nested tables a few percent faster.


class TableBuilder:
    """Builds a table: a header with one offset per field, after the table's
    total length and version when its layout has them, followed by the fields
    in the order they're set. Arrays, strings and vectors start with their
    count, and a vector then holds the end of each item. Arrays of scalars and
    of structs are padded so their elements are aligned from the start of the
    table. Fields are read back by index with ``ByteBuffer.root_offset()``, or
    ``ByteBuffer.field_offset()`` for a table nested in another.

    Setting a field writes it, and its offset, at the end of the table's
    bytes, so ``finalize()`` only writes the table's length into a sized
    header. Values are copied when they're set: a buffer changed after its
    setter returns doesn't change the table. A table nested from its builder
    is copied as its bytes. ``set_table_in_place()`` and
    ``set_vector_in_place()`` write a nested table, or each item of a vector,
    among this builder's own fields instead, without a builder of its own.

    A setter checks its field before it writes anything, so one that raises
    leaves the builder as it was: ``ValueError`` when its field would make the
    table longer than the layout's ``offset_type`` addresses, which is 255
    bytes, 64 KiB or 4 GiB, or for a scalar that its type can't hold.
    Structs, and tables and vector items given as bytes, must be C-contiguous
    buffers: a setter given one that isn't, such as a slice with a step,
    raises ``TypeError``. Arrays are converted instead.
    """

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
        "__size",
        "__sized",
        "__slots",
        "__written",
    ]

    # The table's bytes so far, the root's and those of any table being
    # written in place in it
    __data: bytearray
    # Where the table being written starts in the bytes, which its offsets
    # count from: the root's 0, or a table nested in place
    __base: int
    # The position in the bytes of the table's first slot, its number of
    # fields, the number set so far, and the largest alignment that a field
    # needs from its start, which a parent pads it to when it nests it
    __slots: int
    __fields: int
    __index: int
    __alignment: ElementSize
    # The table once written: its bytes, and then the buffer of them that
    # finalize() returns
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
        """``fields`` is the number of fields that the table will hold, and
        ``layout`` its header and offset type, which its readers must use too.
        ``version`` is written to the header, and required when the layout is
        versioned. Readers can check it with ``ByteBuffer.get_version()``
        before reading any field.

        Raises ``TypeError`` if ``version`` is given for a layout without a
        version slot, or missing for one with it. Raises ``ValueError`` if
        ``version`` doesn't fit in the layout's offset type, or the header
        alone is longer than the layout addresses.
        """
        length = header_length(fields, layout, version)
        data = bytearray(length)
        # A negative number of fields that a header's slots outnumber, as
        # bytearray() rejects a negative length
        if fields < 0:
            raise ValueError("negative count")
        size = layout.offset_size()
        sized = layout.sized()
        header_size = layout.header_size()

        self.__size = size
        self.__length = OFFSET_STRUCTS[size]
        self.__max_length = MAX_LENGTHS[size]
        self.__layout = layout
        self.__sized = sized
        self.__header_size = header_size
        self.__data = data
        self.__base = 0
        self.__slots = header_size
        self.__fields = fields
        self.__index = 0
        self.__alignment = _UNALIGNED
        self.__written = None

        if version is not None:
            self.__length.pack_into(data, size if sized else 0, version)

    def finalize(self) -> ByteBuffer:
        """Finishes the table, once, and returns a ``ByteBuffer`` that reads
        it; later calls return the same buffer.

        Raises ``ValueError`` if fewer fields were set than the table holds,
        or if it's called while ``fill`` sets the fields of a table nested in
        place, leaving the builder as it was.
        """
        written = self.__written
        if written is None:
            written = self.__write()
        elif isinstance(written, ByteBuffer):
            return written

        buffer = ByteBuffer(written, self.__layout)
        self.__written = buffer

        return buffer

    def bytes(self) -> memoryview[int]:
        """Finishes the table, once, as ``finalize()`` does, and returns its
        bytes, without the buffer that ``finalize()`` creates. Raises as
        ``finalize()`` does."""
        written = self.__written
        if written is None:
            return self.__write()

        return written.bytes() if isinstance(written, ByteBuffer) else written

    def __write(self) -> memoryview[int]:
        """Writes the table's length into a sized header, and returns a view of
        its bytes"""
        # Only a table being written in place has its slots elsewhere than
        # after the root's header
        if self.__slots != self.__header_size:
            raise _written_in_place_error()
        if self.__index != self.__fields:
            raise _unset_fields_error(self.__index, self.__fields, "the table's")

        data = self.__data
        if self.__sized:
            self.__length.pack_into(data, 0, len(data))

        written = memoryview(data)
        self.__written = written

        return written

    # The scalar setters are written out, from a closure over each one's
    # Struct, since a helper's call would add to every field. Each packs its
    # value before it writes the offset, so that a setter that raises writes
    # nothing.

    def _scalar_setter(
        packing: Struct, name: str
    ) -> Callable[[TableBuilder, Any], None]:
        pack = packing.pack
        size = packing.size

        def set_scalar(self: TableBuilder, value: Any) -> None:
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

        set_scalar.__name__ = name
        set_scalar.__qualname__ = f"TableBuilder.{name}"
        return set_scalar

    # Type checkers see the setters as methods, with their values' types,
    # since they don't bind a function that a class attribute is assigned
    if TYPE_CHECKING:

        def set_int8(self, value: int) -> None: ...
        def set_uint8(self, value: int) -> None: ...
        def set_int16(self, value: int) -> None: ...
        def set_uint16(self, value: int) -> None: ...
        def set_int32(self, value: int) -> None: ...
        def set_uint32(self, value: int) -> None: ...
        def set_int64(self, value: int) -> None: ...
        def set_uint64(self, value: int) -> None: ...
        def set_float32(self, value: float) -> None: ...
        def set_float64(self, value: float) -> None: ...

    else:
        set_int8 = _scalar_setter(int8, "set_int8")
        set_uint8 = _scalar_setter(uint8, "set_uint8")
        set_int16 = _scalar_setter(int16, "set_int16")
        set_uint16 = _scalar_setter(uint16, "set_uint16")
        set_int32 = _scalar_setter(int32, "set_int32")
        set_uint32 = _scalar_setter(uint32, "set_uint32")
        set_int64 = _scalar_setter(int64, "set_int64")
        set_uint64 = _scalar_setter(uint64, "set_uint64")
        set_float32 = _scalar_setter(float32, "set_float32")
        set_float64 = _scalar_setter(float64, "set_float64")
    del _scalar_setter

    # A contiguous buffer of the field's element type is copied as bytes.
    # Anything else is converted first. Writing a table of 27,028 floats took
    # 0.36 ms from a list, against 5 µs from an array("f").

    def set_int8_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of int8s, such as
        ``array("b")``, is copied as bytes; anything else, such as a list, is
        converted as ``array("b", values)`` would, which takes several times as
        long."""
        self.__set_array("b", SIZEOF_8, values)

    def set_uint8_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. Bytes, or any other contiguous buffer of
        uint8s, are copied as they are; anything else, such as a list, is
        converted as ``array("B", values)`` would, which takes several times as
        long."""
        self.__set_array("B", SIZEOF_8, values)

    def set_int16_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of int16s in the host's
        byte order, such as ``array("h")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("h", values)`` would, which
        takes several times as long."""
        self.__set_array("h", SIZEOF_16, values)

    def set_uint16_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of uint16s in the host's
        byte order, such as ``array("H")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("H", values)`` would, which
        takes several times as long."""
        self.__set_array("H", SIZEOF_16, values)

    def set_int32_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of int32s in the host's
        byte order, such as ``array("i")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("i", values)`` would, which
        takes several times as long."""
        self.__set_array("i", SIZEOF_32, values)

    def set_uint32_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of uint32s in the host's
        byte order, such as ``array("I")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("I", values)`` would, which
        takes several times as long."""
        self.__set_array("I", SIZEOF_32, values)

    def set_int64_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of int64s in the host's
        byte order, such as ``array("q")`` or a numpy int64 array, is copied as
        bytes; anything else, such as a list, is converted as
        ``array("q", values)`` would, which takes several times as long."""
        self.__set_array("q", SIZEOF_64, values)

    def set_uint64_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of uint64s in the host's
        byte order, such as ``array("Q")`` or a numpy uint64 array, is copied
        as bytes; anything else, such as a list, is converted as
        ``array("Q", values)`` would, which takes several times as long."""
        self.__set_array("Q", SIZEOF_64, values)

    def set_float32_array(self, values: Buffer | Iterable[float]) -> None:
        """Sets an array field. A contiguous buffer of float32s in the host's
        byte order, such as ``array("f")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("f", values)`` would, which
        takes several times as long and writes a finite value past float32's
        range as an infinity of its sign."""
        self.__set_array("f", SIZEOF_32, values)

    def set_float64_array(self, values: Buffer | Iterable[float]) -> None:
        """Sets an array field. A contiguous buffer of float64s in the host's
        byte order, such as ``array("d")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("d", values)`` would, which
        takes several times as long."""
        self.__set_array("d", SIZEOF_64, values)

    def set_string(self, string: str) -> None:
        """Writes ``string`` as UTF-8. Raises ``UnicodeEncodeError``, a
        ``ValueError``, if it has a lone surrogate, which UTF-8 can't encode."""
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
        data += encoded
        self.__index = index + 1

    def set_struct(self, data: Buffer) -> None:
        """Sets a struct field from its bytes: its scalars, little-endian, one
        after another, which are written as they are, without padding. A
        ``struct.Struct`` with a ``<`` format packs them."""
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
        """Sets an array of structs of ``size`` bytes each, from their bytes
        one after another. It's padded so that the structs start at a multiple
        of ``alignment`` from the start of the table, which is the size of the
        structs' largest scalar.

        Raises ``ValueError`` if ``size`` is less than 1, or ``data`` isn't a
        whole number of structs.
        """
        view = byte_view(data)
        byte_length = view.nbytes
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
        buffer += view
        self.__index = index + 1
        if alignment > self.__alignment:  # noqa: PLR1730
            self.__alignment = alignment

    def set_table(
        self, table: TableBuilder | Buffer, alignment: ElementSize | None = None
    ) -> None:
        """Embeds a table, from its builder or its bytes, without a length
        prefix. Readers find its fields with ``ByteBuffer.field_offset()`` from
        the field's offset, so its layout must be ``TableLayout.DEFAULT``.

        A builder must have all its fields set, and is copied as the bytes
        that it writes. It can still be finalized or nested elsewhere too,
        since a complete builder doesn't change.

        The nested table's arrays are aligned from its own start, so it's
        padded to start at a multiple of ``alignment``. For a builder, the
        default is the largest alignment of its arrays, of scalars and of
        structs, and of its nested tables, or 1 if it has neither. For bytes,
        the default of 4 suits tables of 8-, 16- and 32-bit elements; pass 8
        for a table with 64-bit arrays.

        Raises ``ValueError`` if the builder has fields left to set.
        """
        if isinstance(table, TableBuilder):
            nested = TableBuilder.__bytes_of(table)
            default_alignment = table.__alignment
        else:
            nested = byte_view(table)
            default_alignment = SIZEOF_32

        if alignment is None:
            alignment = default_alignment

        data = self.__data
        offset = len(data) - self.__base
        padding = padding_to(offset, alignment)
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        length = offset + padding + len(nested)
        if length > self.__max_length:
            raise self.__too_long(index, length)

        if padding:
            data += _ZEROS[padding]
        self.__length.pack_into(
            data, self.__slots + index * self.__size, offset + padding
        )
        data += nested
        self.__index = index + 1
        if alignment > self.__alignment:  # noqa: PLR1730
            self.__alignment = alignment

    def set_vector(self, vector: Sequence[TableBuilder | Buffer]) -> None:
        """Writes a vector of tables, from their builders or their bytes, after
        the end of each, which lets readers find any item without reading the
        others. Builders must have all their fields set, and are copied as the
        bytes that they write, as ``set_table()`` copies them. Items aren't
        padded.

        Raises ``ValueError`` if a builder has fields left to set.
        """
        items = [
            TableBuilder.__bytes_of(item)
            if isinstance(item, TableBuilder)
            else byte_view(item)
            for item in vector
        ]
        size = self.__size
        byte_length = size * (1 + len(items))
        for item in items:
            byte_length += len(item)

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
        start = len(data)
        ends = len(items) * size
        data += packing.pack(len(items))
        data += _ZEROS[ends] if ends < len(_ZEROS) else bytes(ends)

        item = 0
        for table in items:
            data += table
            item += 1  # noqa: SIM113
            packing.pack_into(data, start + item * size, len(data) - start)
        self.__index = index + 1

    # The in-place setters write the nested table, or each item, at the end
    # of the bytes, with the builder's counters set to it, and restore its
    # parent's once fill has set its fields. A nested table is written
    # unpadded, since its alignment is known only once its fields are set,
    # and then moved forward by its padding. Its offsets count from its own
    # start, so none of them changes. One that fails is truncated away.
    #
    # fill() is given the builder itself, whose setters write the nested
    # table's fields. Writing the builder, or nesting it in another table,
    # would write the nested table's bytes as the parent's, so it raises
    # instead.

    def set_table_in_place[*Args](
        self,
        fill: Callable[[TableBuilder, *Args], object],
        *args: *Args,
        fields: int,
    ) -> None:
        """Writes a table of ``fields`` fields nested in this one, as
        ``set_table()`` writes a builder of it, but without a builder of its
        own: ``fill(builder, *args)`` sets its fields in order on this
        builder, which writes them as the nested table's while ``fill`` runs.
        The table is padded to start at a multiple of its largest alignment,
        as ``set_table()`` pads a builder by default, and the two write the
        same bytes.

        Until ``fill`` returns, the builder holds the nested table's fields
        rather than its own, so ``fill`` mustn't finalize it or nest it in
        another table, which raises ``ValueError``.

        Raises ``ValueError`` if the table's slots alone are longer than the
        layout addresses, before ``fill`` runs, if ``fill`` leaves fields of
        the table unset, or as the setters do, and passes on what ``fill``
        raises, leaving the builder as it was either way.
        """
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        data = self.__data
        size = self.__size
        start = len(data)
        base = self.__base
        offset = start - base
        slot_bytes = self.__slot_bytes(index, offset, fields)

        slots = self.__slots
        parent_fields = self.__fields
        parent_alignment = self.__alignment
        self.__base = start
        self.__slots = start
        self.__fields = fields
        self.__index = 0
        self.__alignment = _UNALIGNED
        try:
            data += (
                _ZEROS[slot_bytes] if slot_bytes < len(_ZEROS) else bytes(slot_bytes)
            )
            fill(self, *args)
            if self.__index != fields:
                raise _unset_fields_error(self.__index, fields, "the nested table's")
        except BaseException:
            self.__roll_back(start, base, slots, parent_fields, index, parent_alignment)
            raise

        # Padded from its parent's start, as a nested builder is
        alignment = self.__alignment
        padding = -offset % alignment
        length = offset + padding + len(data) - start
        if length > self.__max_length:
            self.__roll_back(start, base, slots, parent_fields, index, parent_alignment)
            raise self.__too_long(index, length)

        if padding:
            data[start:start] = _ZEROS[padding]
        self.__length.pack_into(data, slots + index * size, offset + padding)

        self.__base = base
        self.__slots = slots
        self.__fields = parent_fields
        self.__index = index + 1
        self.__alignment = max(parent_alignment, alignment)

    def set_vector_in_place[T](
        self,
        values: Collection[T],
        fill: Callable[[TableBuilder, T], object],
        *,
        fields: int,
    ) -> None:
        """Writes a vector of tables of ``fields`` fields each, one for each of
        ``values``, as ``set_table_in_place()`` writes a table:
        ``fill(builder, value)`` sets the fields of the value's item in order
        on this builder. Items aren't padded, as ``set_vector()`` doesn't pad
        them, and the two write the same bytes.

        Raises ``ValueError`` if the vector's count, or an item's slots, make
        the table too long, before any item is written, if ``fill`` leaves an
        item's fields unset, if iterating ``values`` gives more or fewer than
        their ``len()``, or as the setters do, and passes on what ``fill``
        raises, leaving the builder as it was either way.
        """
        count = len(values)
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        data = self.__data
        size = self.__size
        max_length = self.__max_length
        base = self.__base
        start = len(data)
        offset = start - base
        length = offset + size * (1 + count)
        if length > max_length:
            raise self.__too_long(index, length)
        slot_bytes = self.__slot_bytes(index, length, fields) if count else 0

        slots = self.__slots
        parent_fields = self.__fields
        alignment = self.__alignment
        packing = self.__length
        ends = count * size
        zeros = _ZEROS[slot_bytes] if slot_bytes < len(_ZEROS) else bytes(slot_bytes)

        data += packing.pack(count)
        data += _ZEROS[ends] if ends < len(_ZEROS) else bytes(ends)
        self.__fields = fields
        try:
            # Each item counts its offsets and alignment from its own start,
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
                data += zeros
                fill(self, value)
                if self.__index != fields:
                    raise _unset_fields_error(self.__index, fields, f"item {item}'s")

                item += 1
                # An end that its slot can't hold makes the table too long,
                # which is raised once every item is written
                end = len(data) - start
                if end <= max_length:
                    packing.pack_into(data, start + item * size, end)

            if item != count:
                raise _count_error(count)
        except BaseException:
            self.__roll_back(start, base, slots, parent_fields, index, alignment)
            raise

        length = len(data) - base
        if length > max_length:
            self.__roll_back(start, base, slots, parent_fields, index, alignment)
            raise self.__too_long(index, length)

        packing.pack_into(data, slots + index * size, offset)
        self.__base = base
        self.__slots = slots
        self.__fields = parent_fields
        self.__index = index + 1
        self.__alignment = alignment

    def __slot_bytes(self, index: int, length: int, fields: int) -> int:
        """The bytes of the slots of a table of ``fields`` fields nested in
        place, which starts ``length`` bytes into its parent. Raises
        ``ValueError`` for slots that make the table too long, before they're
        written, or for a negative number of fields, as ``bytearray()`` does
        for a negative length."""
        slot_bytes = fields * self.__size
        if slot_bytes > self.__max_length:
            raise self.__too_long(index, length + slot_bytes)
        if slot_bytes < 0:
            raise ValueError("negative count")
        return slot_bytes

    def __roll_back(
        self,
        start: int,
        base: int,
        slots: int,
        fields: int,
        index: int,
        alignment: ElementSize,
    ) -> None:
        """Truncates the bytes to ``start``, where a nested table or vector
        that failed starts, and restores its parent's counters"""
        del self.__data[start:]
        self.__base = base
        self.__slots = slots
        self.__fields = fields
        self.__index = index
        self.__alignment = alignment

    def __set_array(
        self,
        type_code: TypeCode,
        item_size: ElementSize,
        values: Buffer | Iterable[float],
    ) -> None:
        """Pads the array so that its elements, after the count, are aligned"""
        converted = as_elements(type_code, values)
        if type(converted) is array:
            byte_length = len(converted) * converted.itemsize
        else:
            byte_length = memoryview(converted).nbytes

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

        # Read when it's called, so that the tests can treat the host as
        # big-endian
        if not elements.IS_LITTLE_ENDIAN:
            converted = little_endian_bytes(type_code, converted)

        if padding:
            data += _ZEROS[padding]
        self.__length.pack_into(data, self.__slots + index * size, offset + padding)
        data += self.__length.pack(byte_length // item_size)
        data += converted
        self.__index = index + 1
        if item_size > self.__alignment:  # noqa: PLR1730
            self.__alignment = item_size

    @staticmethod
    def __bytes_of(table: TableBuilder) -> memoryview[int] | bytearray:
        """The bytes of a builder to nest, with its length in a sized header,
        which it has once all its fields are set. Raises ``ValueError`` if it
        has fields left to set, or is writing a table in place."""
        written = table.__written
        if written is not None:
            return written.bytes() if isinstance(written, ByteBuffer) else written

        if table.__slots != table.__header_size:
            raise _written_in_place_error()
        if table.__index != table.__fields:
            raise _unset_fields_error(table.__index, table.__fields, "the table's")

        data = table.__data
        if table.__sized:
            table.__length.pack_into(data, 0, len(data))
        return data

    def __full(self) -> Exception:
        """The error of a field that the table has no room for: it's
        finalized, or holds all its fields"""
        if self.__written is not None:
            return _finalized_error()
        return _field_count_error(self.__index)

    def __too_long(self, index: int, length: int) -> ValueError:
        return ValueError(
            f"Field {index + 1} makes the table {describe_int(length)} bytes, "
            f"too long for {self.__layout.offset_type()} offsets"
        )

    def __value_error(self, value: object, error: Exception) -> ValueError:
        """The error that ``TableWriter`` raises for a scalar that its type
        can't hold, at the position where it would be written"""
        return ValueError(f"Can't write {value!r} at {len(self.__data)}: {error}")


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
