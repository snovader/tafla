# tafla-gen

Compiles a schema of messages, tables, structs and enums into code that
writes and reads them with tafla's runtime for TypeScript, JavaScript, Python, Java,
Rust, Go, C++, Swift or C#. It runs as a command, as a Rust library, and as a
WebAssembly module without imports, which the packages for the other
languages run.

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

A schema declares messages, tables, structs and enums, in any order:

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
- An **enum** names some of the integers of an integer type, which stores
  it. A field of an enum is a field of that type, in tables and structs, and
  an array of an enum is an array of it.

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
| An enum's name                                                           | An integer of the enum    |
| `[` an enum's name `]`                                                   | An array of the enum      |

A `bool` is written as 0 or 1 and read as true when its byte isn't 0, as the
format describes it, so that any byte reads as a value. It's `boolean` in
TypeScript, JavaScript and Java, `bool` in Python, Rust, Go, C++ and C#, and
`Bool` in Swift, and the generated code reads and writes it with the runtime's
`uint8` getters and setters. It's a scalar, in structs too, but has no arrays: a `[uint8]` of 0
and 1 holds bools in the meantime.

An enum gives its integer type after its name, and an integer to each of
its values:

```text
/// How soon to do a task
enum Priority: int8 {
    low = -1;
    normal = 0;
    /// Before the others
    high = 1;
}
```

