from collections.abc import Iterator

from .table_reader import TableReader


class StringVector:
    """A vector of strings in a table, which reads each string when it's asked
    for, from its end and the end of the string before it, so that a string
    whose bytes are corrupt raises ``DecodeError`` when it's read, rather than
    when the vector is viewed. The code that tafla-gen generates returns one
    from the getter of a ``[string]`` field.

    This is the pure-Python class, which reads through the pure-Python
    ``TableReader``, as the native one reads through the native reader.
    """

    __slots__ = ["__length", "__position", "__reader"]

    def __init__(self, reader: TableReader, position: int) -> None:
        """Views the vector of strings at ``position``, as ``root_offset()``
        or ``field_offset()`` finds it.

        Raises ``DecodeError`` if the vector's count or the ends of its
        strings lie outside the bytes.
        """
        self.__reader = reader
        self.__position = position
        self.__length = reader.get_vector_length(position)

    def __len__(self) -> int:
        """The number of strings"""
        return self.__length

    def __getitem__(self, index: int) -> str:
        """Reads string ``index``, without reading the others, counting from
        the end if it's negative.

        Raises ``IndexError`` if the vector has no string ``index``, and
        ``DecodeError`` if the string lies outside the bytes or isn't valid
        UTF-8.
        """
        if index < 0:
            index += self.__length
        # The count that the constructor checked, rather than reading and
        # checking it for each string
        return self.__reader.get_string_vector_item(
            self.__position, self.__length, index
        )

    def __iter__(self) -> Iterator[str]:
        """Reads every string, as ``tolist()`` does, and then iterates over
        them.

        Raises ``DecodeError`` if a string lies outside the bytes or isn't
        valid UTF-8, before returning any.
        """
        return iter(self.tolist())

    def tolist(self) -> list[str]:
        """Reads every string into a list, from the first to the last, as
        ``TableReader.get_string_vector()`` reads them, which a list of
        many strings takes less time to than reading each.

        Raises ``DecodeError`` if a string lies outside the bytes or isn't
        valid UTF-8.
        """
        return self.__reader.get_string_vector(self.__position)
