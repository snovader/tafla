"""The native implementation of the classes that the code that tafla-gen
generates uses, which ``tafla.implementation`` chooses unless pure Python is
forced. They have the API of the pure-Python classes, whose types this
re-exports."""

from .byte_buffer import ByteBuffer as ByteBuffer
from .fixed_fields import FixedFields as FixedFields
from .layout import TableLayout as TableLayout
from .table_writer import TableWriter as TableWriter

def _set_little_endian_host(little_endian: bool) -> bool:
    """Treats the host as little-endian or not, for the tests, and returns
    whether it was treated as little-endian"""
