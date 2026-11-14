"""The builders that the tests check and the benchmarks time, by name: the
runtime's ``TableBuilder``, and each prototype in each variant. A native
variant is the native class with other defaults, which ``using()`` sets for
the builders created while it's entered."""

import sys
from collections.abc import Generator
from contextlib import contextmanager
from dataclasses import dataclass
from typing import Any

import tafla
from tafla import TableBuilder
from tafla_prototype import direct_table_builder as pure

# The native class, only where tafla uses the native implementation, so that
# the implementations never mix, and only in a build with the feature
# prototype
NativeTableBuilder: Any = None
_set_prototype_defaults: Any = None
if tafla.IMPLEMENTATION == "native":
    from tafla import _native

    NativeTableBuilder = getattr(_native, "DirectTableBuilder", None)
    _set_prototype_defaults = getattr(_native, "_set_prototype_defaults", None)


@dataclass(frozen=True, slots=True)
class Builder:
    """A builder class, and the defaults that the native class needs for it:
    its shortest segment, and whether it grows by copying"""

    name: str
    cls: Any
    native_defaults: tuple[int, bool] | None = None


@contextmanager
def using(builder: Builder) -> Generator[Any]:
    """Sets the native class's defaults for ``builder`` while it's entered,
    and yields its class"""
    if builder.native_defaults is None or _set_prototype_defaults is None:
        yield builder.cls
        return

    previous = _set_prototype_defaults(*builder.native_defaults)
    try:
        yield builder.cls
    finally:
        _set_prototype_defaults(*previous)


TABLE_BUILDER = Builder(f"TableBuilder ({tafla.IMPLEMENTATION} writer)", TableBuilder)

PURE: tuple[Builder, ...] = (
    Builder("pure, +=", pure.DirectTableBuilder),
    Builder("pure, +=, copying", pure.CopyingTableBuilder),
    Builder("pure, preallocated", pure.PreallocatedTableBuilder),
    Builder("pure, preallocated, copying", pure.CopyingPreallocatedTableBuilder),
)
"""The pure-Python prototypes, which work with either implementation"""

NATIVE: tuple[Builder, ...] = (
    ()
    if NativeTableBuilder is None
    else (
        Builder("native", NativeTableBuilder, (pure.SEGMENT_BYTES, False)),
        Builder("native, copying", NativeTableBuilder, (sys.maxsize, False)),
        Builder("native, copy growth", NativeTableBuilder, (pure.SEGMENT_BYTES, True)),
    )
)
"""The native prototype's variants, when the native module was built with
the feature ``prototype``"""
