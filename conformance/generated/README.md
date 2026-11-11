# Tables of the generated code

Tables and a struct that the code generated from
[`schemas/shapes.tafla`](../../schemas/shapes.tafla) writes, with the values
of their fields. The code generated in every language writes each of them
byte for byte from the values, and reads the values back from the bytes, so
the generators of every language are checked to write one format.

| Language   | Test                                                                               |
| ---------- | ---------------------------------------------------------------------------------- |
| TypeScript | `typescript/core/test/generated/shapesCases.test.ts`, of TypeScript and JavaScript |
| Python     | `python/core/tests/generated/test_shapes_cases.py`                                 |
| Java       | `java/core/src/test/java/io/github/snovader/tafla/test/ShapesCasesTest.java`       |
| Rust       | `rust/gen/tests/shapes_cases.rs`                                                   |
| Go         | `go/tests/generated/shapes_cases_test.go`                                          |

Each language's CI runs its test when this directory changes.

## Files

- `shapes.json`: the description of every case, described below
- `shapes/<name>.bin`: the bytes of each case

The generated TypeScript writes them. The cases are in
`typescript/core/test/generated/shapesCases.ts`; to change a case or add one,
edit it there and regenerate the files from `typescript/`:

```sh
yarn conformance:generate
```

`shapesCases.test.ts` fails when the checked-in files no longer match what
it writes. Run the other languages' tests after regenerating. A change to the
generator or to `schemas/shapes.tafla` that changes these bytes changes what
the generated code of every language writes.

## Descriptions

`shapes.json` holds `{ "schema": "schemas/shapes.tafla", "cases": [...] }`,
and is ASCII, as `cases.json` is:

```json
{
    "name": "point",
    "description": "Signed zero, NaN, the smallest int64 and characters of one to four bytes of UTF-8",
    "type": "Point",
    "file": "shapes/point.bin",
    "fields": { "x": "-0", "y": "NaN", "id": "-9223372036854775808", "name": "a\u00e5\u4e16\ud83e\udd80" }
}
```

- `type` is a message or struct of the schema: `Point`, `Document`,
  `ItemList` or `Sample`.
- `file` is relative to `shapes.json`.
- `fields` holds the value of every field of the type by its name in the
  schema, and of a nested table's fields as an object, a vector's tables as an
  array of objects and an array's elements as an array. Numbers are written
  as in `cases.json`: 64-bit integers as decimal strings, floats that JSON has
  no number for as `"NaN"`, `"Infinity"`, `"-Infinity"` or `"-0"`, and a
  float32 as the float64 that equals it.

## Tests

A language's test loads `shapes.json` and, for each case:

1. Writes the fields with the generated code, with `serialize()` or its
   language's equivalent, and compares the bytes with the file.
2. Reads the file with the generated code and compares every field with the
   description. Floats compare by their bits, or by value with NaN and signed
   zero told apart.