An enum is its integer in the format: a field of `Priority` is stored as an
`int8`, in the same bytes, and an array of it as an array of `int8`, so
changing a field from `int8` to `Priority` changes no bytes, and a reader
without the enum reads the integer. A field of an enum holds any integer of
its type, not only those that the enum names, as a `bool` reads any byte, so
that a reader never rejects a value that a newer schema names, and corrupted
bytes still read as values: the generated code reads and writes an enum
exactly as it does its integer, without a check, and an array of an enum is
the view in place of an array of its integer. A value's integer is part of
the format, so it's given rather than counted, and a value that's renamed
keeps its integer. The generated code declares each value as a constant, in
each language's form, as [enums](#enums) describes.

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
schema      = (declaration | enum)*
declaration = doc* ("message" | "table" | "struct") NAME attributes? "{" field* "}"
attributes  = "(" (attribute ("," attribute)* ","?)? ")"
attribute   = NAME (":" (INTEGER | NAME))?
field       = doc* NAME ":" type ";"
type        = NAME | "[" NAME "]"
enum        = doc* "enum" NAME ":" NAME "{" value* "}"
value       = doc* NAME "=" "-"? INTEGER ";"
```

### Rules

The checker rejects a schema that breaks a rule, since tafla couldn't write
or read its tables, or a target couldn't generate its code:

- Declarations are `PascalCase` and fields and enums' values `snake_case`,
  in ASCII. Each name is declared once, by a message, table, struct or enum,
  and a declaration can't take a built-in type's name.
- A declaration can't take a name that a target's generated code imports or
  relies on, which it would clash with or hide, in any target, so that a
  schema generates code for every target. These are the runtime's classes,
  such as `TableReader` and `TableWriter`, the names of each language's
  standard library that the code names, such as `Number` in TypeScript,
  `TypedDict` in Python, `List`, `String` and `Override` in Java, `Option`
  in Rust and `Int`, `RawSpan`, `Hashable` and `RawRepresentable` in Swift,
  `Fields`, the record of a Java class's fields, keywords, such as `None` in
  Python and `Self` in Rust and Swift, the macros of the C standard library
  that a C++ header may define, such as `NULL` and `EOF`, and the members
  that C#'s generated types declare, which a type of the same name can't,
  such as a struct's `Size` and `Write()`, a record's `ToString()` and the
  `ToArray()` of the view of an array of `To`s, and `Enumerator`, which its
  views nest; C++ and C# name everything else in full, and Swift names
  `Swift.Range`, so that a schema can declare `Range`.
  `src/generate/reserved.rs` lists them.
- A definition's fields have distinct names in `camelCase` too, which
  TypeScript, JavaScript, Java and Swift name them in. It drops the
  underscores, so `a_1` and `a1` are both `a1`.
- Messages, tables and structs hold at least one field, and enums at least
  one value.
- An enum's type is an integer type, `int8` to `uint64`. Each of its values
  is an integer of that type, and no two values share a name, in
  `camelCase` too, or an integer, so that each integer has at most one name.
  An enum can't take the name of a variant of Rust's prelude, `Ok`, `Err`,
  `Some` or `None`, which the function of its tuple struct would hide.
- An enum has at most 32,750 values, or 21,833 of `uint32`, `int64` or
  `uint64`, since the generated Java declares a constant of each value. A
  constant's name takes a slot of the class's constant pool, and its integer
  one more, or two as a `long`, and tafla-gen allows them 65,500 of the
  pool's 65,534.
- A value of an enum can't take a name that the generated Go declares
  beside the enum, `<Enum><Value>` in `PascalCase`, such as `PriorityHigh`
  for `high` of `Priority`, where a declaration, a function or constant of
  the generated Go, or a value of another enum takes it.
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
- A declaration can't take the name of a function or constant that the
  generated Go declares beside a message, in the package of the schema's
  declarations: `New<Message>`, which reads it, and a versioned message's
  `<Message>VersionOf` and `<Message>FormatVersion`.
- A field can't take the name of the view of a vector or an array of structs
  beside it, `<field>_view`, in `camelCase` too, nor that of the view of an
  array of scalars, which Java declares, and a declaration can't take
  the name of a view that the schema's vectors and arrays of structs need,
  `<Table>Vector` or `<Struct>Array`, nor another declaration's
  `<Name>Fields`, which the generated code below declares, nor a message's
  or a table's `<Name>FieldValues`, which Python declares.

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
is written whole, in its parent's call or in one of its own, and a vector of
such tables in one call. Every table has `field_values()`, which reads its
fields' values into a tuple, `<Name>FieldValues`, each as its getter returns
it, but a nested table's: its values, in place of it when they're all of
fixed size, and in a tuple of their own otherwise. It reads the leading fields of fixed size in
one call, and the others after them in the same call, nested tables'
included, or every field through the getters when the table's offsets
aren't the ones that its writer writes, so that its values and errors are
the getters'. A table whose vectors are of tables of only those fields also
has `fields()`, which reads them into the `TypedDict` that it's written
from, with the items of its vectors and arrays of structs. A message of
scalars, strings, arrays and vectors of tables of fixed size without
structs has `serialize_values()`, which writes it from its values as
positional arguments, each vector's as its items' values, as its view's
`field_values()` reads them. A struct of scalars has
`read_values()`, `read_array_values()` and `serialize_values()`, which do
the same for its values without creating its dataclass. A message of only
fields of fixed size, and a string after them, is written in one call into
bytes of its own, which its `serialize()` and `serialize_values()` return
rather than a view of a writer's, through the runtime's
`TableWriter.write_fields()`. Every message has a static
`read_values(data)`, which reads its values as `field_values()` does without
creating a reader: those of a message of only such fields in one call,
through `TableReader.read_fields()`. A view of a vector
has `field_values()`, which reads every item's values, at once when the
table is of fixed size, `iter_field_values()`, which iterates over them,
reading each item when it reaches it, and `item_field_values(index)`, which
reads one item's, and a vector of tables of fixed size `fields()`. A view of an array
of structs of scalars has `field_values()`, `iter_field_values()`, which
iterates over their values, and `item_field_values(index)`. Go's messages
and tables have `Fields()`, which reads every field at once into the
`<Name>Fields` that writes it, nested tables and the items of vectors with
their own `Fields()`, in one function rather than a getter's call for each
field. A table has none when a vector of it holds tables that allocate when
they're read whole, with arrays, arrays of structs or vectors, which
corrupted offsets could lead every item to, so that what `Fields()`
allocates stays in proportion to the bytes; no table that holds itself has
one. Go lays out the same fields itself, at positions that
the generator works out, since Go's compiler keeps a sizer's and a writer's
fields in memory at every call: its sizer and writer take a table's leading
fields of fixed size together, and a nested table of only those fields
whole, and a message of only those fields and a string after them is
written into the bytes that the runtime's `AppendTable()` returns, without a
sizer and a writer. C++ writes such a message whole too, each field at a
constant position, without a sizer and a writer, into new bytes or, from
`serialize_into()`, the start of the given bytes, which the runtime's
`room_for_table()` checks hold it. A nested table of only those fields has a
`SIZE` and a `to_bytes()`, with which such a message writes it. Rust writes
such a message whole too, into the bytes that the runtime's
`TableWriter::table_bytes()` returns with its header, and its nested tables
of only those fields have a `SIZE` and a `to_bytes()` too, whose bytes the
parent's writer copies whole, and whose `SIZE` its sizer adds for every item
of a vector of them at once. TypeScript and JavaScript write such a message
whole too, into the bytes that the runtime's `TableWriter.tableBytes()`
returns with its header, with the runtime's write functions. A nested table
of only those fields has a `SIZE` and a `writeAt()`, which writes it at an
offset in the bytes that its parent's writer reserves for it, and its parent's
sizer adds the `SIZE` of every item of a vector of them at once. The items of
such a vector, and the structs of an array of them, are written one after
another where the writer reserves them, rather than into bytes of their own:
a run of fewer than 128 bytes with `writeAt()`, and a longer one through a
`DataView` with `writeView()`, which writes each scalar faster but takes
longer to create, the longest for bytes of up to 64 that V8 keeps on its heap.
C# writes such a message whole too, into the bytes that the runtime's
`TableWriter.CreateTableBytes()` returns with its header, after
`TableSizer.Utf8Length()` counts its string, and a nested table of only
those fields with its fields' `PutTo()`, at constant positions in the bytes
that the runtime's `TableWriter.ReserveTableBytes()` reserves for it, whose
length its parent's sizer adds with `TableSizer.AddTableBytes()`, for every
item of a vector of them at once. Its `SerializeInto()` writes field by
field. Each nested
table has an `ALIGNMENT`, which its parent pads it to: the largest alignment
of its arrays and nested tables. Structs are read and written whole, and
arrays of
them are read into a struct each, and written from them into one run of
bytes. A versioned message's reader fails with the runtime's decode error
when the bytes are in another version.

| Target     | Files                                    | Messages and tables                                                      | Structs                                              |
| ---------- | ---------------------------------------- | ------------------------------------------------------------------------ | ---------------------------------------------------- |
| TypeScript | `<schema>.ts`, in kebab-case             | Classes, written from interfaces of their fields                         | Classes, read eagerly and written at any offset      |
| JavaScript | `<schema>.js`, an ES module with `JSDoc` | As in TypeScript, with `@typedef`s of their fields                       | As in TypeScript                                     |
| Python     | `<schema>.py`                            | Classes, written from `TypedDict`s of their fields as keyword arguments  | Frozen dataclasses, read and written with `struct`   |
| Java       | `<package>/<Name>.java`                  | Final classes, written from a nested `Fields` record                     | Records, read and written in place with `VarHandle`s |
| Rust       | `<schema>.rs`                            | `Copy` views that borrow strings and arrays, written from `<Name>Fields` | Plain structs, read and written as arrays of bytes   |
| Go         | `<package>.go`, a package of its name    | Readers that hold their bytes, written from `<Name>Fields` structs       | Plain structs, read and written at fixed offsets     |
| C++        | `<schema>.hpp`, a header of C++20        | Views of their bytes, written from aggregates, `<Name>Fields`            | Aggregates, read and written at their offsets        |
| Swift      | `<schema>.swift`                         | Nonescapable views of their bytes, written from `<Name>Fields` structs   | Structs, read and written at their offsets           |
| C#         | `<Schema>.cs`, of C# 14 for .NET 10      | `readonly ref struct`s over a span, written from `<Name>Fields` records  | `readonly record struct`s, read and written as spans |

A table's getter of a vector or an array of structs returns a view of its
items, which reads an item only when it's asked for, as [Views](#views)
describes, and a caller that wants every item at once collects the view, as
with `iter().collect()` in Rust or `to_vector()` in C++. Every target reads a
vector's tables in place, within the message's bytes, as `SPEC.md` describes,
so that it reads the same fields from the same bytes as the others, even
where a corrupted item's offsets lead past its end. Each struct writes an
array of them with `serializeArray()`, `serialize_array()` or
`array_to_bytes()`.

In TypeScript and JavaScript, `fields()` reads every field of a message or a
table at once into the `<Name>Fields` that it's written from, nested tables
and the items of its vectors and arrays of structs included, without a reader
for each, which a nested table's static `read(reader, position)` reads. A
view's `fields()` reads every item's, and a struct's static
`read(bytes, offset)` reads its fields without creating it. One call reads
each nested table once, and allocates in proportion to the bytes: a table has
`fields()` only when its vectors hold tables of scalars, structs and such
tables, since an item that holds a string, an array or a vector would read
it again for each item that corrupted offsets point at it, and a vector of
its own type, as a tree's, at itself. The view of an array of structs also
has a getter of each field of its structs, such as `triples.value(i)`, which
reads that field of struct `i` without creating the struct; one named after
the view's own members, `length`, `get` or `fields`, takes a `_`. The view of
a vector has none, since each would find the item again, which takes longer
than finding it once for `get(i)`.

### Enums

Each enum is declared before the messages, tables and structs, with its
values in the order of the schema and their documentation. Where a language
has a type that holds any integer of the enum's type at no cost, the getters
of a field of the enum return it and its fields take it; elsewhere they
return the integer, as they do for a field of that type, and the enum holds
the values as constants of it. Arrays of an enum are those of its integer
type in every language, the views in place that a field of that type
returns, since no language reads a run of integers as a run of another
type without a copy. Nothing checks that an integer has a name: a value
that the enum doesn't name reads as itself.

| Target                 | Enum `Priority: int8`                                                 | Value `high`              | A field of the enum          |
| ---------------------- | --------------------------------------------------------------------- | ------------------------- | ---------------------------- |
| TypeScript, JavaScript | A frozen object, and a type of its name, an alias of `number`         | `Priority.High`           | `Priority`, that is a number |
| Python                 | A class of `enum.IntEnum`                                             | `Priority.HIGH`           | `int`, which a member is     |
| Java                   | A final class of constants, in a file of its own                      | `Priority.HIGH`, a `byte` | The integer's type, `byte`   |
| Rust                   | `pub struct Priority(pub i8)`, `Copy`, `Eq`, `Ord` and `Hash`         | `Priority::HIGH`          | `Priority`                   |
| Go                     | `type Priority int8`                                                  | `PriorityHigh`            | `Priority`                   |
| C++                    | `enum class Priority : std::int8_t`                                   | `Priority::high`          | `Priority`                   |
| Swift                  | A `RawRepresentable` struct of its `rawValue`, `Hashable`, `Sendable` | `Priority.high`           | `Priority`                   |
| C#                     | `enum Priority : sbyte`                                               | `Priority.High`           | `Priority`                   |

A value in upper case or `PascalCase`, or after its enum's name as in Go,
can't be a keyword. C++ gives a value named after one of its keywords, or
after a macro that a field takes a `_` after, a `_`, as `class_` and
`linux_`, and Swift escapes a value's name as a field's, as `` `class` ``
and `self_`. In Python, a field of an enum is an `int`, which a member is,
so a struct written from members holds them as given, and one read from
bytes holds integers; the generated code never calls the enum, which would
reject an integer that it doesn't name.

### Views

A view of a vector or an array of structs reads an item only when it's asked
for: a `<Table>Vector` or a `<Struct>Array`, which the generated code
declares for each table that a vector holds and each struct that an array
holds. The field's getter returns it. A view finds item `i` without reading the
others: a vector's from the ends of that item and the one before it, with the
runtime's `getVectorOffset()`, `getVectorItemOffset()` and the count that the
view read in Java, TypeScript and JavaScript, `Vector::get_in_place()` in
Rust or `Vector.GetInPlace()` in Go, and an array's at `i` times its struct's
size. When it's made, it checks that the bytes hold the vector's count and an
end for each item, or the array's structs, so that its length is never more
than the bytes can hold. It reads each item when it's asked for, which fails
with the decode error if the item's bytes are corrupt. So a getter that
returns a view succeeds although an item's bytes are corrupt, and the item
fails when it's read. In TypeScript and JavaScript, an array of structs
checks an index only by reading within its bytes, so an index outside the
array throws `DecodeError`, which is a `RangeError`, and the index must be an
integer. A Go `<Struct>Array` also returns all of its structs at once, as the
runtime's `ArrayView` returns arrays: `AsSlice()` in place, where the struct
lies in memory as its bytes do, without a bool, and the bytes are aligned to
it on a little-endian host, and `Slice()` in place where it can, and
otherwise a copy.

Since a view reads an item without the items before it, it reads an item
that corrupted ends start at the vector's count, where reading every item in
order fails at an item before it. Such an item can hold itself,
when its table holds a vector of its own type, as
[SPEC.md](https://github.com/snovader/tafla/blob/main/SPEC.md#validation)
describes, so a caller that walks a tree recursively through views bounds
how deep it walks.

| Target                 | Length                | Item `i`                                                                                    | Every item                                             |
| ---------------------- | --------------------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| TypeScript, JavaScript | `length()`            | `get(i)`, or a `RangeError`, a `DecodeError` outside an array of structs                    | A loop of `get(i)`, or `fields()`                      |
| Python                 | `len(view)`           | `view[i]`, from the end if `i` is negative, or an `IndexError`                              | `for item in view`, which reads every item's end first |
| Java                   | `size()`              | `get(i)`, or an `IndexOutOfBoundsException`: a view is a random-access, unmodifiable `List` | `for (var item : view)`, reading each item's end once  |
| Rust                   | `len()`, `is_empty()` | `get(i)`, or `None`, as a `Result` for a vector's table                                     | `iter()`, which reads each item's end once             |
| Go                     | `Len()`               | `Get(i)`, with an error for a vector's table, or a panic past the end, as indexing a slice  | `range view.All()`, which reads each item's end once   |
| C++                    | `size()`, `empty()`   | `get(i)` of a vector, `at(i)` of an array, or a `std::out_of_range`                         | `for (auto item : view)`, reading each item's end once |
| Swift                  | `count`, `isEmpty`    | `view[i]`, which throws for a vector's table, and stops the program past the end            | `for i in view.indices`, or `Array(view)` of structs   |
| C#                     | `Count`               | `view[i]`, or an `ArgumentOutOfRangeException`, from a `ref struct` view                    | `foreach (var item in view)`, reading each end once    |

A view creates the reader of each item that it reads, but not those of the
items it skips, nor an array of them. TypeScript's views aren't iterable,
since a loop of `for...of` over them took twice as long as a loop of `get(i)`
in V8. The comparison suites time reading the last item and every item
through views, against `TableReader` by hand.

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

let items = ItemList::new(&bytes).items()?; // an ItemVector
let last = items.get(items.len() - 1).ok_or("no items")??; // reads the last item alone
assert_eq!(last.value()?, 3.0);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Field names that are keywords in a target take a `_` after them, such as
`class_` in Python and Java, or are raw identifiers, such as `r#type` in Rust,
or escaped, such as `` `class` `` in Swift, where `self` takes a `_`, since
an initializer's parameter of its name would hide the instance.
So do names that would clash, in a target, with a member that the generated
code defines or inherits, or with a local that it reads fields beside, such as
`constructor_`, `toString_` and `fields_` in TypeScript, `serialize_`, `int_`, `bool_`
and `version_of_` in Python, `hashCode_` and `serialize_` in Java, `new_` and
`version_of_` in Rust, `MarshalBinary_` in Go, as Go exports every name of a
schema in `PascalCase`, which keywords don't clash with, `serialize_` in
Swift, and `serialize_`,
`serialize_into_`, `size_`, `write_`, `read_`, `to_bytes_` and `version_of_`
in C++, `Serialize_`, `Size_`, `Write_`, `ToString_` and `VersionOf_` in C#,
whose `PascalCase` names keywords don't clash with either, and where a field
named after its own definition or its definition's fields, such as `point`
in `Point`, takes a `_` too, since C# reserves a type's name for its
constructors, and C++, which also
renames macros in `snake_case`: the C standard library's, such as `errno_`,
those that compilers predefine outside strict ISO modes, such as `linux_` and
`unix_`, and `min_`, `max_`, `major_` and `minor_`, which glibc's and Windows'
headers define. So do the names of the Go methods whose signatures `go vet`
checks, such as `Format_`. So does `then` in TypeScript and JavaScript, since a
`then()` method would make a reader a thenable, which `await` calls and never settles. `MEMBERS` in each
target's generator lists them; other targets keep the name.

