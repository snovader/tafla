# tafla

Binary tables of scalars, strings, arrays and structs, in tafla's format.
`TableBuilder` writes a table at its exact size, copying arrays in bulk,
and `ByteBuffer` reads fields back by offset, borrowing arrays, strings and
structs from the bytes. Every read checks that it stays within the bytes it was
given.

```rust
use tafla::{ByteBuffer, DecodeError, TableBuilder, TableLayout};

// A format's header, shared by its writer and its readers
const LAYOUT: TableLayout<true> = TableLayout::DEFAULT.sized().versioned();
const FORMAT_VERSION: u32 = 1;

let timestamps = vec![0_u32, 86_400];
let values = vec![21.75_f32, 18.5];

let mut series = TableBuilder::new(2, TableLayout::DEFAULT)?;
series.set_u32_array(&timestamps)?; // borrowed until the table is written
series.set_f32_array(values.clone())?; // or owned

let mut builder = TableBuilder::versioned(3, LAYOUT, FORMAT_VERSION)?;
builder.set_f64(59.3293)?;
builder.set_str("Europe/Stockholm")?;
builder.set_table(series)?; // written straight into the table
let bytes = builder.to_bytes()?;

let buffer = ByteBuffer::new(&bytes, LAYOUT);
let version = buffer.version()?;
if version != FORMAT_VERSION {
    let supported = FORMAT_VERSION;
    return Err(DecodeError::UnsupportedVersion { version, supported }.into());
}

let latitude = buffer.get_f64(buffer.root_offset(0)?)?;
let time_zone = buffer.get_str(buffer.root_offset(1)?)?;
let table = buffer.root_offset(2)?;
let read = buffer.get_f32_array(buffer.field_offset(table, 1)?)?;

assert_eq!((latitude, time_zone), (59.3293, "Europe/Stockholm"));
assert_eq!(read.to_vec(), values);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Records at fixed offsets without a header or offsets, such as stream frames,
are better written and read with the standard library's `to_le_bytes()` and
`from_le_bytes()`.

`TableSizer` and `TableWriter` write a table whose fields are known before
it's written, as the code that tafla-gen generates writes its tables. The
sizer counts the table's length from its fields, and its writer writes the
same fields, in the same order, into bytes of that length, without recording
them first as `TableBuilder` does. The builder then writes its fields through
a `TableWriter` too, so the two write the same bytes, and the sizer's
`writer()` returns `TableBuilder`'s errors.

`TableWriter` is the target of the generated code, whose calls follow from
the schema, so it doesn't check them again. Its contract:

- Fields are set in index order, each once, as the sizer added them, and
  each vector item is followed by its `set_item_end()`.
- A field index outside its table, or an item outside its vector, writes an
  offset or item end over another slot or field, so the table's bytes are
  undefined. Every write still stays within the writer's bytes: one whose
  slot lies past them writes nothing, and `into_bytes()` returns an error.
- Fields that differ from those the sizer counted, in their lengths or
  number, write nothing past the bytes, and `into_bytes()` returns
  `BuildError::NotAsSized`.

Rust's writer also reports an index past its table's fields, as
`BuildError::TooManyFields`, and an item past its vector's count, as
`BuildError::NotAsSized`, since the `WriterParent` and `VectorPosition` that
the generated code hands back already carry the counts: it cost nothing
measurable. The others don't keep a count for each nested table, so the
contract doesn't promise it. Hand-written tables use `TableBuilder`, which
checks every field.

The generated code writes a table of fields whose sizes the schema fixes
whole, at positions that it works out: a nested one from its bytes, which
the sizer's `add_table_bytes()` adds and the writer's `copy_table()` copies,
and a message of such fields, and a string after them, into the bytes that
`TableWriter::table_bytes()` or `versioned_table_bytes()` returns with its
header, without a sizer and a writer. Like Go's `AppendTable()`, these are
for the generated code, outside the compatibility promise of the rest of
the API, and may change with the generator.

## Format

All values are little-endian. A table is its header, one offset per field
and then the fields in the order they were set:

- The header holds the table's byte length when the layout is `sized`, and
  then its version when it's `versioned`.
- Header slots, offsets, counts and vector item ends are all of the
  layout's offset type, `OffsetType::Uint8`, `Uint16` or `Uint32`, whose
  offset size is 1, 2 or 4 bytes. It limits the table to 255 bytes, 64 KiB or
  4 GiB.
- Offsets count from the table's start. A table nested with `set_table()` has
  no header, and its offsets count from its own start.
- A struct is its scalars, one after another without padding, which
  `set_struct()` writes from their bytes and `get_struct()` returns.
- Arrays, strings and arrays of structs start with their count. Arrays are
  padded so that their elements are aligned to their size from the table's
  start, and arrays of structs to the size of their largest scalar.
  `set_table()` aligns a nested table to the largest alignment of its arrays
  when given its builder, and to 4 bytes by default when given bytes.
- A vector starts with its item count, then the end of each item, counted
  from the vector's start, and then the items, so `Vector::get()` finds any
  item from two ends without reading the others. An item without a header
  and with its parent's offset type is read in place, from the bytes that
  `Vector::get_in_place()` returns, which run on to the end of its parent's,
  since its offsets may lead past its end. Any other item is read from its
  own bytes, which `Vector::get()` returns.

The crate writes the same bytes as the TypeScript runtime, `tafla` on
npm, for the same fields, and reads its tables; `tests/compatibility.rs` checks
tables that it wrote, and `tests/conformance.rs` reads and rebuilds the
repository's conformance suite, which covers the whole format.

## Borrowing and copying

- **Builders borrow or own their values.** Array setters take anything that
  converts into a `Cow` of the element type: a slice or `&Vec` is borrowed
  until the table is written, and a `Vec` is owned. `set_str()` takes a
  `&str` or a `String` the same way. The borrow checker keeps borrowed values
  from changing before the table is written.
- **Nest builders rather than bytes.** `set_table()` and `set_vector()` take
  a `TableBuilder`, owned or borrowed, and write it straight into its parent,
  so the outermost table allocates and writes all the bytes once. A table
  nested as bytes was written into an allocation of its own and is copied,
  and so is a builder with a header or another offset type than its
  parent's.
- **Record nested tables in place.** `set_table_with()` and
  `set_vector_with()` record a nested table, or each item of a vector of
  them, straight among the builder's own fields, rather than from a builder
  of their own: a closure sets the fields on the `InPlaceTable` that it's
  given, whose setters are the builder's. The builder is borrowed meanwhile,
  so only the `InPlaceTable` sets fields. They write the bytes that nested
  builders write, and leave the builder as it was if the closure returns an
  error, leaves fields unset or panics. A table nested from a builder or
  bytes, with `set_table()` and `set_vector()`, is still what a table built
  elsewhere, or reused, needs.

  ```rust
  # use tafla::{BuildError, TableBuilder, TableLayout};
  let items = [(1.0_f32, [2.0_f32, 3.0]), (4.0, [5.0, 6.0])];

  // Items of a float32 and a nested table of two
  let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
  builder.set_vector_with(items.len(), 2, |index, item| {
      let (value, pair) = items[index];
      item.set_f32(value)?;
      item.set_table_with(2, |nested| {
          nested.set_f32(pair[0])?;
          nested.set_f32(pair[1])
      })
  })?;
  let bytes = builder.to_bytes()?;
  # Ok::<(), BuildError>(())
  ```
- **Arrays are read in place.** `ByteBuffer`'s array getters return an
  `ArrayView`, which reads elements from the bytes at any alignment and in
  any host byte order. `as_slice()` returns them as a slice when the host is
  little-endian and the elements are aligned to their type in memory, and
  `to_cow()` falls back to a copy otherwise. `TableBuilder` aligns arrays
  from the table's start, so they're aligned in memory when the table is, as
  a `Vec<u8>` from the system allocator typically is but isn't guaranteed to
  be. Byte arrays, strings, structs and vector items are always slices.

## Performance

`benchmarks/` compares the runtime with the flatbuffers, prost, rmp-serde
and rkyv crates on the workloads that every language's benchmarks run, as
its README describes. Its fastest samples, on an Apple M2 Pro:

- **Arrays are written in bulk.** A document of 100,000 rows of three
  columns, 1.2 MB, took 18 µs to write, about as long as rkyv, which copies
  arrays whole too, and 6 to 43 times faster than flatbuffers, prost and
  rmp-serde, which write them element by element. The builder writes the
  table at its exact size, appending to reserved capacity rather than zeroing
  it first.
- **Arrays are read in place.** Reading the document, with its columns as
  slices from `ArrayView::to_cow()`, took 34 ns at 100 rows and at 100,000
  alike. rkyv took 15 µs to copy the large document's columns out of its
  archive, and the others 0.2 to 1.2 ms.
- **Read arrays whole.** An `ArrayView`'s iterator reads each element
  without a check of its own: summing the document's 100,000 floats through
  it took 86 µs, bound by the additions, as long as through flatbuffers' and
  rkyv's accessors. `get_f32()` at each position would check every read and
  return early from the loop on an error.
- **Small tables are quick to write and read.** A table of two `f64`s, an
  `i64` and a 16-byte string took 79 ns to write, including allocating the
  builder's record of fields and the table, and 13 ns to read.
- **Each nested builder allocates.** A vector of 100 tables that each nest
  another took 9.0 µs to write, 2.5 to 6.4 times as long as flatbuffers,
  rmp-serde and prost, which took 1.4 µs: each of its 200 tables is a
  builder, which allocates its record of fields. It took 0.30 µs to read.
  Recorded in place with `set_vector_with()` and `set_table_with()`, it took
  4.4 µs, in 4 allocations rather than 208, against 8.8 µs from a builder
  per item and 3.5 µs through flatbuffers in the same runs.

## Conversions

Each setter takes its field's own Rust type, such as `u8` for a `uint8` and
`f32` for a `float32`, so the caller converts a value to it, with `as` or
`TryFrom`, and the runtime writes it as it is:

| Value                                    | Written as                                                                                                          |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| An integer outside its field's range     | Not expressible. `as` keeps the low bits (`300_i32 as u8` is 44), and `u8::try_from()` returns an error.            |
| A fraction or NaN for an integer field   | Not expressible. `as` truncates and saturates (`1.5_f64 as u8` is 1, and `f64::NAN as u8` is 0).                    |
| An `f64` too large for an `f32`          | `as f32` makes it infinity of its sign, and rounds other values to the nearest `f32`, which the setter writes.      |
| A lone surrogate in a string             | Not expressible: a `&str` is always UTF-8.                                                                          |

## Validation and errors

Validation is always on:

- Every read checks its range against the bytes the `ByteBuffer` was given
  and returns a `DecodeError` if it would leave them. Strings must be valid
  UTF-8. A truncated or corrupted buffer therefore returns an error rather
  than values from outside it, and no bytes make a read panic.
- `DecodeError::OutOfRange` gives the read's position and length and the
  bytes available, and `DecodeError::InvalidUtf8` has the `Utf8Error` as its
  source.
- `get_vector()` checks the vector's count and ends, and each item is checked
  when `Vector::get()` or the iterator reads it, which returns a `Result`. An
  item that ends before the one before it is
  `DecodeError::ItemEndsBeforeStart`.
- `ByteBuffer` doesn't compare versions. A reader returns
  `DecodeError::UnsupportedVersion` when `version()` isn't one it reads, as
  the example does, so that every decoding failure is a `DecodeError`.
- Checks can't tell a value that was corrupted within range from a real one,
  such as an offset that points at another field. A format that needs that
  guarantee can add a checksum.

`TableBuilder` returns a `BuildError`, whatever the values' bytes, rather
than panicking, since WebAssembly builds abort on panic:

- A setter returns an error for a field beyond the number the table was
  created with, or one that would make the table too long for its offset
  type, and leaves the builder as it was, as `set_struct_array()`
  does for bytes that aren't a whole number of structs. `to_bytes()`, and
  nesting a builder, return an error while fields are left to set. A version
  that doesn't fit, or a header too long for its offset type, is an error
  from the constructor.
- The compiler requires a version exactly for a versioned layout, and offers
  `ByteBuffer::version()` only for one, through `TableLayout`'s `VERSIONED`
  parameter.
