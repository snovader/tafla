// Package tafla writes and reads binary tables of scalars, strings, arrays,
// structs and nested tables, in tafla's format, which the TypeScript, Python,
// Java and Rust runtimes write and read too.
//
// [TableBuilder] writes a table at its exact size, copying arrays in bulk,
// and [ByteBuffer] reads fields back by offset, sharing arrays, strings and
// structs with the bytes. Every read checks that it stays within the bytes
// it was given, and fails with a [*DecodeError] otherwise, so tables can come
// from untrusted sources.
//
// [TableSizer] and [TableWriter] write a table whose fields are known before
// it's written, as the code that tafla-gen generates writes its tables: the
// sizer counts the table's length from its fields, and its writer writes the
// same fields, in the same order, into bytes of that length, without
// recording them first as TableBuilder does. Fields whose sizes a schema
// fixes, the generated code lays out itself: the sizer and the writer take
// them together, and [AppendTable] appends a table that its caller lays out
// entirely.
//
// # Format
//
// All values are little-endian. A table is its header, one offset per field
// and then the fields in the order they were set:
//
//   - The header holds the table's byte length when the layout is sized, and
//     then its version when it's versioned.
//   - Header slots, offsets, counts and vector item ends are all of the
//     layout's offset type, [Uint8], [Uint16] or [Uint32], whose offset size
//     is 1, 2 or 4 bytes. It limits the table to 255 bytes, 64 KiB or 4 GiB.
//   - Offsets count from the table's start. A table nested with
//     [TableBuilder.SetTable] has no header, and its offsets count from its
//     own start.
//   - A struct is its scalars, one after another without padding.
//   - Arrays, strings and arrays of structs start with their count. Arrays
//     are padded so that their elements are aligned to their size from the
//     table's start, and arrays of structs to the size of their largest
//     scalar.
//   - A vector starts with its item count, then the end of each item, counted
//     from the vector's start, and then the items, so [Vector.Get] finds any
//     item from two ends without reading the others.
//
// SPEC.md, at the root of the repository, describes the format in full.
//
// # Sharing and copying
//
// Builders keep their values: a setter of an array, a string or a struct's
// bytes keeps the slice or string, and nesting a builder keeps the builder,
// until the table is written, so their elements must not change in between.
// Nesting a builder rather than bytes writes it straight into its parent,
// so that the outermost table allocates and writes all the bytes once.
// [TableBuilder.SetTableFunc] and [TableBuilder.SetVectorFunc] record a
// nested table, or each item of a vector, among the builder's own fields,
// without a builder of its own.
//
// Readers share the bytes: [ArrayView] reads an array's elements in place,
// at any alignment and in any host byte order, and [ArrayView.AsSlice]
// returns them as a slice of the bytes when the host is little-endian and
// they're aligned to their type in memory, as TableBuilder aligns them
// within an aligned table. A slice that Go allocates for a table usually
// is, though Go doesn't promise it, and a subslice or a slice from
// elsewhere may not be. A string
// shares the bytes too, which Go's strings otherwise don't, so the bytes
// must not change while it's in use. Byte arrays, structs and vector items
// are slices of the bytes.
//
// # For generated code
//
// [AppendTable], [AppendVersionedTable], [TableSizer.AddFields],
// [TableSizer.AddTableBytes], [TableWriter.ReserveFields] and
// [TableWriter.ReserveTableBytes] are for the code that tafla-gen generates,
// which lays out the fields whose sizes the schema fixes itself, and
// [Uint32FieldOffset] for its getters, which know each field's slot. They're
// exported because generated code lies in its user's package, but they're
// outside the compatibility promise of the rest of the API, and may change in
// any release, with the generator.
//
// # Errors and panics
//
// Every getter returns its value and an error, which is a *DecodeError when
// the bytes can't be read, whatever they hold: no bytes make a read panic.
// Builders return a *[BuildError] for a table that they can't write, such as
// one too long for its offset type, and leave the builder as it was.
//
// Functions panic only for mistakes of the code that calls them, which no
// bytes can cause: an index past an [ArrayView]'s or a [Vector]'s length, as
// indexing a slice does, an offset type or alignment that isn't one of the
// constants, a negative number of fields or of a vector's items, a negative
// field index or struct size, the version of a layout without one, or a nil
// nested table. A [TableWriter] given other fields than its
// sizer counted panics for a write past the table's bytes, rather than
// writing past them.
package tafla
