"""Prints what the timings don't show: each codec's encoded size of every
workload, and what it does with a document cut short, as the TypeScript
comparison prints after its timings. Run it with
``python -m tafla_benchmarks.encoding``."""

from collections.abc import Buffer, Callable

from tafla_benchmarks.codecs import CODECS, MSGPACK_PURE, STRUCT, samples
from tafla_benchmarks.workloads import (
    DOCUMENT_ROWS,
    LIST_ITEMS,
    RECORD,
    SAMPLE,
    DocumentInput,
    DocumentOutput,
    document_input,
    list_input,
)

_NAME_WIDTH = 16
_COLUMN_WIDTH = 12

# msgpack's pure-Python build packs the same bytes as its C extension, and
# raises the same error for a truncated document
_CODECS = [codec for codec in CODECS if codec is not MSGPACK_PURE]


def main() -> None:
    documents = [document_input(rows) for rows in DOCUMENT_ROWS]

    print_sizes(documents)
    print()
    print_truncation(documents[-1])


def print_sizes(documents: list[DocumentInput]) -> None:
    columns = [
        "record",
        *(f"doc {rows}" for rows in DOCUMENT_ROWS),
        "vector",
        "structs",
        "fixed",
    ]
    print("Encoded size (bytes)")
    print(f"  {'':{_NAME_WIDTH}}" + "".join(f"{c:>{_COLUMN_WIDTH}}" for c in columns))

    vector = list_input(LIST_ITEMS)
    for codec in _CODECS:
        sizes = [
            _size(codec.record.serialize(RECORD)),
            *(_size(codec.document.serialize(document)) for document in documents),
            _size(codec.list.serialize(vector)),
            _size(codec.structs.serialize(vector)),
            _size(codec.sample.serialize(SAMPLE)),
        ]
        print(_row(codec.name, [str(size) for size in sizes]))

    # struct encodes only the fixed-size record
    fixed = _size(samples()[STRUCT].serialize(SAMPLE))
    print(_row(STRUCT, [""] * (len(columns) - 1) + [str(fixed)]))


def print_truncation(document: DocumentInput) -> None:
    """Deserializes copies of a document cut short, as a dropped connection
    would leave it. A copy, since a view could read on into the full bytes."""
    rows = len(document.a)
    print(f"Truncated document of {rows} rows: deserialize()")

    for codec in _CODECS:
        data = bytes(codec.document.serialize(document))
        for length in (len(data) // 2, 24):
            outcome = _describe_outcome(codec.document.deserialize, data[:length], rows)
            print(f"  {codec.name:{_NAME_WIDTH}}{length:>8} B  {outcome}")


def _describe_outcome(
    deserialize: Callable[[Buffer], DocumentOutput], data: bytes, rows: int
) -> str:
    try:
        output = deserialize(data)
    # Any exception, since which one each library raises is what's reported
    except Exception as error:  # noqa: BLE001
        message = str(error)[:50].rstrip()
        name = _qualified_name(type(error))
        return f"raised {name}: {message}" if message else f"raised {name}"

    return f"returned {len(output.a)} of {rows} values"


def _qualified_name(error: type[Exception]) -> str:
    """Names an exception by its module, since struct's is called ``error``"""
    if error.__module__ == "builtins":
        return error.__qualname__

    return f"{error.__module__}.{error.__qualname__}"


def _size(data: Buffer) -> int:
    return memoryview(data).nbytes


def _row(name: str, cells: list[str]) -> str:
    return f"  {name:{_NAME_WIDTH}}" + "".join(f"{c:>{_COLUMN_WIDTH}}" for c in cells)


if __name__ == "__main__":
    main()
