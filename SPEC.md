# tafla format

This document describes tafla's binary format. Every implementation writes
and reads it, and the conformance suite in `conformance/` checks them against
it byte for byte. Until the first release, the format may still change.

## Overview

A tafla table holds a fixed number of fields, read by their index. The bytes
don't describe themselves: the reader knows each field's type from the format
it reads, such as code generated from a schema. A table has no magic number,
schema identifier or checksum; a format that needs one can add it as a field
or around the table.

All multi-byte values are little-endian. Integers are two's complement and
floats are IEEE 754 binary32 and binary64.

## Layout

A table's layout is fixed by its format, and the reader must know it:

- **Offset type**: `uint8`, `uint16` or `uint32`, the unsigned integer type
  of the table's header slots, field offsets, counts, lengths and item ends.
  Its width in bytes, 1, 2 or 4, is the table's **offset size**. A table
  can't be longer than the largest value of its offset type: 255 bytes,
  65,535 bytes or 4,294,967,295 bytes.
- **Sized**: whether the header holds the table's length.
- **Versioned**: whether the header holds a version.

The default layout has the offset type `uint32` and no header slots.

## Table

A table is its header, then one offset per field, then the fields' values:

```text
[length]? [version]? offset[0] … offset[n-1] value … value
```

- **Length**, present when the layout is sized, is the byte length of the
  table.
- **Version**, present when the layout is versioned, is an unsigned integer
  chosen by the format, so that readers can reject versions they don't read.
- **Offset `i`** is the position of field `i`'s value, counted from the start
  of the table. The offsets of a table of `n` fields start right after the
  header, so field `i`'s offset is at `headerSize + i × offsetSize`, where
  `offsetSize` is the offset size and the header is 0, 1 or 2 slots of the
  offset type.

Writers write the values in field order, each after the previous one, with
only the padding described below between them. Padding bytes are zero, and
readers ignore them.

## Values

| Type                                    | Bytes at the field's offset                                    |
| --------------------------------------- | -------------------------------------------------------------- |
| `int8` … `uint64`, `float32`, `float64` | The value, in 1, 2, 4 or 8 bytes                               |
| `bool`                                  | One byte, 0 for false, written as 0 or 1 and true if not 0     |
| Array of an integer or float type       | A count `n`, then `n` elements                                 |
| String                                  | A count `n`, then `n` bytes of UTF-8                           |
| Struct                                  | Its scalars, one after another                                 |
| Array of structs                        | A count `n`, then `n` structs                                  |
| Nested table                            | The table's bytes, with no length prefix                       |
| Vector of tables                        | A count `n`, then the end of each item, then the `n` items     |

Counts, lengths and item ends are of the offset type, as offsets are.

- **Scalars**, the integers, floats and `bool`, aren't aligned. Writers
  write a `bool` as 0 or 1, and readers accept any byte, as true when it
  isn't 0, so that corrupted bytes still read as values and a `bool` needs
  no check. A `bool` has no arrays.
- **Structs** are a fixed sequence of scalars, which the format defines, as
  it defines a table's fields; a struct within a struct adds its scalars in
  its place. A struct has no header, offsets or padding, so its scalars lie
  at fixed offsets from its start. Its size is the sum of its scalars' sizes,
  and its alignment the largest of them. A struct field isn't aligned, like a
  scalar.
- **Arrays** are aligned: writers pad before an array's count so that its
  first element starts at a multiple of its alignment from the start of the
  table, which is the element's size for scalars, and the struct's alignment
  for structs. Readers must accept unaligned arrays too, since tables can
  arrive at any position in memory.
- **Strings** must be valid UTF-8, without a terminator. Readers reject
  invalid UTF-8.
- **Nested tables** are padded to their alignment. A nested builder is
  aligned to the largest alignment of its arrays and of its own nested
  tables, or 1 if it has neither; a nested table given as bytes is aligned to
  4 unless the writer passes another alignment. A nested table has no length
  prefix, so its own offsets lead to its fields.
