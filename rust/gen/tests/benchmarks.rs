//! The code generated from each comparison suite's schema of its workloads
//! matches the files checked in under the suite, as `BENCHMARK_SCHEMAS` in
//! `tests/common/mod.rs` lists them, and the suites' schemas are the same.
//! So does the code generated from the schemas of the suites that only one
//! language has, which `SINGLE_SUITE_SCHEMAS` lists. `TAFLA_GEN_UPDATE=1`
//! rewrites the files.

mod common;

use common::{BENCHMARK_SCHEMAS, SINGLE_SUITE_SCHEMAS, assert_no_differences, compare_tree};
use tafla_gen::{Target, compile};

#[test]
fn generated_code_matches_the_suites_files() {
    let mut differences = Vec::new();

    for suite in BENCHMARK_SCHEMAS.iter().chain(&SINGLE_SUITE_SCHEMAS) {
        let Some(target) = Target::from_name(suite.target) else {
            continue; // A build without the target's feature
        };
        let golden = suite.golden();
        let files = compile(suite.schema, &suite.source(), target, &golden.options)
            .unwrap_or_else(|_| panic!("{} has errors", suite.schema));

        differences.extend(compare_tree(&golden.directory, &golden.relative(files)));
    }

    assert_no_differences(&differences);
}

#[test]
fn every_suite_has_the_same_schema() {
    let [first, others @ ..] = &BENCHMARK_SCHEMAS;
    let source = first.source();

    for suite in others {
        assert!(
            suite.source() == source,
            "{} differs from {}",
            suite.schema,
            first.schema
        );
    }
}
