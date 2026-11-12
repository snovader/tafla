//! The names that each target's generated code imports or relies on beside a
//! schema's declarations, which a declaration can't take, since it would
//! clash with them or hide them. They're listed here rather than in each
//! target's generator, which a build can leave out, since the checker
//! rejects them in every build, so that a schema that one build accepts
//! generates code for every target.
//!
//! Only names that start with a capital letter are listed, since
//! declarations are `PascalCase`. `tests/diagnostics.rs` checks that every
//! such name that the code generated from the example schemas imports is
//! listed. Go and C++ list none: Go names what it imports by its package, and
//! C++ names the runtime and the standard library in full, and renames the
//! types named after the macros of C's headers itself.

/// The runtime's classes that TypeScript and JavaScript import, and the
/// globals that they read arrays and check indices with
const TYPESCRIPT: &[&str] = &[
    "Array",
    "ArrayLike",
    "BigInt64Array",
    "BigUint64Array",
    "ByteBuffer",
    "DecodeError",
    "Float32Array",
    "Float64Array",
    "Int16Array",
    "Int32Array",
    "Int8Array",
    "Number",
    "RangeError",
    "StructWriter",
    "TableLayout",
    "TableSizer",
    "TableWriter",
    "Uint16Array",
    "Uint32Array",
    "Uint8Array",
];

/// The names that Python imports from the runtime, `collections.abc`,
/// `typing` and `struct`, the built-in exceptions that it raises and catches,
/// and the keywords that start with a capital letter
const PYTHON: &[&str] = &[
    "Buffer",
    "BufferError",
    "ByteBuffer",
    "ClassVar",
    "DecodeError",
    "False",
    "Final",
    "FixedFields",
    "IndexError",
    "Iterable",
    "Iterator",
    "Literal",
    "None",
    "OverflowError",
    "Protocol",
    "Self",
    "Sequence",
    "Struct",
    "TableLayout",
    "TableWriter",
    "True",
    "TypeError",
    "TypedDict",
    "Unpack",
    "ValueError",
];

/// The names that Java imports from the runtime, `java.util`,
/// `java.lang.invoke` and `java.nio`, the classes of `java.lang` that it
/// names, which a class of its package would hide, and `Fields`, the record
/// nested in each message and table, which would hide a declaration of that
/// name. It names `java.util.Arrays` in full instead, since a schema may well
/// declare `Arrays`.
const JAVA: &[&str] = &[
    "AbstractList",
    "Byte",
    "ByteBuffer",
    "ByteOrder",
    "DecodeException",
    "Fields",
    "IndexOutOfBoundsException",
    "Integer",
    "List",
    "MethodHandles",
    "Objects",
    "OffsetType",
    "Override",
    "RandomAccess",
    "Short",
    "String",
    "SuppressWarnings",
    "TableLayout",
    "TableSizer",
    "TableWriter",
    "VarHandle",
];

/// The names that Rust imports from the runtime, the types and traits of the
/// prelude that it names, which a struct of the same name would hide, and
/// `Self`, a keyword
const RUST: &[&str] = &[
    "Alignment",
    "ArrayView",
    "BuildError",
    "ByteBuffer",
    "DecodeError",
    "ExactSizeIterator",
    "OffsetType",
    "Option",
    "Result",
    "Self",
    "TableLayout",
    "TableSizer",
    "TableWriter",
    "Vec",
    "Vector",
];

/// Each target's names, after the names of the targets that use them
const RESERVED: [(&[&str], &[&str]); 4] = [
    (&["TypeScript", "JavaScript"], TYPESCRIPT),
    (&["Python"], PYTHON),
    (&["Java"], JAVA),
    (&["Rust"], RUST),
];

/// The targets whose generated code uses `name`, which a declaration can't
/// take, in the order that the README lists them
pub fn targets_using(name: &str) -> Vec<&'static str> {
    RESERVED
        .iter()
        .filter(|(_, names)| names.contains(&name))
        .flat_map(|&(targets, _)| targets.iter().copied())
        .collect()
}
