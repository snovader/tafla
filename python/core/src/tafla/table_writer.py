import builtins
from collections.abc import Buffer, Iterable, Iterator, Sequence
from struct import error as StructError
from typing import Any, Literal, Self, overload

from . import structs
from .constants import SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64
from .elements import as_elements, little_endian_bytes, struct_count
from .errors import describe_int
from .fixed_fields import FixedFields
from .layout import (
    ALIGNMENTS,
    MAX_LENGTHS,
    OFFSET_TYPES,
    TableLayout,
    alignment_error,
    header_length,
)
from .structs import OFFSET_STRUCTS
from .types import ElementSize, OffsetSize, TypeCode

# The scalars' Structs, assigned rather than imported, as in byte_buffer.py,
# so that their methods' calls specialize
int8, uint8 = structs.int8, structs.uint8
int16, uint16 = structs.int16, structs.uint16
int32, uint32 = structs.int32, structs.uint32
int64, uint64 = structs.int64, structs.uint64
float32, float64 = structs.float32, structs.float64

# Zeros by their number: those that pad a field to its alignment, and those
# that hold the offsets of a nested table of up to 16 fields until they're
# written. Appending them rather than a new bytes() made writing a vector of
# 100 tables that each nest another 5% faster.
_ZEROS = tuple(bytes(length) for length in range(16 * SIZEOF_32 + 1))

# What struct raises for a value or slot that it can't write: struct.error for
# an integer out of range or a slot past the bytes, OverflowError for a float
# too large for a float32, and for a slot beyond a C ssize_t, which
# pack_into() raises as IndexError in Python 3.14
_PACK_ERRORS = (StructError, OverflowError, IndexError)

# What set_fields() and write_table() fall back to the setters for: what
# struct raises, and the UnicodeEncodeError of a lone surrogate in a string
_FIELD_ERRORS = (*_PACK_ERRORS, UnicodeEncodeError)


