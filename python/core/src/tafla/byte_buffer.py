from array import array
from collections.abc import Buffer, Callable, Iterator
from struct import error as StructError
from typing import Any, overload

from . import structs
from .constants import SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64
from .elements import check_struct_size, from_little_endian, not_contiguous
from .errors import DecodeError, describe_int
from .fixed_fields import FixedFields
from .layout import TableLayout
from .structs import OFFSET_STRUCTS
from .types import FloatTypeCode, IntTypeCode, OffsetSize, TypeCode

# The scalars' Structs, assigned rather than imported. Python 3.14 compiles a
# method call on a name that an import bound as a load of a module's
# attribute, which doesn't specialize for a Struct and creates a bound method
# for each call: each scalar's read took 25 ns longer, and reading a vector of
# 100 tables that each nest another 8% longer.
int8, uint8 = structs.int8, structs.uint8
int16, uint16 = structs.int16, structs.uint16
int32, uint32 = structs.int32, structs.uint32
int64, uint64 = structs.int64, structs.uint64
float32, float64 = structs.float32, structs.float64

# The array type codes of the offset sizes' unsigned integers, which the ends
# of a vector's items are read as
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
    """Reads a table within the supplied bytes, in the table layout it was
    written in, whose multi-byte values are little-endian. Array reads return
    views of the bytes on a little-endian host, whatever their alignment, and
    copies in the host's byte order on a big-endian host.

    Every read checks that it stays within the bytes and raises ``DecodeError``
    otherwise, at any position, however large. The bytes must be C-contiguous,
    and bytes that aren't raise ``TypeError``.

    Tables are written with ``TableBuilder``, whose ``finalize()`` returns a
    ``ByteBuffer`` that reads the table, or with ``TableWriter``.
    """

    __slots__ = [
        "__header_size",
        "__layout",
        "__length",
        "__size",
        "__size_struct",
        "__view",
    ]

    __size: OffsetSize

    def __init__(
        self, bytes: Buffer, layout: TableLayout = TableLayout.DEFAULT
    ) -> None:
        """Reads ``bytes`` in ``layout``, which must be the layout that the
        table was written in.

        Raises ``TypeError`` if ``bytes`` isn't C-contiguous, such as a slice
        with a step. Reads return views of the bytes, which a copy of them
        wouldn't be.
        """
        # byte_view() inline, since its call made a constructor 10% slower,
        # and every reader that tafla-gen generates constructs one
        view = memoryview(bytes)
        if not view.c_contiguous:
            raise not_contiguous()

        self.__view = view if view.format == "B" and view.ndim == 1 else view.cast("B")
        self.__length = len(self.__view)

        # The layout itself, rather than copies of the flags that only
        # get_size() and get_version() read. Copying those, and what its
        # former setters used, took a quarter of the constructor's time:
        # 0.32 µs rather than 0.24 µs.
        self.__layout = layout
        # The offset size, the width of offsets, lengths and header slots
        self.__size = size = layout.offset_size()
        self.__size_struct = OFFSET_STRUCTS[size]
        # The byte length of the header slots before the root's field offsets
        self.__header_size = layout.header_size()

    def bytes(self) -> memoryview:
        return self.__view

    def get_size(self) -> int:
        return self.__get_size(0) if self.__layout.sized() else self.__length

    def get_version(self) -> int:
        """Returns the version that the table's header carries after its
        length. It's as wide as the header's other slots.

        Raises ``TypeError`` if the buffer's layout isn't versioned.
        """
        layout = self.__layout
        if not layout.versioned():
            raise TypeError("The table has no version")

        return self.__get_size(self.__size if layout.sized() else 0)

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
        check_struct_size(size)

        byte_length = self.__get_size(position) * size
        start = self.__check_range(position + self.__size, byte_length)

        return self.__view[start : start + byte_length]

    def get_string(self, position: int) -> str:
        """Raises ``DecodeError`` if the string isn't valid UTF-8."""
        length = self.__get_size(position)
        start = self.__check_range(position + self.__size, length)

        try:
            return str(self.__view[start : start + length], "utf-8")
        except UnicodeDecodeError as error:
            raise DecodeError(f"The string at {position} isn't valid UTF-8") from error

    def get_fields(self, position: int, fields: FixedFields) -> tuple[Any, ...] | None:
        """Returns the values of the fields that ``fields`` describes, of the
        table at ``position``, unpacked in one call, in the order that
        ``TableWriter.set_fields()`` takes them, with the string's last. It's
        for a table whose fields it describes whole, when its offsets are the
        constants that ``TableWriter`` writes them at. Otherwise, or when the
        fields don't lie within the buffer or the string isn't valid UTF-8,
        it returns ``None``, for the field getters to read them, which then
        raise ``DecodeError`` for whatever they can't read. Each value is what
        its getter returns, but a struct's, which is its bytes.

        Generated code calls it to read every field of such a table at once.
        """
        struct, header, check, expected, values, string = fields._readers[self.__size]
        start = position + header
        try:
            if position >= 0:
                unpacked = struct.unpack_from(self.__view, start)
                if check(unpacked) == expected:
                    if not string:
                        return values(unpacked)

                    # The string's length is the last value, and its bytes
                    # follow the Struct's
                    begin = start + struct.size
                    end = begin + unpacked[-1]
                    if end <= self.__length:
                        text = str(self.__view[begin:end], "utf-8")
                        return (*values(unpacked), text)
        except StructError, OverflowError, UnicodeDecodeError:
            pass

        return None

    @staticmethod
    def read_fields(
        data: Buffer, fields: FixedFields, version: int | None = None
    ) -> tuple[Any, ...] | None:
        """Returns the values of the fields that ``fields`` describes whole, a
        message's, of the message in ``data``, in the layout that ``fields``
        was created with, as ``ByteBuffer(data, layout).get_fields(0,
        fields)`` returns them, or ``None`` as it does, without creating a
        ``ByteBuffer``. It returns ``None`` too if ``version`` isn't the one
        that the header holds, for the reader's constructor to raise for.

        Raises ``TypeError`` if ``data`` isn't C-contiguous, if ``fields`` has
        no layout, as a nested table's doesn't, and if ``version`` is given
        for a layout without a version.

        Generated code calls it to read every field of a message of only such
        fields at once, without a reader."""
        # The constructor's view, without the ByteBuffer
        view = memoryview(data)
        if not view.c_contiguous:
            raise not_contiguous()
        if view.format != "B" or view.ndim != 1:
            view = view.cast("B")

        header = fields._header
        if header is None:
            raise TypeError("A nested table's fields have no layout to read")
        size, _, sized, versioned = header
        if version is not None and not versioned:
            raise TypeError("The table has no version")

        struct, start, check, expected, values, string = fields._readers[size]
        try:
            if version is not None:
                position = size if sized else 0
                if OFFSET_STRUCTS[size].unpack_from(view, position)[0] != version:
                    return None

            # get_fields() of the root, inline
            unpacked = struct.unpack_from(view, start)
            if check(unpacked) == expected:
                if not string:
                    return values(unpacked)

                begin = start + struct.size
                end = begin + unpacked[-1]
                if end <= len(view):
                    return (*values(unpacked), str(view[begin:end], "utf-8"))
        except StructError, OverflowError, UnicodeDecodeError:
            pass

        return None

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

    def get_vector_item_offset(self, position: int, count: int, index: int) -> int:
        """Returns the position of item ``index`` of the vector at
        ``position``, as ``get_vector_offset()`` does, for a vector whose
        ``count`` ``get_vector_length()`` returned, which it doesn't read or
        check again. A view of the vector checks its count once, and passes it
        for each item.

        Raises ``IndexError`` if ``index`` isn't below ``count``, and
        ``DecodeError`` if the item's ends don't lie within the buffer, or the
        item doesn't.
        """
        # get_vector_offset()'s checks of the index and the item, repeated
        # rather than shared, since a helper would add a call to each item
        if not 0 <= index < count:
            raise IndexError(
                f"Item {describe_int(index)} is outside the vector of {count} "
                f"at {position}"
            )

        size = self.__size
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

    def get_vector_fields(
        self, position: int, fields: FixedFields
    ) -> list[tuple[Any, ...]] | None:
        """Returns the values of every item of the vector at ``position``, a
        table whose fields ``fields`` describes whole, each as
        ``get_fields()`` returns them, or ``None`` when it returns ``None``
        for any item, for the field getters to read them.

        It's all the items or none: every implementation's writer writes such
        a table at the offsets that ``get_fields()`` expects, so an item that
        it can't read is corrupted or was written by hand, and the getters,
        which check each read, then read the whole vector.

        Raises ``DecodeError`` if the vector's count or an item's ends don't
        lie within the buffer, as ``get_vector_offsets()`` does.

        Generated code calls it to read every field of every item at once.
        """
        offsets = self.get_vector_offsets(position)
        struct, header, check, expected, values, string = fields._readers[self.__size]
        view, length = self.__view, self.__length

        # get_fields() for each item, inline, which saves a call for each.
        # Each offset lies within the buffer, so none is negative.
        items: list[tuple[Any, ...]] = []
        for offset in offsets:
            start = offset + header
            try:
                unpacked = struct.unpack_from(view, start)
                if check(unpacked) != expected:
                    return None
                if not string:
                    items.append(values(unpacked))
                    continue

                begin = start + struct.size
                end = begin + unpacked[-1]
                if end > length:
                    return None
                text = str(view[begin:end], "utf-8")
            except StructError, OverflowError, UnicodeDecodeError:
                return None

            items.append((*values(unpacked), text))

        return items

    def iter_vector_fields[T](
        self, position: int, fields: FixedFields, read: Callable[[int], T]
    ) -> Iterator[tuple[Any, ...] | T]:
        """Returns an iterator over the values of every item of the vector at
        ``position``, a table whose fields ``fields`` describes whole: each
        item's as ``get_fields()`` returns them, or, for an item whose offsets
        aren't those that its writer writes, what ``read`` returns for the
        item's position, which the item's getters read.

        It reads each item's end when it reaches the item, after the end of
        the item before it, so it raises ``DecodeError`` for an item that
        doesn't lie within the buffer when it reaches it, as
        ``get_vector_offsets()`` raises for it, after the items before it.

        Raises ``DecodeError`` at once if the vector's count or the ends after
        it don't lie within the buffer, as ``get_vector_length()`` does.

        Generated code calls it to iterate over the values of a vector's
        items, without reading them all at once.
        """
        ends = self.__get_item_ends(position)

        return self.__iter_vector_fields(position, ends, fields, read)

    def __iter_vector_fields[T](
        self,
        position: int,
        ends: memoryview[int],
        fields: FixedFields,
        read: Callable[[int], T],
    ) -> Iterator[tuple[Any, ...] | T]:
        # get_vector_fields()'s reads, an item at a time
        struct, header, check, expected, values, string = fields._readers[self.__size]
        view, length = self.__view, self.__length

        start = (len(ends) + 1) * self.__size
        for end in ends:
            if start > end or position + end > length:
                raise self.__item_error(position, start, end)
            offset = position + start
            start = end

            item = offset + header
            try:
                unpacked = struct.unpack_from(view, item)
                if check(unpacked) == expected:
                    if not string:
                        yield values(unpacked)
                        continue

                    begin = item + struct.size
                    stop = begin + unpacked[-1]
                    if stop <= length:
                        yield (*values(unpacked), str(view[begin:stop], "utf-8"))
                        continue
            except StructError, OverflowError, UnicodeDecodeError:
                pass

            yield read(offset)

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

    def __get_size(self, position: int) -> int:
        try:
            if position >= 0:
                return self.__size_struct.unpack_from(self.__view, position)[0]
        except StructError, OverflowError:
            pass
        raise DecodeError.out_of_range(position, self.__size, self.__length)
