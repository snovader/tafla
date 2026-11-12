# tafla conformance suite

Golden tables written by TypeScript tafla, the reference implementation. Every
implementation of tafla reads each table back, compares its values with the
table's description in `cases.json`, and rebuilds it with its own
`TableBuilder`, byte for byte. Corrupted and truncated tables must fail with a
decode error, and tables that differ from a valid one only in bytes that
readers accept, such as a bool's, must read its values. That makes "one
format, several implementations" a checked claim.

| Implementation | Test                                                                         |
| -------------- | ---------------------------------------------------------------------------- |
| TypeScript     | `typescript/core/test/conformance/conformance.test.ts`                       |
| Python         | `python/core/tests/test_conformance.py`                                      |
| Java           | `java/core/src/test/java/io/github/snovader/tafla/test/ConformanceTest.java` |
| Rust           | `rust/core/tests/conformance.rs`                                             |
| Go             | `go/conformance_test.go`                                                     |
| C++            | `cpp/tests/conformance_test.cpp`                                             |

Each language's CI runs its test when this directory changes.

## Files

- `cases.json`: the description of every case, described below
- `valid/<name>.bin`: the table of each valid case
- `equivalent/<name>.bin`: the changed table of each equivalent case
- `invalid/<name>.bin`: the corrupted table of each invalid case
- `generated/`: the tables that the code generated from the example schema
  `schemas/shapes.tafla` writes in every language, which
  [`generated/README.md`](generated/README.md) describes

The TypeScript runtime's `test/conformance` writes them:

- `cases.ts`: the cases
- `tables.ts`: builds tables from descriptions and reads them back
- `files.ts` and `generate.ts`: write `cases.json` and the tables

To change a case or add one, edit `cases.ts` and regenerate the files from
`typescript/`:

```sh
yarn conformance:generate
```

`conformance.test.ts` fails when the checked-in files no longer match what the
generator writes. Run the other implementations' tests after regenerating.
A change to the bytes of an existing case is a change to the format, which
every implementation has to follow.

## Descriptions

`cases.json` holds `{ "valid": [...], "equivalent": [...], "invalid": [...] }`,
and is ASCII: every
other character in a string is a `\u` escape, with characters outside the
Basic Multilingual Plane as surrogate pairs, which JSON parsers combine.

### Valid cases

```json
{
    "name": "vector-item-layouts",
    "description": "A vector of a table read in place and then tables of other layouts, ...",
    "file": "valid/vector-item-layouts.bin",
    "table": {
        "layout": { "sized": false, "versioned": false, "offsetType": "uint32" },
        "fields": [
            {
                "type": "vector",
                "items": [
                    {
                        "from": "builder",
                        "table": {
                            "layout": { "sized": false, "versioned": false, "offsetType": "uint32" },
                            "fields": [{ "type": "uint8", "value": 1 }]
                        }
                    },
                    ...
                ]
            }
        ]
    }
}
```

`file` is relative to `cases.json`. A table has:

- `layout`: `sized`, `versioned` and `offsetType`, the offset type: `uint8`,
  `uint16` or `uint32`, as `SPEC.md` defines it. A `TableLayout` takes these.
  Every layout is written out in full.
- `version`: the version in the header, present only when `versioned`.
- `fields`: in the order that they're set, which is the order of their
  offsets in the header.

Each field has a `type`:

| `type`                                                | Holds                                                                  | Set with                       |
| ----------------------------------------------------- | ---------------------------------------------------------------------- | ------------------------------ |
| `int8`, `uint8`, `int16`, `uint16`, `int32`, `uint32` | `value`: a number                                                      | `setInt8()`, `setUint8()`, ... |
| `int64`, `uint64`                                     | `value`: a decimal string                                              | `setInt64()`, `setUint64()`    |
| `float32`, `float64`                                  | `value`: a number, or `"NaN"`, `"Infinity"`, `"-Infinity"` or `"-0"`   | `setFloat32()`, `setFloat64()` |
| `bool`                                                | `value`: `true` or `false`                                             | `setUint8()`, with 1 or 0      |
| `int8[]`, `uint8[]`, ..., `float64[]`                 | `value`: an array of values as the scalar type holds them              | `setInt8Array()`, ...          |
| `string`                                              | `value`: a string                                                      | `setString()`                  |
| `struct`                                              | `fields`: `{ "type", "value" }` for each scalar, as a scalar field     | `setStruct()`                  |
| `struct[]`                                            | `fields`: the scalars' types, and `items`: each struct's values        | `setStructArray()`             |
| `table`                                               | `from` and `table`, a nested table, and `alignment` when one is passed | `setTable()`                   |
| `vector`                                              | `items`: `{ "from", "table" }` for each item                           | `setVector()`                  |

