"""Each library's encoding of the workloads, with tafla first as the reference
that the others are compared with"""

from tafla_benchmarks.codecs.codec import Codec, DocumentWorkload, Workload
from tafla_benchmarks.codecs.flatbuffers import FLATBUFFERS
from tafla_benchmarks.codecs.msgpack import MSGPACK, MSGPACK_PURE
from tafla_benchmarks.codecs.protobuf import (
    IMPLEMENTATION_VARIABLE,
    PROTOBUF,
    PROTOBUF_NAMES,
)
from tafla_benchmarks.codecs.struct_sample import STRUCT_SAMPLE
from tafla_benchmarks.codecs.tafla import TAFLA
from tafla_benchmarks.codecs.tafla_gen import TAFLA_GEN
from tafla_benchmarks.workloads import SampleInput

CODECS: tuple[Codec, ...] = (
    TAFLA,
    TAFLA_GEN,
    FLATBUFFERS,
    PROTOBUF,
    MSGPACK,
    MSGPACK_PURE,
)
"""Every codec, with msgpack's pure-Python build after its C build. protobuf's
implementation is the one that this process imported, which
``PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION`` selects for the whole process, so its
pure-Python implementation runs in a process of its own."""

CODEC_NAMES: tuple[str, ...] = tuple(
    name
    for codec in CODECS
    for name in (PROTOBUF_NAMES.values() if codec is PROTOBUF else (codec.name,))
)
"""Every codec's name, in the order of ``CODECS``, with protobuf's under the
names of both of its implementations, whichever this process imported"""

STRUCT = "struct"
"""The name of the standard library's encoding of the fixed-size record"""


def samples() -> dict[str, Workload[SampleInput, SampleInput]]:
    """Returns the encodings of the fixed-size record by name: every codec's,
    and the standard library's ``struct``, which encodes only records at fixed
    offsets"""
    return {codec.name: codec.sample for codec in CODECS} | {STRUCT: STRUCT_SAMPLE}


__all__ = [
    "TAFLA",
    "TAFLA_GEN",
    "CODEC_NAMES",
    "CODECS",
    "Codec",
    "DocumentWorkload",
    "FLATBUFFERS",
    "IMPLEMENTATION_VARIABLE",
    "MSGPACK",
    "MSGPACK_PURE",
    "PROTOBUF",
    "PROTOBUF_NAMES",
    "STRUCT",
    "Workload",
    "samples",
]
