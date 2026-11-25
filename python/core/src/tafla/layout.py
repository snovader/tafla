from collections.abc import Callable
from functools import partial
from typing import ClassVar, override

from .types import ElementSize, OffsetSize, OffsetType

# The offset size of each offset type
OFFSET_SIZES: dict[OffsetType, OffsetSize] = {
    "uint8": 1,
    "uint16": 2,
    "uint32": 4,
}

# The offset type of each offset size, which errors name. TableWriter looks it
# up when it raises, since storing the offset type made its constructor 22 ns,
# or 6%, slower; TableBuilder and TableReader keep their layout.
OFFSET_TYPES: dict[OffsetSize, OffsetType] = {
    size: offset_type for offset_type, size in OFFSET_SIZES.items()
}


# The longest table that offsets and lengths of each offset size address
MAX_LENGTHS: dict[OffsetSize, int] = {
    1: 0xFF,
    2: 0xFFFF,
    4: 0xFFFF_FFFF,
}


class TableLayout:
    """The header and offset type of a table. A format creates its layout once
    and passes it to both the ``TableBuilder`` that writes the table and the
    ``TableReader`` that reads it, so that the two can't disagree. Nested tables
    use ``TableLayout.DEFAULT``.

    - ``sized``: whether the header starts with the table's total byte length.
    - ``version``: the version that the header holds, after the length when
      ``sized``, which gives it a slot for one: an integer that fits in the
      offset type, or ``None``, the default, for a header without one. Writers
      write it, and readers can check the one that a table holds, with
      ``TableReader.get_version()``, before reading any field.
    - ``offset_type``: the offset type, the unsigned integer type of the header
      slots, field offsets, counts, lengths and item ends, which limits the
      table's length: ``"uint8"``, of offset size 1, for up to 255 bytes,
      ``"uint16"``, of 2, for 65,535 and ``"uint32"``, of 4, the default, for
      4 GiB.

    A layout with a version is the layout of that version of the format, and
    its readers read tables of any version, so that an application can read a
    table's version before it chooses how to read the fields. The layouts of a
    format's versions should differ in the version only, as ``with_version()``
    makes them.

    Raises ``ValueError`` if ``offset_type`` isn't ``"uint8"``, ``"uint16"`` or
    ``"uint32"``, or ``version`` doesn't fit in it.
    """

    __slots__ = [
        "__header_size",
        "__offset_size",
        "__offset_type",
        "__sized",
        "__version",
    ]

    DEFAULT: ClassVar[TableLayout]
    """No header slots, and offsets and lengths of the offset type ``uint32``"""

    def __init__(
        self,
        *,
        sized: bool = False,
        version: int | None = None,
        offset_type: OffsetType = "uint32",
    ) -> None:
        # The isinstance() check keeps an unhashable value from raising
        # TypeError from the lookup
        offset_size = (
            OFFSET_SIZES.get(offset_type) if isinstance(offset_type, str) else None
        )
        if offset_size is None:
            raise ValueError(f"{offset_type!r} isn't an offset type")

        if version is not None and not 0 <= version <= MAX_LENGTHS[offset_size]:
            raise ValueError(f"Version {version} doesn't fit in a {offset_type} slot")

        self.__sized = sized
        self.__version = version
        self.__offset_type: OffsetType = offset_type
        self.__offset_size: OffsetSize = offset_size
        header_slots = (1 if sized else 0) + (0 if version is None else 1)
        self.__header_size = header_slots * offset_size

    @override
    def __reduce__(self) -> tuple[Callable[[], TableLayout], tuple[()]]:
        """Pickles and copies the layout as a call of its class with its
        arguments, as the native class does"""
        arguments = partial(
            TableLayout,
            sized=self.__sized,
            version=self.__version,
            offset_type=self.__offset_type,
        )
        return arguments, ()

    @override
    def __repr__(self) -> str:
        return (
            f"TableLayout(sized={self.__sized}, version={self.__version}, "
            f"offset_type={self.__offset_type!r})"
        )

    def sized(self) -> bool:
        return self.__sized

    def versioned(self) -> bool:
        """Whether the header has a slot for a version"""
        return self.__version is not None

    def version(self) -> int | None:
        """The version that writers write to the header, when it has a slot"""
        return self.__version

    def with_version(self, version: int) -> TableLayout:
        """Returns the layout with ``version`` in place of its own, for another
        version of the same format.

        Raises ``ValueError`` if ``version`` doesn't fit in the offset type.
        """
        return TableLayout(
            sized=self.__sized, version=version, offset_type=self.__offset_type
        )

    def offset_type(self) -> OffsetType:
        return self.__offset_type

    def offset_size(self) -> OffsetSize:
        """The offset size, the width in bytes of the offset type"""
        return self.__offset_size

    def header_size(self) -> int:
        """The byte length of the header slots before the field offsets"""
        return self.__header_size


TableLayout.DEFAULT = TableLayout()


def header_length(fields: int, layout: TableLayout) -> int:
    """Returns the byte length of the header of a table of ``fields`` fields in
    ``layout``.

    Raises ``ValueError`` if the header alone is longer than the layout
    addresses.
    """
    size = layout.offset_size()
    max_length = MAX_LENGTHS[size]

    length = layout.header_size() + fields * size
    if length > max_length:
        raise ValueError(
            f"A header of {fields} fields doesn't fit in {layout.offset_type()} offsets"
        )

    return length


# The alignments that arrays, structs and nested tables are padded to: the
# sizes of the scalars
ALIGNMENTS = frozenset((1, 2, 4, 8))


def padding_to(offset: int, alignment: ElementSize) -> int:
    """Returns the bytes that bring ``offset`` from a table's start to a
    multiple of ``alignment``.

    Raises ``ValueError`` if ``alignment`` isn't 1, 2, 4 or 8.
    """
    # The type comes first, since 2.0 is in ALIGNMENTS and a list isn't
    # hashable
    if type(alignment) is not int or alignment not in ALIGNMENTS:
        raise alignment_error(alignment)

    return -offset % alignment


def alignment_error(alignment: object) -> ValueError:
    """The error of an alignment that isn't 1, 2, 4 or 8"""
    return ValueError(f"An alignment of {alignment!r} isn't 1, 2, 4 or 8 bytes")