- A bool is the uint8 1 or 0, since the runtimes have no getters or setters
  of bools; the code that tafla-gen generates writes and reads them so. Its
  value is true when the byte read isn't 0. It's a field only, and never a
  struct's scalar or an array's element.
- 64-bit integers are strings, since JSON numbers lose precision past 2^53 in
  JavaScript and in other parsers.
- Floats that JSON has no number for are named. Their bytes are what
  JavaScript writes: NaN is the quiet NaN, `0x7fc00000` as a float32 and
  `0x7ff8000000000000` as a float64.
- A float32 is written as the float64 that equals it, such as
  `0.10000000149011612`, so that it parses and narrows to a float32 exactly.
- A struct's scalars lie one after another, in the order of `fields`, so a
  struct within a struct is described by its scalars. `setStruct()` takes the
  struct's bytes, and `setStructArray()` the bytes of every struct, the size
  of one, which is the sum of its scalars' sizes, and its alignment, which is
  the largest of them. `getStruct()` and `getStructArray()` return the bytes
  that hold the structs, whose scalars the test reads at their offsets.
- `from` is `"builder"` for a nested table passed to `setTable()` or
  `setVector()` as its `TableBuilder`, and `"bytes"` for the bytes that the
  builder writes. They're written alike, except that `setTable()` aligns a
  builder to the largest element size of its arrays and nested tables by
  default, and bytes to 4.

Reading a field starts from its offset, from `rootOffset(index)` for a field of
the root and `fieldOffset(position, index)` for a field of a nested table read
in place. A nested table that has no header and its parent's offset type is
read in place, within its parent's bytes, as `SPEC.md` describes: a vector
item from its position, from `getVectorOffsets()`. Any other is read as a
table of its own, with a buffer of its own layout: a vector item from its
bytes, from `getVector()`, and a table field from its position to the end of
its parent, since its length isn't written. The suite only nests tables
without a header with `setTable()`.

A table read from its own bytes, the root or a vector item, has its header
checked too: its version must equal `version`, and when it's sized, its size
must equal its byte length. A vector must hold as many items as `items`.

### Equivalent cases

```json
{
    "name": "bool-nonzero",
    "description": "The bytes of the last two bools of bools are 2 and 0x80, which read as true, like 1",
    "file": "equivalent/bool-nonzero.bin",
    "case": "bools",
    "corruption": { "kind": "overwrite", "at": 18, "bytes": [2, 128] }
}
```

Bytes that differ from a valid case's but read the same values, since readers
accept more than writers write. `case` names the valid case, whose table
description reads the bytes as a valid table's, its header included, and
`corruption` records how they differ, as an invalid case's does. They aren't
rebuilt, since a writer writes the valid case's bytes.

### Invalid cases

```json
{
    "name": "invalid-utf8-continuation",
    "description": "A string starts with a continuation byte, 0x80",
    "file": "invalid/invalid-utf8-continuation.bin",
    "case": "strings",
    "corruption": { "kind": "overwrite", "at": 49, "bytes": [128] },
    "field": 2
}
```

`case` names the valid case whose table description reads the bytes. Each
root field before `field` must read the values that the description gives, and
reading `field`, its offset included, must fail with the implementation's
decode error: `DecodeError` in TypeScript, Python, Rust and C++,
`DecodeException` in Java, and a `*DecodeError` in Go. Its header isn't
checked. `corruption` records how the bytes differ from the valid case's: cut
to `length` bytes with `"kind": "truncate"`, or with `bytes` written at `at`
with `"kind": "overwrite"`. Readers read the file rather than apply it.

## Tests

An implementation's test loads `cases.json` and:

1. Reads every valid table as described above, and compares what it reads with
   the description. Comparing the description with its values replaced by the
   ones read, as the TypeScript and Python tests do, covers every value. The
   TypeScript test compares floats by value, with NaN and negative zero by
   their names; the Python, Java, Rust, Go and C++ tests compare their bits.
2. Builds every valid table from its description and compares its bytes with
   the file.
