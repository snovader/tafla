# tafla for C++

A header-only runtime of C++20 in `include/tafla/`, which depends only on the
standard library, and its tests, fuzz targets and benchmarks. `TableBuilder`
writes a table at its exact size, copying arrays in bulk, and `TableReader`
reads fields back by offset, with strings and aligned arrays as views of the
bytes. Every read checks that it stays within the bytes it was given, and
throws a `tafla::DecodeError` otherwise.

```cpp
#include <tafla/tafla.hpp>

// A format's header, shared by its writer and its readers
constexpr std::uint32_t format_version = 1;
constexpr auto layout = tafla::TableLayout().sized().with_version(format_version);

tafla::TableBuilder series(2, tafla::TableLayout());
series.set_uint32_array(timestamps);  // a std::span, which the builder views
series.set_float32_array(std::move(values));  // a std::vector, which it owns

tafla::TableBuilder builder(3, layout);
builder.set_float64(59.3293);
builder.set_string("Europe/Stockholm");
builder.set_table(std::move(series));  // written straight into the table
const tafla::Bytes bytes = builder.to_bytes();

const tafla::TableReader reader(bytes, layout);
if (reader.version() != format_version) {
    throw tafla::DecodeError::unsupported_version(reader.version(), format_version);
}

const double latitude = reader.get_float64(reader.root_offset(0));
const std::string_view time_zone = reader.get_string(reader.root_offset(1));
const auto table = reader.root_offset(2);
const tafla::ArrayView<float> read = reader.get_float32_array(reader.field_offset(table, 1));
```

The names are the other runtimes' in C++'s case: table, field, layout, offset
type, offset size, vector, builder and decode error.

| Header                 | Holds                                                                         |
| ---------------------- | ----------------------------------------------------------------------------- |
| `tafla/tafla.hpp`      | Everything below                                                              |
| `layout.hpp`           | `OffsetType`, `Alignment` and `TableLayout<Versioned>`                        |
| `errors.hpp`           | `Error`, and `DecodeError` and `BuildError`, which derive from it             |
| `table_reader.hpp`      | `TableReader<Versioned>`, which reads a table                                  |
| `array_view.hpp`       | `ArrayView<T>`, a random-access range of an array's elements                  |
| `bools.hpp`            | `Bools`, the bools that `TableReader::get_bool_array()` reads                 |
| `vector.hpp`           | `Vector`, a vector of tables, and its ranges of items                         |
| `table_builder.hpp`    | `TableBuilder` and `NestedTable`, which write a table field by field          |
| `table_writer.hpp`     | `TableSizer` and `TableWriter`, which the generated code writes tables with   |
| `table_length.hpp`     | The arithmetic that `TableBuilder` and `TableSizer` count a table's length in |
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
  is, so that the layout's `version()` and `TableReader::version()` compile
  only where the header has one, and a layout without a version takes no
  bytes for one. A layout is a `constexpr` value, and a `TableReader` made
  with a constant layout reads its offsets without branching on their type.
- **Views borrow, and owners own.** A getter returns a `std::string_view`, an
  `ArrayView`, a `std::span` of bytes or a `Vector` of the bytes it was given,
  which must outlive them. A setter that takes a `std::span` or a
  `std::string_view` views it until `to_bytes()`, and one that takes a
  `std::vector`, a `std::string`, `Bytes` or a builder by rvalue owns it. A
  temporary that a setter can't own, such as a `std::array` returned by
  value, doesn't compile, rather than leaving the builder a view of it. A
  builder nested by `const` reference is recorded when it's set: the parent
  shares what it owns, or writes it at once, so that it may then change or be
  destroyed, and only what it borrows must outlive the parent's
  `to_bytes()`.
- **Nested tables can be recorded in place.** `set_table(fields, fill)` and
  `set_vector(count, fields, fill)` record a nested table, or each item of a
  vector, straight among the builder's fields, through the
  `TableBuilder::Item` that `fill` sets the fields on, rather than from a
  builder of its own. They write the bytes that nested builders write, and
  leave the builder as it was if `fill` throws or leaves fields unset. The
  benchmarks' vector of 100 nested tables took about 4.0 µs so, against
  7.5 µs from a builder per item and 3.0 µs through FlatBuffers. A table nested
  from a builder or bytes, with `set_table(table)` and `set_vector(items)`,
  is still what a table built elsewhere, or reused, needs.
- **`Bytes` aren't zeroed.** A writer allocates a table's bytes at its exact
  length, uninitialized, and writes each byte once, which a `std::vector`
  would zero first. `Bytes` convert to a `std::span`, and `to_vector()` copies
  them.
