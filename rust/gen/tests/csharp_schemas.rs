//! The C# generated from the C# tests' own schema matches the file that they
//! compile and run, in a directory of the schema's name beside it:
//! `csharp/tests/names.tafla`, of names that would clash in C#, in a
//! namespace of keywords. `TAFLA_GEN_UPDATE=1` rewrites it.

mod common;

use std::fs;
use std::path::Path;

use common::{REPOSITORY, assert_no_differences, compare_tree};
use tafla_gen::{Options, Target, compile};

/// The schema's name, and the package of its namespace
const SCHEMA: (&str, &str) = ("names", "Names.event.global");

#[test]
fn generated_code_of_the_csharp_tests_schema_matches_its_file() {
    let Some(target) = Target::from_name("csharp") else {
        return; // A build without the C# target
    };
    let tests = Path::new(REPOSITORY).join("csharp/tests");
    let (name, package) = SCHEMA;

    let file_name = format!("csharp/tests/{name}.tafla");
    let source = fs::read_to_string(tests.join(format!("{name}.tafla"))).unwrap();
    let options = Options {
        runtime: None,
        package: Some(package.parse().unwrap()),
    };
    let files = compile(&file_name, &source, target, &options)
        .unwrap_or_else(|_| panic!("{file_name} has errors"));

    assert_no_differences(&compare_tree(&tests.join(name), &files));
}
