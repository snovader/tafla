import copy
import pickle
import unittest
from typing import Any, cast

from tafla import OffsetSize, OffsetType, TableLayout


class TestTableLayout(unittest.TestCase):
    def test_defaults_to_no_header_slots_and_uint32_offsets(self) -> None:
        for layout in [TableLayout.DEFAULT, TableLayout()]:
            with self.subTest(layout=layout):
                self.assertFalse(layout.sized())
                self.assertFalse(layout.versioned())
                self.assertEqual(layout.offset_type(), "uint32")
                self.assertEqual(layout.offset_size(), 4)
                self.assertEqual(layout.header_size(), 0)

    def test_has_the_offset_type_it_was_given_and_its_size_in_bytes(self) -> None:
        cases: list[tuple[OffsetType, OffsetSize]] = [
            ("uint8", 1),
            ("uint16", 2),
            ("uint32", 4),
        ]

        for offset_type, offset_size in cases:
            with self.subTest(offset_type=offset_type):
                layout = TableLayout(offset_type=offset_type)

                self.assertEqual(layout.offset_type(), offset_type)
                self.assertEqual(layout.offset_size(), offset_size)

    def test_has_a_header_slot_for_the_length_and_the_version(self) -> None:
        cases: list[tuple[bool, bool, OffsetType, int]] = [
            (False, False, "uint8", 0),
            (True, False, "uint8", 1),
            (False, True, "uint16", 2),
            (True, True, "uint16", 4),
            (True, True, "uint32", 8),
        ]

        for sized, versioned, offset_type, header_size in cases:
            with self.subTest(sized=sized, versioned=versioned, type=offset_type):
                layout = TableLayout(
                    sized=sized, versioned=versioned, offset_type=offset_type
                )

                self.assertEqual(layout.header_size(), header_size)

    def test_rejects_anything_but_an_offset_type(self) -> None:
        """Signed types, other widths, offset sizes in bytes and values that
        aren't strings, unhashable ones included"""
        for offset_type in ["int16", "uint64", "UINT8", "", 2, 4, None, ["uint8"]]:
            with self.subTest(offset_type=offset_type):
                with self.assertRaises(ValueError) as raised:
                    TableLayout(offset_type=cast(Any, offset_type))

                self.assertEqual(
                    str(raised.exception), f"{offset_type!r} isn't an offset type"
                )

    def test_rejects_the_offset_size_as_a_keyword(self) -> None:
        """A keyword that isn't read would leave its default in its place"""
        cases: list[dict[str, Any]] = [
            {"offset_size": 2},
            {"offset_type": "uint16", "offset_size": 2},
        ]

        for options in cases:
            with (
                self.subTest(options=options),
                self.assertRaisesRegex(TypeError, "offset_size"),
            ):
                TableLayout(**options)

    def test_shows_its_offset_type_in_its_repr(self) -> None:
        layout = TableLayout(sized=True, offset_type="uint16")

        self.assertEqual(
            repr(layout),
            "TableLayout(sized=True, versioned=False, offset_type='uint16')",
        )

    def test_pickles_and_copies_as_a_layout_of_the_same_header_and_offset_type(
        self,
    ) -> None:
        layouts = [
            TableLayout.DEFAULT,
            TableLayout(sized=True, versioned=True, offset_type="uint16"),
        ]
        for layout in layouts:
            copies = [
                pickle.loads(pickle.dumps(layout)),
                copy.copy(layout),
                copy.deepcopy(layout),
            ]
            for copied in copies:
                with self.subTest(layout=layout, copied=copied):
                    self.assertIs(type(copied), type(layout))
                    self.assertEqual(repr(copied), repr(layout))
                    self.assertEqual(copied.header_size(), layout.header_size())


if __name__ == "__main__":
    unittest.main()
