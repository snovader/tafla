"""Chooses the implementation of the classes that the code that tafla-gen
generates uses, once, when ``tafla`` is first imported: the native one, in
the extension module ``tafla._native`` of the platform wheels, or the pure
Python one otherwise. A native module that's installed but fails to import
warns with ``RuntimeWarning`` before the pure-Python one is used. Setting
the environment variable ``TAFLA_PUREPYTHON`` to any value but the empty
string chooses pure Python, as ``MSGPACK_PUREPYTHON`` does for msgpack.

``TableBuilder`` and the rest of the package are Python in both, and use the
chosen classes, so that the implementations never mix."""

import importlib
import os
import warnings
from types import ModuleType
from typing import TYPE_CHECKING, Final, Literal


def _import_native() -> ModuleType | None:
    """Returns the native module, or ``None`` if pure Python is forced or the
    module is missing, as on a platform without a native wheel. A module that
    fails to import otherwise, such as one built for another Python, warns
    with ``RuntimeWarning`` before ``None`` is returned, so that a broken
    install doesn't fall back to the slower classes silently."""
    if os.environ.get("TAFLA_PUREPYTHON"):
        return None

    # Imported by name, since "from . import _native" raises ImportError of
    # the package for a missing module, as it does for a broken one
    name = f"{__package__}._native"
    try:
        return importlib.import_module(name)
    except ModuleNotFoundError as error:
        if error.name != name:
            _warn_of(error)
        return None
    except ImportError as error:
        _warn_of(error)
        return None


def _warn_of(error: ImportError) -> None:
    warnings.warn(
        f"tafla's native module failed to import, so tafla uses its pure-Python "
        f"classes: {error}",
        RuntimeWarning,
        # The warning points at the import of tafla
        stacklevel=5,
    )


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
