"""Compares the native implementation of the classes that generated code uses
with the pure-Python one, which it replaces when it's available: the same
calls, with any arguments, write the same bytes, read the same values and
raise the same errors. The rest of the tests run against the implementation
that ``tafla`` chose, and CI runs them with each."""

import inspect
import os
import struct
import subprocess
import sys
import unittest
from array import array
from collections.abc import Callable
from dataclasses import dataclass
from functools import partial
from types import ModuleType
from typing import Any, override

from hypothesis import given
from hypothesis.strategies import (
    DataObject,
    DrawFn,
    SearchStrategy,
    binary,
    booleans,
    builds,
    characters,
    composite,
    data,
    floats,
    integers,
    just,
    lists,
    none,
    one_of,
    sampled_from,
    text,
    tuples,
)

import tafla
from tafla import byte_buffer, fixed_fields, layout, table_writer
from tests.tables import FORMATS

try:
    from tafla import _native
except ImportError:
    _native = None

OFFSET_TYPES = ("uint8", "uint16", "uint32")

# Integers at the limits of the arguments that the classes take: within an
# i64, beyond it, and beyond the digits that Python converts to a string.
# They're computed from their exponents, so that Hypothesis can describe the
# strategy without converting them.
LARGE_INTEGERS: SearchStrategy[int] = builds(
    lambda exponent, below, sign: sign * (2**exponent - below),
    sampled_from([63, 64, 100, 15000]),
    sampled_from([0, 1]),
    sampled_from([1, -1]),
)
INTEGERS: SearchStrategy[int] = one_of(integers(-8, 80), integers(), LARGE_INTEGERS)

# Counts of a writer's fields or of a vector's tables: small ones, and ones
# that no offset type can write, which fail before the writer allocates. The
# writers allocate what a count in between asks for, up to 16 GB.
COUNTS: SearchStrategy[int] = one_of(
    integers(-8, 80), integers(max_value=-1), integers(min_value=2**32), LARGE_INTEGERS
)

SCALARS = [*sorted(FORMATS), "bool"]


class Index:
    """An object with ``__index__()``, which the classes and ``struct`` take
    as an integer, and whose error they propagate"""

    def __init__(self, value: int | None) -> None:
        self.value = value

    def __index__(self) -> int:
        if self.value is None:
            raise KeyError("no index")
        return self.value

    @override
    def __repr__(self) -> str:
        return f"Index({self.value!r})"


FLOATS: SearchStrategy[float] = one_of(
    floats(),
    floats(width=32),
    sampled_from([0.0, -0.0, 1e39, -1e39, 3.4028235677973366e38, 5e-324]),
    # NaNs of every payload, quiet and signaling
    integers(0, 2**64 - 1).map(
        lambda bits: struct.unpack("<d", bits.to_bytes(8, "little"))[0]
    ),
)

VALUES: SearchStrategy[Any] = one_of(
    INTEGERS,
    FLOATS,
    booleans(),
    none(),
    just("x"),
    binary(max_size=6),
    builds(Index, one_of(none(), integers(-300, 300))),
)


@dataclass
class Implementation:
    """The classes of one implementation, which never mix with the other's"""

    name: str
    ByteBuffer: Any
    FixedFields: Any
    TableLayout: Any
    TableWriter: Any


PURE = Implementation(
    "python",
    byte_buffer.ByteBuffer,
    fixed_fields.FixedFields,
    layout.TableLayout,
    table_writer.TableWriter,
)


def native_implementation(module: ModuleType) -> Implementation:
    return Implementation(
        "native",
        module.ByteBuffer,
        module.FixedFields,
        module.TableLayout,
        module.TableWriter,
    )


@dataclass(frozen=True)
class Raised:
    """An error as the comparison sees it: its class and message, and its
    cause's class. A ``TypeError`` from using the API wrongly, such as with
    bytes that aren't C-contiguous, compares by its class alone, since CPython
    words it differently where the pure-Python classes rely on it."""

    error: type[BaseException]
    message: str
    cause: type[BaseException] | None


