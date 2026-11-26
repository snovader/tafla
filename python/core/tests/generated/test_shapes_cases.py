"""Writes and reads the cases of ``conformance/generated``: values of the
messages and the struct of ``schemas/shapes.tafla``, whose bytes the generated
TypeScript wrote. See ``conformance/generated/README.md`` at the root of the
repository for the format of their descriptions in ``shapes.json``."""

import json
import unittest
from collections.abc import Buffer
from dataclasses import asdict
from typing import Any

from tests.generated.expectations import assert_same_fields
from tests.generated.shapes.shapes import (
    Document,
    DocumentFields,
    ItemFields,
    ItemList,
    ItemListFields,
    Point,
    PointFields,
    Priority,
    Sample,
    State,
    Step,
    Task,
    TaskFields,
)
from tests.generated.shapes_tables import (
    describe_document,
    describe_item_list,
    describe_point,
    describe_sample,
    describe_task,
    read_document,
    read_item_list,
    read_point,
    read_task,
)
from tests.tables import Description, from_json
from tests.test_conformance import CONFORMANCE

GENERATED = CONFORMANCE / "generated"

CASES: list[Description] = json.loads(
    (GENERATED / "shapes.json").read_text(encoding="utf-8")
)["cases"]


def read_bytes(file: str) -> bytes:
    return (GENERATED / file).read_bytes()


# The fields of each type as the generated code takes them and the tests'
# readers return them, from JSON that holds 64-bit integers as strings and the
# floats that JSON has no number for as names


def _point(fields: Description) -> PointFields:
    return {
        "x": from_json("float64", fields["x"]),
        "y": from_json("float64", fields["y"]),
        "id": from_json("int64", fields["id"]),
        "name": fields["name"],
    }


def _document(fields: Description) -> DocumentFields:
    columns = fields["columns"]

    return {
        "x": from_json("float64", fields["x"]),
        "y": from_json("float64", fields["y"]),
        "name": fields["name"],
        "columns": {
            "base": from_json("int64", columns["base"]),
            "keys": columns["keys"],
            "a": [from_json("float32", value) for value in columns["a"]],
            "b": [from_json("float32", value) for value in columns["b"]],
        },
    }


def _item_list(fields: Description) -> ItemListFields:
    return {"items": [_item(item) for item in fields["items"]]}


def _item(fields: Description) -> ItemFields:
    pair = fields["pair"]

    return {
        "value": from_json("float32", fields["value"]),
        "pair": {
            "first": from_json("float32", pair["first"]),
            "second": from_json("float32", pair["second"]),
        },
    }


def _sample(fields: Description) -> dict[str, Any]:
    return {
        "key": fields["key"],
        "a": from_json("float32", fields["a"]),
        "b": from_json("float32", fields["b"]),
    }


def _task(fields: Description) -> TaskFields:
    """A task's fields, whose enums JSON holds as their integers"""
    step = fields["step"]

    return {
        "priority": fields["priority"],
        "history": fields["history"],
        "step": Step(step["state"], step["priority"], step["seconds"]),
        "name": fields["name"],
    }


def values_of(case: Description) -> Any:
    """Returns a case's fields as the generated code takes them"""
    fields = case["fields"]

    match case["type"]:
        case "Point":
            return _point(fields)
        case "Document":
            return _document(fields)
        case "ItemList":
            return _item_list(fields)
        case "Sample":
            return _sample(fields)
        case "Task":
            return _task(fields)
        case other:
            raise AssertionError(f"shapes.tafla has no message or struct {other}")


def serialize(kind: str, values: Any) -> Buffer:
    """Writes the values of a message or struct with the generated code"""
    match kind:
        case "Point":
            return Point.serialize(**values)
        case "Document":
            return Document.serialize(**values)
        case "ItemList":
            return ItemList.serialize(**values)
        case "Sample":
            return Sample(**values).serialize()
        case "Task":
            return Task.serialize(**values)
        case other:
            raise AssertionError(f"shapes.tafla has no message or struct {other}")


def read(kind: str, data: Buffer) -> object:
    """Reads every field of a message or struct with the generated code"""
    match kind:
        case "Point":
            return read_point(Point(data))
        case "Document":
            return read_document(Document(data))
        case "ItemList":
            return read_item_list(ItemList(data))
        case "Sample":
            return asdict(Sample.read(data))
        case "Task":
            return read_task(Task(data))
        case other:
            raise AssertionError(f"shapes.tafla has no message or struct {other}")


def describe(kind: str, values: Any) -> list[Description]:
    """Describes the fields of a message or struct's values, to compare them
    with their floats by their bits"""
    match kind:
        case "Point":
            return describe_point(values)["fields"]
        case "Document":
            return describe_document(values)["fields"]
        case "ItemList":
            return describe_item_list(values)["fields"]
        case "Sample":
            return describe_sample(Sample(**values))
        case "Task":
            return describe_task(values)["fields"]
        case other:
            raise AssertionError(f"shapes.tafla has no message or struct {other}")


class TestShapesCases(unittest.TestCase):
    def test_writes_the_bytes_of_every_case(self) -> None:
        for case in CASES:
            with self.subTest(case["name"]):
                serialized = serialize(case["type"], values_of(case))

                self.assertEqual(bytes(serialized), read_bytes(case["file"]))

    def test_reads_the_values_of_every_case(self) -> None:
        for case in CASES:
            with self.subTest(case["name"]):
                kind = case["type"]
                values = read(kind, read_bytes(case["file"]))

                assert_same_fields(
                    self, describe(kind, values), describe(kind, values_of(case))
                )

    def test_finds_cases_of_every_message_and_the_struct(self) -> None:
        kinds = {case["type"] for case in CASES}

        self.assertEqual(kinds, {"Point", "Document", "ItemList", "Sample", "Task"})

    def test_writes_the_task_of_named_values_from_its_enums_members(self) -> None:
        """The case ``task`` holds integers that the schema names, which the
        members of the generated enums are"""
        serialized = Task.serialize(
            priority=Priority.LOW,
            history=[State.WAITING, State.RUNNING, State.FAILED],
            step=Step(State.DONE, Priority.HIGH, 0xFFFF_FFFF),
            name="task",
        )

        self.assertEqual(bytes(serialized), read_bytes("shapes/task.bin"))


if __name__ == "__main__":
    unittest.main()
