"""Reads and rebuilds the tables of tafla's conformance suite, which the
TypeScript runtime writes, and the cases of presence beside it. See
``conformance/README.md`` at the root of the repository for the format of
their descriptions in ``cases.json``."""

import json
import unittest
from dataclasses import dataclass
from pathlib import Path
from typing import Self

from tafla import DecodeError
from tests.tables import (
    Description,
    DescriptionReader,
    build_table,
    comparable_field,
    comparable_table,
    read_table,
)

CONFORMANCE = Path(__file__).parents[3] / "conformance"


@dataclass(frozen=True)
class Suite:
    """The cases that a directory's ``cases.json`` describes, whose tables'
    files lie in the directory"""

    directory: Path
    cases: Description

    @classmethod
    def load(cls, directory: Path) -> Self:
        cases = json.loads((directory / "cases.json").read_text(encoding="utf-8"))
        return cls(directory, cases)

    def bytes_of(self, file: str) -> bytes:
        return (self.directory / file).read_bytes()

    def valid_table(self, name: str) -> Description:
        """The table of the valid case ``name``"""
        return next(
            case["table"] for case in self.cases["valid"] if case["name"] == name
        )


SUITES: dict[str, Suite] = {
    "conformance": Suite.load(CONFORMANCE),
    # The cases of absent fields, whose offsets must be 0, and of tables that
    # a later revision reads, whose `missing` counts the fields that it finds
    # missing from them, which lie apart from the suite's other cases
    "presence": Suite.load(CONFORMANCE / "presence"),
}


class TestConformance(unittest.TestCase):
    def test_reads_every_valid_table(self) -> None:
        for name, suite in SUITES.items():
            for case in suite.cases["valid"]:
                with self.subTest(suite=name, case=case["name"]):
                    table = case["table"]
                    read = read_table(suite.bytes_of(case["file"]), table)

                    self.assertEqual(comparable_table(read), comparable_table(table))

    def test_rebuilds_every_valid_table_byte_for_byte(self) -> None:
        for name, suite in SUITES.items():
            for case in suite.cases["valid"]:
                with self.subTest(suite=name, case=case["name"]):
                    built = bytes(build_table(case["table"]).bytes())
                    self.assertEqual(built, suite.bytes_of(case["file"]))

    def test_reads_every_equivalent_table_as_its_valid_case(self) -> None:
        """Readers accept more than writers write, such as any byte other than
        0 as a true bool, so these bytes read the valid case's values"""
        for name, suite in SUITES.items():
            for case in suite.cases["equivalent"]:
                with self.subTest(suite=name, case=case["name"]):
                    table = suite.valid_table(case["case"])
                    read = read_table(suite.bytes_of(case["file"]), table)

                    self.assertEqual(comparable_table(read), comparable_table(table))

    def test_reads_invalid_tables_up_to_the_field_that_raises(self) -> None:
        for name, suite in SUITES.items():
            for case in suite.cases["invalid"]:
                with self.subTest(suite=name, case=case["name"]):
                    table = suite.valid_table(case["case"])
                    fields = table["fields"]
                    data = suite.bytes_of(case["file"])
                    reader = DescriptionReader(data, table["layout"])

                    for index in range(case["field"]):
                        read = reader.read_root_field(index, fields[index])
                        self.assertEqual(
                            comparable_field(read), comparable_field(fields[index])
                        )

                    with self.assertRaises(DecodeError):
                        reader.read_root_field(case["field"], fields[case["field"]])

    def test_finds_valid_equivalent_and_invalid_cases(self) -> None:
        for name, suite in SUITES.items():
            with self.subTest(suite=name):
                self.assertGreater(len(suite.cases["valid"]), 0)
                self.assertGreater(len(suite.cases["invalid"]), 0)

        # The cases of presence have no equivalent tables
        self.assertGreater(len(SUITES["conformance"].cases["equivalent"]), 0)


if __name__ == "__main__":
    unittest.main()
