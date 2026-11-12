//! Generated documentation names a declaration after its indefinite article,
//! by how the name sounds: a single letter by the letter's name, and any other
//! name by its first sounds.

use tafla_gen::{Options, Target, compile};

const SCHEMA: &str = "\
struct S {
    x: uint8;
}

struct Item {
    x: uint8;
}

struct Point {
    x: uint8;
}

table Entry {
    x: uint8;
}

message Order {
    x: uint8;
}
";

fn generate(target: &str) -> String {
    let target = Target::from_name(target).unwrap();
    let files = compile("articles.tafla", SCHEMA, target, &Options::default()).unwrap();

    files.into_iter().map(|file| file.contents).collect()
}

#[test]
fn python_names_each_struct_after_its_article() {
    let code = generate("python");
    for phrase in [
        "The fields of an ``S``",
        "attributes of an ``S``, or",
        "The fields of an ``Item``",
        "attributes of an ``Item``, or",
        "attributes of a ``Point``, or",
    ] {
        assert!(
            code.contains(phrase),
            "Python doesn't say {phrase:?}:\n{code}"
        );
    }
    assert!(!code.contains("a ``S``"), "{code}");
    assert!(!code.contains("a ``Item``"), "{code}");
}

#[test]
fn typescript_names_each_declaration_after_its_article() {
    for target in ["typescript", "javascript"] {
        let code = generate(target);
        for phrase in [
            "The byte length of an `S`",
            "Reads an `S` at `offset` in `bytes`",
            "Writes an `S` at the writer's position",
            "Writes an `S` at `offset` in `bytes`",
            "The fields of an `S`, to write",
            "The byte length of an `Item`",
            "The fields of an `Item`, to write",
            "The byte length of a `Point`",
            "The fields of a `Point`, to write",
            "pads an `Entry` to",
            "Writes an `Entry` where its parent's writer is",
            "The fields of an `Entry`, to write",
            "Writes an `Order` with every field",
            "Writes an `Order` with every field into the start of `bytes`",
            "The fields of an `Order`, to write",
        ] {
            assert!(
                code.contains(phrase),
                "{target} doesn't say {phrase:?}:\n{code}"
            );
        }
        assert!(!code.contains("a `S`"), "{code}");
        assert!(!code.contains("a `Item`"), "{code}");
        assert!(!code.contains("a `Entry`"), "{code}");
        assert!(!code.contains("a `Order`"), "{code}");
    }
}