def outcome(call: Callable[[], object]) -> object:
    """What a call returns, in a form that compares floats by their bits and
    views by their contents, or the error that it raises"""
    try:
        return comparable(call())
    except Exception as error:  # noqa: BLE001
        cause = None if error.__cause__ is None else type(error.__cause__)
        message = "" if type(error) is TypeError else str(error)
        return Raised(type(error), message, cause)


def comparable(value: object) -> object:
    if isinstance(value, float):
        return ("float", struct.pack("<d", value))
    if isinstance(value, memoryview):
        return ("view", value.format, value.shape, value.tobytes())
    if isinstance(value, array):
        return ("array", value.typecode, value.tobytes())
    if isinstance(value, (list, tuple)):
        return (type(value).__name__, *(comparable(item) for item in value))
    return value


# Structs and counts of fields too large for any table, whose bytes pass a C
# ssize_t, alone or together, as two of 2**63 bytes wrap a 64-bit length to
# 0, which FixedFields rejects as struct does. Each is computed from its
# exponent, as LARGE_INTEGERS is. Those within a ssize_t can't be compared:
# no writer can allocate them, and the pure-Python one raises MemoryError for
# packing them where the native one writes them with the setters.
HUGE_STRUCTS: SearchStrategy[str] = builds(
    lambda exponent, below: f"{2**exponent - below}s",
    sampled_from([63, 64, 100]),
    sampled_from([0, 1]),
)
HUGE_COUNTS: SearchStrategy[int] = builds(
    lambda exponent: 2**exponent, sampled_from([63, 64, 15000])
)


@composite
def descriptions(draw: DrawFn, depth: int = 1) -> tuple[int, list[Any], bool]:
    """A ``FixedFields``' arguments: the table's fields, the formats of its
    leading fields, nested tables' as descriptions, and whether a string
    follows them. A few are fields of no table, or fewer fields than they
    describe, which both reject."""
    nested = descriptions(depth - 1).map(lambda nested: (*nested[:2], False))
    format_strategy: SearchStrategy[Any] = sampled_from(
        ["b", "B", "h", "H", "i", "I", "q", "Q", "f", "d", "?", "3s", "8s"]
    )
    format_strategy = one_of(format_strategy, HUGE_STRUCTS)
    if depth > 0:
        format_strategy = one_of(format_strategy, nested)

    formats = draw(lists(format_strategy, max_size=4))
    string = draw(booleans()) if depth == 1 else False
    extra = draw(one_of(integers(0, 2), integers(-2, -1), HUGE_COUNTS))
    fields = len(formats) + string + extra
    return fields, formats, string


def fixed_fields_of(
    implementation: Implementation,
    description: tuple[int, list[Any], bool],
    table_layout: Any = None,
) -> Any:
    fields, formats, string = description
    arguments = [
        fixed_fields_of(implementation, each) if isinstance(each, tuple) else each
        for each in formats
    ]
    return implementation.FixedFields(
        fields, *arguments, string=string, layout=table_layout
    )


def value_count(description: tuple[int, list[Any], bool]) -> int:
    _, formats, _ = description
    return sum(value_count(each) if isinstance(each, tuple) else 1 for each in formats)


def layout_arguments() -> SearchStrategy[dict[str, Any]]:
    return builds(
        dict,
        sized=booleans(),
        versioned=booleans(),
        offset_type=sampled_from(OFFSET_TYPES),
    )


