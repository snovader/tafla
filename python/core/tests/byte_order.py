"""Takes the runtime's big-endian branches on any host, for the implementation
that ``tafla`` uses"""

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

    little_endian = _native._set_little_endian_host(False)
    try:
        yield
    finally:
        _native._set_little_endian_host(little_endian)
