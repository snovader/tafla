# tafla for Swift

`Tafla` is tafla's runtime for Swift, a Swift package in this directory. It
writes and reads binary tables of scalars, strings, arrays, structs, nested
tables and vectors of tables and of strings in tafla's format, which the
TypeScript, Python, Java, Rust, Go and C++ runtimes write and read too.
`TableBuilder` writes a table at its exact size, copying arrays in bulk, and
`TableReader` reads fields back by offset, with arrays, structs and vector
items as views of the bytes. Every read checks that it stays within the bytes
it was given. `TableSizer` and `TableWriter` write the tables of the code
that tafla-gen generates from a schema. It depends only on Swift's standard
library, without Foundation.

It needs Swift 6.2 or later, and runs on macOS 12, iOS 15, tvOS 15, watchOS 9
and visionOS 1 or later, and on Linux. Each release is tagged `<version>`, and
[CHANGELOG.md](CHANGELOG.md) lists the changes that it brings. SwiftPM resolves
the package from the repository, whose root holds a manifest that declares
this directory's library alone, so a package depends on a release by the
repository's URL:

```swift
dependencies: [
    .package(url: "https://github.com/snovader/tafla.git", from: "0.1.0")
],
targets: [
    .target(name: "App", dependencies: [.product(name: "Tafla", package: "tafla")])
]
```

```swift
import Foundation
import Tafla

// A format's header, shared by its writer and its readers
let formatVersion: UInt32 = 1
let layout = TableLayout(sized: true, version: formatVersion)

var series = try TableBuilder(fields: 2)
try series.setUInt32Array([0, 86_400])
try series.setFloat32Array([21.75, 18.5])

var builder = try TableBuilder(fields: 3, layout: layout)
try builder.setFloat64(59.3293)
try builder.setString("Europe/Stockholm")
try builder.setTable(.builder(series))  // written straight into the table
let data = Data(try builder.bytes())

let reader = TableReader(data.bytes, layout: layout)  // a view of data's bytes
let version = try reader.version
guard version == formatVersion else {
    throw DecodeError.unsupportedVersion(version: version, supported: formatVersion)
}

let timeZone = try reader.string(at: reader.rootOffset(1))  // "Europe/Stockholm"
let table = try reader.rootOffset(2)
let values = try reader.float32Array(at: reader.fieldOffset(1, inTableAt: table))
values[1]  // 18.5, read in place
values.span  // a Span<Float> of the bytes, when they're aligned on a little-endian host
let copy = Array(values)  // [21.75, 18.5], which can outlive data
```

`Tests/TaflaTests/ExampleTests.swift` runs this example. Records at fixed
offsets without a header or offsets, such as stream frames, are better read
with `RawSpan.unsafeLoadUnaligned(fromByteOffset:as:)`.

## Format

All values are little-endian. A table is its header, one offset per field and
then the fields in the order they were set:

- The header holds the table's byte length when the layout is `sized`, and
  then the layout's `version` when it has one.
- Header slots, offsets, counts and vector item ends are all of the layout's
  offset type, `.uint8`, `.uint16` or `.uint32`, whose offset size is 1, 2 or
  4 bytes. It limits the table to 255 bytes, 64 KiB or 4 GiB.
  `TableLayout.default` has no header and `uint32` offsets.
- Offsets count from the table's start. A table nested with `setTable()` has no
  header, and its offsets count from its own start.
- A struct is its scalars, one after another without padding, which
  `setStruct()` writes from their bytes and `struct(at:size:)` returns.
- Arrays, strings and arrays of structs start with their count. Arrays are
  padded so that their elements are aligned to their size from the table's
  start, and arrays of structs to the size of their largest scalar.
  `setTable()` aligns a nested table to the largest alignment of its arrays
  when given its builder, and to 4 bytes by default when given bytes.
- A vector starts with its item count, then the end of each item, counted from
  the vector's start, and then the items, so `Vector.item(at:)` finds any item
  from two ends without reading the others. An item without a header and with
  its parent's offset type is read in place, from the bytes that
  `Vector.itemInPlace(at:)` returns, which run on to the end of its parent's.
- A vector of strings is a vector whose items are the strings' UTF-8, one
  after another, without a count or padding of their own, so
  `StringVector`'s subscript finds any string from two ends without reading
  the others. Each string must be valid UTF-8 on its own.

The runtime writes the same bytes as the other runtimes for the same fields,
and reads their tables: `ConformanceTests.swift` reads and rebuilds the
repository's conformance suite, which covers the whole format.

## Views of the bytes

