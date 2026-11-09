# tafla-gen

Compiles a schema of messages, tables and structs into code that writes and
reads them with tafla's runtime for TypeScript, JavaScript, Python, Java or
Rust. It runs as a command, as a Rust library, and as a WebAssembly module
without imports, which the packages for the other languages run.

```text
/// A document, whose header holds its length and version, with a nested
/// table of columns of the same length
message Document (sized, version: 1) {
    x: float64;
    y: float64;
    name: string;
    columns: Columns;
}

/// Columns with a value per key
table Columns {
    /// The value that `keys` count from
    base: int64;
    keys: [uint32];
    a: [float32];
    b: [float32];
}
```

`schemas/` at the root of the repository holds example schemas, which use
every part of the language.

## Schemas

A schema declares messages, tables and structs, in any order:

- A **message** is a table that's sent on its own and read from its bytes.
  Its attributes set its layout, as the
  [format](https://github.com/snovader/tafla/blob/main/SPEC.md) describes
  it: `sized` puts the table's length in its header, `version: <n>` puts a
  version there, which readers check, and `offset_type: uint8`, `uint16` or
  `uint32`, the default, sets the offset type, the unsigned integer type of
  its offsets and lengths, which limits the table to 255 bytes, 64 KiB or
  4 GiB.
- A **table** is nested in a message or another table, as a field of its type
  or in a vector of them. It has no header, and the offset type `uint32`.
- A **struct** holds scalars and other structs at fixed offsets, without a
  header, offsets or padding. It's read and written on its own, such as the
  frames of a stream, and tables hold it in place, like a scalar, and in
  arrays, which store its values one struct after another, aligned to its
  largest scalar.

Each field is `name: type;`, of one of these types:

| Type                                                                     | Field                     |
| ------------------------------------------------------------------------ | ------------------------- |
| `int8`, `uint8`, `int16`, `uint16`, `int32`, `uint32`, `int64`, `uint64` | An integer                |
| `float32`, `float64`                                                     | A float                   |
| `bool`                                                                   | A bool, in one byte       |
| `[int8]` … `[float64]`                                                   | An array of a scalar type |
| `string`                                                                 | A UTF-8 string            |
| A struct's name                                                          | A struct, in place        |
| `[` a struct's name `]`                                                  | An array of structs       |
| A table's name                                                           | A nested table            |
| `[` a table's name `]`                                                   | A vector of tables        |

A `bool` is written as 0 or 1 and read as true when its byte isn't 0, as the
format describes it, so that any byte reads as a value. It's `boolean` in
TypeScript, JavaScript and Java, and `bool` in Python and Rust, and the
generated code reads and writes it with the runtime's `uint8` getters and
setters. It's a scalar, in structs too, but has no arrays: a `[uint8]` of 0
and 1 holds bools in the meantime.

Every field is required, and is read by its index, so its position is part of
the format: a message that changes its fields needs another version, which
should only append fields to the previous version's, as the format's
[evolution](https://github.com/snovader/tafla/blob/main/SPEC.md#evolution)
describes. A versioned message's reader rejects every other version, so an
application that reads several versions declares a message for each, reads
the version first with any of them, and then constructs the message of that
version, as [versions](#versions) shows.

`///` comments document the declaration or field after them, and the
generated code carries them, with Markdown's code spans in each language's
markup. Javadoc and `JSDoc` read the rest as text: Javadoc's HTML writes `&`,
`<`, `>` and `@` as entities, and `JSDoc` writes `@` as one, so that a line
such as `@deprecated` can't become a tag. A doc comment can't hold a control
character other than a tab, a line or paragraph separator, or a control of
the direction of text, which would end or garble the generated comments.
`//` comments are left out.

A schema is UTF-8. It may start with a byte-order mark, which is skipped, and
its lines may end with `\r\n`.

The grammar, where `NAME` is ASCII letters, digits and `_` that don't start
with a digit, and `INTEGER` is decimal digits:

```text
schema      = declaration*
declaration = doc* ("message" | "table" | "struct") NAME attributes? "{" field* "}"
attributes  = "(" (attribute ("," attribute)* ","?)? ")"
attribute   = NAME (":" (INTEGER | NAME))?
field       = doc* NAME ":" type ";"
type        = NAME | "[" NAME "]"
```

### Rules

The checker rejects a schema that breaks a rule, since tafla couldn't write
or read its tables, or a target couldn't generate its code:

- Declarations are `PascalCase` and fields `snake_case`, in ASCII. Each name
  is declared once, and a declaration can't take a built-in type's name.
- A declaration can't take a name that a target's generated code imports or
  relies on, which it would clash with or hide, in any target, so that a
  schema generates code for every target. These are the runtime's classes,
  such as `ByteBuffer` and `TableWriter`, the names of each language's
  standard library that the code names, such as `Number` in TypeScript,
  `TypedDict` in Python, `List`, `String` and `Override` in Java and `Option`
  in Rust, `Fields`, the record of a Java class's fields, and keywords, such
  as `None` in Python and `Self` in Rust. `src/generate/reserved.rs` lists
  them.
- A definition's fields have distinct names in `camelCase` too, which
  TypeScript, JavaScript and Java name them in. It drops the underscores, so
  `a_1` and `a1` are both `a1`.
- Messages, tables and structs hold at least one field.
- Only messages have attributes, each at most once.
- A message's attributes are `sized`, `version: INTEGER` and
  `offset_type: NAME`, where the name is `uint8`, `uint16` or `uint32`.
- A version fits in the message's offset type: 255 for `uint8`, 65,535 for
  `uint16` and 4,294,967,295 for `uint32`.
- Messages can't be nested; declare a table to nest it.
- A table can't contain itself, except in a vector, which can be empty.
- A message of the offset type `uint8` or `uint16` can't nest tables, since
  readers read a nested table's offsets as of its message's offset type, and
  tables have `uint32` offsets.
- Structs hold scalars and structs only, and a struct can't contain itself,
  even within another struct. Arrays of strings and of bools aren't
  supported.
- A definition's fields fit in a Java constructor, which takes at most 254
  slots of parameters, since the generated Java reads a struct into a record
  of its fields and writes a message or table from a record of them. A
  `uint32`, `int64`, `uint64` or `float64` takes two slots, as a `long` or
  `double`, and every other field one.
- A struct takes at most 65,536 bytes, since the generated code reads and
  writes each of its scalars in place.
- The paths from a struct to its scalars take at most 1,024 fields in all: a
  struct whose field `start` holds a struct of `x` and `y` reaches them as
  `start.x` and `start.y`, through 4 fields. The generated Python and Rust
  read and write each scalar through its path, indented as deep as it lies,
  so that 17 lines of structs that each hold two of the one before would
  otherwise generate 70 MB of Python. A struct at the limit, such as one of
  32 structs of 16 scalars, generates about 75 KB. It holds at most 1,024
  scalars, and a struct that nests others 64 deep holds its scalars in the
  innermost.
- Messages and tables nest in place at most 64 deep, one within another and
  the outermost included, and structs too, since the generated code writes
  nested tables by calling their writers, and nests the reads and writes of
  a struct in those of the structs that hold it, as deep as they nest. A
  struct in a table adds nothing to the table's depth, and vectors nest
  tables without a limit.
- A field can't take the name of the view of a vector or an array of structs
  beside it, `<field>_view`, in `camelCase` too, nor that of the view of an
  array of scalars, which Java declares, and a declaration can't take
  the name of a view that the schema's vectors and arrays of structs need,
  `<Table>Vector` or `<Struct>Array`, nor another declaration's
  `<Name>Fields`, which the generated code below declares.

Fields named after a member of the generated code are renamed instead, as
the generated code below describes.

Errors point at their cause, as compilers print them, and the checker reports
every error it finds, after the first syntax error if there's one:

```text
error: unknown type `flaot64`
 --> point.tafla:2:8
  |
2 |     x: flaot64;
  |        ^^^^^^^ did you mean `float64`?
```

## Generated code

Messages and tables read their fields lazily from the bytes, and are written
from their fields in one call, without the runtime's `TableBuilder`: the
generated code adds each field to the runtime's `TableSizer`, which counts the
table's length, and then writes them into bytes of that length with its
`TableWriter`, with nested tables in place in their parent. Python appends
the fields to its `TableWriter`'s bytes instead, without sizing them first,
since a call there costs more than the bytes' growth. For the same reason,
it writes a table's leading fields of fixed size, whose offsets depend on no
value, and a string after them, in one call that packs them with a `struct`,
from a `FixedFields` that describes them: a nested table of only those fields
is written whole, in its parent's call or in one of its own. Each nested table has an
`ALIGNMENT`, which its parent pads it to: the largest alignment of its arrays
and nested tables. Structs are read and written whole, and arrays of
them are read into a struct each, and written from them into one run of
bytes. A versioned message's reader fails with the runtime's decode error
when the bytes are in another version.

| Target     | Files                                    | Messages and tables                                                      | Structs                                              |
| ---------- | ---------------------------------------- | ------------------------------------------------------------------------ | ---------------------------------------------------- |
| TypeScript | `<schema>.ts`, in camelCase              | Classes, written from interfaces of their fields                         | Classes, read eagerly and written at any offset      |
| JavaScript | `<schema>.js`, an ES module with `JSDoc` | As in TypeScript, with `@typedef`s of their fields                       | As in TypeScript                                     |
| Python     | `<schema>.py`                            | Classes, written from `TypedDict`s of their fields as keyword arguments  | Frozen dataclasses, read and written with `struct`   |
| Java       | `<package>/<Name>.java`                  | Final classes, written from a nested `Fields` record                     | Records, read and written in place with `VarHandle`s |
| Rust       | `<schema>.rs`                            | `Copy` views that borrow strings and arrays, written from `<Name>Fields` | Plain structs, read and written as arrays of bytes   |

A table's getter of a vector or an array of structs reads every item at once.
Vectors return an array of tables in TypeScript and JavaScript, a `List` in
Java, a `list` in Python and a `Vec` in Rust. Every target reads a vector's
tables in place, within the message's bytes, as `SPEC.md` describes, so that
it reads the same fields from the same bytes as the others, even where a
corrupted item's offsets lead past its end. Arrays of structs return an
array of structs in TypeScript, JavaScript and Java, as Java's getters of
arrays of scalars return Java arrays, a `list` in Python, and an iterator
that reads each from the bytes in Rust. Each struct writes an array of them
with `serializeArray()`, `serialize_array()` or `array_to_bytes()`.

### Views

Beside each getter of a vector or an array of structs, `<field>View()`, or
`<field>_view()` in Python and Rust, returns a view of its items, which reads
an item only when it's asked for: a `<Table>Vector` or a `<Struct>Array`,
which the generated code declares for each table that a vector holds and
each struct that an array holds. A view finds item `i` without reading the
others: a vector's from the ends of that item and the one before it, with the
runtime's `getVectorOffset()`, `getVectorItemOffset()` and the count that the
view read in Java, TypeScript and JavaScript, or `Vector::get_in_place()` in
Rust, and an array's at `i` times its struct's size. When it's made, it checks that the
bytes hold the vector's count and an end for each item, or the array's
structs, so that its length is never more than the bytes can hold. It reads
each item when it's asked for, which fails with the decode error if the
item's bytes are corrupt.

| Target                 | Length                | Item `i`                                                                                    | Every item                                             |
| ---------------------- | --------------------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| TypeScript, JavaScript | `length()`            | `get(i)`, or a `RangeError`                                                                 | A loop of `get(i)`                                     |
| Python                 | `len(view)`           | `view[i]`, from the end if `i` is negative, or an `IndexError`                              | `for item in view`, which reads every item's end first |
| Java                   | `size()`              | `get(i)`, or an `IndexOutOfBoundsException`: a view is a random-access, unmodifiable `List` | `for (var item : view)`                                |
| Rust                   | `len()`, `is_empty()` | `get(i)`, or `None`, as a `Result` for a vector's table                                     | `iter()`, which reads each item's end once             |

A view creates the reader of each item that it reads, as the getter does, but
not those of the items it skips, nor an array of them. TypeScript's views
aren't iterable, since a loop of `for...of` over them took twice as long as a
loop of `get(i)` in V8. The comparison suites time reading the last item and
every item through views, against the getters and `ByteBuffer` by hand.

Java's getter of an array of scalars returns a copy, a Java array, where the
other targets return the array in place. So beside it, `<field>View()`
returns the runtime's read-only, little-endian view of the elements in place,
such as a `java.nio.FloatBuffer`. Unsigned elements are in the signed type of
their size, with the same bits, such as a `uint32[]` in an `IntBuffer`, which
`Integer.toUnsignedLong()` widens.

```rust
# mod shapes {
#     include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/shapes/rust/shapes.rs"));
# }
use shapes::{ItemFields, ItemList, ItemListFields, PairFields};

let item = |value| ItemFields { value, pair: PairFields { first: 0.5, second: 1.5 } };
let bytes = ItemListFields { items: &[item(1.0), item(2.0), item(3.0)] }.serialize()?;

let items = ItemList::new(&bytes).items_view()?; // an ItemVector
let last = items.get(items.len() - 1).ok_or("no items")??; // reads the last item alone
assert_eq!(last.value()?, 3.0);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Field names that are keywords in a target take a `_` after them, such as
`class_` in Python and Java, or are raw identifiers, such as `r#type` in Rust.
So do names that would clash, in a target, with a member that the generated
code defines or inherits, or with a local that it reads fields beside, such as
`constructor_` and `toString_` in TypeScript, `serialize_`, `int_`, `bool_`
and `version_of_` in Python, `hashCode_` and `serialize_` in Java, and `new_` and
`version_of_` in Rust. So does `then` in TypeScript and JavaScript, since a
`then()` method would make a reader a thenable, which `await` calls and never settles. `MEMBERS` in each
target's generator lists them; other targets keep the name.

Each target's code is formatted as its formatter in this repository leaves
it: Biome for TypeScript and JavaScript, Ruff for Python and rustfmt for
Rust, with Java at 100 columns. With the schema above, from
`schemas/shapes.tafla`, the generated code reads and writes a document like
this:

```ts
import { Document } from './shapes.ts';

const bytes = Document.serialize({
    x: 1.5,
    y: 2,
    name: 'a',
    columns: { base: 0n, keys: [1, 2], a: [0.5, 1.5], b: [2.5, 3.5] }
});
const document = new Document(bytes); // checks the version
document.columns().a(); // a Float32Array
```

```python
from shapes import Document

data = Document.serialize(
    x=1.5,
    y=2,
    name="a",
    columns={"base": 0, "keys": [1, 2], "a": [0.5, 1.5], "b": [2.5, 3.5]},
)
document = Document(data)  # checks the version
document.columns().a()  # a memoryview of floats
```

```java
var columns = new Columns.Fields(
    0, new long[] {1, 2}, new float[] {0.5f, 1.5f}, new float[] {2.5f, 3.5f});
byte[] bytes = Document.serialize(new Document.Fields(1.5, 2, "a", columns));
var document = new Document(bytes); // checks the version
document.columns().a(); // a float[]
document.columns().aView(); // a FloatBuffer of the bytes
```

```rust
# mod shapes {
#     include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/shapes/rust/shapes.rs"));
# }
use shapes::{ColumnsFields, Document, DocumentFields};

let columns = ColumnsFields { base: 0, keys: &[1, 2], a: &[0.5, 1.5], b: &[2.5, 3.5] };
let bytes = DocumentFields { x: 1.5, y: 2.0, name: "a", columns }.serialize()?;

let document = Document::new(&bytes)?; // checks the version
assert_eq!(document.columns()?.a()?.to_vec(), [0.5, 1.5]); // from an ArrayView<f32>
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Items of the caller's types

In Rust, a `<Name>Fields` takes the items of each of its vectors and arrays
of structs as a slice of any type whose reference converts into the vector's
`<Table>Fields` or the array's struct, with `From`. So a caller writes the
items it holds without collecting them into the generated types first, and
the generated code converts each one as it writes it. Each of these fields
has a type parameter, `T`, `U` and so on in the order of the fields, which
defaults to the generated type, so a slice of that type is taken as before.

A conversion makes a `<Table>Fields` with its own defaults, and so does a
nested table's field, so the vectors and arrays of structs within them take
the generated types. A vector of a table that can hold the vector's own
table again through vectors, such as a tree's children, takes its
`<Table>Fields` only, since its default type would name itself.

```rust
# mod shapes {
#     include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/shapes/rust/shapes.rs"));
# }
use shapes::{ItemFields, ItemList, ItemListFields, PairFields};

/// An item as the application holds it, with its pair's values in place
struct Row {
    value: f32,
    first: f32,
    second: f32,
}

impl From<&Row> for ItemFields {
    fn from(row: &Row) -> Self {
        let pair = PairFields { first: row.first, second: row.second };
        Self { value: row.value, pair }
    }
}

let rows = [Row { value: 1.0, first: 0.5, second: 1.5 }];
let bytes = ItemListFields { items: &rows }.serialize()?; // a slice of Row
assert_eq!(ItemList::new(&bytes).items()?[0].pair()?.second()?, 1.5);
# Ok::<(), Box<dyn std::error::Error>>(())
```

TypeScript's and JavaScript's fields are interfaces, which any object with
their properties satisfies, so they take the caller's objects as they are.

### Versions

A versioned message has a `FORMAT_VERSION`, which its writer writes and its
reader checks, and a static function that returns the version in the header
of bytes without checking it, which fails with the decode error when the
bytes don't hold the header:

| Target                 | Version of bytes                                             |
| ---------------------- | ------------------------------------------------------------ |
| TypeScript, JavaScript | `Document.versionOf(bytes)`, a `number`                      |
| Python                 | `Document.version_of(data)`, an `int`                        |
| Java                   | `Document.versionOf(bytes)`, a `long`                        |
| Rust                   | `Document::version_of(&bytes)`, a `Result<u32, DecodeError>` |

An application that reads two versions of a document, declared with the same
header as `message DocumentV1 (sized, version: 1)` and
`message Document (sized, version: 2)`, so that their versions lie in the
same slot, dispatches on the version without constructing a reader of the
wrong fields:

```ts
switch (Document.versionOf(bytes)) {
    case DocumentV1.FORMAT_VERSION:
        return fromV1(new DocumentV1(bytes));
    case Document.FORMAT_VERSION:
        return fromV2(new Document(bytes));
    default:
        throw new Error('Unknown document version');
}
```

## Command

```sh
tafla-gen <typescript|javascript|python|java|rust> <schema>... [--out <dir>] [--runtime <module>] [--package <package>]
```

`ts`, `js`, `py` and `rs` abbreviate the targets. Options may come before,
between or after the schemas, and `--` ends them, so that every argument
after it is a schema. The command writes each schema's files under `--out`,
which is the current directory by default, and names the schema by its path
in their header. It prints the errors of the schemas that have any to stderr
and writes none of their files, but writes the files of the others, and then
fails. `-h` or `--help` prints the usage, the targets and the options, and
`-V` or `--version` the version, both to stdout.

| Exit code | When                                                                                                                                                            |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0         | Every schema generated its files                                                                                                                                |
| 1         | A schema has errors, can't be read or isn't UTF-8, two schemas write the same file, or a file can't be written                                                  |
| 2         | The arguments are wrong, such as an unknown target or option, an option without its value or given twice, no schema, or a `--package` that isn't a Java package |

With wrong arguments, the command prints the error and its usage to stderr,
and writes nothing.

A file is named after its schema up to the schema's first dot, so schemas can
write the same file, such as `a/shapes.tafla` and `b/shapes.tafla`, or
`shapes.v1.tafla` and `shapes.v2.tafla`. Two Java schemas can declare the same
class, and paths that differ only in case are one file on macOS and Windows.
The command writes neither of such schemas, and prints an error for each file
they share.

- `--runtime` names the module, package or crate that the generated code
  imports tafla's runtime from, by default:

  | Target                               | Runtime                    |
  | ------------------------------------ | -------------------------- |
  | TypeScript, JavaScript, Python, Rust | `tafla`                    |
  | Java                                 | `io.github.snovader.tafla` |

- `--package` names the package of the Java classes, which is `generated` by
  default, since they belong in a package of the project that uses them,
  which the generator can't know. It's identifiers in ASCII separated by
  dots, such as `com.example.shapes`, which are the directories of the
  classes' files under `--out` too. Other targets ignore it.

From this repository, `cargo run -p tafla-gen --` runs the command.

## Library

`compile()` generates a schema's files, and `parse()` only checks it. Both
return the schema's errors as `Diagnostic`s, which `Diagnostic::render_all()`
renders as the command prints them. `Options` name the runtime and the Java
package, which is a `JavaPackage` parsed from its name, as `--package` takes
it. Every generated file's path is relative to the output directory and
within it.

```rust
use tafla_gen::{Diagnostic, Options, Target, compile};

let source = "message Point { x: float64; y: float64; }";
let files = compile("point.tafla", source, Target::Python, &Options::default())
    .expect("the schema is valid");
assert_eq!(files[0].path, "point.py");

let source = "message Point { x: flaot64; }";
let errors = compile("point.tafla", source, Target::Rust, &Options::default())
    .expect_err("the schema has an error");
assert!(Diagnostic::render_all(&errors, "point.tafla", source)
    .starts_with("error: unknown type `flaot64`\n --> point.tafla:1:20\n"));
```

Each target has a Cargo feature, `typescript` (which generates JavaScript
too), `python`, `java` and `rust`, and the default features are all four. A
crate that generates one language from its build script can depend on that
feature alone.

## WebAssembly

```sh
rustup target add wasm32-unknown-unknown
cargo build -p tafla-gen --lib --target wasm32-unknown-unknown --profile wasm
```

The build writes `target/wasm32-unknown-unknown/wasm/tafla_gen.wasm`, of
about 280 KB. It has no imports, so any host runs it with an empty import
object, without WASI or bindings. The `wasm` profile in the workspace's
`Cargo.toml` optimizes for size and aborts on a panic.

| Export                                            | Does                                                              |
| ------------------------------------------------- | ----------------------------------------------------------------- |
| `memory`                                          | The module's memory, which holds requests and responses           |
| `tafla_alloc(length: i32) -> i32`                 | Allocates `length` bytes for a request, and returns their address |
| `tafla_compile(request: i32, length: i32) -> i32` | Compiles a request, frees it, and returns the response's address  |
| `tafla_free(pointer: i32, length: i32)`           | Frees a response, whose length is in its first 4 bytes            |

Every string is its byte length as a little-endian `u32`, then its UTF-8
bytes.

- A request is five strings: the target's name, the schema's file name, its
  source, the runtime and the Java package. An empty runtime or package
  takes the default.
- A response is its own length as a `u32`, a `u32` status, and then, for
  status 0, a `u32` count of files and each file's path and contents, or, for
  status 1, the errors as the command prints them. An unknown target, a
  package that isn't a Java package and a malformed request are errors too.

Allocating and compiling may grow the memory, which detaches a host's views
of it, so a host makes its views after each call. `tests/wasm/check.mjs`
runs the module in Node.

## Development

Run the commands of `rust/README.md` from `rust/`. The tests are:

- `tests/golden.rs`: the code generated from each example schema in
  `schemas/`, for each target, matches its golden files, and no golden
  directory is left of a schema or target that's gone.
- `tests/benchmarks.rs`: the code generated from each language's comparison
  suite's `workloads.tafla` matches the files in the suite, and the suites'
  copies of the schema are the same.
- `tests/diagnostics.rs`: each invalid schema in `tests/diagnostics/` fails
  with the errors in the `.stderr` file beside it, chains of 10,000 tables or
  structs fail with an error on the WebAssembly module's stack, structs whose
  paths to their scalars take 1,024 fields generate code for every target,
  and no declaration can take a name that the generated code imports.
- `tests/properties.rs`: any input parses or fails with errors that render,
  any schema that the checker accepts generates code for every target,
  structs that contain themselves, take more than 65,536 bytes or reach their
  scalars through more than 1,024 fields and chains of tables or structs
  more than 64 deep fail with errors that point at them, and the command writes and prints what the library generates and
  finds.
- `tests/cli.rs` and `tests/compile.rs`: the command, its arguments,
  output and exit codes, and the library's options.
- `tests/members.rs`: fields named after the generated code's members take a
  `_` in the targets where the names clash.
- `tests/generated_<schema>.rs`: the golden Rust, compiled against
  `tafla` so that clippy and rustfmt check it, reads back what it
  writes, writes the tables that `TableBuilder` writes by hand, rejects other
  versions, and reads corrupted bytes as values or decode errors, through
  its views as through its getters.
- `tests/shapes_cases.rs`: the golden Rust of `shapes.tafla` writes and reads
  the tables of `conformance/generated`, as the code generated in every
  language does.

The golden files lie in the test tree of their target's language, whose
checks type-check, lint and format-check them and whose tests run them with
its runtime, in the same ways as the Rust:

| Target                 | Golden files, from the root of the repository                               |
| ---------------------- | --------------------------------------------------------------------------- |
| TypeScript, JavaScript | `typescript/core/test/generated/<schema>/<target>/`                         |
| Python                 | `python/core/tests/generated/<schema>/`                                     |
| Java                   | `java/core/src/test/java/io/github/snovader/tafla/test/generated/<schema>/` |
| Rust                   | `rust/gen/tests/golden/<schema>/rust/`                                      |

The Java is generated with the package
`io.github.snovader.tafla.test.generated.<schema>`, so that it's part of the
test module. The tests of the generated code lie beside the directories of
the schemas, and in Java in the test module's package.

The comparison suites' generated codecs lie in each suite, which runs them
against the hand-built `TableBuilder` codecs, and whose checks check them as
they check the suite. `BENCHMARK_SCHEMAS` in `tests/common/mod.rs` lists
where:

| Target     | Schema and generated code, from the root of the repository                                                               |
| ---------- | ------------------------------------------------------------------------------------------------------------------------ |
| TypeScript | `typescript/benchmarks/comparison/workloads.tafla`, `typescript/benchmarks/comparison/generated/tafla/`                  |
| Python     | `python/benchmarks/src/tafla_benchmarks/generated/workloads.tafla`, `…/generated/tafla/`                                 |
| Java       | `java/benchmarks/src/main/tafla/workloads.tafla`, `…/src/main/java/io/github/snovader/tafla/benchmarks/generated/tafla/` |
| Rust       | `rust/benchmarks/workloads.tafla`, `rust/benchmarks/src/codecs/tafla_gen/`                                               |

After a change to the generator or the schemas, rewrite the golden files,
the suites' generated code and the expected errors, review their diff, and
run every language's tests:

```sh
TAFLA_GEN_UPDATE=1 cargo test -p tafla-gen
```

CI also builds the WebAssembly module, and checks that it has no imports and
generates the golden files and the expected errors:

```sh
node gen/tests/wasm/check.mjs target/wasm32-unknown-unknown/wasm/tafla_gen.wasm
```
