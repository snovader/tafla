"""Runs the benchmarks with pyperf, which takes its own options too. See
README.md for full and quick runs."""

import argparse
import sysconfig

import pyperf

from tafla_benchmarks.codecs import IMPLEMENTATION_VARIABLE
from tafla_benchmarks.suite import selected_groups, time_variant


def main() -> None:
    runner = pyperf.Runner(
        program_args=("-m", "tafla_benchmarks"),
        add_cmdline_args=_add_worker_options,
        metadata={"python_build": _python_build()},
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
        help="run only the codecs whose name matches",
    )
    runner.argparser.add_argument(
        "--reverse",
        action="store_true",
        help="run each group's codecs in reverse order",
    )
    args = runner.parse_args()
    _inherit_protobuf_implementation(args)

    # A worker registers the same benchmarks in the same order, and runs the
    # one whose index it was given
    for group in selected_groups(group=args.filter, codec=args.codec):
        variants = reversed(group.variants) if args.reverse else group.variants
        for variant in variants:
            runner.bench_time_func(
                f"{variant.codec}: {group.name}",
                time_variant,
                variant.prepare,
                metadata={"group": group.name, "codec": variant.codec},
            )


def _inherit_protobuf_implementation(args: argparse.Namespace) -> None:
    """Passes the variable that selects protobuf's implementation on to the
    workers, since pyperf passes them only some variables. Each process names
    protobuf's codec after the implementation that it imported, so without the
    variable, workers would time upb, under upb's name, where this process asked
    for pure Python."""
    inherited = args.inherit_environ or []
    if IMPLEMENTATION_VARIABLE not in inherited:
        args.inherit_environ = [*inherited, IMPLEMENTATION_VARIABLE]


def _add_worker_options(command: list[str], args: argparse.Namespace) -> None:
    command.extend(("--filter", args.filter, "--codec", args.codec))
    if args.reverse:
        command.append("--reverse")


def _python_build() -> str:
    """Whether the interpreter was built with the GIL or free-threaded"""
    return "free-threaded" if sysconfig.get_config_var("Py_GIL_DISABLED") else "GIL"


if __name__ == "__main__":
    main()
