"""Times the builders on the comparison's builder rows, the serialization of
the record, the documents of 100 and 100,000 rows, the vector of 100 nested
tables and the array of 100 structs, with the comparison's harness: pyperf,
with each benchmark in worker processes of its own, and the suite's timing
loop and round-trip check. Each builder writes through the comparison's own
tafla codec, compiled anew for it.

The builders are those of the implementation that ``tafla`` uses, which
``TAFLA_PUREPYTHON`` selects and the workers inherit: ``TableBuilder`` twice,
as an A/A pair, each pure-Python prototype, and with the native
implementation, the native prototype twice and in its other variants, and
tafla-gen's code, msgpack and protobuf for reference. Run it from
``python/`` as ``python -m tafla_prototype.bench``, with pyperf's options,
and ``--reverse`` to run each row's builders in reverse order;
``tafla_prototype.report`` prints the results.
"""

import argparse
import types
from collections.abc import Callable
from dataclasses import dataclass
from functools import partial
from pathlib import Path
from typing import Any

import pyperf

import tafla
import tafla_benchmarks.codecs.tafla as tafla_codec
from tafla_benchmarks.check import serialize_checked
from tafla_benchmarks.codecs import MSGPACK, PROTOBUF, TAFLA_GEN, Codec, Workload
from tafla_benchmarks.suite import Call, time_variant
from tafla_benchmarks.workloads import RECORD, document_input, list_input
from tafla_prototype.builders import (
    NATIVE,
    PURE,
    TABLE_BUILDER,
    Builder,
    _set_prototype_defaults,
)

_IMPLEMENTATION_VARIABLE = "TAFLA_PUREPYTHON"


@dataclass(frozen=True, slots=True)
class Row:
    """A workload's serialization, by the comparison's group name"""

    name: str
    select: Callable[[Codec], Workload[Any, Any]]
    input: Callable[[], Any]


ROWS = (
    Row("record: serialize", lambda codec: codec.record, lambda: RECORD),
    Row(
        "document of 100 rows: serialize",
        lambda codec: codec.document,
        partial(document_input, 100),
    ),
    Row(
        "document of 100,000 rows: serialize",
        lambda codec: codec.document,
        partial(document_input, 100_000),
    ),
    Row(
        "vector of 100 nested tables: serialize",
        lambda codec: codec.list,
        partial(list_input, 100),
    ),
    Row(
        "array of 100 structs: serialize",
        lambda codec: codec.structs,
        partial(list_input, 100),
    ),
)


@dataclass(frozen=True, slots=True)
class Variant:
    """A codec to time on each row, and the native defaults its builder
    needs"""

    name: str
    codec: Codec
    native_defaults: tuple[int, bool] | None = None


def codec_of(builder: Builder, copy: str) -> Codec:
    """The comparison's tafla codec with ``builder``'s class as its
    ``TableBuilder``, from the codec's source compiled anew, so that no two
    variants share code that CPython specializes"""
    module = types.ModuleType(f"tafla_codec_{copy}")
    source = Path(tafla_codec.__file__).read_text(encoding="utf-8")
    exec(compile(source, f"<tafla codec, {copy}>", "exec"), module.__dict__)  # noqa: S102
    module.TableBuilder = builder.cls

    return module.TAFLA


def variants() -> list[Variant]:
    """The builders of the implementation that tafla uses, each A/A pair's
    second builder right after its first"""

    def variant(builder: Builder, name: str) -> Variant:
        return Variant(name, codec_of(builder, name), builder.native_defaults)

    chosen = [
        variant(TABLE_BUILDER, TABLE_BUILDER.name),
        variant(TABLE_BUILDER, f"{TABLE_BUILDER.name} A/A"),
        *(variant(builder, builder.name) for builder in PURE),
    ]
    if NATIVE:
        chosen += [
            variant(NATIVE[0], NATIVE[0].name),
            variant(NATIVE[0], f"{NATIVE[0].name} A/A"),
            *(variant(builder, builder.name) for builder in NATIVE[1:]),
        ]
    if tafla.IMPLEMENTATION == "native":
        chosen += [
            Variant("tafla-gen", TAFLA_GEN),
            Variant("msgpack", MSGPACK),
            Variant("protobuf (upb)", PROTOBUF),
        ]
    else:
        chosen.append(Variant("tafla-gen", TAFLA_GEN))

    return chosen


def prepare(variant: Variant, row: Row) -> Call[Any]:
    """Sets the native class's defaults for the variant, in the worker that
    times it alone, and returns its call after checking its round trip"""
    if variant.native_defaults is not None and _set_prototype_defaults is not None:
        _set_prototype_defaults(*variant.native_defaults)

    workload = row.select(variant.codec)
    value = row.input()
    serialize_checked(workload, value)

    return Call(workload.serialize, value)


def main() -> None:
    runner = pyperf.Runner(
        program_args=("-m", "tafla_prototype.bench"),
        add_cmdline_args=_add_worker_options,
        metadata={"tafla_implementation": tafla.IMPLEMENTATION},
    )
    runner.argparser.add_argument(
        "--reverse",
        action="store_true",
        help="run each row's builders in reverse order",
    )
    args = runner.parse_args()
    _inherit_implementation(args)

    chosen = variants()
    for row in ROWS:
        ordered = reversed(chosen) if args.reverse else chosen
        for variant in ordered:
            runner.bench_time_func(
                f"{variant.name}: {row.name}",
                time_variant,
                partial(prepare, variant, row),
                metadata={
                    "group": row.name,
                    "codec": variant.name,
                    "tafla_implementation": tafla.IMPLEMENTATION,
                },
            )


def _inherit_implementation(args: argparse.Namespace) -> None:
    """Passes the variable that selects tafla's implementation on to the
    workers, which pyperf otherwise wouldn't"""
    inherited = args.inherit_environ or []
    if _IMPLEMENTATION_VARIABLE not in inherited:
        args.inherit_environ = [*inherited, _IMPLEMENTATION_VARIABLE]


def _add_worker_options(command: list[str], args: argparse.Namespace) -> None:
    if args.reverse:
        command.append("--reverse")


if __name__ == "__main__":
    main()
