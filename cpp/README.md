# tafla for C++

A header-only runtime of C++20 in `include/tafla/`, which depends only on the
standard library, and its tests, fuzz targets and benchmarks. `TableBuilder`
writes a table at its exact size, copying arrays in bulk, and `ByteBuffer`
reads fields back by offset, with strings and aligned arrays as views of the
bytes. Every read checks that it stays within the bytes it was given, and
throws a `tafla::DecodeError` otherwise.

```cpp
#include <tafla/tafla.hpp>

// A format's header, shared by its writer and its readers
constexpr auto layout = tafla::TableLayout().sized().versioned();
constexpr std::uint32_t format_version = 1;

tafla::TableBuilder series(2, tafla::TableLayout());
series.set_uint32_array(timestamps);  // a std::span, which the builder views
series.set_float32_array(std::move(values));  // a std::vector, which it owns

tafla::TableBuilder builder(3, layout, format_version);
builder.set_float64(59.3293);
builder.set_string("Europe/Stockholm");
builder.set_table(std::move(series));  // written straight into the table
const tafla::Bytes bytes = builder.to_bytes();

const tafla::ByteBuffer buffer(bytes, layout);
if (buffer.version() != format_version) {
    throw tafla::DecodeError::unsupported_version(buffer.version(), format_version);
}

const double latitude = buffer.get_float64(buffer.root_offset(0));
const std::string_view time_zone = buffer.get_string(buffer.root_offset(1));
const auto table = buffer.root_offset(2);
const tafla::ArrayView<float> read = buffer.get_float32_array(buffer.field_offset(table, 1));
```

The names are the other runtimes' in C++'s case: table, field, layout, offset
type, offset size, vector, builder and decode error.

| Header                 | Holds                                                                         |
| ---------------------- | ----------------------------------------------------------------------------- |
| `tafla/tafla.hpp`      | Everything below                                                              |
| `layout.hpp`           | `OffsetType`, `Alignment` and `TableLayout<Versioned>`                        |
| `errors.hpp`           | `Error`, and `DecodeError` and `BuildError`, which derive from it             |
| `byte_buffer.hpp`      | `ByteBuffer<Versioned>`, which reads a table                                  |
| `array_view.hpp`       | `ArrayView<T>`, a random-access range of an array's elements                  |
| `vector.hpp`           | `Vector`, a vector of tables, and its ranges of items                         |
| `table_builder.hpp`    | `TableBuilder` and `NestedTable`, which write a table field by field          |
| `table_writer.hpp`     | `TableSizer` and `TableWriter`, which the generated code writes tables with   |
| `bytes.hpp`            | `Bytes`, the bytes of a written table                                         |
| `structs.hpp`          | The `Struct` concept and `StructArray<T>`, a view of an array of structs      |
| `table_vector.hpp`     | `TableVector<T>`, a view of a vector of the tables that `T` reads             |
| `element.hpp`          | The `Element` concept, and `load_le()` and `store_le()` of records' scalars   |

## Design

- **Errors are exceptions.** A read throws a `DecodeError` and a setter or
  `to_bytes()` a `BuildError`, which leave a builder as it was. A read that
  succeeds costs nothing for them, and its code is a load after one or two
  compares, where a `std::expected` would carry its error out of every
  getter. A read that fails takes about 4.5 µs on macOS, almost all of it
  unwinding, so arbitrary bytes read as decode errors at about 200,000 a
  second a core. `kind()` and the accessors of each error describe what
  failed, as the other runtimes' errors do.
- **`TableLayout<true>` is a versioned layout**, as Rust's `TableLayout<true>`
  is, so that a versioned table's builder requires a version and
  `ByteBuffer::version()` compiles only where the header has one. A layout is
  a `constexpr` value, and a `ByteBuffer` made with a constant layout reads
  its offsets without branching on their type.
