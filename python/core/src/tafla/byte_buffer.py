from array import array
from collections.abc import Buffer, Iterable, Sequence
from struct import Struct, error as StructError
from typing import Any, overload

from . import structs
from .constants import SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64
from .elements import as_elements, from_little_endian, little_endian_bytes
from .errors import DecodeError, describe_int
from .layout import MAX_LENGTHS, TableLayout
from .types import FloatTypeCode, IntTypeCode, OffsetSize, TypeCode

# The scalars' Structs, assigned rather than imported. Python 3.14 compiles a
# method call on a name that an import bound as a load of a module's
# attribute, which doesn't specialize for a Struct and creates a bound method
# for each call: each scalar's read or write took 25 ns longer, and reading a
# vector of 100 tables that each nest another 8% longer.
int8, uint8 = structs.int8, structs.uint8
int16, uint16 = structs.int16, structs.uint16
int32, uint32 = structs.int32, structs.uint32
int64, uint64 = structs.int64, structs.uint64
float32, float64 = structs.float32, structs.float64

# The unsigned integer of each offset size, which lengths, offsets and header
# slots are written as
_SIZES: dict[OffsetSize, Struct] = {
    SIZEOF_8: uint8,
    SIZEOF_16: uint16,
    SIZEOF_32: uint32,
}
# Their array type codes, which the ends of a vector's items are read as
_SIZE_CODES: dict[OffsetSize, IntTypeCode] = {
    SIZEOF_8: "B",
    SIZEOF_16: "H",
    SIZEOF_32: "I",
}
# One 64-bit item position, which get_vector_offsets() repeats for every item
# and then fills in. Repeating it, rather than array("Q", bytes(8 * count)),
# doesn't hold the count's zeros as bytes too while they're copied.
_ZERO_OFFSET = array("Q", [0])


