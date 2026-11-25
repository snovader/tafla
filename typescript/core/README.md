# tafla

Binary tables of scalars, strings and typed arrays. `TableBuilder` writes a
table at its exact size, copying typed arrays in bulk, and `TableReader` reads
fields back by offset, with arrays as views of the bytes where it can. Every
read checks that it stays within the bytes it was given.

```ts
import { TableArena, TableBuilder, TableLayout, TableReader } from 'tafla';

// A format's header, shared by its writer and its readers
const FORMAT_VERSION = 1;
const LAYOUT = TableLayout.create({ sized: true, version: FORMAT_VERSION });

const series = new TableBuilder(TableArena.DEFAULT, 2);
series.setUint32Array(timestamps); // a Uint32Array is copied as bytes
series.setFloat32Array(values); // a number[] would be converted first

const builder = new TableBuilder(TableArena.DEFAULT, 3, LAYOUT);
builder.setFloat64(59.3293);
builder.setString('Europe/Stockholm');
builder.setTable(series); // copied into the table
const bytes = builder.bytes();

const reader = new TableReader(bytes, LAYOUT);
if (reader.getVersion() !== FORMAT_VERSION) {
    throw new Error('Unsupported format version');
}

const latitude = reader.getFloat64(reader.rootOffset(0));
const timeZone = reader.getString(reader.rootOffset(1));
const table = reader.rootOffset(2);
const read = reader.getFloat32Array(reader.fieldOffset(table, 1));
```

`StructWriter`, and the read and write functions, one of each per scalar
type, such as `readUint32()` and `writeFloat64()`, read and write records at
fixed offsets, such as stream frames, without a header or offsets. The write
functions write into a `Uint8Array` at an offset, and throw a `RangeError` for
a value that doesn't lie within it, before converting the value and without
writing any of it.

`TableSizer` and `TableWriter` write a table whose fields are known before
it's written, as the code that tafla-gen generates writes its tables. The
sizer counts the table's length from its fields, and its writer writes the
same fields, in the same order, into bytes of that length. `TableBuilder`
writes each field into a buffer as the field is set, and then copies the
table out of it into bytes of its length, and the two write the same bytes.

A builder writes in the buffer of the `TableArena` that it's given, in bytes
of its own, which it gives back when its table is written. The arena keeps
the buffer between builders: 4 KiB at first, and up to 1 MiB once a table
needed that much. A longer table is written in a buffer of its own, which
isn't kept. `TableArena.DEFAULT` is the arena for a program that doesn't
need one of its own; one that knows its tables sets both lengths:

```ts
// Room for tables of 256 KiB from the first, and tables of up to 8 MiB
// written in a buffer that's kept
const arena = new TableArena({ initialBytes: 256 * 1024, maxRetainedBytes: 8 * 1024 * 1024 });

const builder = new TableBuilder(arena, 3, LAYOUT);
```

`arena.retainedBytes()` is the length of the buffer that the arena holds,
and `arena.release()` drops it, as after a burst of long tables. A builder
that's never written keeps the buffer that it wrote in alive. An arena
belongs to the thread that created it.

The sizer's `writer()` writes into new bytes, and `writer(bytes)` into the
start of `bytes`, such as bytes that a program reuses for each table it
sends, and its writer's `bytes()` is then the view of them that holds the
table. The bytes stay the caller's, and the writer keeps no reference to
them, so the view, and what's read from it, change when they're written
again. The generated `serializeInto()` writes a message so. The table's
arrays are aligned from its start, so bytes that start at an offset of their
`ArrayBuffer` that isn't a multiple of 8 make readers copy the arrays that
are then unaligned, as for any bytes received at such an offset.

`TableWriter` is the target of the generated code, whose calls follow from
the schema, so it doesn't check them again. Its contract:

- Fields are set in index order, each once, as the sizer added them, and
  each vector item is followed by its `setItemEnd()`.