- **Views borrow, and owners own.** A getter returns a `std::string_view`, an
  `ArrayView`, a `std::span` of bytes or a `Vector` of the bytes it was given,
  which must outlive them. A setter that takes a `std::span`, a
  `std::string_view` or a `const TableBuilder&` views it until `to_bytes()`,
  and one that takes a `std::vector`, a `std::string`, `Bytes` or a builder by
  rvalue owns it.
- **`Bytes` aren't zeroed.** A writer allocates a table's bytes at its exact
  length, uninitialized, and writes each byte once, which a `std::vector`
  would zero first. `Bytes` convert to a `std::span`, and `to_vector()` copies
  them.
- **`ArrayView::as_span()`** returns a `std::span<const T>` of the elements
  where they're aligned on a little-endian host, as `TableBuilder` aligns
  them within an aligned table, reading the table's bytes as `T`, as
  FlatBuffers reads its vectors. Its `operator[]`, iterators and `to_vector()`
  read any array, at any alignment, in any byte order.
- **Big-endian hosts** read and write each element with a byte swap. Defining
  `TAFLA_FORCE_BYTE_SWAP`, or configuring with `-DTAFLA_FORCE_BYTE_SWAP=ON`,
  makes a little-endian host take that path, with the same values, so that
  the tests cover it anywhere. CI also runs the tests on s390x under QEMU.
- **`TableWriter` checks its calls**, as Rust's does: it never writes past the
  table's length, whatever it's given, and `finish()` throws the `BuildError`
  of fields other than those its `TableSizer` counted, of an index past a
  table's fields, or of an item past its vector's count.
- **Reads and writes inline.** `ByteBuffer`'s, `Vector`'s, `TableSizer`'s and
  `TableWriter`'s functions are `TAFLA_ALWAYS_INLINE`, and their failures
  are values or static functions, so that no call takes a reader's or
  writer's address: a function that did kept the writer of a generated
  `serialize()` in memory rather than in registers, and read its offset type
  from memory rather than folding the constant layout's. The generated code
  inlines the writers of the tables nested in a message, but for tables that
  hold themselves through vectors, whose writers call themselves.

Structs are the generated code's, or any type that the `Struct` concept
accepts: a `SIZE`, an `ALIGNMENT`, `from_bytes()` and `to_bytes()`. The
writer's `set_struct()` and `set_structs()` write them, and `StructArray`
reads an array of them. `load_le()` and `store_le()` read and write a record's
scalars at fixed offsets, without bounds, at any alignment.

## Development

CMake 3.25 or later builds the tests with any C++20 compiler; CI runs GCC and
Clang on Linux and Apple Clang on macOS. Run the commands below from this
directory. The tests' dependencies, GoogleTest, RapidCheck and nlohmann/json,
are fetched at their pinned versions when CMake configures the build.

```sh
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DTAFLA_WARNINGS_AS_ERRORS=ON
cmake --build build
ctest --test-dir build --output-on-failure
```

clang-format 23.1.5, with the style in `.clang-format`, formats the sources,
tafla-gen's golden files among them; `pipx run clang-format==23.1.5` runs
that version:

```sh
git ls-files -z '*.hpp' '*.cpp' | xargs -0 clang-format --dry-run --Werror
```

The options of the build:

| Option                           | Does                                                                     |
| -------------------------------- | ------------------------------------------------------------------------ |
| `-DTAFLA_WARNINGS_AS_ERRORS=ON`  | Compiles tafla's own targets with `-Werror`, as CI does                  |
| `-DTAFLA_SANITIZERS=<list>`      | Builds everything with sanitizers, such as `address,undefined`           |
| `-DTAFLA_FORCE_BYTE_SWAP=ON`     | Reads and writes arrays as a big-endian host does                        |
| `-DTAFLA_BUILD_FUZZERS=ON`       | Builds the libFuzzer targets in `fuzz/`                                  |
| `-DTAFLA_BUILD_BENCHMARKS=ON`    | Builds the comparison in `benchmarks/`, as its README describes          |

CI builds the tests with AddressSanitizer and UndefinedBehaviorSanitizer,
with the byte-swapping path, and on s390x, a big-endian host, under QEMU:

```sh
cmake -S . -B build-sanitized -G Ninja -DCMAKE_BUILD_TYPE=Debug -DTAFLA_SANITIZERS=address,undefined
cmake -S . -B build-swapped -G Ninja -DTAFLA_FORCE_BYTE_SWAP=ON
```

### Tests

The tests in `tests/` are GoogleTest's, with RapidCheck's properties, which
run 256 cases each under `ctest`. `RC_PARAMS="max_success=10000"` runs more,
and a failure prints the `RC_PARAMS` that reproduces it.

- `conformance_test`: the conformance suite in `conformance/`: valid tables
  read back their values and rebuild byte for byte, also through
  `TableSizer` and `TableWriter`, equivalent tables read their valid case's
  values, and invalid tables fail with a `DecodeError` at their field.
- `round_trip_test`, `format_test`, `decode_error_test`, `table_builder_test`
  and `table_writer_test`: properties of tables generated from the
  conformance suite's descriptions, by `tests/common/strategy.cpp`, as Rust's
  tests generate them: they read back their values at any address, follow
  the format's rules, read as values or decode errors cut short, overwritten
  or arbitrary, and fail with a `BuildError` where they can't be written, with
  examples of every error.
- `byte_buffer_test`, `array_view_test`, `utf8_test`, `layout_test` and
  `compatibility_test`: every getter at every position of arbitrary bytes,
  views at every alignment, UTF-8 at its limits, layouts, and tables that the
  TypeScript runtime wrote.
- `tests/generated`: the C++ that tafla-gen generates from the example schemas
  in `schemas/`, in a directory of each schema, and the tests that run it: it
  writes the tables of `conformance/generated` byte for byte, writes every
  table as `TableBuilder` does, reads back what it writes through its getters
  and its views, rejects other versions, and reads corrupted bytes as values
  or decode errors. They're the generator's golden files, which
  `rust/gen/README.md` describes: rewrite them with the generator rather than
  by hand.

### Fuzzing

`fuzz/` holds libFuzzer's targets, which mirror `rust/fuzz`'s. They need a
Clang with libFuzzer, such as Homebrew's `llvm` or a Linux distribution's,
since Apple's Clang has none. A crash, a sanitizer's report or a failed check
is a finding:

- `fuzz_byte_buffer` reads the rest of its input with every getter of
  `ByteBuffer` at a position, in an offset type and header, which the input's
  first four bytes choose, and reads every array, struct array and vector
  there. Each read must return the bytes that lie where the format puts them,
  exactly when they lie within the input, and the decode error otherwise.
  Rust's target reads at every position of every input instead, which ran 6
  inputs a second in C++, where most reads of arbitrary bytes throw.
- `fuzz_conformance` reads its input as every valid case of the conformance
  suite describes it, as the conformance test reads them.

```sh
cmake -S . -B build-fuzz -G Ninja -DCMAKE_BUILD_TYPE=RelWithDebInfo \
    -DCMAKE_CXX_COMPILER=/opt/homebrew/opt/llvm/bin/clang++ \
    -DTAFLA_BUILD_FUZZERS=ON -DTAFLA_SANITIZERS=address,undefined
cmake --build build-fuzz --target fuzz_byte_buffer fuzz_conformance
mkdir -p build-fuzz/corpus/byte_buffer
build-fuzz/fuzz/fuzz_byte_buffer build-fuzz/corpus/byte_buffer \
    ../conformance/valid ../conformance/equivalent ../conformance/invalid -max_total_time=300
```

The C++ workflow runs each target for a minute, and the fuzz workflow for
five minutes every week, uploading what fails. A finding becomes a regression
test in `tests/`.

### Benchmarks

`benchmarks/` compares tafla with FlatBuffers, protobuf and MessagePack, on
the workloads of the other languages' comparisons, as its
[README](benchmarks/README.md) describes, with results.

## Releases

The runtime isn't packaged or published yet; `cmake --install` installs its
headers. A release would add a CMake package configuration, and a port to
vcpkg or Conan.