class TableWriter:
    """Writes a table forward, appending each field to its bytes as it's set,
    for code that knows a table's fields as it writes them, such as the code
    that tafla-gen generates. Unlike ``TableBuilder``, it doesn't record the
    fields first, which takes most of a small table's time.

    Fields are set in order, each with its index in its table. A nested table
    follows its parent's ``set_table()``, between ``start_table()`` and
    ``end_table()``; the items of a vector follow its ``set_vector()`` the same
    way, each followed by ``set_item_end()``, which writes where it ends. Each
    field's offset is written as the writer reaches the field. ``TableBuilder``
    writes through a writer too, once it has recorded its fields and counted
    their length, so the two write the same bytes for the same fields, and
    convert arrays, strings and structs the same way, with the same errors: a
    struct that isn't a C-contiguous buffer raises ``TypeError``.

    ``set_fields()`` writes a table's leading fields whose offsets depend on no
    value, which a ``FixedFields`` describes, in one call: scalars, structs,
    tables nested with only those, and a string after them. ``write_table()``
    writes a nested table of only those fields whole. Both write the bytes
    that the setters would, and raise what they would.

    It's the target of the code that tafla-gen generates, whose calls follow
    from the schema, so it doesn't check them again: every field is set, in
    index order, each once, and each vector item is followed by its
    ``set_item_end()``. A field index outside its table, or an item outside its
    vector, writes an offset or end over another slot or field, so the table's
    bytes are undefined, though never outside them: a slot past them raises
    ``ValueError``. Without a sizer, the writer can't tell a field that wasn't
    set: ``bytes()`` returns a table without it, whose offset is 0.
    Hand-written tables use ``TableBuilder``, which checks every field.

    The bytes grow as fields are appended, and ``bytes()`` returns a view of
    them, without copying them. While the view is held, a field that would
    change their length raises ``BufferError``. A table too long for the
    layout's ``offset_type`` raises ``ValueError``, from the setter whose
    offset or count doesn't fit or from ``bytes()``. A scalar setter that
    raised wrote nothing, but another setter that raised may have written part
    of its field, after which the writer is of no further use.
    """

    __slots__ = [
        "__data",
        "__header_size",
        "__length",
        "__max_length",
        "__size",
        "__sized",
        "__slots",
        "__table",
    ]

    __size: OffsetSize

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
        """``fields`` is the number of fields of the table, and ``layout`` its
        header and offset type, which its readers must use too. ``version`` is
        written to the header, and required when the layout is versioned.

        Raises ``TypeError`` if ``version`` is given for a layout without a
        version slot, or missing for one with it. Raises ``ValueError`` if
        ``version`` doesn't fit in the layout's offset type, or the header alone
        is longer than the layout addresses.
        """
        size = layout.offset_size()
        data = bytearray(header_length(fields, layout, version))

        # The offset size, the width of offsets, lengths and header slots
        self.__size = size
        self.__length = OFFSET_STRUCTS[size]
        self.__max_length = MAX_LENGTHS[size]
        self.__sized = layout.sized()
        self.__header_size = layout.header_size()
        self.__data = data
        # The start of the table being written, which its offsets count from,
        # and the position of its first offset, after the root's header
        self.__table = 0
        self.__slots = self.__header_size

        if version is not None:
            self.__length.pack_into(data, size if self.__sized else 0, version)

    @classmethod
    def _checked(cls, fields: int, layout: TableLayout, version: int | None) -> Self:
        """Returns a writer as the constructor does, for arguments that
        ``TableBuilder``'s constructor checked already, without checking them
        again."""
        # The constructor's assignments, repeated, since a helper that both
        # called would add a call to the public constructor, which generated
        # code makes for every message. Without the checks, the builder wrote
        # its record 4% faster.
        writer = cls.__new__(cls)
        size = layout.offset_size()
        sized = layout.sized()
        header_size = layout.header_size()
        data = bytearray(header_size + fields * size)

        writer.__size = size
        writer.__length = OFFSET_STRUCTS[size]
        writer.__max_length = MAX_LENGTHS[size]
        writer.__sized = sized
        writer.__header_size = header_size
        writer.__data = data
        writer.__table = 0
        writer.__slots = header_size

        if version is not None:
            writer.__length.pack_into(data, size if sized else 0, version)

        return writer

    def bytes(self) -> memoryview[int]:
        """Returns a view of the table's bytes, without copying them. While
        it's held, a field that would change their length raises
        ``BufferError``, as a ``bytearray``'s does. ``memoryview(writer)``
        returns the same view.

        Raises ``ValueError`` if the table is longer than its layout addresses.
        """
        data = self.__data
        if len(data) > self.__max_length:
            raise self.__too_long(len(data))

        if self.__sized:
            self.__length.pack_into(data, 0, len(data))

        return memoryview(data)

    def __buffer__(self, flags: int, /) -> memoryview[int]:
        """Exports the view that ``bytes()`` returns, so that
        ``memoryview(writer)`` is ``writer.bytes()``"""
        return self.bytes()

    def _copy_table(self, table: Buffer) -> None:
        """Writes a table that was written already, from its bytes as they
        are, in place of the fields that ``start_table()`` and ``end_table()``
        would enclose: after its parent's ``set_table()``, or as a vector item
        before its ``set_item_end()``. ``TableBuilder`` copies the tables that
        it can't write in place with it: tables nested as bytes, builders that
        were written already, and builders whose layout has a header or
        another offset type. Generated code writes every nested table in
        place, so it isn't public."""
        self.__data += table

    def start_table(self, fields: int) -> int:
        """Starts the fields of a nested table or vector item of ``fields``
        fields, after its parent's ``set_table()`` or ``set_vector()``, and
        returns the parent's start, which ``end_table()`` takes back."""
        parent = self.__table
        data = self.__data

        self.__table = self.__slots = len(data)
        slots = fields * self.__size
        data += _ZEROS[slots] if slots < len(_ZEROS) else bytes(slots)

        return parent

    def end_table(self, parent: int) -> None:
        """Ends the fields of the table that ``start_table()`` started"""
        self.__table = parent
        # Only the root starts at 0, and only its offsets follow a header
        self.__slots = self.__header_size if parent == 0 else parent

    # set_fields() and write_table() pack fields of fixed offsets in one call,
    # and fall back to the setters for a value that the Struct can't pack, so
    # that they raise what the setters raise, for the first field that can't
    # be written, after writing those before it. A lone surrogate in the
    # string falls back too, so that a scalar before it raises first. Each
    # packs and falls back itself, rather than through a shared helper, whose
    # call every table that they write would make.

    def set_fields(
        self, fields: FixedFields, values: tuple[Any, ...], string: str = ""
    ) -> None:
        """Writes the leading fields that ``fields`` describes, from their
        ``values`` in order, with a nested table's values in place of it, and
        ``string`` when they end with a string, as their setters would. It's
        called first, after the constructor or ``start_table()``, in place of
        those fields' setters, and the table's other fields follow with
        theirs.

        Generated code calls it for the messages and tables that start with
        such fields, and ``write_table()`` for nested tables of only those.
        Code written by hand calls the setters, or uses ``TableBuilder``."""
        pack = fields._packers[self.__size]
        try:
            if fields._string:
                encoded = string.encode()
                packed = pack(*values, len(encoded))
            else:
                encoded = b""
                packed = pack(*values)
        except _FIELD_ERRORS:
            self.__set_each(fields, iter(values), string)
            return

        # The packed fields start with the table's slots, which the
        # constructor or start_table() wrote as zeros
        data = self.__data
        data[self.__slots :] = packed
        data += encoded

    def write_table(
        self, fields: FixedFields, values: tuple[Any, ...], string: str = ""
    ) -> None:
        """Writes a nested table whose fields ``fields`` describes whole, from
        their ``values`` as ``set_fields()`` takes them, in place of
        ``start_table()``, their setters and ``end_table()``: after its
        parent's ``set_table()``, or as a vector item before its
        ``set_item_end()``."""
        pack = fields._packers[self.__size]
        try:
            if fields._string:
                encoded = string.encode()
                packed = pack(*values, len(encoded))
            else:
                encoded = b""
                packed = pack(*values)
        except _FIELD_ERRORS:
            parent = self.start_table(fields._fields)
            self.__set_each(fields, iter(values), string)
            self.end_table(parent)
            return

        data = self.__data
        data += packed
        data += encoded

    def __set_each(
        self, fields: FixedFields, values: Iterator[Any], string: str
    ) -> None:
        """Writes the fields that ``fields`` describes with their setters, from
        ``values``, as they're written without ``FixedFields``"""
        for index, format in enumerate(fields._formats):
            if isinstance(format, FixedFields):
                # A table of fixed-size fields has no arrays to align
                self.set_table(index, 1)
                parent = self.start_table(format._fields)
                self.__set_each(format, values, "")
                self.end_table(parent)
            elif format == "?":
                self.set_uint8(index, 1 if next(values) else 0)
            elif format in _SETTERS:
                _SETTERS[format](self, index, next(values))
            else:
                self.set_struct(index, next(values))

        if fields._string:
            self.set_string(len(fields._formats), string)

    # The scalar setters are written out, with the offset's write, since a
    # shared helper's call made writing a Point a fifth slower. struct raises
    # _PACK_ERRORS, none of which is a ValueError, and __scalar_error() turns
    # them into ValueError. It's the offset that doesn't fit when the table
    # is longer than its offset type addresses. Each packs its value before
    # it writes the offset, so a setter that raises writes nothing.

    def set_int8(self, index: int, value: int) -> None:
        data = self.__data
        offset = len(data) - self.__table
        try:
            packed = int8.pack(value)
            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
        except _PACK_ERRORS as error:
            raise self.__scalar_error(value, offset, error) from error

        data += packed

    def set_uint8(self, index: int, value: int) -> None:
        data = self.__data
        offset = len(data) - self.__table
        try:
            packed = uint8.pack(value)
            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
        except _PACK_ERRORS as error:
            raise self.__scalar_error(value, offset, error) from error

        data += packed

    def set_int16(self, index: int, value: int) -> None:
        data = self.__data
        offset = len(data) - self.__table
        try:
            packed = int16.pack(value)
            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
        except _PACK_ERRORS as error:
            raise self.__scalar_error(value, offset, error) from error

        data += packed

    def set_uint16(self, index: int, value: int) -> None:
        data = self.__data
        offset = len(data) - self.__table
        try:
            packed = uint16.pack(value)
            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
        except _PACK_ERRORS as error:
            raise self.__scalar_error(value, offset, error) from error

        data += packed

    def set_int32(self, index: int, value: int) -> None:
        data = self.__data
        offset = len(data) - self.__table
        try:
            packed = int32.pack(value)
            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
        except _PACK_ERRORS as error:
            raise self.__scalar_error(value, offset, error) from error

        data += packed

    def set_uint32(self, index: int, value: int) -> None:
        data = self.__data
        offset = len(data) - self.__table
        try:
            packed = uint32.pack(value)
            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
        except _PACK_ERRORS as error:
            raise self.__scalar_error(value, offset, error) from error

        data += packed

    def set_int64(self, index: int, value: int) -> None:
        data = self.__data
        offset = len(data) - self.__table
        try:
            packed = int64.pack(value)
            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
        except _PACK_ERRORS as error:
            raise self.__scalar_error(value, offset, error) from error

        data += packed

    def set_uint64(self, index: int, value: int) -> None:
        data = self.__data
        offset = len(data) - self.__table
        try:
            packed = uint64.pack(value)
            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
        except _PACK_ERRORS as error:
            raise self.__scalar_error(value, offset, error) from error

        data += packed

    def set_float32(self, index: int, value: float) -> None:
        data = self.__data
        offset = len(data) - self.__table
        try:
            packed = float32.pack(value)
            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
        except _PACK_ERRORS as error:
            raise self.__scalar_error(value, offset, error) from error

        data += packed

    def set_float64(self, index: int, value: float) -> None:
        data = self.__data
        offset = len(data) - self.__table
        try:
            packed = float64.pack(value)
            self.__length.pack_into(data, self.__slots + index * self.__size, offset)
        except _PACK_ERRORS as error:
            raise self.__scalar_error(value, offset, error) from error

        data += packed

    # Each array is converted as TableBuilder converts it: a contiguous buffer
    # of the field's element type is copied as bytes, and anything else, such
    # as a list, converted as array() converts an iterable, which raises
    # OverflowError for an integer out of range and TypeError for a value of
    # another type, and converts a float past float32's range to an infinity.

    def set_int8_array(self, index: int, values: Buffer | Iterable[int]) -> None:
        self.__set_array(index, "b", SIZEOF_8, values)

    def set_uint8_array(self, index: int, values: Buffer | Iterable[int]) -> None:
        self.__set_array(index, "B", SIZEOF_8, values)

    def set_int16_array(self, index: int, values: Buffer | Iterable[int]) -> None:
        self.__set_array(index, "h", SIZEOF_16, values)

    def set_uint16_array(self, index: int, values: Buffer | Iterable[int]) -> None:
        self.__set_array(index, "H", SIZEOF_16, values)

    def set_int32_array(self, index: int, values: Buffer | Iterable[int]) -> None:
        self.__set_array(index, "i", SIZEOF_32, values)

    def set_uint32_array(self, index: int, values: Buffer | Iterable[int]) -> None:
        self.__set_array(index, "I", SIZEOF_32, values)

    def set_int64_array(self, index: int, values: Buffer | Iterable[int]) -> None:
        self.__set_array(index, "q", SIZEOF_64, values)

    def set_uint64_array(self, index: int, values: Buffer | Iterable[int]) -> None:
        self.__set_array(index, "Q", SIZEOF_64, values)

    def set_float32_array(self, index: int, values: Buffer | Iterable[float]) -> None:
        self.__set_array(index, "f", SIZEOF_32, values)

    def set_float64_array(self, index: int, values: Buffer | Iterable[float]) -> None:
        self.__set_array(index, "d", SIZEOF_64, values)

    def set_string(self, index: int, string: str) -> None:
        """Writes ``string`` as UTF-8. Raises ``UnicodeEncodeError``, a
        ``ValueError``, if it has a lone surrogate, which UTF-8 can't encode."""
        encoded = string.encode()

        self.__set_offset(index)
        self.__set_count(len(encoded), len(encoded))
        self.__data += encoded

    def _set_encoded_string(self, index: int, encoded: builtins.bytes) -> None:
        """Writes a string that's encoded as UTF-8 already, as ``set_string()``
        writes it. ``TableBuilder`` encodes a string outside ASCII when it's
        set, to count its bytes, and writes them with it. Generated code
        writes strings with ``set_string()``, so it isn't public, and this
        repeats its write rather than ``set_string()`` calling it, which would
        add a call to every string that the generated code writes."""
        self.__set_offset(index)
        self.__set_count(len(encoded), len(encoded))
        self.__data += encoded

    def set_struct(self, index: int, data: Buffer) -> None:
        """Writes a struct field from its bytes: its scalars, little-endian, one
        after another, without padding. A ``struct.Struct`` with a ``<`` format
        packs them."""
        self.__set_offset(index)
        self.__data += data

    def set_struct_array(
        self, index: int, data: Buffer, size: int, alignment: ElementSize
    ) -> None:
        """Writes an array of structs of ``size`` bytes each, from their bytes
        one after another, padded so that they start at a multiple of
        ``alignment`` from the start of the table.

        Raises ``ValueError`` if ``size`` is less than 1, ``data`` isn't a
        whole number of structs, or ``alignment`` isn't 1, 2, 4 or 8.
        """
        byte_length = memoryview(data).nbytes
        count = struct_count(byte_length, size)
        if type(alignment) is not int or alignment not in ALIGNMENTS:
            raise alignment_error(alignment)

        self.__start_array(index, count, byte_length, alignment)
        self.__data += data

    def set_table(self, index: int, alignment: ElementSize) -> None:
        """Writes the offset of a nested table, padded to start at a multiple
        of ``alignment``, whose fields then follow between ``start_table()``
        and ``end_table()``.

        Raises ``ValueError`` if ``alignment`` isn't 1, 2, 4 or 8.
        """
        # Checked inline, as in set_struct_array(), rather than by
        # padding_to(), whose call made writing a vector of 100 tables that
        # each nest another 3% slower. The type comes first, since 2.0 is in
        # ALIGNMENTS and a list isn't hashable.
        if type(alignment) is not int or alignment not in ALIGNMENTS:
            raise alignment_error(alignment)

        # Padded and its offset written inline too, as __pad() and
        # __set_offset() do, since every nested table and vector item calls
        # it: the calls made writing the vector of 100 tables 3% slower
        data = self.__data
        table = self.__table
        padding = (table - len(data)) % alignment
        if padding:
            data += _ZEROS[padding]
        try:
            self.__length.pack_into(
                data, self.__slots + index * self.__size, len(data) - table
            )
        except _PACK_ERRORS as error:
            raise self.__slot_error(len(data) - table, error) from error

    def set_vector(self, index: int, count: int) -> int:
        """Writes the offset and count of a vector of ``count`` tables, whose
        items then follow, each between ``start_table()`` and ``end_table()``
        and then ``set_item_end()``. Returns the vector's position, which
        ``set_item_end()`` takes."""
        self.__set_offset(index)

        vector = len(self.__data)
        ends = count * self.__size
        self.__set_count(count, ends)
        self.__data += bytes(ends)

        return vector

    def set_item_end(self, vector: int, item: int) -> None:
        """Writes the end of item ``item`` of the vector, which was just
        written"""
        data = self.__data
        try:
            self.__length.pack_into(
                data, vector + (item + 1) * self.__size, len(data) - vector
            )
        except _PACK_ERRORS as error:
            raise self.__slot_error(len(data) - vector, error) from error

    def set_vector_tables(
        self, index: int, fields: FixedFields, items: Sequence[tuple[Any, ...]]
    ) -> None:
        """Writes field ``index`` as a vector of tables whose fields ``fields``
        describes whole, one for each of ``items``, as ``set_vector()``,
        ``write_table()`` for each item and ``set_item_end()`` write it. Each
        item is a table's values as ``write_table()`` takes them, followed by
        its string when ``fields`` ends with one: the tuple that
        ``ByteBuffer.get_fields()`` returns.

        Raises ``ValueError`` if iterating ``items`` gives more or fewer than
        their ``len()``, after which the table is undefined.

        Generated code calls it to write a vector of such tables."""
        count = len(items)
        vector = self.set_vector(index, count)
        has_string = fields._string
        pack = fields._packers[self.__size]
        data = self.__data

        # write_table()'s packing, inline, which saves a call for each item
        item = -1
        for item, values in enumerate(items):
            if has_string:
                table, string = values[:-1], values[-1]
            else:
                table, string = values, ""
            try:
                if has_string:
                    encoded = string.encode()
                    packed = pack(*table, len(encoded))
                else:
                    encoded = b""
                    packed = pack(*table)
            except _FIELD_ERRORS:
                self.write_table(fields, table, string)
            else:
                data += packed
                data += encoded

            self.set_item_end(vector, item)

        # A sequence whose len() isn't its number of items has ends over its
        # items, or ends left at 0, and the table is undefined
        if item + 1 != count:
            raise _items_error(count, item + 1)

    def __set_array(
        self,
        index: int,
        type_code: TypeCode,
        item_size: ElementSize,
        values: Buffer | Iterable[float],
    ) -> None:
        source = little_endian_bytes(type_code, as_elements(type_code, values))

        self.__start_array(index, len(source) // item_size, len(source), item_size)
        self.__data += source

    def __start_array(
        self, index: int, count: int, byte_length: int, alignment: ElementSize
    ) -> None:
        """Pads the array so that its ``byte_length`` bytes of elements, after
        the count, are aligned, and writes its offset and count"""
        self.__pad(len(self.__data) + self.__size, alignment)
        self.__set_offset(index)
        self.__set_count(count, byte_length)

    def __pad(self, position: int, alignment: ElementSize) -> None:
        """Pads so that ``position`` lies at a multiple of ``alignment`` from
        the start of the table. The setters check a caller's alignment, and
        an array's is its element size.
        """
        padding = (self.__table - position) % alignment
        if padding:
            self.__data += _ZEROS[padding]

    def __set_offset(self, index: int) -> None:
        """Writes the offset of field ``index``, which is the table's end"""
        data = self.__data
        try:
            self.__length.pack_into(
                data, self.__slots + index * self.__size, len(data) - self.__table
            )
        except _PACK_ERRORS as error:
            raise self.__slot_error(len(data) - self.__table, error) from error

    def __set_count(self, count: int, byte_length: int) -> None:
        """Appends a count or length, as wide as the offsets, of the
        ``byte_length`` bytes that follow it"""
        try:
            self.__data += self.__length.pack(count)
        except StructError as error:
            length = len(self.__data) + self.__size + byte_length
            raise self.__too_long(length) from error

    def __scalar_error(self, value: float, offset: int, error: Exception) -> ValueError:
        if offset > self.__max_length:
            return self.__too_long(len(self.__data))

        return ValueError(f"Can't write {value!r} at {len(self.__data)}: {error}")

    def __slot_error(self, value: int, error: Exception) -> ValueError:
        """The error of an offset or item end that wasn't written: one too
        large for the offset type, or one whose slot lies outside the bytes,
        which only an index or item that isn't its own has"""
        if value > self.__max_length:
            return self.__too_long(len(self.__data))

        return ValueError(
            f"Can't write {describe_int(value)} in a slot outside the table: {error}"
        )

    def __too_long(self, length: int) -> ValueError:
        """The error of a table whose fields make it ``length`` bytes, too long
        for its offset type, which an offset, count or item end then doesn't
        fit in"""
        offset_type = OFFSET_TYPES[self.__size]
        return ValueError(
            f"The table's fields make it {length} bytes, "
            f"too long for {offset_type} offsets"
        )


def _items_error(length: int, count: int) -> ValueError:
    """The error of items whose ``len()`` was ``length``, but which iterating
    gave ``count`` of"""
    return ValueError(f"The items' len() is {length}, but iterating them gave {count}")


# The setter of each scalar's struct format code, which FixedFields describes
# a field by
_SETTERS = {
    "b": TableWriter.set_int8,
    "B": TableWriter.set_uint8,
    "h": TableWriter.set_int16,
    "H": TableWriter.set_uint16,
    "i": TableWriter.set_int32,
    "I": TableWriter.set_uint32,
    "q": TableWriter.set_int64,
    "Q": TableWriter.set_uint64,
    "f": TableWriter.set_float32,
    "d": TableWriter.set_float64,
}
