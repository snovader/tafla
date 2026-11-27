import struct

from .constants import SIZEOF_8, SIZEOF_16, SIZEOF_32
from .types import IntTypeCode, OffsetSize

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

# The format character of the unsigned integer of each offset size, which
# arrays and memoryviews of a vector's ends take as their type code, and
# struct as the format of each, in a format that repeats it
OFFSET_CODES: dict[OffsetSize, IntTypeCode] = {
    SIZEOF_8: "B",
    SIZEOF_16: "H",
    SIZEOF_32: "I",
}