`TableReader` reads a `RawSpan`, the standard library's view of bytes that
something else owns, such as an array or `Data`, and it's nonescapable, as a
`RawSpan` is: its lifetime depends on the bytes', so the compiler keeps it
from outliving them, and from being stored where it could. The views that it
returns depend on the same bytes, so they can't outlive them either:

- **Arrays are read in place.** The array getters return an `ArrayView<T>`,
  which reads elements from the bytes at any alignment and in any host byte
  order, by index. Its `span` returns them as a `Span<T>` when the host is
  little-endian and the elements are aligned to their type in memory, and
  `nil` otherwise; bytes, and empty arrays, are always in place.
  `TableBuilder` aligns arrays from the table's start, so they're aligned in
  memory when the table is. `Array(view)` copies the elements into an array
  of their own, in one copy where the host is little-endian.
- **Structs and vector items are bytes.** `struct(at:size:)`,
  `structArray(at:size:)`, `Vector.item(at:)` and `Vector.itemInPlace(at:)`
  return a `RawSpan` of the bytes, from which a `TableReader` reads a vector
  item.
- **Strings are copied.** `string(at:)` validates the UTF-8 and returns a
  `String` of its own, since a `String` owns its bytes.
  `stringVector(at:)` returns a `StringVector`, a view of a vector of
  strings, whose subscript copies one string, and `Array(_:)` every string
  into an array of their own.
- **The bytes come from their owner.** `Data.bytes` and `ArraySlice.span.bytes`
  are a `RawSpan` on every system that the package runs on, and
  `Array.span.bytes` from macOS 26 and iOS 26 on, and on Linux, so
  `let all = bytes[...]` and then `TableReader(all.span.bytes)` reads an
  array on older systems. Bytes in memory that something else manages are
  read through `RawSpan(_unsafeBytes:)`, within the scope where they stay
  valid.

The views are nonescapable types of the package's own, which Swift 6.2 to 6.4
declare with the experimental feature `Lifetimes` and the attribute
`@_lifetime`. The package enables the feature for its own target only: code
that uses the views needs nothing, and SwiftPM builds the package as a
dependency, as it does Apple's `swift-binary-parsing`, which needs the same
feature. Views are nonescapable, rather than structs of unsafe pointers, so
that the compiler, rather than the caller, keeps them within the bytes'
lifetime, without retaining the bytes as an array would, and without copying
`Data` into storage of the runtime's own. `RawSpan` and `Span` are part of
the standard library from Swift 6.2 on, and back-deployed to every system
above.

## Builders

`TableBuilder` is a value. A setter of an array, a string, a vector of
strings or a struct's bytes keeps the array or string, and `setTable()` and
`setVector()` keep the nested builder or the bytes, until `bytes()` writes
the table. Swift shares them rather than copies them until either side
changes, so changing a value, or a nested builder, after it's set changes
neither the builder nor its table. A table's length is counted when each
field is set.

A builder nested with `setTable(.builder(_:))` or `setVector()` is written
straight into its parent, in its own layout, so the outermost table allocates
and writes all the bytes once. `bytes()` allocates an array of the table's
exact length and writes every byte of it once.

A builder records up to four fields within itself, and a builder nested with
`setTable(.builder(_:))` records its fields among its parent's, so a small
table, nested or not, allocates nothing until it's written. A table of more
fields keeps them in an array, which a parent shares rather than copies.

## Recording nested tables in place

`setTable(fields:_:)` records a nested table straight among its parent's
fields, rather than from a builder of its own that its parent then copies,
and `setVector(count:fields:_:)` records each item of a vector so. The closure
sets the table's fields, in order, on the `TableBuilder.Item` that it's given,
whose setters are the builder's, and can record the tables nested in it in
place too. The bytes are those that nesting builders of the same fields
writes, so the example's table can be written so:

```swift
var builder = try TableBuilder(fields: 3, layout: layout)
try builder.setFloat64(59.3293)
try builder.setString("Europe/Stockholm")
try builder.setTable(fields: 2) { series throws(BuildError) in
    try series.setUInt32Array([0, 86_400])
    try series.setFloat32Array([21.75, 18.5])
}
```

The closure throws a `BuildError`, as the item's setters do, and says so,
since Swift infers that a closure that throws throws `any Error`. A vector's
closure is given each item's index:

```swift
let readings: [(time: UInt32, value: Float)] = [(0, 21.75), (86_400, 18.5)]

var list = try TableBuilder(fields: 1)
try list.setVector(count: readings.count, fields: 2) { index, item throws(BuildError) in
    try item.setUInt32(readings[index].time)
    try item.setFloat32(readings[index].value)
}
```

