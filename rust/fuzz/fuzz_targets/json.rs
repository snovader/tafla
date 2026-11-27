//! Decodes arbitrary bytes as every message and struct of the example
//! schemas with tafla-gen, as JSON and as a layout, and encodes them as
//! JSON of each, when they're UTF-8.
//!
//! Decoding reads untrusted bytes, and runs in the WebAssembly module too,
//! where a panic aborts its host, so any panic is a finding, as is a hang, a
//! stack overflow or an allocation that libFuzzer's limits catch, which the
//! limits of how deep tables nest and how often their bytes are read rule
//! out. A table that decodes encodes back into bytes that decode as the same
//! JSON, unless the bytes that its offsets share make it too long for its
//! offset type, and text that doesn't encode fails with errors that point
//! into it.

#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use tafla_gen::{DecodeOptions, Root, Schema, parse};

const SOURCES: [&str; 4] = [
    include_str!("../../../schemas/all_types.tafla"),
    include_str!("../../../schemas/fixed_fields.tafla"),
    include_str!("../../../schemas/layouts.tafla"),
    include_str!("../../../schemas/shapes.tafla"),
];

/// Each example schema, and the names of its messages and structs
static SCHEMAS: LazyLock<Vec<(Schema, Vec<&str>)>> = LazyLock::new(|| {
    SOURCES
        .iter()
        .map(|source| {
            let schema = parse(source).expect("an example schema is valid");
            (schema, roots(source))
        })
        .collect()
});

/// The names of the messages and structs that a schema's source declares
fn roots(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line
                .strip_prefix("message ")
                .or_else(|| line.strip_prefix("struct "))?;
            rest.split([' ', '(', '{']).next()
        })
        .collect()
}

fuzz_target!(|bytes: &[u8]| {
    let text = std::str::from_utf8(bytes).ok();

    for (schema, names) in SCHEMAS.iter() {
        for name in names {
            let root = Root::find(schema, name).expect("a message or a struct");
            for any_version in [false, true] {
                decode(&root, bytes, any_version);
            }
            if let Some(text) = text {
                encode(&root, text);
            }
        }
    }
});

fn decode(root: &Root, bytes: &[u8], any_version: bool) {
    let options = DecodeOptions {
        any_version,
        ..DecodeOptions::default()
    };
    let decoded = root.decode(bytes, &options);
    let layout = root.layout(bytes, &options);
    assert_eq!(decoded.as_ref().err(), layout.error.as_ref());

    let Ok(json) = decoded else {
        return;
    };
    match root.encode(&json) {
        Ok(written) => assert_eq!(root.decode(&written, &options), Ok(json)),
        Err(errors) => assert!(
            errors[0].message.contains("longer than its offset type"),
            "{errors:?}"
        ),
    }
}

fn encode(root: &Root, text: &str) {
    let Err(errors) = root.encode(text) else {
        return;
    };

    assert_ne!(errors, []);
    for error in &errors {
        let span = error.span;
        assert!(span.start <= span.end && span.end <= text.len());
        assert!(error.render("table.json", text).starts_with("error: "));
    }
}
