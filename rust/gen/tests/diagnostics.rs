//! Each invalid schema in `tests/diagnostics` fails with the errors in the
//! `.stderr` file beside it, rendered as the CLI prints them.
//! `TAFLA_GEN_UPDATE=1` rewrites them.

mod common;

use std::path::Path;

use std::collections::BTreeSet;
use std::fmt::Write;

use common::{
    DIAGNOSTICS, assert_no_differences, compare_file, example_schemas, files_with_extension,
    targets, update,
};
use tafla_gen::{Diagnostic, Options, compile, parse};

#[test]
fn invalid_schemas_fail_with_the_expected_errors() {
    let mut differences = Vec::new();

    for (name, path) in files_with_extension(Path::new(DIAGNOSTICS), "tafla") {
        let source = std::fs::read_to_string(&path).unwrap();
        let file_name = format!("{name}.tafla");

        match parse(&source) {
            Ok(_) => differences.push(format!("{file_name} has no errors")),
            Err(diagnostics) => {
                let rendered = Diagnostic::render_all(&diagnostics, &file_name, &source);
                let expected = path.with_extension("stderr");
                differences.extend(compare_file(&expected, &rendered));
            }
        }
    }

    assert_no_differences(&differences);
}

#[test]
fn every_expected_output_has_a_schema() {
    let directory = Path::new(DIAGNOSTICS);
    let schemas = files_with_extension(directory, "tafla");
    let stale: Vec<_> = files_with_extension(directory, "stderr")
        .into_iter()
        .filter(|(name, _)| !schemas.iter().any(|(schema, _)| schema == name))
        .map(|(_, path)| path)
        .collect();

    if update() {
        for path in &stale {
            std::fs::remove_file(path).unwrap();
        }
        return;
    }
    assert!(stale.is_empty(), "outputs without a schema: {stale:?}");
}

#[test]
fn a_syntax_error_is_the_only_error() {
    // The duplicate field would be an error too, but the checks don't run
    let source = "message Point { x: float64; x: float64; y float64; }";

    let diagnostics = parse(source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "expected `:`, found `float64`");
}

#[test]
fn errors_are_in_source_order() {
    let source = "message B { x: Unknown; }\ntable a { y: float64; }\ntable C { c: C; }";

    let diagnostics = parse(source).unwrap_err();
    let starts: Vec<usize> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.span.start)
        .collect();

    let mut sorted = starts.clone();
    sorted.sort_unstable();
    assert_eq!(starts, sorted);
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
}

#[test]
fn cycles_and_nested_offsets_are_checked_once_the_rest_is_valid() {
    // A cycle is only reported once every type resolves
    let source = "table Node { next: Node; other: Missing; }";

    let diagnostics = parse(source).unwrap_err();

    let messages: Vec<&str> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert_eq!(messages, ["unknown type `Missing`"]);
}

/// The stack of the WebAssembly module, which aborts on a panic, so that an
/// overflow would trap its host
const WASM_STACK: usize = 1 << 20;

#[test]
fn long_chains_fail_with_an_error_rather_than_overflowing_the_stack() {
    // The 65th link, counting from the innermost, rather than every link
    // that holds it, and of structs the 45th, the first whose paths to its
    // scalars, one in each link, take more than 1024 fields
    let expected_errors = [
        (
            "struct",
            &[
                "`Link44` has too many scalars for a struct",
                "`Link64` nests too deeply",
            ][..],
        ),
        ("table", &["`Link64` nests too deeply"][..]),
    ];

    for (keyword, expected) in expected_errors {
        let mut source = format!("{keyword} Link0 {{ value: uint8; }}\n");
        for link in 1..10_000 {
            let inner = link - 1;
            writeln!(
                source,
                "{keyword} Link{link} {{ value: uint8; inner: Link{inner}; }}"
            )
            .unwrap();
        }

        let diagnostics = std::thread::Builder::new()
            .stack_size(WASM_STACK)
            .spawn(move || parse(&source).unwrap_err())
            .unwrap()
            .join()
            .unwrap();

        let messages: Vec<&str> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert_eq!(messages, expected, "{keyword}");
    }
}

