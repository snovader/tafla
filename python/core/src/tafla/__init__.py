"""Binary tables of scalars, strings and typed arrays. ``TableBuilder`` writes a
table at its exact size, copying typed arrays in bulk, and ``ByteBuffer`` reads
fields back by offset, with arrays as views of the bytes. Every read checks
that it stays within the bytes it was given. ``TableWriter`` writes a table
whose fields are known as it's written, as the code that tafla-gen generates
writes its tables, without recording them first as ``TableBuilder`` does.

>>> from array import array
>>> from tafla import ByteBuffer, TableBuilder, TableLayout

A format's header, shared by its writer and its readers:

>>> LAYOUT = TableLayout(sized=True, versioned=True)
>>> FORMAT_VERSION = 1

>>> series = TableBuilder(2)
>>> series.set_uint32_array(array("I", [0, 86400]))  # copied as bytes
>>> series.set_float32_array([21.75, 18.5])  # a list is converted first

>>> builder = TableBuilder(3, LAYOUT, FORMAT_VERSION)
>>> builder.set_float64(59.3293)
>>> builder.set_string("Europe/Stockholm")
>>> builder.set_table(series)  # written straight into the table
>>> data = builder.bytes()

>>> buffer = ByteBuffer(data, LAYOUT)
>>> buffer.get_version() == FORMAT_VERSION
True
>>> buffer.get_float64(buffer.root_offset(0))
59.3293
>>> buffer.get_string(buffer.root_offset(1))
'Europe/Stockholm'
>>> table = buffer.root_offset(2)
>>> buffer.get_float32_array(buffer.field_offset(table, 1)).tolist()
[21.75, 18.5]

Structs are bytes that the standard library's ``struct`` packs and unpacks
little-endian, without padding:

>>> from struct import Struct
>>> POINT = Struct("<hf")  # an int16 and a float32, so aligned to 4
>>> shape = TableBuilder(2)
>>> shape.set_struct(POINT.pack(0, 1.5))
>>> shape.set_struct_array(POINT.pack(1, 2.5) + POINT.pack(2, -0.5), POINT.size, 4)
>>> buffer = shape.finalize()
>>> POINT.unpack(buffer.get_struct(buffer.root_offset(0), POINT.size))
(0, 1.5)
>>> points = buffer.get_struct_array(buffer.root_offset(1), POINT.size)
>>> list(POINT.iter_unpack(points))
[(1, 2.5), (2, -0.5)]

Records at fixed offsets without a header or offsets, such as stream frames,
are better written and read with ``struct`` alone.

Format
------

All values are little-endian. A table is its header, one offset per field and
then the fields in the order they were set:

- The header holds the table's byte length when the layout is ``sized``, and
  then its version when it's ``versioned``.
- Header slots, offsets, counts and vector item ends are all of the layout's
  offset type, ``offset_type``: ``uint8``, ``uint16`` or ``uint32``, whose
  offset size is 1, 2 or 4 bytes. It limits the table to 255 bytes, 64 KiB or
  4 GiB.
- Offsets count from the table's start. A table nested with ``set_table()`` has
  no header, and its offsets count from its own start.
- Arrays and strings start with their element count. Typed arrays are padded so
  that their elements are aligned to their size from the table's start.
- A struct is its scalars, one after another without padding, and isn't
  aligned. An array of structs starts with their count, and is padded so that
  the structs are aligned to the size of their largest scalar.
- ``set_table()`` aligns a nested table to the largest alignment of its arrays
  when given its builder, and to 4 bytes by default when given bytes.
- A vector starts with its item count, then the end of each item, counted from
  the vector's start, and then the items. An item starts where the one before
  it ends, so ``get_vector_offset()`` finds any item from two ends.

Performance
-----------

Measured in CPython 3.14 on an Apple M2 Pro, with the garbage collector off:

- **Buffers of the field's element type are copied as bytes.** That includes
  an ``array`` of the type code and a numpy array of the type. Anything else,
  such as a list or an array of another type, is converted as ``array()``
  converts an iterable, at about 13 ns per element: a table of 27,028 floats
  took 5 µs to write from an ``array("f")`` and 0.36 ms from a list.
- **Array reads return views.** A view shares the bytes' storage, so it's free
  and reflects later changes to the bytes. Unlike typed arrays in JavaScript,
  a ``memoryview`` reads elements at any alignment, so arrays received at an
  odd offset are views too. On a big-endian host, arrays of multi-byte
  elements are copies, with each element's bytes swapped to the host's order.
- **Read arrays whole.** Each scalar getter is a call with a range check:
  summing a 27,028-element float32 array with ``get_float32()`` took 3.4 ms,
  against 0.24 ms through the array's view.
- **Nest builders rather than bytes.** ``set_table()`` and ``set_vector()``
  write a ``TableBuilder`` straight into its parent when the parent is
  finalized, so the outermost table allocates and writes all the bytes once.
  A table nested as bytes was built into a buffer of its own and is copied, as
  is a builder that was finalized already or whose layout has a header or
  another offset type. A vector of 100 tables that each nest another took
  0.27 ms to build from builders and 0.42 ms from bytes, and a table nesting
  27,028 rows of three columns took 16 µs and 31 µs.
- **Small tables cost their calls.** Each field is a few Python calls to set
  and to write, so a table of two float64s, an int64 and a 16-byte string took
  2.6 µs, far more than allocating and copying its 60 bytes.

Conversions
-----------

``TableBuilder`` and ``TableWriter`` convert each value to its field's type
as ``struct`` and ``array`` convert it, and raise for one they can't write. A
scalar's error comes from ``TableWriter``'s setter, and from
``TableBuilder.finalize()`` or ``bytes()``; an array's from its setter:

==========================================  ===================================
Value                                       Written as
==========================================  ===================================
An integer outside its field's range        ``ValueError`` from a scalar,
                                            ``OverflowError`` from an array
A fraction or NaN for an integer field      ``ValueError`` from a scalar,
                                            ``TypeError`` from an array
A float too large for a float32             ``ValueError`` from a scalar, and
                                            infinity of its sign in an array,
                                            as ``array("f")`` converts it.
                                            Other floats round to the nearest
                                            float32.
A lone surrogate in a string                ``UnicodeEncodeError``, a
                                            ``ValueError``
==========================================  ===================================

Validation and errors
---------------------

Validation is always on:

- Every read checks its range against the bytes the ``ByteBuffer`` was given
  and raises ``DecodeError`` if it would leave them, at any position, however
  large. Strings must be valid UTF-8, and vector items must start no later
  than they end. A truncated or corrupted buffer therefore raises rather than
  returning values from outside it.
- ``DecodeError`` is a ``ValueError``. Its message gives the read's position
  and length and the bytes available, and an invalid string's error has the
  ``UnicodeDecodeError`` as its cause. Positions, lengths and indices of more
  than 64 bits are given by their bit length, such as ``<15001-bit integer>``,
  since Python converts at most ``sys.get_int_max_str_digits()`` digits to a
  string. So no error fails to build, whatever that limit is.
  ``DecodeError.out_of_range()`` builds the error of a read outside the bytes,
  as the structs that tafla-gen generates raise it.
- Checks can't tell a value that was corrupted within range from a real one,
  such as an offset that points at another field. A format that needs that
  guarantee can add a checksum.

Using the API wrongly raises other errors, whatever the bytes:

- A buffer that isn't C-contiguous, such as a slice with a step, raises
  ``TypeError`` where it's taken as bytes: by ``ByteBuffer()``, whose reads
  return views of the bytes rather than of a copy, and as a struct, or a table
  or vector item, by a builder's setter. Array setters convert it instead.
- ``TableBuilder`` raises ``ValueError`` for more or fewer fields than it was
  created with, a version that doesn't fit, or arrays, structs or tables given
  as buffers whose lengths changed before ``finalize()`` so that the table's
  did, which leaves it unfinalized. A field that would make the table too long
  for its ``offset_type`` raises ``ValueError`` from its setter and isn't
  added. A value that its field's type can't hold raises as Conversions lists.
  A version missing from or given for a layout, or a field set after
  ``finalize()``, raises ``TypeError``.
- ``TableWriter`` raises the errors of ``TableBuilder`` for the same values,
  but from the setter that writes them, and ``ValueError`` for a table too
  long for its ``offset_type`` from the setter whose offset or count doesn't
  fit, or from ``bytes()``. It doesn't check the order of its calls, as the
  next section describes.
- An alignment other than 1, 2, 4 or 8 raises ``ValueError`` from
  ``set_struct_array()`` and ``set_table()``.
- A struct size less than 1 raises ``ValueError`` from
  ``set_struct_array()`` and ``get_struct_array()``, and so do bytes that
  aren't a whole number of structs from ``set_struct_array()``.
- ``get_vector_offset()`` raises ``IndexError`` for an item outside the
  vector.
- ``get_version()`` raises ``TypeError`` when the layout isn't versioned.

TableWriter's contract
----------------------

``TableWriter`` is the target of the generated code, whose calls follow from
the schema, so it doesn't check them again. Its contract:

- Every field is set, in index order, each once, and each vector item is
  followed by its ``set_item_end()``.
- A field index outside its table, or an item outside its vector, writes an
  offset or item end over another slot or field, so the table's bytes are
  undefined. Every write still stays within the writer's bytes: one whose slot
  lies past them raises ``ValueError``.
- The TypeScript, Java and Rust writers write into bytes that a sizer counted,
  and so fail for fields that differ from those counted. Python's appends
  without a sizer, so a field that wasn't set isn't caught: ``bytes()``
  returns a table without it, whose offset is 0. Checking the root's offsets
  in ``bytes()`` made writing the record 18% slower, so it's left to the
  caller.

Rust's writer also reports an index past its table's fields, since the parent
that the generated code hands back already carries the count. The others don't
keep that count for each nested table, so the contract doesn't promise it.
Hand-written tables use ``TableBuilder``, which checks every field.
"""

from .byte_buffer import ByteBuffer
from .constants import SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64
from .errors import DecodeError
from .layout import TableLayout
from .table_builder import TableBuilder
from .table_writer import TableWriter
from .types import OffsetSize, OffsetType

__all__ = [
    "ByteBuffer",
    "DecodeError",
    "OffsetSize",
    "OffsetType",
    "TableBuilder",
    "TableLayout",
    "TableWriter",
    "SIZEOF_8",
    "SIZEOF_16",
    "SIZEOF_32",
    "SIZEOF_64",
]
