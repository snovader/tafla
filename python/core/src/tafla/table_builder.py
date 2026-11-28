from array import array
from collections.abc import Buffer, Callable, Collection, Iterable, Sequence
from itertools import accumulate
from operator import index as _index
from struct import Struct, error as StructError, pack
from typing import TYPE_CHECKING, Any

from . import elements
from .constants import SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64
from .elements import (
    as_elements,
    bool_bytes,
    byte_view,
    little_endian_bytes,
    str_vector_error,
    struct_count,
)
from .errors import describe_int
from .layout import MAX_LENGTHS, TableLayout, header_length, padding_to
from .structs import (
    OFFSET_CODES,
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
from .table_reader import TableReader
from .types import ElementSize, TypeCode

# Zeros by their number: those that pad a field to its alignment, and those
# that hold the offsets of a nested table of up to 16 fields, or a vector's
# item ends, until they're written, as TableWriter appends them
_ZEROS = tuple(bytes(length) for length in range(16 * SIZEOF_32 + 1))

# What Struct.pack() raises for a value that its format can't hold: struct.error
# for an integer out of range or of another type, and OverflowError for a float
# too large for a float32. A value's own __index__() may raise one of them, or
# IndexError, which the setters can't tell from struct's, so they raise
# ValueError for it as TableWriter's setters do.
_VALUE_ERRORS = (StructError, OverflowError, IndexError)

# Encodes a string as UTF-8 as str's own method, so that a subclass's encode()
# isn't called, and anything but a str raises TypeError, as the native builder
# takes them
_encode = str.encode

# The alignment of a table before any field needs one, as a parent pads a
# table without arrays, which is 1 in effect, since every offset is a
# multiple of 1. It's declared rather than inferred, so that a type checker
# takes a table's alignment after its fields are set for any of them.
_UNALIGNED: ElementSize = SIZEOF_8

# The items of the checkpoint, a list, that the builder keeps for a table
# nested in place while it's written: where the table starts, which it's
# truncated to if it fails, its parent's counters, and the builder's previous
# checkpoint, which is the parent's if the parent is nested in place too. The
# parent's alignment is that of its own fields, which a table that fails
# restores, and as the table's fields raised it, which pads the parent if
# it's nested in place too, and which the parent takes once the table ends.
# The positions move whenever a field pads the tables they're nested in.
_START = 0
_BASE = 1
_SLOTS = 2
_FIELDS = 3
_INDEX = 4
_ALIGNMENT = 5
_RAISED = 6
_IN_PLACE = 7
_PADDING = 8
_PREVIOUS = 9

# Comparisons rather than max() for alignments, and counted indices rather
# than enumerate(), whose iteration Python 3.14 doesn't specialize: each made
# writing the vector of 100 nested tables a few percent faster.


class TableBuilder:
    """Builds a table: a header with one offset per field, after the table's
    total length and version when its layout has them, followed by the fields
    in the order they're set. Arrays, strings and vectors start with their
    count, and a vector, of tables or of strings, then holds the end of each
    item. Arrays of scalars and of structs are padded so their elements are
    aligned from the start of the table. Fields are read back by index with
    ``TableReader.root_offset()``, or ``TableReader.field_offset()`` for a
    table nested in another.

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
        "__checkpoint",
        "__data",
        "__fields",
        "__header_size",
        "__in_place",
        "__index",
        "__layout",
        "__length",
        "__max_length",
        "__padding",
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
    # For a table nested in place, its offset in its parent before its
    # padding, or -1, and the padding before it so far, which aligns it to its
    # alignment so far
    __in_place: int
    __padding: int
    # The checkpoint of the innermost table being written in place, or None
    __checkpoint: list[Any] | None
    # The table once written: its bytes, and then the reader of them that
    # finalize() returns
    __written: memoryview[int] | TableReader | None

    def __init__(self, fields: int, layout: TableLayout = TableLayout.DEFAULT) -> None:
        """``fields`` is the number of fields that the table will hold, and
        ``layout`` its header and offset type, which its readers must use too,
        with the version to write to the header when it has one. Readers can
        check it with ``TableReader.get_version()`` before reading any field.

        Raises ``ValueError`` if the header alone is longer than the layout
        addresses.
        """
        length = header_length(fields, layout)
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
        self.__in_place = -1
        self.__padding = 0
        self.__checkpoint = None
        self.__written = None

        version = layout.version()
        if version is not None:
            self.__length.pack_into(data, size if sized else 0, version)

    def finalize(self) -> TableReader:
        """Finishes the table, once, and returns a ``TableReader`` that reads
        it; later calls return the same reader.

        Raises ``ValueError`` if fewer fields were set than the table holds,
        or if it's called while ``fill`` sets the fields of a table nested in
        place, leaving the builder as it was.
        """
        written = self.__written
        if written is None:
            written = self.__write()
        elif isinstance(written, TableReader):
            return written

        reader = TableReader(written, self.__layout)
        self.__written = reader

        return reader

    def bytes(self) -> memoryview[int]:
        """Finishes the table, once, as ``finalize()`` does, and returns its
        bytes, without the reader that ``finalize()`` creates. Raises as
        ``finalize()`` does."""
        written = self.__written
        if written is None:
            return self.__write()

        return written.bytes() if isinstance(written, TableReader) else written

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

    def set_bool_array(self, values: Iterable[bool]) -> None:
        """Sets an array of bools, which is written as a ``uint8`` array: 1 for
        each value that's true, as ``bool()`` takes it, and 0 for each that's
        false. A value whose truth can't be taken raises its error."""
        self.__set_array("B", SIZEOF_8, bool_bytes(values))

    def set_string(self, string: str) -> None:
        """Writes ``string`` as UTF-8. Raises ``UnicodeEncodeError``, a
        ``ValueError``, if it has a lone surrogate, which UTF-8 can't encode."""
        encoded = _encode(string)
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

    def set_string_vector(self, strings: Iterable[str]) -> None:
        """Writes a vector of strings, each as UTF-8, after the end of each,
        which lets readers find any string without reading the others. It
        takes no more bytes for each string than the end of it, where a
        vector of tables of one string each takes each table's offset and the
        string's count too.

        Raises ``UnicodeEncodeError``, a ``ValueError``, if a string has a
        lone surrogate, which UTF-8 can't encode, and ``TypeError`` for a
        ``str``, which is a sequence of its characters.
        """
        if isinstance(strings, str):
            raise str_vector_error()

        # Each string is encoded as str's own method encodes it, in order,
        # which raises the error of the first that can't be
        encoded = list(map(_encode, strings))
        count = len(encoded)
        index = self.__index
        if index == self.__fields:
            raise self.__full()

        data = self.__data
        size = self.__size
        head = (count + 1) * size
        # The ends, after the count in place of the first string's start,
        # which they count from
        ends = list(accumulate(map(len, encoded), initial=head))
        offset = len(data) - self.__base
        length = offset + ends[-1]
        if length > self.__max_length:
            raise self.__too_long(index, length)

        ends[0] = count
        self.__length.pack_into(data, self.__slots + index * size, offset)
        data += pack(f"<{count + 1}{OFFSET_CODES[size]}", *ends)
        data += b"".join(encoded)
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
        # Any integer, as the native builder takes it, and TypeError for
        # anything else, such as a float that struct would raise for later
        if type(size) is not int:
            size = _index(size)
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

        if alignment > self.__alignment:
            self.__align_to(alignment)
        if padding:
            buffer += _ZEROS[padding]
        self.__length.pack_into(
            buffer, self.__slots + index * offset_size, offset + padding
        )
        buffer += self.__length.pack(count)
        buffer += view
        self.__index = index + 1

    def set_table(
        self, table: TableBuilder | Buffer, alignment: ElementSize | None = None
    ) -> None:
        """Embeds a table, from its builder or its bytes, without a length
        prefix. Readers find its fields with ``TableReader.field_offset()`` from
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

        if alignment > self.__alignment:
            self.__align_to(alignment)
        if padding:
            data += _ZEROS[padding]
        self.__length.pack_into(
            data, self.__slots + index * self.__size, offset + padding
        )
        data += nested
        self.__index = index + 1

    def set_vector(self, vector: Sequence[TableBuilder | Buffer]) -> None:
        """Writes a vector of tables, from their builders or their bytes, after
        the end of each, which lets readers find any item without reading the
        others. Builders must have all their fields set, and are copied as the
        bytes that they write, as ``set_table()`` copies them. Items aren't
        padded.

        Raises ``ValueError`` if a builder has fields left to set.
        """
        # Bytes, which can't change, are kept as they are, rather than as a
        # view, which holds a few hundred bytes for each item
        items = [
            item
            if type(item) is bytes
            else TableBuilder.__bytes_of(item)
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
    # and moved forward by more padding whenever a field raises it, as are
    # the tables it's nested in in place, whose alignment the field raises
    # too. Offsets count from their table's start, so none of them changes.
    # One that fails is truncated away, with the padding that its fields gave
    # the tables it's nested in.
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
        if type(fields) is not int:
            fields = _index(fields)

        data = self.__data
        size = self.__size
        start = len(data)
        base = self.__base
        offset = start - base
        slot_bytes = self.__slot_bytes(index, offset, fields)

        alignment = self.__alignment
        checkpoint: list[Any] = [
            start,
            base,
            self.__slots,
            self.__fields,
            index,
            alignment,
            alignment,
            self.__in_place,
            self.__padding,
            self.__checkpoint,
        ]
        self.__checkpoint = checkpoint
        self.__base = start
        self.__slots = start
        self.__fields = fields
        self.__index = 0
        self.__alignment = _UNALIGNED
        self.__in_place = offset
        self.__padding = 0
        try:
            data += (
                _ZEROS[slot_bytes] if slot_bytes < len(_ZEROS) else bytes(slot_bytes)
            )
            fill(self, *args)
            if self.__index != fields:
                raise _unset_fields_error(self.__index, fields, "the nested table's")
        except BaseException:
            self.__roll_back_table(checkpoint)
            raise

        # Padded from its parent's start, as a nested builder is, which its
        # fields did as they raised its alignment, and its parent's with it
        padding = self.__padding
        (
            _,
            base,
            slots,
            parent_fields,
            _,
            _,
            alignment,
            in_place,
            parent_padding,
            previous,
        ) = checkpoint
        length = len(data) - base
        if length > self.__max_length:
            self.__roll_back_table(checkpoint)
            raise self.__too_long(index, length)

        self.__base = base
        self.__slots = slots
        self.__fields = parent_fields
        self.__alignment = alignment
        self.__in_place = in_place
        self.__padding = parent_padding
        self.__checkpoint = previous
        self.__length.pack_into(data, slots + index * size, offset + padding)
        self.__index = index + 1

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
        if type(fields) is not int:
            fields = _index(fields)
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
        if count:
            slot_bytes = self.__slot_bytes(index, length, fields)
        else:
            fields = slot_bytes = 0

        parent = (
            start,
            base,
            self.__slots,
            self.__fields,
            index,
            self.__alignment,
            self.__in_place,
            self.__padding,
        )
        packing = self.__length
        ends = count * size
        zeros = _ZEROS[slot_bytes] if slot_bytes < len(_ZEROS) else bytes(slot_bytes)

        data += packing.pack(count)
        data += _ZEROS[ends] if ends < len(_ZEROS) else bytes(ends)
        # Until the first item starts, and between items, the builder holds
        # the vector's state, in which any field raises, as a table that holds
        # all its fields does, and writing the builder raises, as within a
        # fill. The values' iterator runs then, and in its parent's state
        # could set the vector's own field or write the builder over the
        # vector's bytes.
        self.__base = start
        self.__slots = start
        self.__fields = fields
        self.__index = fields
        self.__in_place = -1
        self.__padding = 0
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
                self.__padding = 0
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
            self.__roll_back(*parent)
            raise

        length = len(data) - base
        if length > max_length:
            self.__roll_back(*parent)
            raise self.__too_long(index, length)

        self.__roll_back_counters(*parent)
        packing.pack_into(data, self.__slots + index * size, offset)
        self.__index = index + 1

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
        in_place: int,
        padding: int,
    ) -> None:
        """Truncates the bytes to ``start``, where a nested table or vector
        that failed starts, and restores its parent's counters"""
        del self.__data[start:]
        self.__roll_back_counters(
            start, base, slots, fields, index, alignment, in_place, padding
        )

    def __roll_back_table(self, checkpoint: list[Any]) -> None:
        """Rolls back a table nested in place that failed, with the alignment
        of its parent's own fields, and the padding that its fields gave the
        tables it's nested in"""
        (
            start,
            base,
            slots,
            fields,
            index,
            alignment,
            _,
            in_place,
            padding,
            previous,
        ) = checkpoint
        self.__checkpoint = previous
        self.__roll_back(
            start, base, slots, fields, index, alignment, in_place, padding
        )
        self.__lower_alignment()

    def __roll_back_counters(
        self,
        _start: int,
        base: int,
        slots: int,
        fields: int,
        index: int,
        alignment: ElementSize,
        in_place: int,
        padding: int,
    ) -> None:
        """Restores the counters of a nested table's or vector's parent, which
        its checkpoint holds with where it started"""
        self.__base = base
        self.__slots = slots
        self.__fields = fields
        self.__index = index
        self.__alignment = alignment
        self.__in_place = in_place
        self.__padding = padding

    def __align_to(self, alignment: ElementSize) -> None:
        """Raises the table's alignment to ``alignment``, which is more than
        it is, and that of the tables it's nested in, as far as it's more
        than theirs, and pads each that's nested in place to it, this table
        included. A field raises it before it's written, so a large array
        moves the tables so far, which are often their slots alone.

        From the outermost table in, each moves forward by its own padding
        and that of the tables it's nested in."""
        self.__alignment = alignment
        in_place = self.__in_place
        if in_place < 0:
            return

        # A table nested in place has a checkpoint, which holds its parent's
        # counters
        shift = 0
        parent: Any = self.__checkpoint
        if parent[_RAISED] < alignment:
            parent[_RAISED] = alignment
            if parent[_IN_PLACE] >= 0:
                shift = self.__pad_parents(parent, alignment)

        base = self.__base + shift
        more = -in_place % alignment - self.__padding
        if more:
            self.__data[base:base] = _ZEROS[more]
            shift += more
        self.__base = base + more
        self.__slots += shift
        self.__padding += more

    def __pad_parents(self, parent: list[Any], alignment: ElementSize) -> int:
        """Raises the alignment of the tables that this table is nested in,
        from its parent, which is nested in place too and whose checkpoint is
        ``parent``, out, as far as it's more than theirs, and pads each that's
        nested in place to it. Returns the zeros inserted, which move this
        table forward."""
        # The checkpoints of the tables that it pads, from the parent out. A
        # parent nested in place has a checkpoint of its own.
        padded = [parent]
        parent = parent[_PREVIOUS]
        while parent[_RAISED] < alignment:
            parent[_RAISED] = alignment
            if parent[_IN_PLACE] < 0:
                break
            padded.append(parent)
            parent = parent[_PREVIOUS]

        data = self.__data
        shift = 0
        for parent in reversed(padded):
            base = parent[_BASE] + shift
            more = -parent[_IN_PLACE] % alignment - parent[_PADDING]
            if more:
                data[base:base] = _ZEROS[more]
                shift += more
            parent[_BASE] = base + more
            parent[_SLOTS] += shift
            parent[_PADDING] += more
            # Where its nested table starts, after its slots
            parent[_START] += shift
        return shift

    def __lower_alignment(self) -> None:
        """Lowers the alignment of the tables that this table is nested in,
        once a table nested in it fails, to what their own fields and the
        tables still nested in them need, and removes the padding that they
        no longer need from before each that's nested in place, and from
        before this table. From the outermost table in, each moves back by
        its own padding removed and that of the tables it's nested in."""
        in_place = self.__in_place
        if in_place < 0:
            return

        padded = []
        alignment = self.__alignment
        parent = self.__checkpoint
        while parent is not None:
            lowered = max(parent[_ALIGNMENT], alignment)
            if lowered == parent[_RAISED]:
                break
            parent[_RAISED] = lowered
            if parent[_IN_PLACE] < 0:
                break
            padded.append(parent)
            alignment = lowered
            parent = parent[_PREVIOUS]

        data = self.__data
        shift = 0
        for parent in reversed(padded):
            base = parent[_BASE] - shift
            needed = -parent[_IN_PLACE] % parent[_RAISED]
            less = parent[_PADDING] - needed
            if less:
                del data[base - less : base]
                shift += less
            parent[_BASE] = base - less
            parent[_SLOTS] -= shift
            parent[_PADDING] -= less
            parent[_START] -= shift

        base = self.__base - shift
        needed = -in_place % self.__alignment
        less = self.__padding - needed
        if less:
            del data[base - less : base]
            shift += less
        self.__base = base - less
        self.__slots -= shift
        self.__padding -= less

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

        if item_size > self.__alignment:
            self.__align_to(item_size)
        if padding:
            data += _ZEROS[padding]
        self.__length.pack_into(data, self.__slots + index * size, offset + padding)
        data += self.__length.pack(byte_length // item_size)
        data += converted
        self.__index = index + 1

    @staticmethod
    def __bytes_of(table: TableBuilder) -> memoryview[int] | bytearray:
        """The bytes of a builder to nest, with its length in a sized header,
        which it has once all its fields are set. Raises ``ValueError`` if it
        has fields left to set, or is writing a table in place."""
        written = table.__written
        if written is not None:
            return written.bytes() if isinstance(written, TableReader) else written

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
