import sys
from collections.abc import Callable
from functools import partial
from operator import itemgetter
from struct import Struct, calcsize, error as StructError
from typing import Any, cast

from .errors import describe_int
from .layout import TableLayout
from .types import OffsetSize

# The struct format code of each offset size
_OFFSET_CODES: dict[OffsetSize, str] = {1: "B", 2: "H", 4: "I"}

# A function that packs the values of a table's fixed fields
type Packer = Callable[..., bytes]

# What ByteBuffer.get_fields() reads a table's fixed fields with: the Struct,
# the header before the slots, a getter of the offsets from what the Struct
# unpacks and the offsets that it expects, a getter of the values, and
# whether a string's length follows them
type Reader = tuple[Struct, int, itemgetter[Any], object, itemgetter[Any], bool]


class FixedFields:
    """The leading fields of a table whose offsets don't depend on any value,
    which ``TableWriter.set_fields()`` and ``write_table()`` pack with the
    table's offsets in one call to a ``struct.Struct``, rather than with a
    setter for each field, and ``ByteBuffer.get_fields()`` unpacks. The code
    that tafla-gen generates creates one for each table that starts with such
    fields.

    Those fields are scalars, structs and nested tables whose own fields are
    all such fields, since none of them is aligned, and then a string, whose
    length is packed with them and whose bytes follow. ``fields`` is the
    number of fields of the table, and each of ``formats`` describes one of
    its leading fields, in order:

    - a scalar's ``struct`` format code, such as ``"d"`` for a float64 or
      ``"?"`` for a bool, which is written as 1 or 0, as it's truthy;
    - ``"{size}s"`` for a struct of ``size`` bytes, written from its bytes;
    - the ``FixedFields`` of a nested table that ``formats`` describe whole,
      whose values follow in place of it.

    ``string`` adds the string that follows them. ``layout`` is a message's
    layout, whose header its offsets follow. A nested table's fields omit it,
    and are prepared for each offset type, since the root's sets theirs.

    Like ``TableWriter``, it's the target of generated code, which describes
    a table's fields as the schema declares them, so it checks only that it
    can describe them: it raises ``ValueError`` for fewer ``fields`` than it
    describes, and ``struct.error`` for fields whose bytes from the first slot
    pass a C ``ssize_t``, as ``struct`` does.
    """

    __slots__ = (
        "_fields",
        "_formats",
        "_header",
        "_layout",
        "_packers",
        "_readers",
        "_string",
    )

    def __init__(
        self,
        fields: int,
        *formats: str | FixedFields,
        string: bool = False,
        layout: TableLayout | None = None,
    ) -> None:
        leading = len(formats) + (1 if string else 0)
        if fields < leading:
            raise ValueError(
                f"A table of {describe_int(fields)} fields can't start with "
                f"{leading} of them"
            )
        # Slots that no Struct describes, raised before their count is
        # formatted, which may have more digits than Python converts
        if fields > sys.maxsize:
            raise StructError("total struct size too long")

        self._fields = fields
        self._formats = formats
        self._string = string

        # A packer and a reader for each offset size, indexed by it: the
        # root's for its layout's, after its header, and a nested table's for
        # each
        header = 0 if layout is None else layout.header_size()
        sizes = (1, 2, 4) if layout is None else (layout.offset_size(),)
        packers: list[Packer | None] = [None] * 5
        readers: list[Reader | None] = [None] * 5
        for size in sizes:
            codes: list[str] = []
            arguments: list[int | None] = []
            self.__describe(size, header, codes, arguments)
            struct = Struct("<" + "".join(codes))

            packers[size] = _packer(struct, arguments)
            readers[size] = _reader(struct, arguments, header, string)

        self._packers = cast(tuple[Packer, ...], tuple(packers))
        self._readers = cast(tuple[Reader, ...], tuple(readers))

        # A message's layout, which TableWriter.write_fields() and
        # ByteBuffer.read_fields() write and read its header in
        self._layout = layout
        self._header = None if layout is None else _header(layout)

    def __describe(
        self,
        size: OffsetSize,
        start: int,
        codes: list[str],
        arguments: list[int | None],
    ) -> int:
        """Appends the format codes of the table, and its arguments: an offset
        for each of its packed fields' slots, and ``None`` for each value.
        Returns the position after them, up to the string's bytes, from the
        table's start, which lies ``start`` bytes before its slots."""
        code = _OFFSET_CODES[size]
        position = start + self._fields * size
        offsets: list[int] = []
        value_codes: list[str] = []
        value_arguments: list[int | None] = []

        for format in self._formats:
            offsets.append(position)
            if isinstance(format, FixedFields):
                position += format.__describe(size, 0, value_codes, value_arguments)
            else:
                value_codes.append(format)
                value_arguments.append(None)
                position += calcsize("<" + format)

        if self._string:
            offsets.append(position)
            value_codes.append(code)
            value_arguments.append(None)
            position += size

        # The slots of the fields that follow are written by their setters
        codes.append(f"{len(offsets)}{code}{(self._fields - len(offsets)) * size}x")
        codes.extend(value_codes)
        arguments.extend(offsets)
        arguments.extend(value_arguments)

        return position