Each target's code is formatted as its formatter in this repository leaves
it: oxfmt for TypeScript and JavaScript, Ruff for Python, rustfmt for Rust,
gofmt for Go, which also reformats doc comments, as `go/doc/comment` parses
them, clang-format for C++, swift-format for Swift, and `dotnet format` for
C#, with Java at 100 columns. Go's files start with the comment that Go's
tools take as generated code, `// Code generated ... DO NOT EDIT.`, and C#'s
with an `// <auto-generated>` header, which keeps the analyzers and
`dotnet format` of the projects that compile them off them; this repository
checks them with its own, as `csharp/README.md` describes.

A TypeScript or JavaScript module is named after the schema's file name in
kebab-case, as those languages name modules: with a `-` for each `_`, and
its other characters as they are, such as `point-cloud.ts` of
`point_cloud.tafla`. With the schema above, from `schemas/shapes.tafla`, the
generated code reads and writes a document like this:

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

```go
bytes, err := shapes.DocumentFields{X: 1.5, Y: 2, Name: "a", Columns: shapes.ColumnsFields{
	Base: 0, Keys: []uint32{1, 2}, A: []float32{0.5, 1.5}, B: []float32{2.5, 3.5},
}}.MarshalBinary()

document, err := shapes.NewDocument(bytes) // checks the version
columns, err := document.Columns()
a, err := columns.A() // a tafla.ArrayView[float32] of the bytes
```

