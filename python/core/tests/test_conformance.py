"""Reads and rebuilds the tables of tafla's conformance suite, which the
TypeScript runtime writes. See ``conformance/README.md`` at the root of the
repository for the format of their descriptions in ``cases.json``."""

import json
import unittest
from pathlib import Path

from tafla import DecodeError
from tests.tables import (
    Description,
    TableReader,
    build_table,
    build_table_in_place,
    comparable_field,
    comparable_table,
    read_table,
)

CONFORMANCE = Path(__file__).parents[3] / "conformance"

SUITE: Description = json.loads(
    (CONFORMANCE / "cases.json").read_text(encoding="utf-8")
)
VALID_CASES: dict[str, Description] = {case["name"]: case for case in SUITE["valid"]}


def read_bytes(file: str) -> bytes:
    return (CONFORMANCE / file).read_bytes()


class TestConformance(unittest.TestCase):
    def test_reads_every_valid_table(self) -> None:
        for case in SUITE["valid"]:
            with self.subTest(case["name"]):
                table = case["table"]
                read = read_table(read_bytes(case["file"]), table)

                self.assertEqual(comparable_table(read), comparable_table(table))

    def test_rebuilds_every_valid_table_byte_for_byte(self) -> None:
        """From nested builders, and with the tables that a builder can write
        in place written so"""
        for case in SUITE["valid"]:
            for build in (build_table, build_table_in_place):
                with self.subTest(case["name"], build=build.__name__):
                    built = bytes(build(case["table"]).bytes())
                    self.assertEqual(built, read_bytes(case["file"]))

    def test_reads_every_equivalent_table_as_its_valid_case(self) -> None:
        """Readers accept more than writers write, such as any byte other than
        0 as a true bool, so these bytes read the valid case's values"""
        for case in SUITE["equivalent"]:
            with self.subTest(case["name"]):
                table = VALID_CASES[case["case"]]["table"]
                read = read_table(read_bytes(case["file"]), table)

                self.assertEqual(comparable_table(read), comparable_table(table))

    def test_reads_invalid_tables_up_to_the_field_that_raises(self) -> None:
        for case in SUITE["invalid"]:
            with self.subTest(case["name"]):
                fields = VALID_CASES[case["case"]]["table"]["fields"]
                layout = VALID_CASES[case["case"]]["table"]["layout"]
                reader = TableReader(read_bytes(case["file"]), layout)

                for index in range(case["field"]):
                    read = reader.read_root_field(index, fields[index])
                    self.assertEqual(
                        comparable_field(read), comparable_field(fields[index])
                    )

                with self.assertRaises(DecodeError):
                    reader.read_root_field(case["field"], fields[case["field"]])

    def test_finds_valid_equivalent_and_invalid_cases(self) -> None:
        self.assertGreater(len(SUITE["valid"]), 0)
        self.assertGreater(len(SUITE["equivalent"]), 0)
        self.assertGreater(len(SUITE["invalid"]), 0)


if __name__ == "__main__":
    unittest.main()