class ByteBuffer:
    """Reads and writes within the supplied bytes, in the table layout it was
    created with, storing multi-byte values little-endian. Array reads return
    views of the bytes on a little-endian host, whatever their alignment, and
    copies in the host's byte order on a big-endian host.

    Every read checks that it stays within the bytes and raises ``DecodeError``
    otherwise, at any position, however large. The bytes, and any other buffer
    given as bytes, must be C-contiguous, and one that isn't raises
    ``TypeError``. A write that doesn't fit in the bytes raises ``ValueError`` and
    writes nothing. So does an array, string or vector whose count, length or
    item ends are too large for the layout's ``offset_type``. Tables are
    written with ``TableBuilder`` or ``TableWriter``, which reject a table
    that long. A scalar, offset or version that doesn't fit its slot raises
    ``ValueError`` and leaves the position where it was, but CPython's struct
    zeroes the slot before it checks the value.
    """

    __slots__ = [
        "__header_size",
        "__layout",
        "__length",
        "__position",
        "__size",
        "__size_struct",
        "__view",
    ]

    __size: OffsetSize

    def __init__(
        self, bytes: Buffer, layout: TableLayout = TableLayout.DEFAULT
    ) -> None:
        """Reads and writes ``bytes`` in ``layout``, which must be the layout
        that the table was written in.

        Raises ``TypeError`` if ``bytes`` isn't C-contiguous, such as a slice
        with a step. Reads return views of the bytes, which a copy of them
        wouldn't be.
        """
        # byte_view() inline, since its call made a constructor 10% slower,
        # and every reader that tafla-gen generates constructs one
        view = memoryview(bytes)
        if not view.c_contiguous:
            raise _not_contiguous()

        self.__view = view if view.format == "B" and view.ndim == 1 else view.cast("B")
        self.__length = len(self.__view)

        # The layout itself, rather than copies of what only the header's reads
        # and the writes use, which took a quarter of the constructor's time:
        # 0.32 µs rather than 0.24 µs
        self.__layout = layout
        # The offset size, the width of offsets, lengths and header slots
        self.__size = size = layout.offset_size()
        self.__size_struct = _SIZES[size]
        # The byte length of the header slots before the root's field offsets
        self.__header_size = layout.header_size()

        self.__position = 0

    def reset(self) -> None:
        self.__position = 0

    def bytes(self) -> memoryview:
        return self.__view

    def get_size(self) -> int:
        return self.__get_size(0) if self.__layout.sized() else self.__length

    def set_size(self, size: int) -> None:
        self.__set_size(size)

    def get_version(self) -> int:
        """Returns the version that the table's header carries after its
        length. It's as wide as the header's other slots.

        Raises ``TypeError`` if the buffer's layout isn't versioned.
        """
        layout = self.__layout
        if not layout.versioned():
            raise TypeError("The table has no version")

        return self.__get_size(self.__size if layout.sized() else 0)

    def set_version(self, version: int) -> None:
        self.__set_size(version)

    # root_offset() and field_offset() read their slot as __get_size() does,
    # written out as the scalar getters below are, since every field's read
    # starts with one: without the call, reading a vector of 100 tables took
    # 4% less time.

    def root_offset(self, index: int) -> int:
        """Returns the position of field ``index`` of the root table."""
        position = self.__header_size + index * self.__size
        try:
            if position >= 0:
                return self.__size_struct.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, self.__size, self.__length)

    def field_offset(self, table_position: int, index: int) -> int:
        """Returns the position of field ``index`` of a nested table, which has
        no header and stores its field offsets from its own start.
        ``table_position`` is the nested table's position, which is the offset
        of the field that holds it.
        """
        position = table_position + index * self.__size
        try:
            if position >= 0:
                offset = self.__size_struct.unpack_from(self.__view, position)[0]
                return table_position + offset
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, self.__size, self.__length)

    def set_offset(self, offset: int) -> None:
        self.__set_size(offset)

    # The scalar getters are written out, since a shared helper's call made
    # each read a quarter slower. struct checks the end and raises struct.error,
    # which isn't a ValueError, raises OverflowError for a position too large
    # for a C ssize_t, and counts a negative position from the end, so all
    # three fall through to a DecodeError.

    def get_int8(self, position: int) -> int:
        try:
            if position >= 0:
                return int8.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, SIZEOF_8, self.__length)

    def get_uint8(self, position: int) -> int:
        try:
            if position >= 0:
                return uint8.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, SIZEOF_8, self.__length)

    def get_int16(self, position: int) -> int:
        try:
            if position >= 0:
                return int16.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, SIZEOF_16, self.__length)

    def get_uint16(self, position: int) -> int:
        try:
            if position >= 0:
                return uint16.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, SIZEOF_16, self.__length)

    def get_int32(self, position: int) -> int:
        try:
            if position >= 0:
                return int32.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, SIZEOF_32, self.__length)

    def get_uint32(self, position: int) -> int:
        try:
            if position >= 0:
                return uint32.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, SIZEOF_32, self.__length)

    def get_int64(self, position: int) -> int:
        try:
            if position >= 0:
                return int64.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, SIZEOF_64, self.__length)

    def get_uint64(self, position: int) -> int:
        try:
            if position >= 0:
                return uint64.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, SIZEOF_64, self.__length)

    def get_float32(self, position: int) -> float:
        try:
            if position >= 0:
                return float32.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, SIZEOF_32, self.__length)

    def get_float64(self, position: int) -> float:
        try:
            if position >= 0:
                return float64.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, SIZEOF_64, self.__length)

    def get_int8_array(self, position: int) -> memoryview[int]:
        """Reads the array at ``position``, as a view of the buffer."""
        return self.__get_array("b", SIZEOF_8, position)

    def get_uint8_array(self, position: int) -> memoryview[int]:
        """Reads the array at ``position``, as a view of the buffer."""
        length = self.__get_size(position)
        start = self.__check_range(position + self.__size, length)

        return self.__view[start : start + length]

    def get_int16_array(self, position: int) -> memoryview[int]:
        """Reads the array at ``position``: a view of the buffer on a
        little-endian host, and a copy on a big-endian host."""
        return self.__get_array("h", SIZEOF_16, position)

    def get_uint16_array(self, position: int) -> memoryview[int]:
        """Reads the array at ``position``: a view of the buffer on a
        little-endian host, and a copy on a big-endian host."""
        return self.__get_array("H", SIZEOF_16, position)

    def get_int32_array(self, position: int) -> memoryview[int]:
        """Reads the array at ``position``: a view of the buffer on a
        little-endian host, and a copy on a big-endian host."""
        return self.__get_array("i", SIZEOF_32, position)

    def get_uint32_array(self, position: int) -> memoryview[int]:
        """Reads the array at ``position``: a view of the buffer on a
        little-endian host, and a copy on a big-endian host."""
        return self.__get_array("I", SIZEOF_32, position)

    def get_int64_array(self, position: int) -> memoryview[int]:
        """Reads the array at ``position``: a view of the buffer on a
        little-endian host, and a copy on a big-endian host."""
        return self.__get_array("q", SIZEOF_64, position)

    def get_uint64_array(self, position: int) -> memoryview[int]:
        """Reads the array at ``position``: a view of the buffer on a
        little-endian host, and a copy on a big-endian host."""
        return self.__get_array("Q", SIZEOF_64, position)

    def get_float32_array(self, position: int) -> memoryview[float]:
        """Reads the array at ``position``: a view of the buffer on a
        little-endian host, and a copy on a big-endian host."""
        return self.__get_array("f", SIZEOF_32, position)

    def get_float64_array(self, position: int) -> memoryview[float]:
        """Reads the array at ``position``: a view of the buffer on a
        little-endian host, and a copy on a big-endian host."""
        return self.__get_array("d", SIZEOF_64, position)

    @overload
    def __get_array(
        self, type_code: IntTypeCode, item_size: int, position: int
    ) -> memoryview[int]: ...

    @overload
    def __get_array(
        self, type_code: FloatTypeCode, item_size: int, position: int
    ) -> memoryview[float]: ...

    def __get_array(self, type_code: TypeCode, item_size: int, position: int) -> Any:
        byte_length = self.__get_size(position) * item_size
        start = self.__check_range(position + self.__size, byte_length)

        return from_little_endian(type_code, self.__view[start : start + byte_length])

    def get_struct(self, position: int, size: int) -> memoryview[int]:
        """Returns a view of the ``size`` bytes of the struct at ``position``,
        whose scalars lie at fixed offsets from its start."""
        end = position + size
        if position < 0 or size < 0 or end > self.__length:
            raise DecodeError.out_of_range(position, size, self.__length)

        return self.__view[position:end]

    def get_struct_array(self, position: int, size: int) -> memoryview[int]:
        """Returns a view of the structs of the array at ``position``, each of
        ``size`` bytes, one after another after the count. Their number is the
        view's length over ``size``.

        Raises ``ValueError`` if ``size`` is less than 1.
        """
        _check_struct_size(size)

        byte_length = self.__get_size(position) * size
        start = self.__check_range(position + self.__size, byte_length)

        return self.__view[start : start + byte_length]

    def set_int8(self, value: int) -> None:
        self.__pack(int8, value)
        self.__position += SIZEOF_8

    def set_uint8(self, value: int) -> None:
        self.__pack(uint8, value)
        self.__position += SIZEOF_8

    def set_int16(self, value: int) -> None:
        self.__pack(int16, value)
        self.__position += SIZEOF_16

    def set_uint16(self, value: int) -> None:
        self.__pack(uint16, value)
        self.__position += SIZEOF_16

    def set_int32(self, value: int) -> None:
        self.__pack(int32, value)
        self.__position += SIZEOF_32

    def set_uint32(self, value: int) -> None:
        self.__pack(uint32, value)
        self.__position += SIZEOF_32

    def set_int64(self, value: int) -> None:
        self.__pack(int64, value)
        self.__position += SIZEOF_64

    def set_uint64(self, value: int) -> None:
        self.__pack(uint64, value)
        self.__position += SIZEOF_64

    def set_float32(self, value: float) -> None:
        self.__pack(float32, value)
        self.__position += SIZEOF_32

    def set_float64(self, value: float) -> None:
        self.__pack(float64, value)
        self.__position += SIZEOF_64

    def __pack(self, struct: Struct, value: float) -> None:
        """Writes ``value`` at the position. struct raises struct.error, which
        isn't a ValueError, or OverflowError for a float too large for a
        float32. It checks the end before it writes, but zeroes the slot before
        it checks the value. Packing into a copy first would keep the slot, but
        made each write about 50% slower."""
        try:
            struct.pack_into(self.__view, self.__position, value)
        except (StructError, OverflowError) as error:
            raise ValueError(
                f"Can't write {value!r} at {self.__position}: {error}"
            ) from error

    def set_int8_array(self, values: Buffer | Iterable[int]) -> None:
        """Writes the length and then the elements. A contiguous buffer of
        int8s, such as ``array("b")``, is copied as bytes; anything else is
        converted as ``array("b", values)`` would."""
        self.__set_array("b", SIZEOF_8, values)

    def set_uint8_array(self, values: Buffer | Iterable[int]) -> None:
        """Writes the length and then the elements. Bytes, or any other
        contiguous buffer of uint8s, are copied as they are; anything else is
        converted as ``array("B", values)`` would."""
        self.__set_array("B", SIZEOF_8, values)

    def set_int16_array(self, values: Buffer | Iterable[int]) -> None:
        """Writes the length and then the elements. A contiguous buffer of
        int16s in the host's byte order, such as ``array("h")``, is copied as
        bytes; anything else is converted as ``array("h", values)`` would."""
        self.__set_array("h", SIZEOF_16, values)

    def set_uint16_array(self, values: Buffer | Iterable[int]) -> None:
        """Writes the length and then the elements. A contiguous buffer of
        uint16s in the host's byte order, such as ``array("H")``, is copied as
        bytes; anything else is converted as ``array("H", values)`` would."""
        self.__set_array("H", SIZEOF_16, values)

    def set_int32_array(self, values: Buffer | Iterable[int]) -> None:
        """Writes the length and then the elements. A contiguous buffer of
        int32s in the host's byte order, such as ``array("i")``, is copied as
        bytes; anything else is converted as ``array("i", values)`` would."""
        self.__set_array("i", SIZEOF_32, values)

    def set_uint32_array(self, values: Buffer | Iterable[int]) -> None:
        """Writes the length and then the elements. A contiguous buffer of
        uint32s in the host's byte order, such as ``array("I")``, is copied as
        bytes; anything else is converted as ``array("I", values)`` would."""
        self.__set_array("I", SIZEOF_32, values)

    def set_int64_array(self, values: Buffer | Iterable[int]) -> None:
        """Writes the length and then the elements. A contiguous buffer of
        int64s in the host's byte order, such as ``array("q")``, is copied as
        bytes; anything else is converted as ``array("q", values)`` would."""
        self.__set_array("q", SIZEOF_64, values)

    def set_uint64_array(self, values: Buffer | Iterable[int]) -> None:
        """Writes the length and then the elements. A contiguous buffer of
        uint64s in the host's byte order, such as ``array("Q")``, is copied as
        bytes; anything else is converted as ``array("Q", values)`` would."""
        self.__set_array("Q", SIZEOF_64, values)

    def set_float32_array(self, values: Buffer | Iterable[float]) -> None:
        """Writes the length and then the elements. A contiguous buffer of
        float32s in the host's byte order, such as ``array("f")``, is copied as
        bytes; anything else is converted as ``array("f", values)`` would."""
        self.__set_array("f", SIZEOF_32, values)

    def set_float64_array(self, values: Buffer | Iterable[float]) -> None:
        """Writes the length and then the elements. A contiguous buffer of
        float64s in the host's byte order, such as ``array("d")``, is copied as
        bytes; anything else is converted as ``array("d", values)`` would."""
        self.__set_array("d", SIZEOF_64, values)

    def __set_array(
        self, type_code: TypeCode, item_size: int, values: Buffer | Iterable[float]
    ) -> None:
        source = little_endian_bytes(type_code, as_elements(type_code, values))
        count = len(source) // item_size
        self.__check_writable(self.__size + len(source))
        self.__check_size(count)

        self.__set_size(count)
        self.__write(source)

    def set_struct_array(self, data: Buffer, size: int) -> None:
        """Writes the count of the ``size``-byte structs in ``data``, and then
        the bytes. Padding that aligns the structs goes before the count.

        Raises ``ValueError`` if ``size`` is less than 1, or ``data`` isn't a
        whole number of structs.
        """
        source = byte_view(data)
        count = struct_count(len(source), size)
        self.__check_writable(self.__size + len(source))
        self.__check_size(count)

        self.__set_size(count)
        self.__write(source)

    def set_bytes(self, bytes: Buffer) -> None:
        """Writes ``bytes`` as they are, without a length prefix, such as a
        struct."""
        source = byte_view(bytes)
        end = self.__position + len(source)
        if end > self.__length:
            raise self.__overflow(len(source))

        self.__view[self.__position : end] = source
        self.__position = end

    def set_padding(self, byte_length: int) -> None:
        """Writes ``byte_length`` zeros, such as to align the field after them."""
        self.__check_writable(byte_length)

        self.__write(memoryview(bytes(byte_length)))

    def get_string(self, position: int) -> str:
        """Raises ``DecodeError`` if the string isn't valid UTF-8."""
        length = self.__get_size(position)
        start = self.__check_range(position + self.__size, length)

        try:
            return str(self.__view[start : start + length], "utf-8")
        except UnicodeDecodeError as error:
            raise DecodeError(f"The string at {position} isn't valid UTF-8") from error

    def set_string(self, encoded: Buffer) -> None:
        """Writes a string encoded as UTF-8, after its length."""
        source = byte_view(encoded)
        self.__check_writable(self.__size + len(source))
        self.__check_size(len(source))

        self.__set_size(len(source))
        self.__write(source)

    def get_vector_length(self, position: int) -> int:
        """Returns the number of items of the vector at ``position``.

        Raises ``DecodeError`` if the count or the ends after it don't lie
        within the buffer. Every item has an end, so a count that the bytes
        can't hold fails before anything is allocated for it.
        """
        size = self.__size
        count = self.__get_size(position)
        self.__check_range(position + size, count * size)

        return count

    def get_vector_offset(self, position: int, index: int) -> int:
        """Returns the position of item ``index`` of the vector at
        ``position``. It's read from the ends of that item and the one before
        it, without reading any other item.

        Raises ``DecodeError`` if the count or the ends after it don't lie
        within the buffer, which is checked before ``index``, or the item
        doesn't, and ``IndexError`` if the vector has no item ``index``.
        """
        size = self.__size
        count = self.get_vector_length(position)
        if not 0 <= index < count:
            raise IndexError(
                f"Item {describe_int(index)} is outside the vector of {count} "
                f"at {position}"
            )

        if index == 0:
            start = (count + 1) * size
        else:
            start = self.__get_size(position + index * size)
        end = self.__get_size(position + (index + 1) * size)

        return self.__item_offset(position, start, end)

    # get_vector_offsets() and get_vector() read the ends at once and check
    # each item inline, which took a third of the time that a read and a call
    # for each item took: 6 µs against 16 µs for 100 items.

    def get_vector_offsets(self, position: int) -> array[int]:
        """Returns the position of every item of the vector at ``position``,
        as 64-bit unsigned integers, since the buffer may be longer than
        4 GiB, as a memory map of many tables is."""
        ends = self.__get_item_ends(position)
        offsets = _ZERO_OFFSET * len(ends)
        length = self.__length

        start = (len(ends) + 1) * self.__size
        for i, end in enumerate(ends):
            if start > end or position + end > length:
                raise self.__item_error(position, start, end)

            offsets[i] = position + start
            start = end

        return offsets

    def get_vector(self, position: int) -> list[memoryview[int]]:
        """Returns the bytes of every item of the vector at ``position``, as
        views of the buffer."""
        ends = self.__get_item_ends(position)
        view, length = self.__view, self.__length

        vector: list[memoryview[int]] = []
        start = (len(ends) + 1) * self.__size
        for end in ends:
            if start > end or position + end > length:
                raise self.__item_error(position, start, end)

            vector.append(view[position + start : position + end])
            start = end

        return vector

    def set_vector(self, vector: Sequence[Buffer]) -> None:
        """Writes the number of items, the end of each, counted from the
        vector's position, and then the items."""
        sources = [byte_view(item) for item in vector]
        # Item 0 starts after the count and the ends, and the last item ends
        # at the vector's end, which is the largest end
        first = self.__size * (1 + len(sources))
        byte_length = first + sum(len(source) for source in sources)

        # Check that the count, the ends and every item fit before writing
        # any of them
        self.__check_writable(byte_length)
        self.__check_size(len(sources))
        self.__check_size(byte_length)

        self.__set_size(len(sources))

        end = first
        for source in sources:
            end += len(source)
            self.__set_size(end)

        for source in sources:
            self.__write(source)

    def __write(self, source: memoryview) -> None:
        end = self.__position + len(source)
        self.__view[self.__position : end] = source
        self.__position = end

    def __get_item_ends(self, position: int) -> memoryview[int]:
        """Returns the item ends of the vector at ``position``, once they're
        checked to lie within the buffer, before anything is allocated for
        the vector's untrusted count"""
        size = self.__size
        start = position + size
        byte_length = self.get_vector_length(position) * size

        return from_little_endian(
            _SIZE_CODES[size], self.__view[start : start + byte_length]
        )

    def __item_offset(self, position: int, start: int, end: int) -> int:
        """Returns the position of a vector item, from ``start`` to ``end``
        counted from the vector's ``position``, which lies within the buffer.
        Raises ``DecodeError`` if the item starts after it ends, or ends past
        the buffer."""
        if start > end or position + end > self.__length:
            raise self.__item_error(position, start, end)

        return position + start

    def __item_error(self, position: int, start: int, end: int) -> DecodeError:
        """The error for a vector item that starts after it ends, or ends past
        the buffer"""
        if start > end:
            return DecodeError(
                f"The vector item at {position + start} ends before it starts, "
                f"at {position + end}"
            )

        return DecodeError.out_of_range(position + start, end - start, self.__length)

    def __check_range(self, position: int, byte_length: int) -> int:
        """Returns ``position`` if ``byte_length`` bytes from it lie within the
        buffer, and raises ``DecodeError`` otherwise."""
        if position < 0 or position + byte_length > self.__length:
            raise DecodeError.out_of_range(position, byte_length, self.__length)

        return position

    def __check_writable(self, byte_length: int) -> None:
        if self.__position + byte_length > self.__length:
            raise self.__overflow(byte_length)

    def __check_size(self, count: int) -> None:
        """Raises ``ValueError`` if a count, length or vector item end doesn't
        fit in a slot, before anything is written."""
        max_size = MAX_LENGTHS[self.__size]
        if count > max_size:
            raise ValueError(
                f"Can't write {count} at {self.__position}: a "
                f"{self.__layout.offset_type()} count, length or item end holds "
                f"at most {max_size}"
            )

    def __overflow(self, byte_length: int) -> ValueError:
        return ValueError(
            f"Write exceeds the supplied buffer: {byte_length} bytes at "
            f"{self.__position} of {self.__length}"
        )

    def __get_size(self, position: int) -> int:
        try:
            if position >= 0:
                return self.__size_struct.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, self.__size, self.__length)

    def __set_size(self, value: int) -> None:
        self.__pack(self.__size_struct, value)
        self.__position += self.__size


