//! Python's generated documentation names a declaration after its indefinite
//! article, by how the name sounds: a single letter by the letter's name, and
//! any other name by its first sounds.

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
