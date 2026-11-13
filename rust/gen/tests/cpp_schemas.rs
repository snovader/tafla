//! The C++ generated from the C++ tests' own schemas matches the files that
//! they compile and run, in a directory of each schema's name beside it:
//! `cpp/tests/nesting.tafla`, of tables that each hold the next three times,
//! as deep as tables nest, which would compile for ever if the generated code
//! inlined a table into each field that holds it, and
//! `cpp/tests/macros.tafla`, whose names are macros' in a namespace of
//! macros' and reserved names. `TAFLA_GEN_UPDATE=1` rewrites them.

mod common;

use std::fs;
use std::path::Path;

use common::{REPOSITORY, assert_no_differences, compare_tree};
use tafla_gen::{Options, Target, compile};

/// Each schema's name, and the package of its namespace
const SCHEMAS: [(&str, Option<&str>); 2] =
    [("nesting", None), ("macros", Some("linux.EOF._Upper.b__c"))];

#[test]
fn generated_code_of_the_cpp_tests_schemas_matches_their_files() {
    let Some(target) = Target::from_name("cpp") else {
        return; // A build without the C++ target
    };
    let tests = Path::new(REPOSITORY).join("cpp/tests");

    let mut differences = Vec::new();
    for (name, package) in SCHEMAS {
        let file_name = format!("cpp/tests/{name}.tafla");
        let source = fs::read_to_string(tests.join(format!("{name}.tafla"))).unwrap();
        let options = Options {
            runtime: None,
            package: package.map(|package| package.parse().unwrap()),
        };
        let files = compile(&file_name, &source, target, &options)
            .unwrap_or_else(|_| panic!("{file_name} has errors"));

        differences.extend(compare_tree(&tests.join(name), &files));
    }

    assert_no_differences(&differences);
}