def struct_count(byte_length: int, size: int) -> int:
    """Returns the number of structs of ``size`` bytes in ``byte_length``
    bytes. Raises ``ValueError`` if ``size`` is less than 1, or the bytes
    aren't a whole number of structs."""
    _check_struct_size(size)
    if byte_length % size != 0:
        raise ValueError(
            f"{byte_length} bytes aren't a whole number of "
            f"{describe_int(size)}-byte structs"
        )

    return byte_length // size


def _check_struct_size(size: int) -> None:
    if size < 1:
        raise ValueError(
            f"A struct's size must be a positive integer, not {describe_int(size)}"
        )


def byte_view(buffer: Buffer) -> memoryview:
    """Returns the bytes of a C-contiguous buffer, as a one-dimensional view.

    Raises ``TypeError`` for a buffer that isn't C-contiguous, such as a slice
    with a step, as ``memoryview.cast()`` does, and as ``bytearray`` does when
    ``TableWriter`` appends it. A one-dimensional view of bytes needs no cast,
    so it's checked here.
    """
    view = memoryview(buffer)
    if not view.c_contiguous:
        raise _not_contiguous()

    return view if view.format == "B" and view.ndim == 1 else view.cast("B")


def _not_contiguous() -> TypeError:
    return TypeError(
        "Can't view the bytes of a buffer that isn't C-contiguous, such as a "
        "slice with a step"
    )