- **Writing into reused bytes.** `TableSizer::writer(bytes)` and a generated
  message's `serialize_into(bytes)` write the table into the start of a
  `std::span<std::uint8_t>` that the caller owns, rather than into new
  `Bytes`, and return its length, as Java's `serializeInto()` does. They
  write every byte of the table, padding included, so reused bytes needn't be
  zeroed, and throw `std::invalid_argument` for bytes shorter than the table
  before writing any of them. A writer's `finish()` and `finish_into()` each
  throw `std::logic_error` for a writer of the other kind of bytes. In the
  comparison suite's workloads, with the fields built as its tafla-gen codec
  builds them, writing into reused bytes took 5.9 ns for the record against
  23.5 ns into new bytes, 28 ns for the document of 100 rows against 45 ns,
  and 61 ns for the array of 100 structs against 79 ns. It saves less where
  the bytes cost little beside the rest: 5% for the vector of 100 nested
  tables, whose codec allocates the items' fields, and 2% for the document of
  1.2 MB, which copying its columns takes most of. A `serialize()` compiles
  to the same code as before.
- **Arrays of bools are bytes, read into `Bools`.** An array of bools is a
  `uint8` array of 1 for true and 0 for false, which a reader reads as true
  wherever a byte isn't 0. A byte other than 0 or 1 isn't a `bool`, so no
  reader views the bytes as bools: `ArrayView<bool>` doesn't compile,
  `get_uint8_array()` and a generated getter return the bytes in place, and
  `TableReader::get_bool_array()` and `Bools::from_bytes()` read them into
  `Bools`, a `bool` each, one after another, which own them. They convert to
  the `std::span<const bool>` that `TableWriter::set_bool_array()` and
  `TableBuilder::set_bool_array()` take, and the builder owns `Bools` given
  as an rvalue, as it owns a `std::vector`. A `std::vector<bool>` packs its
  bools into bits instead, which no span views, and filling one from 1,000
  bytes took 2.0 µs, where `Bools` took 38 ns. The writers copy a span of
  bools as its bytes, since a `bool` is the byte 0 or 1, as a `static_assert`
  in `table_writer.hpp` checks: a loop that wrote 1 or 0 for each took 19 ns
  for 1,000 bools with GCC 16, where the copy took 12 ns, and 4.9 ns for 100
  with Apple Clang 21, where it took 2.3 ns.
- **`ArrayView::as_span()`** returns a `std::span<const T>` of the elements
  where they're aligned on a little-endian host, as `TableBuilder` aligns
  them within an aligned table. It deliberately reads the table's bytes as
  `T`, as FlatBuffers reads its vectors, relying on C++20's implicit creation
  of objects in the bytes' storage. Its `operator[]`, iterators and
  `to_vector()` read any array, at any alignment, in any byte order, with
  `std::memcpy`.
- **Big-endian hosts** read and write each element with a byte swap. Defining
  `TAFLA_FORCE_BYTE_SWAP`, or configuring with `-DTAFLA_FORCE_BYTE_SWAP=ON`,
  makes a little-endian host take that path, with the same values, so that
  the tests cover it anywhere. CI also runs the tests on s390x under QEMU.
  Every translation unit of a program must define it alike, or none: the
  inline functions that test it would differ otherwise, against the
  one-definition rule.
- **32-bit x86 may quiet signalling NaNs.** Its ABI returns floats and
  doubles on the x87 stack, whose loads quiet a signalling NaN, so a float
  that a getter or `ArrayView::operator[]` returns there may come back
  quieted, depending on the compiler's code. The bytes themselves, and
  `as_span()`'s views of them, keep every bit, and the tests accept either.
- **`TableWriter` checks its calls**, as Rust's does: it never writes past the
  table's length, whatever it's given, and `finish()` throws the `BuildError`
  of fields other than those its `TableSizer` counted, of an index past a
  table's fields, or of an item past its vector's count. It's
  `BasicTableWriter<true>`. `TableBuilder`, whose setters check each field
  as they record it, writes through a `BasicTableWriter<false>`, which skips
  those checks of room, indexes and items, and whose `finish()` still checks
  the length that it wrote.
- **Reads and writes inline.** `TableReader`'s, `Vector`'s, `TableSizer`'s and
  `TableWriter`'s functions are `TAFLA_ALWAYS_INLINE`, and their failures
  are values or static functions, so that no call takes a reader's or
  writer's address: a function that did kept the writer of a generated
  `serialize()` in memory rather than in registers, and read its offset type
  from memory rather than folding the constant layout's. So are
  `TableVector`'s `get()` and iterators, so that a generated getter's view
  reads an item with the offset type of the constant layout that read the
  vector: Apple Clang called `get()` out of line otherwise, which branched
  on the offset type, and reading the last of the comparison's 100 nested
  tables took 4.35 ns rather than 3.23 ns. So are the generated getters:
  Apple Clang called the record's and the document's `name()` out of line
  otherwise, which kept their reader in memory, and reading the record took
  6.65 ns rather than 6.25 ns. A generated getter of a nested table reads it
  through `TableReader::get_table()`, which throws if the table's offset leads
  past the bytes. Returning a reader of no bytes instead put a choice on the
  load of each of the nested table's fields, and summing the comparison's
  100 nested tables took 0.258 µs rather than 0.229 µs. The generated code
  inlines the writers of the tables nested in a message while they're short,
  at most 64 fields with those they inline in turn, so that its code grows
  linearly with the schema rather than doubling with each level of tables
  nested in pairs, and never those of tables that hold themselves through
  vectors, whose writers call themselves.
  `TableBuilder`'s setters inline their check of the field and its record,
  with their errors thrown from cold functions, which made the builder
  write the record 12% faster and the vector of 100 nested tables 7% faster
  than calls did. Its `to_bytes()` walks the tables nested in place with a
  stack of its own, rather than a function that calls itself, so that its
  writer stays in registers too, and a writer zeroes up to 16 bytes, such as
  a nested table's offsets, with stores rather than a call to `memset()`.
  Together they made `to_bytes()` of the vector of 100 nested tables 28%
  faster, 1.45 µs against 2.02 µs, and building and writing it 13% faster.
  Writing through a `BasicTableWriter<false>` then made `to_bytes()` of that
  vector a further 13% faster, 1.27 µs against 1.45 µs, with half the
  machine code.