```cpp
#include "shapes.hpp"

const std::vector<std::uint32_t> keys{1, 2};
const std::vector<float> a{0.5F, 1.5F};
const std::vector<float> b{2.5F, 3.5F};
const auto bytes = shapes::DocumentFields{
    .x = 1.5, .y = 2, .name = "a", .columns = {.base = 0, .keys = keys, .a = a, .b = b},
}.serialize();

const shapes::Document document(bytes);  // checks the version
document.columns().a();  // a tafla::ArrayView<float> of the bytes
```

```swift
let fields = DocumentFields(
    x: 1.5,
    y: 2,
    name: "a",
    columns: ColumnsFields(base: 0, keys: [1, 2], a: [0.5, 1.5], b: [2.5, 3.5])
)
let bytes = try fields.serialize()

let all = bytes[...]
let document = try Document(all.span.bytes)  // checks the version
let a = try document.columns.a  // an ArrayView<Float> of the bytes
```

```csharp
using Shapes;

byte[] bytes = new DocumentFields
{
    X = 1.5,
    Y = 2,
    Name = "a",
    Columns = new() { Base = 0, Keys = new uint[] { 1, 2 }, A = new[] { 0.5f, 1.5f }, B = new[] { 2.5f, 3.5f } },
}.Serialize();

var document = new Document(bytes); // checks the version
ReadOnlySpan<float> a = document.Columns.A; // in place, on a little-endian host
```

