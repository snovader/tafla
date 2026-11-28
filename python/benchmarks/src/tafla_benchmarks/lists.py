"""The lists suite: how tafla-gen's code writes and reads lists of 10, 100 and
10,000 strings, as a vector of strings and as a vector of tables of one string
each, in pyperf benchmarks, as the TypeScript suite's ``yarn bench:lists``
does. ``python -m tafla_benchmarks.lists`` runs it, with pyperf's options and
the suite's own, ``--filter``, ``--codec`` and ``--reverse``, and with
``TAFLA_PUREPYTHON`` set it times the pure-Python classes; README.md describes
it.

A variant checks that it reads back every string that it wrote, and the last
string, before it's timed, when its worker first times it.
"""

import argparse
import re
import sysconfig
from collections.abc import Buffer, Callable, Sequence
from dataclasses import dataclass
from functools import partial
from typing import Any

import pyperf

import tafla
from tafla_benchmarks.check import RoundTripError
from tafla_benchmarks.generated.lists.lists import NameList, StringList
from tafla_benchmarks.list_shapes import LIST_SHAPES, ListShape
from tafla_benchmarks.suite import Call, Group, Variant, time_variant


@dataclass(frozen=True, slots=True)
class ListCodec:
    """One way of writing and reading a list of strings"""

    name: str
    serialize: Callable[[Sequence[str]], Buffer]
    deserialize: Callable[[Buffer], list[str]]
    """Reads every string, in order"""
    read_last: Callable[[Buffer], str]
    """Reads the last string, without reading the others"""


# The vector of strings, written from the strings as they are, and read
# through the getter's StringVector: every string at once with tolist(), and
# the last from its ends


def _serialize_strings(strings: Sequence[str]) -> Buffer:
    return StringList.serialize_values(strings)


def _deserialize_strings(data: Buffer) -> list[str]:
    return StringList(data).items().tolist()


def _read_last_string(data: Buffer) -> str:
    return StringList(data).items()[-1]


# The same code in functions of their own, as an A/A pair with the vector of
# strings, which shows the harness's noise


def _serialize_strings_again(strings: Sequence[str]) -> Buffer:
    return StringList.serialize_values(strings)


def _deserialize_strings_again(data: Buffer) -> list[str]:
    return StringList(data).items().tolist()


def _read_last_string_again(data: Buffer) -> str:
    return StringList(data).items()[-1]


# Today's list of strings, a vector of tables of one string each, which the
# generated code writes from each table's values, a tuple of its string made
# in each call, in one call, and reads into those tuples with the view's
# field_values(), in one call, or the last one's with item_field_values()


def _serialize_names(strings: Sequence[str]) -> Buffer:
    return NameList.serialize_values([(string,) for string in strings])


def _deserialize_names(data: Buffer) -> list[str]:
    return [value for (value,) in NameList(data).items().field_values()]


def _read_last_name(data: Buffer) -> str:
    return NameList(data).items().item_field_values(-1)[0]


STRINGS = ListCodec(
    "tafla-gen [string]", _serialize_strings, _deserialize_strings, _read_last_string
)
"""The vector of strings, which the others are compared with"""

LIST_CODECS: tuple[ListCodec, ...] = (
    STRINGS,
    ListCodec(
        "tafla-gen [Name]", _serialize_names, _deserialize_names, _read_last_name
    ),
    ListCodec(
        "tafla-gen [string] A/A",
        _serialize_strings_again,
        _deserialize_strings_again,
        _read_last_string_again,
    ),
)


def list_groups() -> list[Group]:
    """Each shape's groups, in the TypeScript suite's order: writing the
    table, reading every string, and reading the last string"""
    groups: list[Group] = []
    for shape in LIST_SHAPES:
        for operation, select in _OPERATIONS:
            variants = tuple(
                Variant(codec.name, partial(_prepare, codec, shape, select))
                for codec in LIST_CODECS
            )
            groups.append(Group(f"{shape.name}: {operation}", variants))

    return groups


type _Select = Callable[[ListCodec, ListShape], Call[Any]]