@unittest.skipIf(_native is None, "the native implementation isn't built")
class TestImplementations(unittest.TestCase):
    native: Implementation

    @classmethod
    @override
    def setUpClass(cls) -> None:
        assert _native is not None
        cls.native = native_implementation(_native)

    def test_tafla_uses_the_native_classes_unless_pure_python_is_forced(self) -> None:
        if tafla.IMPLEMENTATION == "native":
            self.assertIs(tafla.TableWriter, self.native.TableWriter)
            self.assertIs(tafla.ByteBuffer, self.native.ByteBuffer)
        else:
            self.assertIs(tafla.TableWriter, PURE.TableWriter)
            self.assertIs(tafla.ByteBuffer, PURE.ByteBuffer)

    def test_have_the_same_methods_with_the_same_parameters(self) -> None:
        for name in ["ByteBuffer", "FixedFields", "TableLayout", "TableWriter"]:
            pure, native = getattr(PURE, name), getattr(self.native, name)
            names = {
                attribute
                for attribute in dir(pure)
                if not attribute.startswith("_") or attribute in BUILDER_METHODS
            }
            for attribute in sorted(names):
                with self.subTest(cls=name, attribute=attribute):
                    self.assertTrue(hasattr(native, attribute))
                    expected = getattr(pure, attribute)
                    if not callable(expected):
                        continue
                    self.assertEqual(
                        parameters(getattr(native, attribute)), parameters(expected)
                    )

    def test_native_classes_take_a_pure_python_layout_as_a_native_one(
        self,
    ) -> None:
        arguments = {"sized": True, "versioned": True, "offset_type": "uint16"}
        tables = []
        for table_layout in [
            self.native.TableLayout(**arguments),
            PURE.TableLayout(**arguments),
        ]:
            fields = self.native.FixedFields(2, "h", layout=table_layout)
            writer = self.native.TableWriter(2, table_layout, 3)
            writer.set_fields(fields, (-2,))
            writer.set_string(1, "a")
            data = bytes(writer.bytes())

            buffer = self.native.ByteBuffer(data, table_layout)
            self.assertEqual(buffer.get_version(), 3)
            self.assertEqual(buffer.get_fields(0, fields), (-2,))
            tables.append(data)

        self.assertEqual(tables[1], tables[0])

    def test_native_classes_raise_type_error_for_a_layout_of_another_type(
        self,
    ) -> None:
        for table_layout in ["uint16", None.__class__, 4]:
            with (
                self.subTest(layout=table_layout),
                self.assertRaisesRegex(TypeError, "A layout is a TableLayout"),
            ):
                self.native.TableWriter(1, table_layout)

    @given(data())
    def test_read_the_same_values_and_raise_the_same_errors_whatever_the_bytes(
        self, draw: DataObject
    ) -> None:
        arguments = draw.draw(layout_arguments())
        content = draw.draw(binary(max_size=48))
        description = draw.draw(descriptions())
        buffers = [
            (
                implementation,
                implementation.ByteBuffer(
                    content, implementation.TableLayout(**arguments)
                ),
            )
            for implementation in (PURE, self.native)
        ]

        for _ in range(8):
            method = draw.draw(sampled_from(READS))
            args = draw.draw(READ_ARGUMENTS[method])
            outcomes = [
                outcome(
                    partial(read, buffer, implementation, method, args, description)
                )
                for implementation, buffer in buffers
            ]
            self.assertEqual(outcomes[1], outcomes[0], describe_call(method, args))

    @given(data())
    def test_write_the_same_bytes_and_raise_the_same_errors_whatever_the_calls(
        self, draw: DataObject
    ) -> None:
        arguments = draw.draw(layout_arguments())
        fields = draw.draw(one_of(integers(0, 6), COUNTS))
        version = draw.draw(one_of(none(), integers(-1, 300), INTEGERS))
        description = draw.draw(descriptions())
        calls = draw.draw(lists(WRITES, max_size=10))

        writers: list[tuple[Implementation, Any]] = []
        constructed = [
            outcome(
                partial(
                    create_writer, writers, implementation, arguments, fields, version
                )
            )
            for implementation in (PURE, self.native)
        ]
        self.assertEqual(constructed[1], constructed[0])
        if len(writers) < 2:
            return

        for method, args in calls:
            outcomes = [
                outcome(
                    partial(write, writer, implementation, method, args, description)
                )
                for implementation, writer in writers
            ]
            self.assertEqual(outcomes[1], outcomes[0], describe_call(method, args))

        for _, writer in writers:
            write(writer, PURE, "release_views", (), description)
        written = [outcome(writer.bytes) for _, writer in writers]
        self.assertEqual(written[1], written[0])

    @given(
        layout_arguments(),
        descriptions(),
        lists(VALUES, max_size=12),
        one_of(text(max_size=4), sampled_from(["\ud800", "é" * 300])),
        booleans(),
        tuples(integers(0, 2**16), integers(0, 255)),
    )
    def test_write_and_read_fixed_fields_as_each_other_whatever_the_values(
        self,
        arguments: dict[str, Any],
        description: tuple[int, list[Any], bool],
        values: list[Any],
        string: str,
        nested: bool,
        corruption: tuple[int, int],
    ) -> None:
        """Values that fit their fields, and any others, as set_fields() and
        write_table() take them, which fall back to the setters, and the
        fields read back from the table, and from it with a byte replaced"""
        fixed = FixedTable(arguments, description, nested)
        values = values[: value_count(description)]
        written = [
            outcome(partial(fixed.write, implementation, tuple(values), string))
            for implementation in (PURE, self.native)
        ]
        self.assertEqual(written[1], written[0])

        data = getattr(written[0], "data", None)
        if not isinstance(data, bytes):
            return
        at, byte = corruption
        corrupted = bytearray(data)
        if corrupted:
            corrupted[at % len(corrupted)] = byte

        for table in [data, bytes(corrupted)]:
            read = [
                outcome(partial(fixed.read, implementation, table))
                for implementation in (PURE, self.native)
            ]
            self.assertEqual(read[1], read[0])