C++'s namespace is the schema's file name, or `--package`'s, each of whose
names takes a `_` for a run of other characters than ASCII letters and
digits, starts with `schema_` when it doesn't start with a letter, and takes
a `_` after a keyword or a macro's name, such as `EOF_`, so that it's never a
name that C++ reserves. Its fields
structs view their strings, arrays and items as `std::string_view`s and
`std::span`s, which must outlive `serialize()`. A reader of a table nested in
a message holds the bytes from the table's start, 16 bytes on a 64-bit host,
and reads its offsets at constant positions from there, so the positions of
its decode errors count from the table's start too, as a vector item's do.
Its parent's getter throws a `tafla::DecodeError` if the table's offset leads
past the bytes, through the runtime's `TableReader::get_table()`.
It also has a default constructor, of no bytes, whose getters throw a
`tafla::DecodeError`, so that `to_vector()` of a vector's view can size its
`std::vector` before reading the items into it.

Swift's messages and tables are nonescapable views of a `RawSpan`, as the
runtime's `TableReader` is, so the compiler keeps them, and the arrays and
views that their getters return, within the lifetime of the bytes that
they're read from. Their getters are throwing properties, which throw a
`DecodeError`, and a nested table's getter returns its view, so that
`try document.columns.a` reads an array of a nested table. Swift 6.2 to 6.4
declare such views with the experimental feature `Lifetimes` and the
attribute `@_lifetime`, so the target that holds the generated code enables
the feature, as `swift/Package.swift` does for the example schemas' modules,
and code that only uses the views, in other targets, needs nothing:

```swift
.target(
    name: "Schemas",
    dependencies: [.product(name: "Tafla", package: "tafla")],
    swiftSettings: [.enableExperimentalFeature("Lifetimes")]
)
```

A schema's types are declared at the top level of its module, so two schemas
that declare the same name, such as `Point`, generate code for a module each.
The views' initializers, getters and subscripts, and structs' readers, are
`@inlinable`, as the runtime's getters are, so that a release build inlines a
read into the module that calls it, rather than calling into the generated
module for each field. A message's `layout` is a computed property, built
where it's read rather than stored, so that the inlined getters know its
offset type. The views and structs are `@frozen`, so that the code compiles
with library evolution too, as a binary framework's does.
A message's `<Name>Fields` writes it with `serialize()`, which sizes it with
the runtime's `TableSizer` and writes it with its `TableWriter`, every field
in turn and a struct's scalars one after another, without laying out fields
of fixed size itself as Go, C++, Rust, TypeScript and C# do. A struct reads
itself from the start of bytes with `init(_:)`, and writes itself with
`serialize()`, and an array of structs with `serializeArray(_:)`.

C#'s namespace is `--package`, or the schema's file name in `PascalCase`:
its runs of ASCII letters and digits, each from a capital letter, after
`Schema` when it doesn't start with a letter, such as `PointCloud` of
`point_cloud.tafla`, which also names its file. A name of `--package` that's
a C# keyword is a verbatim identifier, such as `@event`. The code names the
runtime and the standard library in full, from `global::`, and never names
one of the schema's types where a property of a field could hide it, so that
a field may take the name of any declaration but its own. A message's reader
is a `ref struct` over the `ReadOnlySpan<byte>` that it reads, as the
runtime's `TableReader` is, so it, its nested tables and the views of its
vectors live no longer than the bytes, and `foreach` reads a view without a
collection interface. A reader's default value wraps the runtime's default
`TableReader`, which reads no bytes, so its getters throw a
`DecodeException`, and a view's default value is empty. Its getters are
properties, which read the field each time they're read: a string into a new
`string`, and an array as a `ReadOnlySpan<T>` of the bytes on a little-endian
host. Its fields are a `readonly record struct` with a `required` property of
each field, so that the compiler rejects fields left out, whose arrays,
arrays of structs and vectors are `ReadOnlyMemory<T>`s, which an array
converts into and which are empty by default, and whose nested tables are
their `<Name>Fields`. A struct is a `readonly record struct` with `required`
properties, its `Size` and `Alignment`, a constructor that reads it from the
start of a `ReadOnlySpan<byte>`, and throws a `DecodeException` if the span
is shorter than it, `Write(Span<byte>)`, `Serialize()` and
`SerializeArray()`. The view of an array of structs is a `ref struct` over
its bytes, as an array of scalars is a `ReadOnlySpan<T>`, and its
`ToArray()` reads every struct.

### Writing into reused bytes

