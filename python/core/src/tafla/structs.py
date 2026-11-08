import struct

from .constants import SIZEOF_8, SIZEOF_16, SIZEOF_32
from .types import OffsetSize

int8 = struct.Struct("<b")
uint8 = struct.Struct("<B")

int16 = struct.Struct("<h")
uint16 = struct.Struct("<H")

int32 = struct.Struct("<i")
uint32 = struct.Struct("<I")

int64 = struct.Struct("<q")
uint64 = struct.Struct("<Q")

float32 = struct.Struct("<f")
float64 = struct.Struct("<d")

# The unsigned integer of each offset size, which offsets, lengths, counts,
# item ends and header slots are written and read as
OFFSET_STRUCTS: dict[OffsetSize, struct.Struct] = {
    SIZEOF_8: uint8,
    SIZEOF_16: uint16,
    SIZEOF_32: uint32,
}