# Imports tafla with a finder that the module tafla._native finds first,
# which raises the error that a missing or a broken module raises, of the
# module named missing, and prints
# the implementation that tafla chose and the warnings that importing it gave
IMPORT_WITH_NATIVE_MODULE = """
import sys, warnings

class Finder:
    def find_spec(self, name, path=None, target=None):
        if name == "tafla._native":
            raise {error}("{message}", name={missing!r})
        return None

sys.meta_path.insert(0, Finder())
with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always")
    import tafla
print(tafla.IMPLEMENTATION)
for warning in caught:
    print(warning.category.__name__, warning.message)
"""


class TestChoice(unittest.TestCase):
    """How ``tafla`` chooses its implementation when the native module is
    missing, as without a native wheel, or fails to import, as a module built
    for another Python does"""

    def import_tafla(
        self, error: str, message: str, missing: str = "tafla._native"
    ) -> list[str]:
        code = IMPORT_WITH_NATIVE_MODULE.format(
            error=error, message=message, missing=missing
        )
        environment = {
            name: value
            for name, value in os.environ.items()
            if name != "TAFLA_PUREPYTHON"
        }
        result = subprocess.run(
            [sys.executable, "-c", code],
            capture_output=True,
            text=True,
            env=environment,
            check=True,
        )
        return result.stdout.splitlines()

    def test_uses_pure_python_silently_when_the_native_module_is_missing(
        self,
    ) -> None:
        printed = self.import_tafla(
            "ModuleNotFoundError", "No module named 'tafla._native'"
        )
        self.assertEqual(printed, ["python"])

    def test_warns_when_the_native_module_fails_to_import(self) -> None:
        printed = self.import_tafla("ImportError", "undefined symbol: PyFoo")

        self.assertEqual(printed[0], "python")
        self.assertEqual(len(printed), 2)
        self.assertRegex(printed[1], r"^RuntimeWarning .*undefined symbol: PyFoo")

    def test_warns_when_a_module_that_the_native_module_imports_is_missing(
        self,
    ) -> None:
        printed = self.import_tafla(
            "ModuleNotFoundError", "No module named 'other'", "other"
        )
        self.assertEqual(printed[0], "python")
        self.assertRegex(printed[1], r"^RuntimeWarning ")


def create_writer(
    writers: list[tuple[Implementation, Any]],
    implementation: Implementation,
    arguments: dict[str, Any],
    fields: int,
    version: int | None,
) -> None:
    """Appends a writer, unless creating it raises"""
    table_layout = implementation.TableLayout(**arguments)
    if version is None:
        writer = implementation.TableWriter(fields, table_layout)
    else:
        writer = implementation.TableWriter(fields, table_layout, version)
    writers.append((implementation, writer))


