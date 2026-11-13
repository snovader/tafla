"""Takes the runtime's big-endian branches on any host, for the implementation
that ``tafla`` uses"""

import unittest
from collections.abc import Generator
from contextlib import contextmanager
from unittest.mock import patch

import tafla


@contextmanager
def big_endian_host() -> Generator[None]:
    """Treats the host as big-endian while it's entered: arrays' elements are
    byte-swapped as they're written and read, as on a big-endian host. On a
    little-endian host, each element's bytes come out reversed."""
    if tafla.IMPLEMENTATION == "python":
        with patch("tafla.elements.IS_LITTLE_ENDIAN", False):
            yield
        return

    from tafla import _native

    # Only a build with the feature test-hooks has it, as the development
    # environment's is, and no wheel's
    if not hasattr(_native, "_set_little_endian_host"):
        raise unittest.SkipTest("the native module is built without test hooks")

    little_endian = _native._set_little_endian_host(False)
    try:
        yield
    finally:
        _native._set_little_endian_host(little_endian)
