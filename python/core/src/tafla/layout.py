from typing import ClassVar, Literal, cast, overload, override

from .types import ElementSize, OffsetSize, OffsetType

# The offset size of each offset type
OFFSET_SIZES: dict[OffsetType, OffsetSize] = {
    "uint8": 1,
    "uint16": 2,
    "uint32": 4,
}

# The offset type of each offset size, which errors name. TableWriter looks it
# up when it raises, since storing the offset type made its constructor 22 ns,
# or 6%, slower; TableBuilder and ByteBuffer keep their layout.
OFFSET_TYPES: dict[OffsetSize, OffsetType] = {
    size: offset_type for offset_type, size in OFFSET_SIZES.items()
}


class TableLayout[Versioned: bool = bool]:
    """The header and offset type of a table. A format creates its layout once
    and passes it to both the ``TableBuilder`` that writes the table and the
    ``ByteBuffer`` that reads it, so that the two can't disagree. Nested tables
    use ``TableLayout.DEFAULT``.

    - ``sized``: whether the header starts with the table's total byte length.
    - ``versioned``: whether the header has a slot for a version, after the
      length when ``sized``. Readers can check it before reading any field.
    - ``offset_type``: the offset type, the unsigned integer type of the header
      slots, field offsets, counts, lengths and item ends, which limits the
      table's length: ``"uint8"``, of offset size 1, for up to 255 bytes,
      ``"uint16"``, of 2, for 65,535 and ``"uint32"``, of 4, the default, for
      4 GiB.

    ``Versioned`` lets type checkers require a version where a versioned table
    is built.

    Raises ``ValueError`` if ``offset_type`` isn't ``"uint8"``, ``"uint16"`` or
    ``"uint32"``.
    """

    __slots__ = [
        "__header_size",
        "__offset_size",
        "__offset_type",
        "__sized",
        "__versioned",
    ]

    DEFAULT: ClassVar[TableLayout[Literal[False]]]
    """No header slots, and offsets and lengths of the offset type ``uint32``"""

    __versioned: Versioned

    @overload
    def __init__(
        self: TableLayout[Literal[False]],
        *,
        sized: bool = False,
        versioned: Literal[False] = False,
        offset_type: OffsetType = "uint32",
    ) -> None: ...

    @overload
    def __init__(
        self: TableLayout[Literal[True]],
        *,
        sized: bool = False,
        versioned: Literal[True],
        offset_type: OffsetType = "uint32",
    ) -> None: ...

    @overload
    def __init__(
        self: TableLayout[bool],
        *,
        sized: bool = False,
        versioned: bool,
        offset_type: OffsetType = "uint32",
    ) -> None: ...

    def __init__(
        self,
        *,
        sized: bool = False,
        versioned: bool = False,
        offset_type: OffsetType = "uint32",
    ) -> None:
        # The isinstance() check keeps an unhashable value from raising
        # TypeError from the lookup
        offset_size = (
            OFFSET_SIZES.get(offset_type) if isinstance(offset_type, str) else None
        )
        if offset_size is None:
            raise ValueError(f"{offset_type!r} isn't an offset type")

        self.__sized = sized
        self.__versioned = cast(Versioned, versioned)
        self.__offset_type: OffsetType = offset_type
        self.__offset_size: OffsetSize = offset_size
        header_slots = (1 if sized else 0) + (1 if versioned else 0)
        self.__header_size = header_slots * offset_size

    @override
    def __repr__(self) -> str:
        return (
            f"TableLayout(sized={self.__sized}, versioned={self.__versioned}, "
            f"offset_type={self.__offset_type!r})"
        )

    def sized(self) -> bool:
        return self.__sized

    def versioned(self) -> Versioned:
        return self.__versioned

    def offset_type(self) -> OffsetType:
        return self.__offset_type

    def offset_size(self) -> OffsetSize:
        """The offset size, the width in bytes of the offset type"""
        return self.__offset_size

    def header_size(self) -> int:
        """The byte length of the header slots before the field offsets"""
        return self.__header_size


TableLayout.DEFAULT = TableLayout()


# The longest table that offsets and lengths of each offset size address
MAX_LENGTHS: dict[OffsetSize, int] = {
    1: 0xFF,
    2: 0xFFFF,
    4: 0xFFFF_FFFF,
}


def header_length(fields: int, layout: TableLayout, version: int | None) -> int:
    """Returns the byte length of the header of a table of ``fields`` fields in
    ``layout``, whose writer puts ``version`` in the header.

    Raises ``TypeError`` if ``version`` is given for a layout without a version
    slot, or missing for one with it. Raises ``ValueError`` if ``version``
    doesn't fit in the layout's offset type, or the header alone is longer than
    the layout addresses.
    """
    size = layout.offset_size()
    max_length = MAX_LENGTHS[size]
    versioned = layout.versioned()

    if versioned != (version is not None):
        raise TypeError(
            "A versioned layout needs a version"
            if versioned
            else "The layout has no version slot"
        )

    if version is not None and not 0 <= version <= max_length:
        raise ValueError(
            f"Version {version} doesn't fit in a {layout.offset_type()} slot"
        )

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
