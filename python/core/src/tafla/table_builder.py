from collections.abc import Buffer, Callable, Iterable, Sequence
from typing import Any, Literal, cast, overload

from .byte_buffer import ByteBuffer, byte_view, struct_count
from .constants import SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64
from .elements import as_elements
from .layout import MAX_LENGTHS, TableLayout, header_length, padding_to
from .table_writer import TableWriter
from .types import ElementSize, OffsetSize, TypeCode

type _Write = Callable[..., object]

# The writer's setters of the fields that take more than their index and
# value, which __write_fields() tells apart from the others by identity
_SET_STRUCT_ARRAY = TableWriter.set_struct_array
_SET_TABLE = TableWriter.set_table
_SET_VECTOR = TableWriter.set_vector


class TableBuilder:
    """Builds a table: a header with one offset per field, after the table's
    total length and version when its layout has them, followed by the fields
    in the order they're set. Arrays, strings and vectors start with their
    count, and a vector then holds the end of each item. Arrays of scalars and
    of structs are padded so their elements are aligned from the start of the
    table. Fields are read back by index with ``ByteBuffer.root_offset()``, or
    ``ByteBuffer.field_offset()`` for a table nested in another.

    Setting a field records it and counts the table's length. ``finalize()``
    then writes every field, in order, through a ``TableWriter``, so arrays,
    structs and tables given as buffers must keep their length in between; a
    changed length that changes the table's raises ``ValueError``. Tables
    nested from their builders are written by the same writer, straight into
    this one. A setter raises ``ValueError``, and leaves the builder as it was,
    when its field would make the table longer than the layout's
    ``offset_type`` addresses: 255 bytes, 64 KiB or 4 GiB. Structs, and tables
    and vector items given as bytes, must be C-contiguous buffers: a setter
    given one that isn't, such as a slice with a step, raises ``TypeError``
    and leaves the builder as it was too. Arrays are converted instead.
    """

    __slots__ = [
        "__alignment",
        "__fields",
        "__layout",
        "__max_length",
        "__offset",
        "__records",
        "__size",
        "__version",
        "__written",
    ]

    __size: OffsetSize
    __alignment: ElementSize
    # The table once written: its bytes, and then the buffer of them that
    # finalize() returns
    __written: memoryview[int] | ByteBuffer | None
    # Per field: the TableWriter setter that writes it, the alignment that
    # it's padded to, which is 1 for a field that isn't, and its value. The
    # writer pads each field and writes its offset as it reaches it, so the
    # record needn't hold either. The setters aren't bound to a writer, nor
    # to the builder, so that the builder isn't in a reference cycle with its
    # values.
    __records: list[tuple[_Write, ElementSize, Any]]

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
        size = layout.offset_size()

        self.__written = None
        self.__layout = layout
        self.__version = version
        self.__size = size
        self.__fields = fields
        self.__max_length = MAX_LENGTHS[size]
        # The table's length so far, which is where the next field goes
        self.__offset = header_length(fields, layout, version)
        # The largest alignment that a field needs from the table's start,
        # which a parent pads this table to when it nests it
        self.__alignment = SIZEOF_8
        self.__records = []

    def finalize(self) -> ByteBuffer:
        """Writes the table, once, and returns a buffer of its bytes; later
        calls return the same buffer.

        Raises ``ValueError`` if fewer fields were set than the table holds, or
        if an array, struct or table given as a buffer changed its length since
        it was set so that the table's changed. A failed write leaves the
        builder unfinalized, with its fields set.
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
        """Writes the table, once, as ``finalize()`` does, and returns its
        bytes, without the buffer that ``finalize()`` creates. Raises as
        ``finalize()`` does."""
        written = self.__written
        if written is None:
            return self.__write()

        return written.bytes() if isinstance(written, ByteBuffer) else written

    def set_int8(self, value: int) -> None:
        self.__add_field(TableWriter.set_int8, value, SIZEOF_8)

    def set_uint8(self, value: int) -> None:
        self.__add_field(TableWriter.set_uint8, value, SIZEOF_8)

    def set_int16(self, value: int) -> None:
        self.__add_field(TableWriter.set_int16, value, SIZEOF_16)

    def set_uint16(self, value: int) -> None:
        self.__add_field(TableWriter.set_uint16, value, SIZEOF_16)

    def set_int32(self, value: int) -> None:
        self.__add_field(TableWriter.set_int32, value, SIZEOF_32)

    def set_uint32(self, value: int) -> None:
        self.__add_field(TableWriter.set_uint32, value, SIZEOF_32)

    def set_int64(self, value: int) -> None:
        self.__add_field(TableWriter.set_int64, value, SIZEOF_64)

    def set_uint64(self, value: int) -> None:
        self.__add_field(TableWriter.set_uint64, value, SIZEOF_64)

    def set_float32(self, value: float) -> None:
        self.__add_field(TableWriter.set_float32, value, SIZEOF_32)

    def set_float64(self, value: float) -> None:
        self.__add_field(TableWriter.set_float64, value, SIZEOF_64)

    # A contiguous buffer of the field's element type is copied when the table
    # is written. Anything else is converted when it's set, so that a value
    # that doesn't fit raises from its setter, and the builder owns the copy.
    # Writing a table of 27,028 floats took 0.36 ms from a list, against 5 µs
    # from an array("f").

    def set_int8_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of int8s, such as
        ``array("b")``, is copied as bytes; anything else, such as a list, is
        converted as ``array("b", values)`` would, which takes several times as
        long."""
        self.__add_array("b", SIZEOF_8, TableWriter.set_int8_array, values)

    def set_uint8_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. Bytes, or any other contiguous buffer of
        uint8s, are copied as they are; anything else, such as a list, is
        converted as ``array("B", values)`` would, which takes several times as
        long."""
        self.__add_array("B", SIZEOF_8, TableWriter.set_uint8_array, values)

    def set_int16_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of int16s in the host's
        byte order, such as ``array("h")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("h", values)`` would, which
        takes several times as long."""
        self.__add_array("h", SIZEOF_16, TableWriter.set_int16_array, values)

    def set_uint16_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of uint16s in the host's
        byte order, such as ``array("H")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("H", values)`` would, which
        takes several times as long."""
        self.__add_array("H", SIZEOF_16, TableWriter.set_uint16_array, values)

    def set_int32_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of int32s in the host's
        byte order, such as ``array("i")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("i", values)`` would, which
        takes several times as long."""
        self.__add_array("i", SIZEOF_32, TableWriter.set_int32_array, values)

    def set_uint32_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of uint32s in the host's
        byte order, such as ``array("I")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("I", values)`` would, which
        takes several times as long."""
        self.__add_array("I", SIZEOF_32, TableWriter.set_uint32_array, values)

    def set_int64_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of int64s in the host's
        byte order, such as ``array("q")`` or a numpy int64 array, is copied as
        bytes; anything else, such as a list, is converted as
        ``array("q", values)`` would, which takes several times as long."""
        self.__add_array("q", SIZEOF_64, TableWriter.set_int64_array, values)

    def set_uint64_array(self, values: Buffer | Iterable[int]) -> None:
        """Sets an array field. A contiguous buffer of uint64s in the host's
        byte order, such as ``array("Q")`` or a numpy uint64 array, is copied
        as bytes; anything else, such as a list, is converted as
        ``array("Q", values)`` would, which takes several times as long."""
        self.__add_array("Q", SIZEOF_64, TableWriter.set_uint64_array, values)

    def set_float32_array(self, values: Buffer | Iterable[float]) -> None:
        """Sets an array field. A contiguous buffer of float32s in the host's
        byte order, such as ``array("f")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("f", values)`` would, which
        takes several times as long and writes a finite value past float32's
        range as an infinity of its sign."""
        self.__add_array("f", SIZEOF_32, TableWriter.set_float32_array, values)

    def set_float64_array(self, values: Buffer | Iterable[float]) -> None:
        """Sets an array field. A contiguous buffer of float64s in the host's
        byte order, such as ``array("d")``, is copied as bytes; anything else,
        such as a list, is converted as ``array("d", values)`` would, which
        takes several times as long."""
        self.__add_array("d", SIZEOF_64, TableWriter.set_float64_array, values)

    def set_string(self, string: str) -> None:
        """Writes ``string`` as UTF-8. Raises ``UnicodeEncodeError``, a
        ``ValueError``, if it has a lone surrogate, which UTF-8 can't encode."""
        # An ASCII string is as long as its UTF-8, so the writer encodes it.
        # Any other is encoded here, to count its bytes, and written as they
        # are, since encoding it again made a table of a string of 1,000
        # characters outside ASCII take 40% longer to build.
        if string.isascii():
            self.__add_field(TableWriter.set_string, string, self.__size + len(string))
        else:
            encoded = string.encode()
            self.__add_field(
                TableWriter._set_encoded_string, encoded, self.__size + len(encoded)
            )

    def set_struct(self, data: Buffer) -> None:
        """Sets a struct field from its bytes: its scalars, little-endian, one
        after another, which are written as they are, without padding. A
        ``struct.Struct`` with a ``<`` format packs them."""
        byte_length = byte_view(data).nbytes
        self.__add_field(TableWriter.set_struct, data, byte_length)

    def set_struct_array(self, data: Buffer, size: int, alignment: ElementSize) -> None:
        """Sets an array of structs of ``size`` bytes each, from their bytes
        one after another. It's padded so that the structs start at a multiple
        of ``alignment`` from the start of the table, which is the size of the
        structs' largest scalar.

        Raises ``ValueError`` if ``size`` is less than 1, or ``data`` isn't a
        whole number of structs.
        """
        byte_length = byte_view(data).nbytes
        struct_count(byte_length, size)
        padding = padding_to(self.__offset + self.__size, alignment)

        self.__add_field(
            _SET_STRUCT_ARRAY,
            (data, size),
            self.__size + byte_length,
            padding,
            alignment,
        )
        self.__align_to(alignment)

    def set_table(
        self, table: TableBuilder | Buffer, alignment: ElementSize | None = None
    ) -> None:
        """Embeds a table, from its builder or its bytes, without a length
        prefix. Readers find its fields with ``ByteBuffer.field_offset()`` from
        the field's offset, so its layout must be ``TableLayout.DEFAULT``.

        A builder must have all its fields set. ``finalize()`` writes it
        straight into this table, without bytes of its own. It can still be
        finalized or nested elsewhere too, since a complete builder doesn't
        change. A builder that was finalized already, or whose layout has a
        header or another ``offset_type``, is finalized and copied instead.

        The nested table's arrays are aligned from its own start, so it's
        padded to start at a multiple of ``alignment``. For a builder, the
        default is the largest alignment of its arrays, of scalars and of
        structs, and of its nested tables, or 1 if it has neither. For bytes,
        the default of 4 suits tables of 8-, 16- and 32-bit elements; pass 8
        for a table with 64-bit arrays.

        Raises ``ValueError`` if the builder has fields left to set.
        """
        if isinstance(table, TableBuilder):
            byte_length = TableBuilder.__length_of(table)
            default_alignment = table.__alignment
        else:
            byte_length = byte_view(table).nbytes
            default_alignment = SIZEOF_32

        if alignment is None:
            alignment = default_alignment

        padding = padding_to(self.__offset, alignment)
        self.__add_field(_SET_TABLE, table, byte_length, padding, alignment)
        self.__align_to(alignment)

    def set_vector(self, vector: Sequence[TableBuilder | Buffer]) -> None:
        """Writes a vector of tables, from their builders or their bytes, after
        the end of each, which lets readers find any item without reading the
        others. Builders must have all their fields set, and are written
        straight into this table as ``set_table()`` writes them, except for
        those finalized already and those whose layout has a header or another
        ``offset_type``, which are finalized and copied. Items aren't padded.

        Raises ``ValueError`` if a builder has fields left to set.
        """
        items = tuple(vector)
        byte_length = self.__size * (1 + len(items))
        for item in items:
            byte_length += TableBuilder.__length_of(item)

        self.__add_field(_SET_VECTOR, items, byte_length)

    def __add_array(
        self,
        type_code: TypeCode,
        item_size: ElementSize,
        set_array: _Write,
        values: Buffer | Iterable[float],
    ) -> None:
        """Pads the array so that its elements, after the length, are aligned"""
        elements = as_elements(type_code, values)
        byte_length = memoryview(elements).nbytes
        padding = padding_to(self.__offset + self.__size, item_size)

        self.__add_field(
            set_array, elements, self.__size + byte_length, padding, item_size
        )
        self.__align_to(item_size)

    def __align_to(self, alignment: ElementSize) -> None:
        self.__alignment = max(self.__alignment, alignment)

    def __add_field(
        self,
        write: _Write,
        value: Any,
        byte_length: int,
        padding: int = 0,
        alignment: ElementSize = 1,
    ) -> None:
        """Records a field of ``byte_length`` bytes after ``padding`` bytes,
        which pad it to ``alignment``: 1 for a field that isn't padded.
        Checking the table's length here, rather than in finalize(), reports a
        field too long for the layout from its own setter and leaves the
        builder as it was. Every offset and length is shorter than the table,
        so the table's length is the one value to check.

        Raises ``ValueError`` if the table already holds all its fields, or the
        field would make it longer than the layout addresses. Raises
        ``TypeError`` if the table is already written.
        """
        if self.__written is not None:
            raise TypeError("The table was already finalized")

        records = self.__records
        index = len(records)
        if index == self.__fields:
            raise ValueError(f"Can't add field {index + 1} to a table of {index}")

        length = self.__offset + padding + byte_length
        if length > self.__max_length:
            raise ValueError(
                f"Field {index + 1} makes the table {length} bytes, "
                f"too long for {self.__layout.offset_type()} offsets"
            )

        records.append((write, alignment, value))
        self.__offset = length

    def __assert_complete(self) -> None:
        if self.__written is None and len(self.__records) != self.__fields:
            raise ValueError(
                f"Only {len(self.__records)} of the table's {self.__fields} "
                "fields were set"
            )

    @staticmethod
    def __length_of(table: TableBuilder | Buffer) -> int:
        """The byte length of a table to nest, which a builder knows once all
        its fields are set. Raises ``ValueError`` if it has fields left to set.
        """
        if not isinstance(table, TableBuilder):
            return byte_view(table).nbytes

        table.__assert_complete()
        return table.__offset

    def __write(self) -> memoryview[int]:
        """Writes the table through a writer, and then releases the values.

        Raises ``ValueError`` if fewer fields were set than the table holds, or
        the fields wrote another length than they counted when they were set.
        """
        self.__assert_complete()

        # The constructor's overload for the layout, whose version the
        # builder's own constructor checked against it
        fields, layout, version = self.__fields, self.__layout, self.__version
        if version is None:
            writer = TableWriter(fields, cast("TableLayout[Literal[False]]", layout))
        else:
            writer = TableWriter(
                fields, cast("TableLayout[Literal[True]]", layout), version
            )
        self.__write_fields(writer)
        data = writer.bytes()

        # The writer writes each offset as it reaches its field, so a buffer
        # whose length changed since it was set shows in the table's length
        if len(data) != self.__offset:
            raise ValueError(
                f"The fields wrote {len(data)} bytes rather than the "
                f"{self.__offset} they counted when they were set"
            )

        # Release the values, which may be large arrays
        self.__records = []
        self.__written = data

        return data

    # The loops below count their index rather than enumerate(), whose
    # iteration Python 3.14 doesn't specialize: building the record took 3%
    # less time, and the vector of 100 nested tables 4.5% less.

    def __write_fields(self, writer: TableWriter) -> None:
        """Writes the fields in order, each with its index in this table"""
        index = 0
        for write, alignment, value in self.__records:
            if write is _SET_TABLE:
                writer.set_table(index, alignment)
                self.__write_table(writer, value)
            elif write is _SET_VECTOR:
                self.__write_vector(writer, index, value)
            elif write is _SET_STRUCT_ARRAY:
                data, size = value
                writer.set_struct_array(index, data, size, alignment)
            else:
                write(writer, index, value)

            index += 1  # noqa: SIM113

    def __write_vector(
        self, writer: TableWriter, index: int, items: tuple[TableBuilder | Buffer, ...]
    ) -> None:
        vector = writer.set_vector(index, len(items))

        item = 0
        for table in items:
            self.__write_table(writer, table)
            writer.set_item_end(vector, item)
            item += 1  # noqa: SIM113

    def __write_table(self, writer: TableWriter, table: TableBuilder | Buffer) -> None:
        """Writes a builder straight into the writer, between ``start_table()``
        and ``end_table()``, unless it was finalized already or its layout has
        a header or another offset type, which a table nested in the writer's
        doesn't. Those tables, and bytes, are copied."""
        if not isinstance(table, TableBuilder):
            writer._copy_table(table)
        elif (
            table.__written is None
            and table.__size == self.__size
            and not table.__layout.header_size()
        ):
            parent = writer.start_table(table.__fields)
            table.__write_fields(writer)
            writer.end_table(parent)
        else:
            writer._copy_table(table.bytes())