- **Strings are checked for ASCII in words OR-ed together, and then from
  their first byte that isn't.** A read checks that a string is UTF-8, and
  most strings are ASCII, so `valid_utf8()` ORs a string's 8-byte words
  together from the first byte and tests their high bits once: its first
  word and its last, which overlap, for 8 to 16 bytes, its first two and its
  last two for 17 to 32, and blocks of four words for a longer string, the
  last of which overlaps the one before it. A string shorter than 8 bytes is
  checked by two halves of a word, which overlap, or by its first, middle
  and last bytes. Reading the comparison's record, whose name is 16 bytes of
  ASCII, took 3.0 ns rather than 6.2 ns, and its document of 100 rows 7.1 ns
  rather than 9.9 ns, once words were checked from the first byte rather
  than the second, and with a last word rather than single bytes at the end.
  Testing them once rather than a word at a time then took the record to
  2.7 ns and the document to 6.7 to 6.9 ns, and, timed alone, ASCII strings
  of 9 to 40 bytes to 0.7 to 1.5 ns rather than 1.1 to 2.4 ns. Where the
  test fails, the high bits of the words that it OR-ed, taken in turn,
  locate the first byte that isn't ASCII, from which `valid_utf8_from()`
  checks the characters one by one, rather than from the start of the word
  that holds it. Strings of 6 to 40 bytes with ASCII before that byte then
  took as long or up to 55% less time. A string whose first byte isn't
  ASCII, which a branch tests on its own, took up to 0.8 ns longer, such as
  6-byte strings of 2- and 3-byte characters, 14 to 17% longer.

Structs are the generated code's, or any type that the `Struct` concept
accepts: a `SIZE`, an `ALIGNMENT`, `from_bytes()` and `to_bytes()`. The
writer's `set_struct()` and `set_structs()` write them, and `StructArray`
reads an array of them. `set_structs<T>()` also writes a range of any items
that convert into `T`, converting each as it writes it, with which a
generated `generic::<Name>Fields` writes a view of the caller's items, a view
that `tafla::ViewOf` accepts. `load_le()` and `store_le()` read and write a record's
scalars at fixed offsets, without bounds, at any alignment.

## Development

CMake 3.25 or later builds the tests in ISO C++20, without GNU's extensions,
which predefine macros such as `linux`. CI tests the compilers that the
runtime supports: GCC and Clang on 64-bit Linux, GCC for 32-bit x86 and for
s390x, and Apple Clang on macOS. Other C++20 compilers, such as MSVC, aren't
tested yet. Run the commands below from this
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
with the byte-swapping path, for 32-bit x86, with `-DCMAKE_CXX_FLAGS=-m32`,
and on s390x, a big-endian host, under QEMU:

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
- `table_reader_test`, `array_view_test`, `bools_test`, `utf8_test`,
  `layout_test` and `compatibility_test`: every getter at every position of
  arbitrary bytes, views at every alignment, bools read from bytes, UTF-8 at
  its limits, layouts, and tables that the TypeScript runtime wrote.
- `exception_safety_test`: each allocation of each setter fails in turn,
  through a replaced `operator new`, and leaves its builder as it was.
- `nesting_test` and `macros_test`: the C++ that tafla-gen generates from the
  tests' own schemas, `nesting.tafla` and `macros.tafla`, into a directory of
  each one's name: tables nested as deep as they nest, each in three fields,
  whose code compiles in a time linear in their depth, and names that macros
  take, which the test defines before it includes the code.
- `package/`: a project that finds the installed package with
  `find_package(tafla)`, which CI builds after `cmake --install`.
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

- `fuzz_table_reader` reads the rest of its input with every getter of
  `TableReader` at a position, in an offset type and header, which the input's
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
cmake --build build-fuzz --target fuzz_table_reader fuzz_conformance
mkdir -p build-fuzz/corpus/table_reader
build-fuzz/fuzz/fuzz_table_reader build-fuzz/corpus/table_reader \
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

The runtime isn't published yet. `cmake --install` installs its headers and
a CMake package, which `find_package(tafla)` finds, of the target
`tafla::tafla`. A release would add a port to vcpkg or Conan.