- A field index outside its table, or an item outside its vector, writes an
  offset or item end over another slot or field, so the table's bytes are
  undefined. Every write still stays within the writer's bytes: one whose
  slot lies past them writes nothing.
- Fields that differ from those the sizer counted, in their lengths or
  number, throw a `RangeError` rather than end past the bytes, and `bytes()`
  throws one when they didn't fill them.

Rust's writer also reports an index past its table's fields, since the parent
that the generated code hands back already carries the count. The others don't
keep that count for each nested table, so the contract doesn't promise it.
Hand-written tables use `TableBuilder`, which checks every field.

The generated code lays out the fields whose sizes the schema fixes itself,
at positions that the generator works out, through API that's for it and may
change with it:

- The sizer's `addTableBytes(count, byteLength)` adds a nested table, or a
  vector's items, of fixed length in one call.
- The writer's `reserveTableBytes()`, `reserveTableVector()` and
  `reserveStructArray()` write a field's offset, and a vector's count and
  item ends or an array's count and padding, and return the position in
  `target()`, the writer's bytes, where the caller writes the nested table,
  the vector's items or the structs.
- `TableWriter.tableBytes()` returns the bytes of a message written whole,
  without a sizer and a writer, with its header written, and checks its
  header and length as the sizer and its `writer()` check them.
  `utf8Length()` and `writeUtf8()` count and write a string after its fields
  of fixed size.
- `viewOf()` makes a `DataView` of a range of bytes, through which the
  generated code writes a run of 128 bytes or more of structs or nested
  tables, and writes shorter runs with the write functions, since a
  `DataView` writes each scalar faster but takes longer to create.

## Format

All values are little-endian. A table is its header, one offset per field
and then the fields in the order they were set:

- The header holds the table's byte length when the layout is `sized`, and
  then the layout's `version` when it has one.
- Header slots, offsets, counts, lengths and item ends are all of the
  layout's offset type, `offsetType`: `'uint8'`, `'uint16'` or `'uint32'`,
  whose offset size is 1, 2 or 4 bytes. It limits the table to 255 bytes,
  64 KiB or 4 GiB.
- Offsets count from the table's start. A table nested with `setTable()` has
  no header, and its offsets count from its own start.
- A struct is its scalars one after another, without padding, and
  `setStruct()` writes its bytes in place, like a scalar.
- Arrays and strings start with their element count. Typed arrays are padded
  so that their elements are aligned to their size from the table's start,
  and arrays of structs, which `setStructArray()` writes, to their largest
  scalar's size. `setTable()` aligns a nested table to the largest alignment
  of its arrays when given its builder, and to 4 bytes by default when given
  bytes.
- A vector starts with its item count and the end of each item, counted from
  the vector's start, so `getVectorOffset()` finds any item from two ends,
  without reading the items before it. `getVectorItemOffset()` finds it from
  the count that `getVectorLength()` returned, so that reading many items
  reads and checks the count once, as the generated views do.

The repository's `conformance` directory holds golden tables of every part of
the format, which every implementation of tafla reads and rebuilds byte for
byte; its README describes them. This package writes them, and
`yarn conformance:generate` regenerates them.

## Performance

- **Typed arrays of the field's element type are copied as bytes.** Any
  other array-like, such as a `number[]` or a typed array of another type, is
  converted as the typed array's constructor would convert it. Writing 27,028
  floats took 3.5–5 µs from a `Float32Array` and 14–20 µs from a `number[]`
  in Node and d8. Conversion is a built-in rather than a JS loop, so it
  doesn't slow down when a program passes many kinds of input.
- **Array reads return views when they can.** A view shares the bytes'
  storage, so it's free and reflects later changes to the bytes. An array
  that isn't aligned to its element size in the backing `ArrayBuffer` is
  copied instead, because typed arrays can't start there. Tables that
  `TableBuilder` writes are aligned; bytes received at an odd offset aren't.
  On a big-endian host, arrays of multi-byte elements are always copies, with
  each element's bytes swapped to the host's order.