@dataclass(frozen=True)
class FixedTable:
    """A table that starts with fixed fields, of a layout, or that nests a
    table of them in its last field"""

    arguments: dict[str, Any]
    description: tuple[int, list[Any], bool]
    nested: bool

    def fields(self, implementation: Implementation) -> Any:
        table_layout = implementation.TableLayout(**self.arguments)
        root_layout = None if self.nested else table_layout
        return fixed_fields_of(implementation, self.description, root_layout)

    def write(
        self, implementation: Implementation, values: tuple[Any, ...], string: str
    ) -> Written:
        table_layout = implementation.TableLayout(**self.arguments)
        version = [1] if self.arguments["versioned"] else []
        writer = implementation.TableWriter(
            self.description[0] + 1, table_layout, *version
        )
        fields = self.fields(implementation)
        if self.nested:
            writer.set_table(self.description[0], 1)
            writer.write_table(fields, values, string)
        else:
            writer.set_fields(fields, values, string)
        return Written(bytes(writer.bytes()))

    def read(self, implementation: Implementation, data: bytes) -> object:
        table_layout = implementation.TableLayout(**self.arguments)
        buffer = implementation.ByteBuffer(data, table_layout)
        position = buffer.root_offset(self.description[0]) if self.nested else 0
        return buffer.get_fields(position, self.fields(implementation))


@dataclass(frozen=True)
class Written:
    data: bytes


def describe_call(method: str, args: tuple[Any, ...]) -> str:
    """A call, for a failure's message, unless an integer in it has more
    digits than Python converts to a string"""
    try:
        return f"{method}{args!r}"
    except ValueError:
        return f"{method}(...)"


# The writer's private methods that TableBuilder calls
BUILDER_METHODS = {"_checked", "_copy_table", "_set_encoded_string"}


def parameters(method: Callable[..., object]) -> list[str]:
    """The names of a method's parameters, without ``self``"""
    names = list(inspect.signature(method).parameters)
    return names[1:] if names and names[0] in {"self", "cls"} else names


SCALAR_READS = [f"get_{kind}" for kind in sorted(FORMATS)]
ARRAY_READS = [f"get_{kind}_array" for kind in sorted(FORMATS)]

READS = [
    *SCALAR_READS,
    *ARRAY_READS,
    "get_string",
    "root_offset",
    "field_offset",
    "get_struct",
    "get_struct_array",
    "get_vector_length",
    "get_vector_offset",
    "get_vector_item_offset",
    "get_vector_offsets",
    "get_vector",
    "get_fields",
    "get_vector_fields",
    "get_size",
    "get_version",
    "bytes",
]

POSITIONS = INTEGERS
SIZES = one_of(integers(-2, 20), INTEGERS)

READ_ARGUMENTS: dict[str, SearchStrategy[tuple[Any, ...]]] = {
    **{method: tuples(POSITIONS) for method in SCALAR_READS + ARRAY_READS},
    "get_string": tuples(POSITIONS),
    "root_offset": tuples(POSITIONS),
    "field_offset": tuples(POSITIONS, POSITIONS),
    "get_struct": tuples(POSITIONS, SIZES),
    "get_struct_array": tuples(POSITIONS, SIZES),
    "get_vector_length": tuples(POSITIONS),
    "get_vector_offset": tuples(POSITIONS, POSITIONS),
    "get_vector_item_offset": tuples(POSITIONS, integers(0, 12), POSITIONS),
    "get_vector_offsets": tuples(POSITIONS),
    "get_vector": tuples(POSITIONS),
    "get_fields": tuples(POSITIONS),
    "get_vector_fields": tuples(POSITIONS),
    "get_size": just(()),
    "get_version": just(()),
    "bytes": just(()),
}


def read(
    buffer: Any,
    implementation: Implementation,
    method: str,
    args: tuple[Any, ...],
    description: tuple[int, list[Any], bool],
) -> object:
    if method in {"get_fields", "get_vector_fields"}:
        # A nested table's fields, for any offset type
        fields = fixed_fields_of(implementation, description)
        return getattr(buffer, method)(*args, fields)
    return getattr(buffer, method)(*args)


