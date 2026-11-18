# tafla for Swift

`Tafla` is tafla's runtime for Swift, a Swift package in this directory. It
writes and reads binary tables of scalars, strings, arrays, structs and nested
tables in tafla's format, which the TypeScript, Python, Java, Rust, Go and C++
runtimes write and read too. `TableBuilder` writes a table at its exact size,
copying arrays in bulk, and `ByteBuffer` reads fields back by offset, with
arrays, structs and vector items as views of the bytes. Every read checks that
it stays within the bytes it was given. It depends only on Swift's standard
library, without Foundation.

It needs Swift 6.2 or later, and runs on macOS 12, iOS 15, tvOS 15, watchOS 9
and visionOS 1 or later, and on Linux. It isn't published yet, and
[CHANGELOG.md](CHANGELOG.md) lists the changes that its first release will
bring. Until then, a package can depend on it by its path in a checkout of the
repository:

```swift
dependencies: [
    .package(name: "tafla", path: "../tafla/swift")
],
targets: [
    .target(name: "App", dependencies: [.product(name: "Tafla", package: "tafla")])
]
```

```swift
import Foundation
import Tafla

// A format's header, shared by its writer and its readers
let layout = TableLayout(sized: true, versioned: true)
let formatVersion: UInt32 = 1

var series = try TableBuilder(fields: 2)
try series.setUInt32Array([0, 86_400])
try series.setFloat32Array([21.75, 18.5])

var builder = try TableBuilder(fields: 3, layout: layout, version: formatVersion)
try builder.setFloat64(59.3293)
try builder.setString("Europe/Stockholm")
try builder.setTable(.builder(series))  // written straight into the table
let data = Data(try builder.bytes())

let buffer = ByteBuffer(data.bytes, layout: layout)  // a view of data's bytes
let version = try buffer.version
guard version == formatVersion else {
    throw DecodeError.unsupportedVersion(version: version, supported: formatVersion)
}

let timeZone = try buffer.string(at: buffer.rootOffset(1))  // "Europe/Stockholm"
let table = try buffer.rootOffset(2)
let values = try buffer.float32Array(at: buffer.fieldOffset(1, inTableAt: table))
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
  then its version when it's `versioned`.
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

The runtime writes the same bytes as the other runtimes for the same fields,
and reads their tables: `ConformanceTests.swift` reads and rebuilds the
repository's conformance suite, which covers the whole format.

## Views of the bytes

`ByteBuffer` reads a `RawSpan`, the standard library's view of bytes that
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
  return a `RawSpan` of the bytes, from which a `ByteBuffer` reads a vector
  item.
- **Strings are copied.** `string(at:)` validates the UTF-8 and returns a
  `String` of its own, since a `String` owns its bytes.
- **The bytes come from their owner.** `Data.bytes` and `ArraySlice.span.bytes`
  are a `RawSpan` on every system that the package runs on, and
  `Array.span.bytes` from macOS 26 and iOS 26 on, and on Linux, so
  `let all = bytes[...]` and then `ByteBuffer(all.span.bytes)` reads an
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

`TableBuilder` is a value. A setter of an array, a string or a struct's bytes
keeps the array or string, and `setTable()` and `setVector()` keep the nested
builder or the bytes, until `bytes()` writes the table. Swift shares them
rather than copies them until either side changes, so changing a value, or a
nested builder, after it's set changes neither the builder nor its table. A
table's length is counted when each field is set.

A builder nested with `setTable(.builder(_:))` or `setVector()` is written
straight into its parent, in its own layout, so the outermost table allocates
and writes all the bytes once. `bytes()` allocates an array of the table's
exact length and writes every byte of it once.

## Errors and preconditions

Each API throws its own error, with typed throws, so a caller catches a
`DecodeError` or a `BuildError` without casting, and no error is boxed:

- **Every getter throws a `DecodeError` when the bytes can't be read.** Every
  read checks its range against the bytes the `ByteBuffer` was given, and
  throws `outOfRange` if it would leave them, with the read's position and
  length and the bytes available. Strings must be valid UTF-8, or they throw
  `invalidUTF8`. A vector item that ends before the one before it throws
  `itemEndsBeforeStart`. No bytes make a read trap, overflow, or allocate more
  than they hold.
- `ByteBuffer` doesn't compare versions. A reader throws
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
  builder, throw `incomplete` while fields are left to set. A version that
  doesn't fit, or a header too long for its offset type, fails the
  initializer.

A precondition stops the program only for a mistake of the code that calls the
runtime, which no bytes can cause, as `Array` stops it for an index out of
range: an index past an `ArrayView`'s or a `Vector`'s count, or negative; a
negative field index or struct size; `version` of a layout without one; a
negative number of fields; and a versioned layout without a version, or a
version for a layout without one.

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
swift format lint --strict --recursive Package.swift Sources Tests
```

`swift format --in-place --recursive Package.swift Sources Tests` formats the
code as `.swift-format` configures it. LeakSanitizer, which AddressSanitizer
runs on Linux, reports the conformance suite that the tests keep in a static,
whose arrays Swift refers to from enum payloads, so `detect_leaks=0` turns it
off, as it's off on macOS. The tests are:

- `ConformanceTests.swift`: reads and rebuilds the conformance suite, at every
  byte offset from an aligned address.
- `RoundTripTests.swift`, `FormatTests.swift`, `DecodeErrorTests.swift` and
  `TableBuilderTests.swift`: properties of tables built from the conformance
  suite's descriptions, which `Tables/` parses, generates, builds and reads,
  as Go's `tests/internal/tables` does, and examples of the errors and of every
  boundary: signed zero, NaN, integer limits, empty arrays, the limits of each
  offset type, strings outside the Basic Multilingual Plane and unaligned
  bytes.
- `UTF8Tests.swift`: the validator of strings on Apple's systems before 2024,
  whose standard library can't validate UTF-8, against the standard library's.
- `ExampleTests.swift`: the example above.

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