#[test]
fn structs_whose_paths_to_scalars_take_1024_fields_generate_code_for_every_target() {
    // 32 structs of 16 scalars, and 16 scalars within 63 structs that wrap
    // them, 64 deep: paths of 2 and 64 fields to each scalar
    let row: Vec<String> = (0..16)
        .map(|column| format!("c{column}: float32;"))
        .collect();
    let rows: Vec<String> = (0..32).map(|row| format!("r{row}: Row;")).collect();
    let mut source = format!(
        "struct Row {{ {} }}\nstruct Grid {{ {} }}\nstruct W0 {{ row: Row; }}\n",
        row.join(" "),
        rows.join(" ")
    );
    for level in 1..63 {
        let inner = level - 1;
        writeln!(source, "struct W{level} {{ inner: W{inner}; }}").unwrap();
    }
    source.push_str("message Holder { grid: Grid; grids: [Grid]; nested: Nested; }\n");
    source.push_str("table Nested { wrapped: W62; all: [W62]; }\n");

    for (name, target) in targets() {
        let files = compile("scalars.tafla", &source, target, &Options::default());
        assert!(files.is_ok(), "{name}: {files:?}");
    }
}

#[test]
fn declarations_cant_take_a_name_that_the_generated_code_imports() {
    let mut names = BTreeSet::new();
    for schema in example_schemas() {
        for (_, target) in targets() {
            let files = compile(
                &schema.file_name,
                &schema.source,
                target,
                &Options::default(),
            )
            .unwrap();
            for file in &files {
                names.extend(imported_names(&file.contents));
            }
        }
    }
    // Some of each target's
    for name in ["TableLayout", "Unpack", "List", "Vector"] {
        assert!(names.contains(name), "{name} in {names:?}");
    }

    for name in names {
        let source = format!("table {name} {{ x: uint8; }}");
        let messages: Vec<String> = parse(&source)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect();
        assert_eq!(
            messages,
            [format!(
                "`{name}` clashes with a name of the generated code"
            )]
        );
    }
}

/// C++'s generated code names the runtime and the standard library in full,
/// so it imports no names, but the headers that it includes define macros
#[test]
fn declarations_cant_take_a_macro_of_the_c_standard_library() {
    for name in ["NULL", "EOF", "NAN", "SIGINT"] {
        let source = format!("message {name} {{ x: uint8; }}");
        let messages: Vec<String> = parse(&source)
            .unwrap_err()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect();

        assert_eq!(
            messages,
            [format!(
                "`{name}` clashes with a name of the generated code"
            )]
        );
    }
}

/// C#'s generated code names the runtime and the standard library in full, so
/// it imports no names, but a type can't declare a member of its own name,
/// such as the `ToArray()` of `ToArray`, the view of an array of `To`s, and
/// the views nest an `Enumerator`
#[test]
fn declarations_cant_take_the_name_of_a_member_of_the_generated_csharp() {
    for name in [
        "Size",
        "Write",
        "ToString",
        "FormatVersion",
        "FirstVersion",
        "To",
        "Enumerator",
    ] {
        let source = format!("struct {name} {{ x: uint8; }}");
        let diagnostics = parse(&source).unwrap_err();
        let messages: Vec<&str> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert_eq!(
            messages,
            [format!(
                "`{name}` clashes with a name of the generated code"
            )]
        );
        assert!(
            diagnostics[0]
                .render("names.tafla", &source)
                .contains(&format!("the code generated for C# uses `{name}`")),
            "{name}"
        );
    }
}