ARRAYS: SearchStrategy[Any] = one_of(
    lists(one_of(integers(-130, 300), INTEGERS, FLOATS), max_size=4),
    builds(array, just("i"), lists(integers(-(2**31), 2**31 - 1), max_size=4)),
    builds(array, just("d"), lists(FLOATS, max_size=4)),
    builds(array, just("B"), lists(integers(0, 255), max_size=4)),
    binary(max_size=8),
    binary(min_size=2, max_size=8).map(lambda data: memoryview(data)[::2]),
)

STRUCTS: SearchStrategy[Any] = one_of(
    binary(max_size=12),
    binary(max_size=12).map(bytearray),
    binary(min_size=2, max_size=12).map(lambda data: memoryview(data)[::2]),
)

ALIGNMENTS: SearchStrategy[Any] = one_of(
    sampled_from([1, 2, 4, 8]), sampled_from([0, 3, -4, 16, 2.0, True])
)

INDICES = one_of(integers(-6, 8), INTEGERS)

WRITES: SearchStrategy[tuple[str, tuple[Any, ...]]] = one_of(
    tuples(
        sampled_from([f"set_{kind}" for kind in sorted(FORMATS)]),
        tuples(INDICES, VALUES),
    ),
    tuples(
        sampled_from([f"set_{kind}_array" for kind in sorted(FORMATS)]),
        tuples(INDICES, ARRAYS),
    ),
    tuples(
        just("set_string"),
        tuples(
            INDICES,
            one_of(text(characters(codec="utf-8"), max_size=6), just("\ud800")),
        ),
    ),
    tuples(just("set_struct"), tuples(INDICES, STRUCTS)),
    tuples(
        just("set_struct_array"),
        tuples(INDICES, STRUCTS, one_of(integers(-1, 4), INTEGERS), ALIGNMENTS),
    ),
    tuples(just("set_table"), tuples(INDICES, ALIGNMENTS)),
    # Counts of fields and parents outside the contract too, which the
    # pure-Python writer takes as any int
    tuples(
        just("start_table"),
        tuples(one_of(integers(0, 4), integers(-80, -1), HUGE_COUNTS, LARGE_INTEGERS)),
    ),
    tuples(just("end_table"), tuples(one_of(integers(0, 40), INTEGERS))),
    # A view of the bytes that's released at once, one that's held, and the
    # release of those held, between which the bytes can't grow
    tuples(sampled_from(["bytes", "hold_view", "release_views"]), just(())),
    tuples(just("set_vector"), tuples(INDICES, one_of(integers(-1, 4), COUNTS))),
    tuples(just("set_item_end"), tuples(one_of(integers(-4, 40), INTEGERS), INDICES)),
    tuples(
        sampled_from(["set_fields", "write_table"]),
        tuples(lists(VALUES, max_size=8), text(max_size=4)),
    ),
    tuples(
        just("set_vector_tables"),
        tuples(
            INDICES,
            lists(tuples(lists(VALUES, max_size=8), text(max_size=4)), max_size=3),
        ),
    ),
)


# The views of each writer that the calls hold, by the writer's id, until a
# call releases them, or the test does
HELD_VIEWS: dict[int, list[memoryview[int]]] = {}


def write(
    writer: Any,
    implementation: Implementation,
    method: str,
    args: tuple[Any, ...],
    description: tuple[int, list[Any], bool],
) -> object:
    if method == "hold_view":
        view = memoryview(writer)
        HELD_VIEWS.setdefault(id(writer), []).append(view)
        return view
    if method == "release_views":
        for view in HELD_VIEWS.pop(id(writer), []):
            view.release()
        return None
    if method in {"set_fields", "write_table"}:
        values, string = args
        fields = fixed_fields_of(implementation, description)
        # At most the fields' values, as the methods take them: given more,
        # the pure-Python packer ignores them for nested tables alone
        values = tuple(values[: value_count(description)])
        return getattr(writer, method)(fields, values, string)
    if method == "set_vector_tables":
        index, items = args
        fields = fixed_fields_of(implementation, description)
        has_string = description[2]
        # Each item's values as set_fields() takes them, and its string last
        tables = [
            (*values[: value_count(description)], *([string] if has_string else []))
            for values, string in items
        ]
        return writer.set_vector_tables(index, fields, tables)
    return getattr(writer, method)(*args)


if __name__ == "__main__":
    unittest.main()
