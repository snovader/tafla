"""Chooses the implementation of the classes that the code that tafla-gen
generates uses, once, when ``tafla`` is first imported: the native one, in
the extension module ``tafla._native`` of the platform wheels, or the pure
Python one otherwise. Setting the environment variable ``TAFLA_PUREPYTHON``
to any value but the empty string chooses pure Python, as ``MSGPACK_PUREPYTHON``
does for msgpack.

``TableBuilder`` and the rest of the package are Python in both, and use the
chosen classes, so that the implementations never mix."""

import os
from types import ModuleType
from typing import TYPE_CHECKING, Final, Literal


def _import_native() -> ModuleType | None:
    """Returns the native module, or ``None`` if pure Python is forced or the
    module can't be imported, as on a platform without a native wheel"""
    if os.environ.get("TAFLA_PUREPYTHON"):
        return None

    try:
        from . import _native
    except ImportError:
        return None

    return _native


_native_module = _import_native()

IMPLEMENTATION: Final[Literal["native", "python"]] = (
    "python" if _native_module is None else "native"
)
"""The implementation of ``ByteBuffer``, ``FixedFields``, ``TableLayout`` and
``TableWriter`` that ``tafla`` uses: ``"native"`` or ``"python"``."""

# Type checkers see the pure-Python classes, whose API the native ones share,
# with the types that they declare, such as TableLayout's type argument
if TYPE_CHECKING or _native_module is None:
    from .byte_buffer import ByteBuffer
    from .fixed_fields import FixedFields
    from .layout import TableLayout
    from .table_writer import TableWriter
else:
    ByteBuffer = _native_module.ByteBuffer
    FixedFields = _native_module.FixedFields
    TableLayout = _native_module.TableLayout
    TableWriter = _native_module.TableWriter

__all__ = ["IMPLEMENTATION", "ByteBuffer", "FixedFields", "TableLayout", "TableWriter"]