- **Read arrays whole.** Each scalar getter is a call with a range check:
  summing a 27,028-element Float32 array with `getFloat32()` took about
  60 µs, against about 23 µs through the array's view.
- **Write nested tables in place.** `setTableInPlace(fields, fill)` and
  `setVectorInPlace(count, fields, fill)` write a nested table, or each item
  of a vector, among the builder's own fields, rather than copying it from a
  builder of its own, so they're the way to nest the tables that a program
  builds as it writes their parent. `fill` sets the fields on the
  `TableItem` that it's given, which is the builder itself, counting the
  nested table's fields until `fill` returns. They write the bytes that
  nested builders write, and leave the builder as it was if `fill` throws or
  leaves fields unset. A table built elsewhere, or reused, is still nested
  from its builder or its bytes.

  ```ts
  builder.setVectorInPlace(items.length, 2, (item, index) => {
      item.setFloat32(items[index].value);
      item.setTableInPlace(2, pair => {
          pair.setFloat32(items[index].first);
          pair.setFloat32(items[index].second);
      });
  });
  ```

  In Node 26, the comparison suite's vector of 100 nested tables took
  5.3 µs so, against 13 µs from a builder per item and 24 µs from the bytes
  of each, and allocated 14 KB on V8's heap rather than 42 KB and 90 KB.
- **Nest builders rather than bytes.** `setTable()` and `setVector()` copy
  a `TableBuilder`'s table into its parent from the builder's bytes of the
  buffer, without bytes of its own, or, if it's 128 bytes or longer, keep the
  builder and write its table once, into the parent's bytes. A table nested
  as bytes was copied out of a builder into bytes of its own first. In
  Node 26, the vector of 100 tables that each nest another took 13 µs from
  builders and 24 µs from bytes. In Node 27, the comparison suite's document
  of 100,000 rows took 33 µs with its columns from a builder and 86 µs from
  bytes, against 32 µs written in place.
- **Write records of fixed size as arrays of structs.** An array of 100
  structs of three float32s took 0.78 µs to write and 1,208 bytes in Node 26,
  against 5.4 µs and 3,208 bytes for a vector of 100 tables that each nest a
  table of two of them, written in place, since it's one field of bytes
  rather than 200 tables.
- **Generated code writes tables directly.** `TableSizer` and `TableWriter`
  write each field once, into bytes of the table's length, where
  `TableBuilder` writes it into a buffer and copies the table out of it. In
  Node 27, the generated code wrote a vector of 100 nested tables of scalars
  in 1.9–2.1 µs, against 5.3–5.5 µs from the builder writing them in place,
  and a table of two float64s, an int64 and a string in 0.10 µs rather than
  0.14 µs. `TableWriter` writes scalars as bytes, since V8 keeps a
  `Uint8Array` of up to 64 bytes on its heap until a `DataView` or
  `subarray()` asks for its `ArrayBuffer`, which took 140 ns.
- **Reuse bytes for tables that you write again and again.** New bytes of
  more than 64 are allocated outside V8's heap and zeroed, and the pages of
  large ones are new to the process, which faults on the first write of
  each. In Node 27, the generated code wrote a table nesting 100,000 rows of
  three columns, 1.2 MB, in 31 µs into new bytes and in 19 µs into reused
  ones, with `serializeInto()`, and one of 100 rows, 1.3 KB, in 0.49 µs and
  0.33 µs. A vector of 100 nested tables, 3.2 KB, took the same 0.2 µs
  less, 5% of its write, and a table of 60 bytes, which V8 allocates on its
  heap, no less.