3. Reads every equivalent table as its valid case describes it, and compares
   what it reads with the description.
4. Reads every invalid table up to `field`, and checks that `field` fails with
   a decode error.

## Coverage

| What                                                       | Cases                                                                                               |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| Every element type as a scalar                             | `integer-scalars`, `float-scalars`                                                                  |
| Bools, and bytes other than 0 and 1 that read as true      | `bools`, `bool-nonzero`                                                                             |
| Every element type as an array, and empty                  | `integer-arrays`, `float-arrays`, `empty-arrays`                                                    |
| Offset types, without and with each header slot            | `offset-type-{uint8,uint16,uint32}`, with `-sized`, `-versioned` and `-sized-versioned`             |
| Values of 128 and more in `uint8` offsets and lengths      | `offset-type-uint8-full`                                                                            |
| The padding that aligns arrays                             | `padding`, `empty-arrays`, `integer-arrays` and the offset type cases                               |
| Nested tables from builders and bytes, and alignments      | `nested-tables`, `nested-two-deep`                                                                  |
| Nested builders of other offset types than their parent's  | `nested-narrower-offsets`, `nested-wider-offsets`                                                   |
| Struct fields, unpadded, of every scalar type              | `structs` and the offset type cases                                                                 |
| Arrays of structs of each alignment, empty too             | `struct-arrays` and the offset type cases                                                           |
| Vectors of tables, empty, and of other layouts             | `vectors`, `vector-item-layouts` and the offset type cases                                          |
| Empty, multi-byte UTF-8 and leading U+FEFF strings         | `strings`                                                                                           |
| Tables of no fields                                        | `empty-table`, `empty-table-sized-versioned`                                                        |
| Truncated tables                                           | `truncated-*`, `struct-past-end`                                                                    |
| Offsets, lengths, counts and item ends past the end        | `offset-past-end`, `array-length-*`, `string-length-past-end`, `struct-array-count-*`, `vector-*`, `nested-offset-past-end` |
| A vector item that ends before it starts                   | `vector-item-ends-before-start`                                                                     |
| Invalid UTF-8                                              | `invalid-utf8-continuation`, `invalid-utf8-overlong`, `invalid-utf8-surrogate`                      |

It doesn't cover:

- Offsets and lengths of 2^15 and more in tables of the offset type `uint16`,
  or of 2^31 and more in tables of `uint32`, which would take files of 32 KiB
  and 2 GiB.
- Strings with lone surrogates, which TypeScript and Java write as U+FFFD,
  Python rejects and Rust strings can't hold, and NaNs other than the quiet
  NaN.
- `StructWriter` and the write functions, which write records without a
  table.
- Bools in structs, which are their uint8 to the runtimes. The tests of the
  code that tafla-gen generates from `schemas/all_types.tafla` cover them.

## Implementation notes

The tests follow each implementation's API where it differs:

- Java's `TableBuilder.setVector()` takes builders or bytes, but not both, so
  the Java test sets a vector that mixes them from every item's bytes. A
  builder writes the same bytes into a vector as on its own.
- Rust's `Vector` returns its items' bytes rather than their positions. The
  Rust test reads an item in place from the bytes that `iter_in_place()`
  returns, which start where the item does and end where its parent's do,
  with a buffer of its parent's offset type, and any other item from the
  bytes that iterating returns.
- Each test also finds every vector item from its ends alone, with
  `getVectorOffset()` or Rust's `Vector::get()` and `Vector::get_in_place()`,
  and compares it with the item that reading the whole vector finds.
- Go's `Vector` returns its items' bytes too, as Rust's does, from
  `ItemsInPlace()` and `GetInPlace()`, and the Go test reads them alike. Go's
  `math.NaN()` isn't the quiet NaN of the cases, `0x7ff8000000000000`, but
  has a payload bit set, so the Go test sets `"NaN"` by its bits.
- JSON parsers can round a float's last digit: the Rust test enables
  `serde_json`'s `float_roundtrip`, and Java's `JsonNumber.doubleValue()`
  converts from a `BigDecimal`, which is exact, as Go's `strconv.ParseFloat`
  is. nlohmann/json, which the C++ test parses with, converts with
  `strtod()`, which is exact too.
- C++'s `Vector` returns its items' bytes, as Rust's does, and its
  `items_in_place()` the bytes from each item to the end of its parent's, which
  the C++ test reads an item in place from, as the Rust test does.