In TypeScript and JavaScript, a message's `serializeInto(fields, bytes)`
writes it into the start of the caller's bytes rather than new ones, through
the runtime's `TableSizer.writer(bytes)`, or `TableWriter.tableBytes()` for a
message written whole, and returns the view of them that holds it. The bytes stay the caller's, so the view, and what's read from it,
change when they're written again, as by the next message written into them.
Reusing bytes saves allocating and zeroing new ones: in Node 27, about
0.2 µs for a table of a few KB, a third of the write of a document of 100
rows, and 12 µs for one of 1.2 MB, but nothing for a table of up to 64
bytes. Bytes shorter than the table throw a `RangeError` before any of them
is written. Bytes that start at an offset of their `ArrayBuffer` that isn't a
multiple of 8 make readers copy the arrays that are then unaligned, as for
any bytes received at such an offset.

Java's messages also declare `serializeInto(fields, bytes)`, which writes the
message into the start of a `byte[]` that the caller owns, rather than into
new bytes, and returns its length. The bytes after it keep their values. It
throws an `IllegalArgumentException` if the bytes are shorter than the
message, before writing any of them, rather than falling back to new bytes,
so that its result always lies in the caller's bytes. The message stays there
until the bytes are written again, by the next `serializeInto()` too, which
also changes what a reader of them reads. So the caller sends or copies it
first, such as with `out.write(bytes, 0, length)` or
`Arrays.copyOf(bytes, length)`:

```java
var bytes = new byte[1 << 20]; // as long as the longest message
int length = Document.serializeInto(fields, bytes);
out.write(bytes, 0, length);
```

Reusing bytes saves allocating, zeroing and collecting new ones for each
message. In Java's comparison suite, on JDK 25, writing into reused bytes took
34% less time than into new bytes for a document of 1.2 MB, 27% less for one
of 1.3 KB, and 17% less for a record of 60 bytes. Where creating the fields'
values costs more than the bytes, it saves little: 1.5% for a vector of 100
nested tables, and nothing for an array of 100 structs.

C++'s messages declare `serialize_into(bytes)`, which writes the message
into the start of a `std::span<std::uint8_t>` that the caller owns, through
the runtime's `TableSizer::writer(bytes)`, and returns its length, as Java's
`serializeInto()` does. Bytes shorter than the message throw a
`std::invalid_argument` before any of them is written. Arrays are viewed in
place, through `as_span()`, only when the message starts at an address
aligned to 8, as a `std::vector`'s bytes do:

```cpp
std::vector<std::uint8_t> bytes(1 << 20);  // as long as the longest message
const auto length = fields.serialize_into(bytes);
send(std::span(bytes).first(length));
```

On the workloads of C++'s comparison suite, on an Apple M2 Pro with Apple
Clang, writing into reused bytes took 75% less time than into new bytes for the record of 60
bytes, 37% less for the document of 1.3 KB, and 22% less for the array of 100
structs, but only 5% less for the vector of 100 nested tables and 2% less for
the document of 1.2 MB, whose columns' copies take most of its time.

C#'s message fields declare `SerializeInto(byte[] bytes)`, which writes the
message into the start of bytes that the caller owns, through the runtime's
`TableSizer.CreateWriter(bytes)`, and returns its length, as Java's
`serializeInto()` does. Bytes shorter than the message throw an
`ArgumentException` before any of them is written.

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
let item = ItemList::new(&bytes).items()?.get(0).ok_or("no item")??;
assert_eq!(item.pair()?.second()?, 1.5);
# Ok::<(), Box<dyn std::error::Error>>(())
```

In C++, beside the `<Name>Fields` of a message or table with such vectors or
arrays of structs, which takes spans of the generated types, a template,
`generic::<Name>Fields`, takes each of them as a view of any items that
convert into the vector's `<Table>Fields` or the array's struct:
`std::views::transform()` of the caller's items, say, or a `std::span` of
items that convert themselves. Each has a type parameter, `T`, `U` and so on,
as in Rust, whose default is a span of the generated type. A view knows its
size and can be iterated more than once, since the items are sized before
they're written, which `tafla::ViewOf` checks, so a `std::vector`, which the
fields struct would hold a copy of, fails to compile. The templates lie in
the namespace `generic`, so that their names can't clash with the schema's,
which are `PascalCase`. Their items convert into the plain `<Table>Fields`,
and their nested tables' fields are plain too, so that no template names
another, whose iterators' functions Clang looks up through every type in
their template arguments: tables that held the next one's views twice, as
deep as tables nest, took time that doubled with each level to compile. So
unlike Rust's, they take every vector as a view, a tree's children too,
since their defaults never name themselves.

```cpp
/// An item as the application holds it, with its pair's values in place
struct Row {
    float value;
    float first;
    float second;
};

const std::vector<Row> rows{{.value = 1, .first = 0.5, .second = 1.5}};
const auto fields_of = [](const Row& row) {
    return shapes::ItemFields{.value = row.value, .pair = {.first = row.first, .second = row.second}};
};
const auto items = rows | std::views::transform(fields_of);
const auto bytes = shapes::generic::ItemListFields{.items = items}.serialize();
```

TypeScript's and JavaScript's fields are interfaces, which any object with
their properties satisfies, so they take the caller's objects as they are.

In Java, a struct also has a static `write(bytes, offset, ...)` of its
values, unless they take more than 253 of a method's parameter slots, and a
message whose only field is an array of such structs also has
`serialize(count, fill)`, named after `TableBuilder.setVector(count, fields,
fill)`. Its fill writes each struct straight into the table with the static
`write()`, from the items that the caller holds, without creating a record
of each struct and an array of them, such as for a message `Triples` of an
array `items` of `Triple`s of three floats:

```java
var bytes = Triples.serialize(items.size(), (index, bytes, offset) -> {
    var item = items.get(index);
    Triple.write(bytes, offset, item.value(), item.first(), item.second());
});
```

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
| Go                     | `DocumentVersionOf(bytes)`, a `uint32` and an `error`        |
| C++                    | `Document::version_of(bytes)`, a `std::uint32_t`             |
| Swift                  | `Document.version(of: bytes)`, a `UInt32` that throws        |
| C#                     | `Document.VersionOf(bytes)`, a `uint`                        |

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
tafla-gen <typescript|javascript|python|java|rust|go|cpp|swift|csharp> <schema>... [--out <dir>] [--runtime <module>] [--package <package>]
```