- **Vectors** hold tables of any layout. Each item's end is its offset from
  the vector's position, which is that of its count. Item `i` starts where
  item `i - 1` ends, and item 0 right after the last end, so readers find any
  item from at most two ends, without reading the items before it. Items
  aren't padded, so their arrays may be unaligned.

### Reading nested tables

A nested table that has no header and the offset type of its parent, a
table field or a vector item, is read in place: its offsets count from its own
start, and it reads within the bytes that its parent reads within. A vector
item read in place isn't bounded by its end, so an offset of its own can lead
past it. Any other nested table is read as a table of its own, with its own
layout and within its own bytes: a vector item from its start to its end, and
a table field from its offset to the end of its parent, since its length isn't
written.

Every implementation reads nested tables this way, so that all of them read
the same values from the same bytes, or fail alike. A vector item whose end
leaves it only its offsets, for example, reads its fields from the bytes after
it, which its parent reads within.

## Evolution

A table carries no count of its fields: the reader knows `n` from its
format, and a reader that expects more fields than a table holds would read
the extra offsets from the first value's bytes. So a format fixes its tables'
fields, and a format whose fields change, such as that of messages that are
stored or sent, gives each set of fields a version of its own in the header:

- A versioned table's reader rejects every version but its own with its
  decode error, even a newer version that only appends fields. An
  application that reads several versions reads the version first, and then
  reads the table with the reader of that version.
- Every version should keep the same layout, so that the version lies in the
  same slot, with the same width, in each of them, and an application can
  read it before it knows which version a table holds.
- A new version should only append fields to the previous version's, so that
  each field keeps its index and type in every version that has it. A reader
  that doesn't check the version then reads the fields that it knows from a
  newer table's bytes alike.

## Validation

Readers decode untrusted bytes. An implementation:

- Checks every read against the bytes it was given, including offsets,
  counts, lengths and item ends, and fails with its decode error when a read
  would leave them, rather than reading other memory or allocating for a
  count that the bytes can't hold. A vector item must start no later than it
  ends.
- Rejects strings that aren't valid UTF-8 with its decode error.
- Reports every failure to decode as its decode error, and doesn't crash,
  panic or hang on any input.

Checks can't tell a value corrupted within range from a real one, such as an
offset that points at another field, or one of a vector item's offsets that
points past the item's end, at bytes that the item reads in place.

Nor do readers check that a vector item lies after the vector's count and
ends. Item `i` is found from the ends of items `i - 1` and `i` alone, so when
the end of item `i - 1` points into the count and ends, item `i` starts there
and reads them as its bytes. An item before it fails to read, since the ends
can't lead back from item 0's start without one of them ending before it
starts. Every implementation reads such an item alike, within the bytes it
was given, so checking it would cost every item read without making any read
safer.

Such an item can hold itself. When item `i` starts at the count, its offset
`i` is the end of item `i - 1`, which is 0, so it leads back to the item's
start. Read in place, an item whose field `i` is a vector of its own type,
such as a tree's vector of trees, finds that vector there, and itself as its
item `i`, without end. A reader reads each value when it's asked for, so it
never loops on such bytes, and one that reads every item of the vector fails
at an item before it. A caller that walks nested tables recursively, reading
an item without the items before it, bounds how deep it walks.

## Writers

Writers produce the same bytes for the same fields in every implementation:
header, offsets, values in field order, the padding above and nothing else. A
writer rejects a table longer than its offset type addresses, and a version
that doesn't fit in it, rather than writing a table that readers would misread.
It converts a value to its field's type as its language converts it, which
each runtime documents, and rejects what it can't write.

## Conformance

`conformance/cases.json` describes valid tables of every part of the format
and corrupted ones, and `conformance/README.md` describes how implementations
check themselves against them. A change to the bytes of an existing case is a
change to this format.