# What TableWriter.write_fields() and ByteBuffer.read_fields() read of a
# message's layout: its offset size, the Struct of its header's slots, the
# length's and then the version's, and whether it has each
type Header = tuple[OffsetSize, Struct, bool, bool]


def _header(layout: TableLayout) -> Header:
    size = layout.offset_size()
    sized, versioned = layout.sized(), layout.versioned()
    slots = Struct("<" + _OFFSET_CODES[size] * (sized + versioned))

    return size, slots, sized, versioned


def _packer(struct: Struct, arguments: list[int | None]) -> Packer:
    """Returns a function that packs the fields' values, followed by the
    string's length, with the offsets that ``arguments`` holds, as ``struct``
    takes them"""
    # The Struct takes the offsets and the values in the order of their
    # bytes, which interleaves them for a nested table. Its offsets are
    # constants, which a partial() passes before the values when they all
    # come first.
    offsets = tuple(argument for argument in arguments if argument is not None)
    order = _order(arguments, len(offsets))
    if order is None:
        return partial(struct.pack, *offsets)

    pack, arrange = struct.pack, itemgetter(*order)

    def pack_nested(*values: Any) -> bytes:
        return pack(*arrange(offsets + values))

    return pack_nested


def _reader(
    struct: Struct, arguments: list[int | None], header: int, string: bool
) -> Reader:
    """Returns what reads the fields that ``struct`` unpacks, whose
    ``arguments`` are offsets or values in its order, with the string's length
    last"""
    offsets = [
        index for index, argument in enumerate(arguments) if argument is not None
    ]
    values = [index for index, argument in enumerate(arguments) if argument is None]
    if string:
        values.pop()

    check = _tuple_getter(offsets)
    expected = check(
        tuple(0 if argument is None else argument for argument in arguments)
    )
    return struct, header, check, expected, _tuple_getter(values), string


def _tuple_getter(indices: list[int]) -> itemgetter[Any]:
    """Returns a getter of the items at ``indices``, which returns a tuple
    even of one item or none, through a slice when they're consecutive"""
    first = indices[0] if indices else 0
    if indices == list(range(first, first + len(indices))):
        return itemgetter(slice(first, first + len(indices)))

    return itemgetter(*indices)


def _order(arguments: list[int | None], offsets: int) -> tuple[int, ...] | None:
    """Returns the index of each argument in the offsets followed by the
    values, or ``None`` when that's their order already"""
    order: list[int] = []
    offset = value = 0
    for argument in arguments:
        if argument is None:
            order.append(offsets + value)
            value += 1
        else:
            order.append(offset)
            offset += 1

    return None if order == list(range(len(order))) else tuple(order)