- **Small tables cost about the allocation of their bytes.** A table of two
  float64s, a string and an int64 took 0.13 µs from `bytes()` in Node 27,
  and 0.28 µs from `finalize()`, whose `TableReader` asks for the bytes'
  `ArrayBuffer` for its `DataView`. Call `bytes()` for a table that you send
  or store rather than read. A table written in place needs neither, so the
  vector of 100 nested tables took 5.3–5.5 µs against 7.6–14 µs in
  protobufjs, msgpackr and flatbuffers, and tafla read it 4–7 times as fast
  as they did.

## Conversions

Builders and writers convert each value to its field's type as `DataView`'s
setters and typed arrays convert it:

| Value                                                                   | Written as                                                                                                                          |
| ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| An integer outside its field's range                                    | Wrapped, as `DataView`'s setters wrap it: 300 as a `uint8` is 44, and -1 is 255. A `bigint` outside its 64-bit type wraps the same. |
| A fraction or `NaN` for an integer field                                | Truncated: 1.5 is 1, and `NaN` is 0.                                                                                                |
| A number too large for a `float32`                                      | Infinity of its sign. Other numbers round to the nearest `float32`.                                                                 |
| A lone surrogate in a string                                            | U+FFFD.                                                                                                                             |
| A `number` for an `int64` or `uint64`, or a `bigint` for any other type | Throws a `TypeError` from the setter, or from a builder's `finalize()` for an array of 1 KiB or more, which it writes then.         |

## Validation and errors

Validation is always on:

- Every read checks its range against the bytes the `TableReader` was given,
  even when the backing `ArrayBuffer` is larger, and throws a `DecodeError`
  if it would leave them. Strings must be valid UTF-8. A truncated or
  corrupted buffer therefore throws rather than returning values from outside
  it.
- Once the bytes' `ArrayBuffer` is detached, or a resizable one has shrunk
  below bytes of fixed length, every read throws a `DecodeError`. Bytes made
  over a resizable buffer without a length track its length instead, so once
  the buffer is resized, a read returns what they then hold or throws a
  `DecodeError`.
- `DecodeError` extends `RangeError`. Its message gives the read's position
  and length and the bytes available, and an invalid string's error has the
  `TypeError` from decoding as its `cause`.
- Checks can't tell a value that was corrupted within range from a real one,
  such as an offset that points at another field. A format that needs that
  guarantee can add a checksum.

Using the API wrongly throws other errors, whatever the bytes:

- `TableLayout.create()` throws a `RangeError` for an offset type other than
  `'uint8'`, `'uint16'` and `'uint32'`, or a version that isn't an integer
  that fits in it, and a `TypeError` for an option other than `sized`,
  `version` and `offsetType`.
- `TableBuilder` throws a `RangeError` for more or fewer fields than it was
  created with. A field that would make the table too long for its offset
  type throws a `RangeError` from its setter and isn't added. A field set
  after `finalize()` throws a `TypeError`. A builder keeps arrays, arrays of
  structs, structs and tables given as bytes of 1 KiB or more, and copies
  them into the table when it's written, so they must not change until
  then: `finalize()` and `bytes()` throw a `RangeError`, and leave the
  builder unfinalized, when such an array has changed its length since it
  was set. Shorter values are copied when they're set.
- `TableSizer` throws as `TableBuilder`'s constructor does for its header,
  and its `byteLength()` and `writer()` throw a `RangeError` for a
  table too long for its offset type, and `writer(bytes)` for a table longer
  than `bytes`, before writing any of them. A `TableWriter` throws a `RangeError`
  from `bytes()` when its fields didn't fill the bytes that its sizer
  counted, and from `setString()` for a string longer than the one counted.
  It's the target of generated code, and trusts each field's index and each
  item end to be its own; hand-written tables use `TableBuilder`.
- A struct size that isn't a positive integer, for an array of structs, and
  an alignment other than 1, 2, 4 or 8 throw a `RangeError`.
- `getVersion()` throws a `TypeError` when the layout has no version.
- `getVectorOffset()` throws a `RangeError` for an index outside the vector,
  once its count has been checked against the bytes, and
  `getVectorItemOffset()` for an index outside the count it's given.