/// The names that a generated file imports which a declaration could take,
/// those that start with a capital letter, from each target's imports:
/// lists in braces or parentheses, on one line or several, and Java's
/// imports of one class
fn imported_names(contents: &str) -> Vec<String> {
    let mut names = Vec::new();
    // The end of the list of names that the lines are in, if they're in one
    let mut list_end = None;

    for line in contents.lines().map(str::trim) {
        let list = if let Some(end) = list_end {
            if line.starts_with(end) {
                list_end = None;
                continue;
            }
            line
        } else if let Some((_, list)) = line
            .split_once(" import ")
            .filter(|_| line.starts_with("from "))
        {
            if list == "(" {
                list_end = Some(")");
                continue;
            }
            list
        } else if line.starts_with("import ")
            || line.starts_with("/** @import ")
            || line.starts_with("use ")
        {
            match (line.find('{'), line.find('}')) {
                (Some(start), Some(end)) => &line[start + 1..end],
                (Some(_), None) => {
                    list_end = Some("}");
                    continue;
                }
                // Java's `import a.b.Name;`
                _ => line.trim_end_matches(';').rsplit('.').next().unwrap_or(""),
            }
        } else {
            continue;
        };

        let imported = list
            .split(',')
            .map(|name| name.trim().trim_start_matches("type ").trim())
            .filter(|name| name.starts_with(|first: char| first.is_ascii_uppercase()));
        names.extend(imported.map(str::to_owned));
    }

    names
}

#[test]
fn doc_comments_cannot_hold_what_would_garble_generated_comments() {
    for (character, name) in [
        ('\0', "U+0000"),
        ('\r', "U+000D"),
        ('\u{1B}', "U+001B"),
        ('\u{7F}', "U+007F"),
        ('\u{85}', "U+0085"),
        ('\u{2028}', "U+2028"),
        ('\u{2029}', "U+2029"),
        ('\u{202E}', "U+202E"),
        ('\u{2066}', "U+2066"),
    ] {
        let source = format!("/// a{character}b\nmessage Point {{ x: uint8; }}");

        let diagnostics = parse(&source).unwrap_err();

        assert_eq!(
            diagnostics[0].message,
            format!("a doc comment can't hold {name}")
        );
        assert_eq!(diagnostics[0].span.start, 5, "{name}");
    }
}

#[test]
fn doc_comments_hold_tabs_and_end_before_a_crlf() {
    let source = "\u{FEFF}/// a\tb\r\nmessage Point {\r\n    /// c\r\n    x: uint8;\r\n}\r\n";

    let schema = parse(source).unwrap();

    let point = &schema.definitions[0];
    assert_eq!(point.docs, ["a\tb"]);
    assert_eq!(point.fields[0].docs, ["c"]);
}

/// Rust's generated code makes and matches the variants of `Result` and
/// `Option` by their names in the prelude, which an enum of the same name
/// would hide, since its struct of one integer is a function too
#[test]
fn enums_cant_take_a_variant_of_rusts_prelude() {
    for name in ["Ok", "Err", "Some", "None"] {
        let source = format!("enum {name}: uint8 {{ a = 0; }}");
        let diagnostics = parse(&source).unwrap_err();
        let messages: Vec<&str> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert_eq!(
            messages,
            [format!(
                "`{name}` clashes with a name of the generated code"
            )]
        );
        assert!(
            diagnostics[0]
                .render("names.tafla", &source)
                .contains(&format!("Rust uses `{name}`")),
            "{name}"
        );
    }
}

#[test]
fn other_declarations_can_take_a_rust_variant_name() {
    for keyword in ["message", "table", "struct"] {
        for name in ["Ok", "Err", "Some"] {
            parse(&format!("{keyword} {name} {{ value: int8; }}")).unwrap();
        }
    }

    // None is also a Python keyword.
    assert!(parse("message None { value: int8; }").is_err());
}

#[test]
fn enum_values_fit_in_the_java_constant_pool() {
    for (scalar, limit) in [
        ("uint16", 32_750),
        ("uint32", 21_833),
        ("int64", 21_833),
        ("uint64", 21_833),
    ] {
        let mut source = format!("enum Code: {scalar} {{\n");
        for i in 0..limit {
            writeln!(source, "code{i} = {i};").unwrap();
        }
        source.push('}');
        parse(&source).unwrap();

        source.pop();
        writeln!(source, "extra = {limit}; }}").unwrap();
        let errors = parse(&source).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].message,
            "`Code` has too many values for a Java class"
        );
        assert_eq!(&source[errors[0].span.start..errors[0].span.end], "Code");
    }
}