The item counts the table's own fields and length, as a builder of it would,
so its setters throw as that builder's would. A table recorded in place has no
header and its parent's offset type, and is padded as `setTable(_:alignment:)`
pads a builder, to the largest alignment of its arrays and nested tables. A
vector's items aren't padded. An item can't be copied, so it can't outlive its
closure, and Swift's exclusive access to the builder keeps the closure from
using the builder itself. Whatever the closure throws is passed on, and the
builder is left as it was, as it is when the errors below are thrown.

## Errors and preconditions

Each API throws its own error, with typed throws, so a caller catches a
`DecodeError` or a `BuildError` without casting, and no error is boxed:

- **Every getter throws a `DecodeError` when the bytes can't be read.** Every
  read checks its range against the bytes the `TableReader` was given, and
  throws `outOfRange` if it would leave them, with the read's position and
  length and the bytes available. Strings must be valid UTF-8, or they throw
  `invalidUTF8`, and so must each string of a vector of strings, on its own.
  A vector item, or a string of a vector of strings, that ends before the one
  before it throws `itemEndsBeforeStart`. No bytes make a read trap,
  overflow, or allocate more than they hold.
- `TableReader` doesn't compare versions. A reader throws
  `DecodeError.unsupportedVersion` when `version` isn't one it reads, as the
  example does, so that every decoding failure is a `DecodeError`.
- On a 32-bit host, such as watchOS's arm64_32, an offset or length past the
  largest `Int`, 2 GiB, throws `outOfRange` before it's read, since no bytes
  reach it.
- **`TableBuilder` throws a `BuildError`, whatever the values, when it can't
  write a table, and leaves the builder as it was.** A setter throws
  `tooManyFields` for a field beyond the number the table was created with,
  and `tooLong` for one that would make the table too long for its offset
  type, or for an `Int`, and `setStructArray()` throws `notWholeStructs` for
  bytes that aren't a whole number of structs. `bytes()`, and nesting a
  builder, throw `incomplete` while fields are left to set, as recording a
  table in place does when its closure leaves them unset. A version that
  doesn't fit, or a header too long for its offset type, fails the
  initializer, as a table recorded in place whose offsets alone are too long
  fails its setter. A vector recorded in place whose count and the ends of
  its items alone make its table too long throws `tooLong` before any item
  is recorded.

A precondition stops the program only for a mistake of the code that calls the
runtime, which no bytes can cause, as `Array` stops it for an index out of
range: an index past an `ArrayView`'s, a `Vector`'s or a `StringVector`'s
count, or negative; a negative field index or struct size; `version` of a
layout without one; and a negative number of fields, or of a vector's items
recorded in place.

## Writing without a builder

`TableSizer` and `TableWriter` write a table whose fields are known before
it's written, as the code that tafla-gen generates writes its tables. The
sizer counts the table's length from its fields, each added with its type's
method, which takes what decides its length: nothing for a scalar, the count
of an array, the string itself, or the strings of a vector of strings.
`bytes(_:)` then allocates bytes of that length, and gives its writer to a
closure that sets the same fields, in the same order, each by its index,
without recording them first as `TableBuilder` does:

```swift
var sizer = try TableSizer(fields: 2)
sizer.addFloat64()
sizer.addString("Europe/Stockholm")

let bytes = try sizer.bytes { writer in
    writer.setFloat64(0, 59.3293)
    writer.setString(1, "Europe/Stockholm")
}
```

A nested table's fields follow its `addTable(alignment:)` and
`setTable(_:alignment:)`, between `startTable(fields:)` and `endTable(_:)`,
and so do each vector item's, after the vector's `addVector(count:)` and
`setVector(_:count:)`, followed by the writer's `setItemEnd(_:item:)`. A
struct's scalars follow its `setStruct(_:)`, and an array's structs its
`setStructArray(_:count:alignment:)`, each scalar written with
`writeScalar(_:)`, so that the generated code writes its structs without
collecting their bytes first. The writer writes into the bytes that
`bytes(_:)` returns, rather than into bytes of its own that it copies, so it
lives only while the closure runs, and can't be copied.

`TableWriter` is the target of the generated code, whose calls follow from
the schema, so it doesn't check them again. Its contract, which the other
runtimes share:

- Fields are set in index order, each once, as the sizer added them, and each
  vector item is followed by its `setItemEnd(_:item:)`. A field index past its
  table's fields, or an item past its vector's count, writes an offset or item
  end over another slot or field, so the table's bytes are undefined.
- Every write still stays within the table's bytes: one past them, such as of
  a longer string than the one sized, is a programming error, which stops the
  program, as a negative index, count or size does.
