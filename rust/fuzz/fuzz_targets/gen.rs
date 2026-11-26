//! Parses and checks arbitrary text as a schema with tafla-gen, and generates
//! every target's code and the description from each schema that it accepts,
//! which is compatible with itself. Text that holds a line of `---`, which no
//! schema can, is two schemas, an old one before it and a new one after it,
//! which are compared when both are accepted.
//!
//! The generator also runs as WebAssembly, where a panic aborts its host, so
//! any panic is a finding, as is a hang or an allocation that libFuzzer's
//! limits catch. A schema that fails must fail with diagnostics that point
//! into it, and a comparison's must point into the new schema. A generated
//! file must lie within the output directory.

#![no_main]

use libfuzzer_sys::fuzz_target;
use tafla_gen::{Diagnostic, GeneratedFile, Options, Target, compatibility, describe, parse};

const FILE_NAME: &str = "schemas/fuzz.tafla";

/// The line that separates an old schema from a new one
const SEPARATOR: &str = "\n---\n";

fuzz_target!(|bytes: &[u8]| {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };

    if let Some((old, new)) = text.split_once(SEPARATOR) {
        compare(old, new);
        return;
    }

    match parse(text) {
        Ok(_) => {
            generate(text);
            inspect(text);
        }
        Err(diagnostics) => check_diagnostics(text, &diagnostics),
    }
});

/// Describes a schema, and compares it with itself, which changes nothing
fn inspect(source: &str) {
    let description = describe(FILE_NAME, source).expect("A schema that parses is described");
    assert!(description.starts_with('{') && description.ends_with("}\n"));

    let comparison = compatibility(source, source).expect("A schema that parses is compared");
    assert_eq!(comparison.diagnostics, []);
    assert_eq!(comparison.undeclared, Vec::<String>::new());
}

/// Compares two schemas, whose errors and notes point into the new one
fn compare(old: &str, new: &str) {
    let Ok(comparison) = compatibility(old, new) else {
        return;
    };

    check_spans(new, &comparison.diagnostics);
    comparison.render(FILE_NAME, new);
}

/// Generates every target's code, each of which the parser and checker
/// accept, as they did when parsing alone
fn generate(source: &str) {
    let options = Options::default();

    for name in Target::names() {
        let target = Target::from_name(name).expect("Every target has a name");
        let files = tafla_gen::compile(FILE_NAME, source, target, &options)
            .expect("A schema that parses compiles");

        files.iter().for_each(check_file);
    }
}

fn check_diagnostics(source: &str, diagnostics: &[Diagnostic]) {
    assert_ne!(diagnostics, []);
    assert!(diagnostics.iter().all(Diagnostic::is_error));
    check_spans(source, diagnostics);
}

/// Each diagnostic points into the source, and renders
fn check_spans(source: &str, diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        let span = diagnostic.span;
        assert!(span.start <= span.end && span.end <= source.len());
        assert!(source.is_char_boundary(span.start) && source.is_char_boundary(span.end));

        let rendered = diagnostic.render(FILE_NAME, source);
        let label = if diagnostic.is_error() {
            "error: "
        } else {
            "note: "
        };
        assert!(rendered.starts_with(label));
    }
}

/// A generated file names its schema, ends with one line break, and has a
/// path within the output directory on any system
fn check_file(file: &GeneratedFile) {
    assert!(file.contents.contains(FILE_NAME));
    assert!(file.contents.ends_with('\n') && !file.contents.ends_with("\n\n"));

    for name in file.path.split('/') {
        assert!(!matches!(name, "" | "." | ".."), "{}", file.path);
        assert!(!name.contains(['\\', ':']), "{}", file.path);
    }
}