`cargo install tafla-gen` installs the command from crates.io. On npm,
`@tafla/gen` holds the same command, which `npx @tafla/gen` runs without a
Rust toolchain, as
[its README](https://github.com/snovader/tafla/tree/main/typescript/gen#readme)
describes.

`ts`, `js`, `py`, `rs` and `cs` abbreviate the targets. Options may come before,
between or after the schemas, and `--` ends them, so that every argument
after it is a schema. The command writes each schema's files under `--out`,
which is the current directory by default, and names the schema by its path
in their header. It prints the errors of the schemas that have any to stderr
and writes none of their files, but writes the files of the others, and then
fails. `-h` or `--help` prints the usage, the targets and the options, and
`-V` or `--version` the version, both to stdout.

| Exit code | When                                                                                                                                                                                            |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0         | Every schema generated its files                                                                                                                                                                |
| 1         | A schema has errors, can't be read or isn't UTF-8, two schemas write the same file, or a file can't be written                                                                                  |
| 2         | The arguments are wrong, such as an unknown target or option, an option without its value or given twice, no schema, or a `--package` that isn't a Java package, or more than one schema for Go |

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

  | Target                               | Runtime                        |
  | ------------------------------------ | ------------------------------ |
  | TypeScript, JavaScript, Python, Rust | `tafla`                        |
  | Java                                 | `io.github.snovader.tafla`     |
  | Go                                   | `github.com/snovader/tafla/go` |
  | C++                                  | `tafla/tafla.hpp`, a header    |
  | Swift                                | `Tafla`, the module            |
  | C#                                   | `Tafla`, a namespace           |

  Go imports it as `tafla`, whatever its path's last element.

- `--package` names the package of the Java classes, which is `generated` by
  default, since they belong in a package of the project that uses them,
  which the generator can't know. It's identifiers in ASCII separated by
  dots, such as `com.example.shapes`, which are the directories of the
  classes' files under `--out` too. C++ takes it as its namespace, such as
  `com::example::shapes`, rather than the schema's file name, with a `_` after
  a C++ keyword, `std` or `tafla`, and C# as its namespace, rather than the
  schema's file name in `PascalCase`, with a C# keyword as a verbatim
  identifier, such as `@event`. Other targets ignore it. Go's package is
  named after the schema's file name, in lowercase ASCII letters and digits,
  and so is its file: `all_types.tafla` generates `alltypes.go`, of
  `package alltypes`, which no suffix such as `_test` or `_windows` makes a
  test or constrains to a platform. A keyword or `main` takes a `_` after it.
  Go takes one schema at a time, since each is a package of its own, which
  needs a directory of its own.

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

The `cli` module is the command without its files, for a program that runs
it on files of its own, as the command's binary and the hosts of the
WebAssembly module do. `Command::parse()` takes the arguments, and returns
what to print and the exit code, for the help, the version and wrong
arguments, or a `Generation`. Its `generate()` takes a function that reads
each of its `schemas()`, and returns the files of each schema to write under
its `out()`, and the errors, as the command prints them, of the others.

```rust
use tafla_gen::cli::{Command, USAGE_ERROR};

let Command::Generate(generation) = Command::parse(["rs", "point.tafla"].map(Into::into)) else {
    panic!("the arguments ask for code");
};
let generated = generation.generate(|_schema| Err("No such file".to_owned()));
assert!(generated.file_sets.is_empty());
assert_eq!(generated.errors, ["point.tafla: No such file\n"]);

let Command::Print(output) = Command::parse(["cobol".into()]) else {
    panic!("the target is unknown");
};
assert_eq!(output.code, USAGE_ERROR);
assert!(output.stderr.starts_with("error: unknown target `cobol`"));
```

Each target has a Cargo feature, `typescript` (which generates JavaScript
too), `python`, `java`, `rust`, `go`, `cpp`, `swift` and `csharp`, and the
default features are all eight. A crate that generates one language from its
build script can depend on that feature alone.

## WebAssembly

```sh
rustup target add wasm32-unknown-unknown
cargo build -p tafla-gen --lib --target wasm32-unknown-unknown --profile wasm
```

The build writes `target/wasm32-unknown-unknown/wasm/tafla_gen.wasm`, of
about 590 KB. It has no imports, so any host runs it with an empty import
object, without WASI or bindings. The `wasm` profile in the workspace's
`Cargo.toml` optimizes for size and aborts on a panic.

| Export                                             | Does                                                                |
| -------------------------------------------------- | ------------------------------------------------------------------- |
| `memory`                                           | The module's memory, which holds requests and responses             |
| `tafla_alloc(length: i32) -> i32`                  | Allocates `length` bytes for a request, and returns their address   |
| `tafla_compile(request: i32, length: i32) -> i32`  | Compiles a schema, as the library's `compile()` does                |
| `tafla_command(request: i32, length: i32) -> i32`  | Parses the command's arguments, and names the schemas to read       |
| `tafla_generate(request: i32, length: i32) -> i32` | Runs the command on the schemas that the host read                  |
| `tafla_free(pointer: i32, length: i32)`            | Frees a response, whose length is in its first 4 bytes              |

`tafla_compile`, `tafla_command` and `tafla_generate` each take a request,
free it, and return the address of a response, which the host frees. Every
count, length and status is a little-endian `u32`, and every string is its
byte length and then its UTF-8 bytes. A response is its own length, a
status, and then what the status says.

`tafla_compile` generates one schema's files from its source:

- Its request is five strings: the target's name, the schema's file name,
  its source, the runtime and the Java package. An empty runtime or package
  takes the default.
- Its response is, for status 0, a count of files and each file's path and
  contents, or, for status 1, the errors as the command prints them. An
  unknown target, a package that isn't a Java package and a malformed
  request are errors too.

`tafla_command` and `tafla_generate` are the command, as the library's `cli`
module runs it, for a host that reads the schemas, writes the files and
prints. A host calls the first with the command's arguments, reads the
schemas that it names, and calls the second with the same arguments and what
it read:

- `tafla_command`'s request is the arguments, without the command's own
  name, each a string, up to the request's end. Its response is, for status
  0, a string of the directory to write under, a count of schemas, and each
  schema's path. For status 1, the command is over, with the help, the
  version or wrong arguments: an exit code, a string to print to stdout, and
  a string to print to stderr.
- `tafla_generate`'s request is a count of arguments and the same arguments,
  and then each of those schemas, in their order: a `u32` of 0 and the
  schema's bytes as a string, which needn't be UTF-8, or a `u32` of 1 and a
  string of why it can't be read, such as the I/O error's message. Its
  response is, for status 0, a count of the schemas that have files to
  write, for each a count of files and each file's path and contents, and
  then a count of errors and each error as a string. For status 1, it's as
  `tafla_command`'s, for arguments that don't generate, and for a malformed
  request, such as one without each of the schemas, with exit code 2.

The host writes each file at its path under the directory, and makes the
path's directories. It writes a schema's files up to the first that fails,
and adds that file's error, `<path>: <why>` and a line feed, to the errors,
and then writes the other schemas' files. It prints the errors to stderr with
a line feed between them, so that an empty line separates them, and exits
with 1 if there are any and 0 otherwise.

Allocating and compiling may grow the memory, which detaches a host's views
of it, so a host makes its views after each call. `tests/wasm/check.mjs`
runs the module in Node, and `@tafla/gen`, the npm package in
`typescript/gen`, and the jar in `java/gen` are hosts of the command.

## Development

Run the commands of `rust/README.md` from `rust/`. The tests are:

- `tests/golden.rs`: the code generated from each example schema in
  `schemas/`, for each target, matches its golden files, and no golden
  directory is left of a schema or target that's gone.
- `tests/benchmarks.rs`: the code generated from each language's comparison
  suite's `workloads.tafla` matches the files in the suite, and the suites'
  copies of the schema are the same. So do the code generated from each
  strings suite's `strings.tafla` and the copies of that schema.
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
  output and exit codes, and the library's options. The tests of
  `typescript/gen` run the command's cases that a host of the WebAssembly
  module could get wrong, through `@tafla/gen`.
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
- `tests/csharp_schemas.rs`: the C# generated from `csharp/tests/names.tafla`,
  of names that would clash in C#, matches the file that the C# build
  compiles and its tests run.

The golden files lie in the test tree of their target's language, whose
checks type-check, lint and format-check them and whose tests run them with
its runtime, in the same ways as the Rust:

| Target                 | Golden files, from the root of the repository                               |
| ---------------------- | --------------------------------------------------------------------------- |
| TypeScript, JavaScript | `typescript/core/test/generated/<schema>/<target>/`                         |
| Python                 | `python/core/tests/generated/<schema>/`                                     |
| Java                   | `java/core/src/test/java/io/github/snovader/tafla/test/generated/<schema>/` |
| Rust                   | `rust/gen/tests/golden/<schema>/rust/`                                      |
| Go                     | `go/tests/generated/<schema>/`                                              |
| C++                    | `cpp/tests/generated/<schema>/`                                             |
| Swift                  | `swift/Tests/Generated/<schema>/`, a module of each                         |
| C#                     | `csharp/tests/Tafla.Generated/<schema>/`, a library of its own              |

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
| Go         | `go/benchmarks/workloads.tafla`, `go/benchmarks/generated/tafla/`                                                        |
| C++        | `cpp/benchmarks/workloads.tafla`, `cpp/benchmarks/generated/tafla/`                                                      |
| C#         | `csharp/benchmarks/workloads.tafla`, `csharp/benchmarks/Tafla.Benchmarks/Generated/TaflaGen/`                            |
| Swift      | `swift/benchmarks/workloads.tafla`, `swift/benchmarks/Sources/GeneratedTafla/`, a module                                 |

The suites of strings have a schema of their own, of which each has a copy,
and `STRINGS_SCHEMAS` lists where:

| Target     | Schema and generated code, from the root of the repository                                                               |
| ---------- | ------------------------------------------------------------------------------------------------------------------------ |
| TypeScript | `typescript/benchmarks/strings/strings.tafla`, `typescript/benchmarks/strings/generated/tafla/`                          |
| Java       | `java/benchmarks/src/main/tafla/strings.tafla`, `…/src/main/java/io/github/snovader/tafla/benchmarks/generated/strings/` |
| Rust       | `rust/benchmarks/strings.tafla`, `rust/benchmarks/src/codecs/tafla_gen_strings/`                                         |
| C++        | `cpp/benchmarks/strings.tafla`, `cpp/benchmarks/generated/strings/`                                                      |

After a change to the generator or the schemas, rewrite the golden files,
the suites' generated code and the expected errors, review their diff, and
run every language's tests:

```sh
TAFLA_GEN_UPDATE=1 cargo test -p tafla-gen
```

CI also builds the WebAssembly module, and checks that it has no imports,
generates the golden files and the expected errors, and runs the command:

```sh
node gen/tests/wasm/check.mjs target/wasm32-unknown-unknown/wasm/tafla_gen.wasm
```
