"""Checks that the lists suite's strings are the TypeScript suite's, which these
were printed from, and that every variant reads back what it wrote"""

import unittest

import tafla
from tafla import TableBuilder
from tafla_benchmarks.check import RoundTripError
from tafla_benchmarks.list_shapes import LIST_SHAPES, ListShape
from tafla_benchmarks.lists import (
    LIST_CODECS,
    STRINGS,
    ListCodec,
    check_round_trip,
    list_groups,
)

# Whether the native classes read and write vectors of strings, which they do
# once they bind the Rust runtime's. Until then, the suite runs with the
# pure-Python classes alone: setting this to True runs its tests with both.
NATIVE_STRING_VECTORS: bool = False

skip_without_string_vectors = unittest.skipUnless(
    tafla.IMPLEMENTATION == "python" or NATIVE_STRING_VECTORS,
    "the native classes don't read or write vectors of strings yet",
)

# Each shape's first 16 characters of its first and last strings, and a 32-bit
# hash of every string's code points in order, as the TypeScript suite's
# shapes.ts printed them
TYPESCRIPT_SHAPES = [
    ("10 × 16 chars of ASCII", "2FsyeXxp1ewxJijA", "CcfRsv23GwWuZIqm", 772871957),
    ("10 × 256 chars of ASCII", "2GYBM2rvPSVojjhN", "3VpjDEeuixVZfJJy", 112022847),
    ("10 × 8 chars of 3 bytes", "鑡絞眙蜩榩眳矝愞", "琑翘趠谎衬耂竎刓", 699265638),
    ("100 × 16 chars of ASCII", "2N45gJLJhyrBp817", "GuM8hMtpdCgpm337", 4043576274),
    ("100 × 256 chars of ASCII", "2OC9KSUHzW0k790o", "e1d0Fdjo5UszbCUg", 1275637513),
    ("100 × 8 chars of 3 bytes", "鑗蝝姘徃栲鈞甕顽", "鱵髩痧剻熉五憓荜", 249289010),
    ("10,000 × 16 chars of ASCII", "2VGBSOLG6JrrCjwD", "FzBrmc8Y0jcGbWah", 3888143744),
    ("10,000 × 256 chars of ASCII", "2WgxnHUMGp0eUivR", "VU0lrgd6V0hWxlMq", 1626208024),
    ("10,000 × 8 chars of 3 bytes", "鑶鄷攴臚鷑靆畖鱯", "淟坓惗聳侞忝鞂姩", 2214151391),
]


def hash_strings(strings: tuple[str, ...]) -> int:
    """Hashes every string's code points, in order, as the TypeScript script
    that printed these did with 32-bit integers"""
    digest = 0
    for string in strings:
        for character in string:
            digest = (digest * 31 + ord(character)) & 0xFFFF_FFFF

    return digest


def unread(codec: ListCodec) -> ListCodec:
    """The codec, but reading every string as one string fewer"""
    return ListCodec(
        codec.name,
        codec.serialize,
        lambda data: codec.deserialize(data)[:-1],
        codec.read_last,
    )


class TestListShapes(unittest.TestCase):
    def test_generates_the_typescript_suites_strings(self) -> None:
        self.assertEqual(
            [shape.name for shape in LIST_SHAPES], [s[0] for s in TYPESCRIPT_SHAPES]
        )

        for shape, (name, first, last, digest) in zip(
            LIST_SHAPES, TYPESCRIPT_SHAPES, strict=True
        ):
            with self.subTest(name):
                strings = shape.strings()

                self.assertEqual(len(strings), shape.count)
                self.assertEqual({len(string) for string in strings}, {shape.length})
                self.assertEqual((strings[0][:16], strings[-1][:16]), (first, last))
                self.assertEqual(hash_strings(strings), digest)


@skip_without_string_vectors
class TestListRoundTrips(unittest.TestCase):
    def test_every_variant_reads_back_every_shape(self) -> None:
        for shape in LIST_SHAPES:
            for codec in LIST_CODECS:
                with self.subTest(shape=shape.name, codec=codec.name):
                    check_round_trip(codec, shape)

    def test_writes_the_vector_of_strings_that_table_builder_writes(self) -> None:
        for shape in LIST_SHAPES:
            with self.subTest(shape.name):
                builder = TableBuilder(1)
                builder.set_string_vector(shape.strings())

                self.assertEqual(
                    bytes(STRINGS.serialize(shape.strings())), bytes(builder.bytes())
                )

    def test_rejects_a_variant_that_doesnt_read_back_every_string(self) -> None:
        shape: ListShape = LIST_SHAPES[0]

        with self.assertRaises(RoundTripError):
            check_round_trip(unread(STRINGS), shape)

    def test_prepares_every_benchmark_after_checking_it(self) -> None:
        """Each group's variants time a call that returns their result for
        the shape's strings, for the shapes of 10 strings, which are quick"""
        for group in list_groups():
            if not group.name.startswith("10 × "):
                continue
            for variant in group.variants:
                with self.subTest(group=group.name, codec=variant.codec):
                    call = variant.prepare()
                    self.assertIsNotNone(call.operation(call.argument))


if __name__ == "__main__":
    unittest.main()