- Fields that fall short of the table make `bytes(_:)` throw
  `BuildError.notAsSized`. The sizer throws the `BuildError` that
  `TableBuilder`'s setter of a field would have, from `byteLength` and
  `bytes(_:)`, for a table too long for its offset type and an array of
  0-byte structs, and its initializer throws as the builder's does.

## Generated code

tafla-gen, tafla's schema compiler, generates Swift from a schema with its
`swift` target, which
[rust/gen/README.md](https://github.com/snovader/tafla/blob/main/rust/gen/README.md)
describes. Its messages and tables are nonescapable views of their bytes,
whose lifetimes it declares with `@_lifetime`, so the target that holds the
generated code enables the experimental feature `Lifetimes`, as this package
enables it for its own targets. Code that only uses the views, such as a
target that imports the generated module, needs nothing:

```swift
.target(
    name: "Schemas",
    dependencies: [.product(name: "Tafla", package: "tafla")],
    swiftSettings: [.enableExperimentalFeature("Lifetimes")]
)
```

A schema's types are declared at the top level of their module, so schemas
that declare the same names generate code for a module each.

## Conversions

Each setter takes its field's own Swift type, such as `UInt8` for a `uint8`,
`Float` for a `float32` and `Double` for a `float64`, so the caller converts a
value to it, as Swift converts:

| Value                                  | Converted                                                                                                |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| An integer outside its field's range   | `UInt8(300)` stops the program, `UInt8(clamping:)` clamps and `UInt8(truncatingIfNeeded:)` keeps the low bits |
| A fraction or NaN for an integer field | `Int32(1.5)` truncates toward zero, and stops the program for NaN and values out of range, which `Int32(exactly:)` returns as `nil` |
| A `Double` too large for a `Float`     | `Float(_:)` makes it infinity of its sign, and rounds other values to the nearest `Float`                |
| Floats                                 | Written as their bits, so NaNs keep their payloads and signs, and -0 its sign                            |
| A string                               | Written as its UTF-8, which a `String` always holds valid                                                |

## Byte orders and word sizes

On a little-endian host, arrays are read in place and copied in bulk. On a
big-endian one, every array is read and written element by element, byte
swapped, and `ArrayView.span` is `nil` for elements of more than one byte. The
compilation condition `TAFLA_PORTABLE` takes the big-endian path on any host,
which `swift test -Xswiftc -DTAFLA_PORTABLE` tests. No big-endian host runs
Swift in CI. On a 32-bit host, offsets and lengths that an `Int` can't hold
throw `outOfRange`, as above; no 32-bit host runs the tests either.

## Development

The package's tests depend on
[PropertyBased](https://github.com/x-sheep/swift-property-based), which runs
the property tests in Swift Testing, and Foundation, which reads the
conformance suite; the library depends on neither. Run the commands below from
this directory:

```sh
swift build
swift test
swift test -c release -Xswiftc -enable-testing
swift test --scratch-path .build/portable -Xswiftc -DTAFLA_PORTABLE
ASAN_OPTIONS=detect_leaks=0 swift test --scratch-path .build/asan --sanitize=address
swift build --scratch-path .build/watchos --target Tafla --triple arm64_32-apple-watchos9.0 --sdk "$(xcrun --sdk watchos --show-sdk-path)"
swift format lint --strict --recursive Package.swift Sources Tests Fuzz/Package.swift Fuzz/Sources
```

`swift format --in-place --recursive Package.swift Sources Tests Fuzz/Package.swift Fuzz/Sources`
formats the code as `.swift-format` configures it. LeakSanitizer, which
AddressSanitizer runs on Linux, reports the conformance suite that the tests
keep in a static, whose arrays Swift refers to from enum payloads, so
`detect_leaks=0` turns it off, as it's off on macOS. On macOS, `swift build`
for watchOS's `arm64_32` compiles the paths for hosts whose `Int` is 32 bits,
without running them.
The tests are:

- `ConformanceTests.swift`: reads and rebuilds the conformance suite, at every
  byte offset from an aligned address.
- `RoundTripTests.swift`, `FormatTests.swift`, `DecodeErrorTests.swift` and
  `TableBuilderTests.swift`: properties of tables built from the conformance
  suite's descriptions, which `Tables/` parses, generates, builds and reads,
  as Go's `tests/internal/tables` does, and examples of the errors and of every
  boundary: signed zero, NaN, integer limits, empty arrays, the limits of each
  offset type, strings outside the Basic Multilingual Plane and unaligned
  bytes.
- `TableWriterTests.swift`: `TableSizer` and `TableWriter` write the tables
  that `TableBuilder` writes from the same descriptions, whose nested tables
  `Tables/Write.swift` writes in place, and fail as it fails, with examples of
  the writer's own errors and of what stops the program.
- `StringVectorTests.swift`: reading every string of a vector of strings
  reads what reading each from its ends reads, and fails where the first
  string that fails does, in overwritten and arbitrary bytes, with examples
  of each decode error, of an end that splits a character and of a string
  that starts within the ends.
- `UTF8Tests.swift`: the decoders of strings, as a `UTF8Span` where the
  standard library has one, and through a validator of its own on Apple's
  systems before 2024, whose standard library can't validate UTF-8, against
  the standard library's `String(validating:as:)`.
- `ExampleTests.swift`: the example above.
- `Generated/`: the code that tafla-gen generates from the example schemas in
  `schemas/` at the root of the repository, which lies in `Tests/Generated`,
  a module of each that `Package.swift` declares. It writes the tables that
  `TableBuilder` writes from the same fields, or fails as it fails, reads back
  what it writes at any byte offset, rejects other versions, and reads
  corrupted and arbitrary bytes as values or decode errors, through its
  getters and views. `ShapesCasesTests.swift` writes the tables of
  `conformance/generated` byte for byte, and reads their values back, as the
  code generated in every language does. The modules are the generator's
  golden files: rewrite them with the generator rather than by hand, with
  `TAFLA_GEN_UPDATE=1 cargo test -p tafla-gen` from `rust/`.

The property tests run 256 cases each, as proptest does in Rust's. A failing
table shrinks by losing fields and vector items, and the failure gives the
seed that `.fixedSeed()` on the test reproduces. Tables are generated by a
seeded generator of the test target's own, `Draw`, since a table's description
is recursive, so other values than tables don't shrink.

CI runs the checks on macOS, and on Linux with Swift 6.2 and the latest
release. With Docker, the Linux checks run from the repository's root, where
the tests find the conformance suite:

```sh
docker run --rm -v "$PWD":/tafla -w /tafla/swift swift:6.2-noble swift test --scratch-path /tmp/build
```

### Fuzzing

`Fuzz/` holds libFuzzer's targets, which mirror Rust's and Go's, in a
package of their own, so that the runtime's package neither builds nor vends
them. A trap, a report of AddressSanitizer's, or a read that disagrees with
`SPEC.md` is a finding:

- `FuzzTableReader` reads arbitrary bytes with every getter of `TableReader`,
  at every position and past the end, in every offset type and header, and
  reads every array, struct array, vector and vector of strings there. Each
  read must return the bytes that lie where the format puts them, exactly
  when they lie within the input, and the decode error otherwise.
- `FuzzConformance` mutates the conformance suite's tables and reads them as
  every valid case's description, as the conformance test reads them. Its
  `Cases.swift`, `Description.swift` and `Read.swift` are links to the test
  target's in `Tests/TaflaTests/Tables/`.

They build only with Linux's toolchain, since Xcode's has no libFuzzer, so
on macOS they run in Docker, from the repository's root. Swift's backtracer
is off, since it held a crash for minutes rather than let libFuzzer report
it, and so is LeakSanitizer, as in the tests:

```sh
docker run --rm -it -v "$PWD":/tafla -w /tafla/swift/Fuzz \
    -e ASAN_OPTIONS=detect_leaks=0 -e SWIFT_BACKTRACE=enable=no swift:6.4-noble bash
swift build -c release --sanitize=fuzzer --sanitize=address
mkdir -p .build/corpus/FuzzTableReader .build/artifacts
.build/release/FuzzTableReader .build/corpus/FuzzTableReader ../../conformance/valid \
    ../../conformance/equivalent ../../conformance/invalid -max_total_time=300 -artifact_prefix=.build/artifacts/
```

Stop it with Ctrl-C or `-max_total_time`. libFuzzer writes an input that
fails to `.build/artifacts/`, and the target reads the files that it's given
instead of fuzzing, which reproduces a finding:

```sh
.build/release/FuzzTableReader .build/artifacts/crash-<sha1>
```

A finding becomes a regression test in `DecodeErrorTests.swift`. CI runs
each target for a minute on every change, and the weekly Fuzz workflow for
five minutes, from the corpus that its earlier runs grew.

### Benchmarks

`benchmarks/` compares the runtime with FlatBuffers, swift-protobuf and a
MessagePack library on the workloads that every language's comparison runs,
in a package of its own, so that the runtime's package depends on none of
them. [benchmarks/README.md](benchmarks/README.md) describes the method and
the results.