def _writing(codec: ListCodec, shape: ListShape) -> Call[Any]:
    return Call(codec.serialize, shape.strings())


def _reading_every_string(codec: ListCodec, shape: ListShape) -> Call[Any]:
    return Call(codec.deserialize, codec.serialize(shape.strings()))


def _reading_the_last_string(codec: ListCodec, shape: ListShape) -> Call[Any]:
    return Call(codec.read_last, codec.serialize(shape.strings()))


_OPERATIONS: tuple[tuple[str, _Select], ...] = (
    ("serialize", _writing),
    ("deserialize", _reading_every_string),
    ("read the last string", _reading_the_last_string),
)


def _prepare(codec: ListCodec, shape: ListShape, select: _Select) -> Call[Any]:
    """Returns the call to time, after checking that the codec reads back the
    shape's strings"""
    check_round_trip(codec, shape)
    return select(codec, shape)


def check_round_trip(codec: ListCodec, shape: ListShape) -> None:
    """Checks that the codec reads back every string of the shape, in order,
    and its last string.

    Raises ``RoundTripError`` if it doesn't.
    """
    strings = shape.strings()
    data = codec.serialize(strings)

    if codec.deserialize(data) != list(strings):
        raise RoundTripError(f"{codec.name} didn't read back {shape.name}")
    if codec.read_last(data) != strings[-1]:
        raise RoundTripError(f"{codec.name} didn't read the last of {shape.name}")


def selected_list_groups(*, group: str = "", codec: str = "") -> list[Group]:
    """Returns the groups whose name matches the regular expression ``group``,
    with their variants whose codec matches ``codec``"""
    group_pattern = re.compile(group)
    codec_pattern = re.compile(codec)

    selected: list[Group] = []
    for candidate in list_groups():
        if not group_pattern.search(candidate.name):
            continue

        variants = [v for v in candidate.variants if codec_pattern.search(v.codec)]
        if variants:
            selected.append(Group(candidate.name, tuple(variants)))

    return selected


def main() -> None:
    runner = pyperf.Runner(
        program_args=("-m", "tafla_benchmarks.lists"),
        add_cmdline_args=_add_worker_options,
        metadata={
            "python_build": _python_build(),
            "tafla_implementation": tafla.IMPLEMENTATION,
        },
    )
    runner.argparser.add_argument(
        "--filter",
        default="",
        metavar="REGEX",
        help="run only the groups whose name matches",
    )
    runner.argparser.add_argument(
        "--codec",
        default="",
        metavar="REGEX",
        help="run only the variants whose name matches",
    )
    runner.argparser.add_argument(
        "--reverse",
        action="store_true",
        help="run each group's variants in reverse order",
    )
    args = runner.parse_args()
    _inherit_implementation(args)

    # A worker registers the same benchmarks in the same order, and runs the
    # one whose index it was given
    for group in selected_list_groups(group=args.filter, codec=args.codec):
        variants = reversed(group.variants) if args.reverse else group.variants
        for variant in variants:
            runner.bench_time_func(
                f"{variant.codec}: {group.name}",
                time_variant,
                variant.prepare,
                metadata={"group": group.name, "codec": variant.codec},
            )


def _inherit_implementation(args: argparse.Namespace) -> None:
    """Passes the variable that selects tafla's pure-Python classes on to the
    workers, since pyperf passes them only some variables, so that they time
    the classes that this process uses"""
    inherited = args.inherit_environ or []
    if _PURE_PYTHON_VARIABLE not in inherited:
        args.inherit_environ = [*inherited, _PURE_PYTHON_VARIABLE]


_PURE_PYTHON_VARIABLE = "TAFLA_PUREPYTHON"


def _add_worker_options(command: list[str], args: argparse.Namespace) -> None:
    command.extend(("--filter", args.filter, "--codec", args.codec))
    if args.reverse:
        command.append("--reverse")


def _python_build() -> str:
    """Whether the interpreter was built with the GIL or free-threaded"""
    return "free-threaded" if sysconfig.get_config_var("Py_GIL_DISABLED") else "GIL"


if __name__ == "__main__":
    main()
