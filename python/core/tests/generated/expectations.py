"""How tests compare what the generated code reads with what it wrote, as
descriptions of ``tests/tables.py``, and read bytes that may be corrupted."""

from collections.abc import Callable, Sequence
from contextlib import suppress
from dataclasses import astuple, is_dataclass
from struct import pack
from typing import Any
from unittest import TestCase

from tafla import DecodeError
from tests.tables import Description, comparable_field, comparable_table


def assert_same_table(
    test: TestCase, actual: Description, expected: Description, msg: str | None = None
) -> None:
    """Asserts that two descriptions of a table hold the same values, with
    floats compared by their bits, so that NaN equals NaN and -0.0 differs from
    0.0, which ``==`` and ``assertEqual()`` treat the other way round"""
    test.assertEqual(comparable_table(actual), comparable_table(expected), msg)


def assert_same_fields(
    test: TestCase, actual: list[Description], expected: list[Description]
) -> None:
    """Asserts that descriptions of a struct's fields hold the same values, as
    ``assert_same_table()`` compares them"""
    test.assertEqual(
        [comparable_field(field) for field in actual],
        [comparable_field(field) for field in expected],
    )


def assert_same_structs[Struct](
    test: TestCase,
    actual: Sequence[Struct],
    expected: Sequence[Struct],
    describe: Callable[[Struct], list[Description]],
) -> None:
    """Asserts that two lists of structs hold the same values, by the
    descriptions of their fields, as ``assert_same_fields()`` compares them"""
    test.assertEqual(
        [[comparable_field(field) for field in describe(item)] for item in actual],
        [[comparable_field(field) for field in describe(item)] for item in expected],
    )


def assert_values_or_decode_error(read: Callable[[], object]) -> None:
    """Calls ``read``, which reads bytes that may be corrupted. It may return
    values or raise ``DecodeError``, and any other exception fails the test."""
    with suppress(DecodeError):
        read()


def fields_or_decode_error(read: Callable[[], object]) -> object:
    """The fields or values that ``read`` returns, with each float as its
    bits, which tells NaNs and -0 apart, or the message of the
    ``DecodeError`` that it raises. Any other exception fails the test."""
    try:
        return _bits(read())
    except DecodeError as error:
        return f"DecodeError: {error}"


def _bits(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: _bits(item) for key, item in value.items()}

    if isinstance(value, (list, tuple)):
        return type(value)(_bits(item) for item in value)

    if isinstance(value, float):
        return pack("<d", value)

    return value


def plain(value: object) -> object:
    """``value`` with what a reader returns as plain values, to compare what
    two readers read: each memoryview as a list of its elements, each struct as
    a tuple of its values, and each view of a vector or an array of structs,
    whose items aren't read, as its class's name and its length"""
    if isinstance(value, memoryview):
        return value.tolist()

    if isinstance(value, dict):
        return {key: plain(item) for key, item in value.items()}

    if isinstance(value, (list, tuple)):
        return type(value)(plain(item) for item in value)

    if is_dataclass(value) and not isinstance(value, type):
        return astuple(value)

    if hasattr(value, "__len__") and not isinstance(value, (str, bytes)):
        return (type(value).__name__, len(value))  # ty: ignore[invalid-argument-type]

    return value
