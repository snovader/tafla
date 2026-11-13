"""Checks that every codec reads back exactly what it wrote for every workload,
so that the benchmarks can't time a codec that skips or corrupts part of the
work"""

import os
import subprocess
import sys
import unittest
from collections.abc import Buffer
from dataclasses import replace
from pathlib import Path
from typing import Any, override

from tafla_benchmarks.check import RoundTripError, comparable, serialize_checked
from tafla_benchmarks.codecs import (
    CODECS,
    IMPLEMENTATION_VARIABLE,
    MSGPACK,
    MSGPACK_PURE,
    PROTOBUF,
    PROTOBUF_NAMES,
    TAFLA,
    TAFLA_GEN,
    Codec,
    Workload,
    samples,
)
from tafla_benchmarks.item_access import ACCESSES, ItemBytes, sum_item, write_items
from tafla_benchmarks.workloads import (
    DOCUMENT_ROWS,
    LIST_ITEMS,
    RECORD,
    SAMPLE,
    DocumentInput,
    ListInput,
    SampleInput,
    document_input,
    document_lists,
    list_input,
)


class TestRoundTrips(unittest.TestCase):
    documents: list[DocumentInput]

    @classmethod
    @override
    def setUpClass(cls) -> None:
        cls.documents = [document_input(rows) for rows in DOCUMENT_ROWS]

    def assert_reads_back[I](self, workload: Workload[I, Any], input: I) -> None:
        data = workload.serialize(input)

        self.assertEqual(comparable(workload.deserialize(data)), comparable(input))
        # The check that each benchmark runs before timing
        self.assertEqual(bytes(serialize_checked(workload, input)), bytes(data))

    def test_reads_back_the_record(self) -> None:
        for codec in CODECS:
            with self.subTest(codec.name):
                self.assert_reads_back(codec.record, RECORD)

    def test_reads_back_documents_of_arrays_and_of_lists(self) -> None:
        for codec in CODECS:
            for document in self.documents:
                for input in (document, document_lists(document)):
                    columns = type(input.a).__name__
                    with self.subTest(codec.name, rows=len(input.a), columns=columns):
                        self.assert_reads_back(codec.document, input)

    def test_sums_a_document_column_in_order(self) -> None:
        for codec in CODECS:
            for document in self.documents:
                with self.subTest(codec.name, rows=len(document.a)):
                    expected = 0.0
                    for value in document.a:
                        expected += value

                    data = codec.document.serialize(document)
                    self.assertEqual(codec.document.sum_a(data), expected)

    def test_reads_back_the_vector_of_nested_tables(self) -> None:
        vector = list_input(LIST_ITEMS)
        for codec in CODECS:
            with self.subTest(codec.name):
                self.assert_reads_back(codec.list, vector)

    def test_reads_back_the_array_of_structs(self) -> None:
        vector = list_input(LIST_ITEMS)
        for codec in CODECS:
            with self.subTest(codec.name):
                self.assert_reads_back(codec.structs, vector)

    def test_reads_back_the_fixed_size_record(self) -> None:
        for name, workload in samples().items():
            with self.subTest(name):
                self.assert_reads_back(workload, SAMPLE)

    def assert_writes_the_same_bytes(self, codec: Codec, reference: Codec) -> None:
        vector = list_input(LIST_ITEMS)
        documents = [
            input
            for document in self.documents
            for input in (document, document_lists(document))
        ]
        cases: list[tuple[str, Workload[Any, Any], Workload[Any, Any], object]] = [
            ("record", codec.record, reference.record, RECORD),
            *(
                ("document", codec.document, reference.document, document)
                for document in documents
            ),
            ("vector", codec.list, reference.list, vector),
            ("structs", codec.structs, reference.structs, vector),
            ("sample", codec.sample, reference.sample, SAMPLE),
        ]

        for name, workload, expected, input in cases:
            with self.subTest(name):
                self.assertEqual(
                    bytes(workload.serialize(input)), bytes(expected.serialize(input))
                )

    def test_generated_code_writes_the_builders_tables(self) -> None:
        self.assert_writes_the_same_bytes(TAFLA_GEN, TAFLA)

    def test_pure_msgpack_packs_the_c_extensions_bytes(self) -> None:
        self.assert_writes_the_same_bytes(MSGPACK_PURE, MSGPACK)

    def test_names_protobuf_after_the_implementation_selected(self) -> None:
        """When the variable is set, as it is in the process of the pure-Python
        round trips, the codec is named after the implementation it selects"""
        selected = os.environ.get(IMPLEMENTATION_VARIABLE)
        if selected is None:
            self.skipTest(f"{IMPLEMENTATION_VARIABLE} isn't set")

        self.assertEqual(PROTOBUF.name, PROTOBUF_NAMES[selected])

    def test_every_access_reads_the_items_of_tafla_and_tafla_gen(self) -> None:
        input = list_input(LIST_ITEMS)
        last = sum_item(input.items[-1])
        every = 0.0
        for item in input.items:
            every += sum_item(item)

        for data in (write_items(input), _items_by_tafla(input)):
            for access in ACCESSES:
                with self.subTest(access.name):
                    self.assertEqual(access.last_table(data.tables), last)
                    self.assertEqual(access.every_table(data.tables), every)
                    self.assertEqual(access.last_struct(data.structs), last)
                    self.assertEqual(access.every_struct(data.structs), every)

    def test_fails_the_check_of_a_codec_that_loses_signed_zero(self) -> None:
        input = SampleInput(key=1, a=-0.0, b=1.0)
        lossy = Workload(_serialize_nothing, _deserialize_positive_zero)

        with self.assertRaisesRegex(RoundTripError, "didn't read back a of"):
            serialize_checked(lossy, input)

    def test_compares_columns_by_their_bits(self) -> None:
        document = document_input(DOCUMENT_ROWS[0])
        signed = replace(document, a=[-0.0, *document.a[1:]])
        unsigned = replace(document, a=[0.0, *document.a[1:]])

        self.assertNotEqual(comparable(signed), comparable(unsigned))


class TestPureProtobuf(unittest.TestCase):
    """protobuf reads the variable that selects its implementation when a
    process first imports it, so its pure-Python implementation's round trips
    run in a process of their own"""

    @unittest.skipIf(
        PROTOBUF.name == PROTOBUF_NAMES["python"],
        "this process runs protobuf's pure-Python implementation",
    )
    def test_round_trips_pass_in_pure_python(self) -> None:
        tests = Path(__file__).resolve().parent
        command = [
            sys.executable,
            *("-m", "unittest", "discover"),
            *("-s", str(tests), "-t", str(tests.parent), "-p", Path(__file__).name),
        ]
        environment = os.environ | {IMPLEMENTATION_VARIABLE: "python"}

        result = subprocess.run(
            command, env=environment, capture_output=True, text=True, check=False
        )

        self.assertEqual(result.returncode, 0, result.stderr)


def _items_by_tafla(input: ListInput) -> ItemBytes:
    """The list as the tafla codec writes it, by hand"""
    return ItemBytes(TAFLA.list.serialize(input), TAFLA.structs.serialize(input))


def _serialize_nothing(sample: SampleInput) -> Buffer:
    return b""


def _deserialize_positive_zero(data: Buffer) -> SampleInput:
    return SampleInput(key=1, a=0.0, b=1.0)


if __name__ == "__main__":
    unittest.main()
