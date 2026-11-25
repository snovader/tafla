//! The code generated from the schemas of the proposed arrays of strings and
//! of bools, which only the TypeScript and JavaScript targets generate so
//! far, matches the files checked in beside their tests, as
//! `PROPOSED_SCHEMAS` in `tests/common/mod.rs` lists them, and every other
//! target rejects the schemas. `TAFLA_GEN_UPDATE=1` rewrites the files.

mod common;

use std::path::Path;

use common::{
    PROPOSED_SCHEMAS, REPOSITORY, assert_no_differences, compare_file, compare_tree, targets,
};
use tafla_gen::{Diagnostic, Options, Target, compile};

#[test]
fn generated_code_matches_the_proposed_files() {
    let mut differences = Vec::new();

    for proposed in &PROPOSED_SCHEMAS {
        for &(target_name, directory) in proposed.goldens {
            let Some(target) = Target::from_name(target_name) else {
                continue; // A build without the target's feature
            };
            let files = compile(
                proposed.schema,
                &proposed.source(),
                target,
                &Options::default(),
            )
            .unwrap_or_else(|_| panic!("{} has errors", proposed.schema));

            differences.extend(compare_tree(&Path::new(REPOSITORY).join(directory), &files));
        }
    }

    assert_no_differences(&differences);
}

#[test]
fn every_other_target_rejects_each_array_of_strings_and_of_bools() {
    for proposed in &PROPOSED_SCHEMAS {
        let source = proposed.source();
        // Each `[string]` and `[bool]` of the schema
        let arrays = source.matches("[string]").count() + source.matches("[bool]").count();

        for (name, target) in targets() {
            if target.generates_lists() {
                continue;
            }

            let errors = compile(proposed.schema, &source, target, &Options::default())
                .expect_err(&format!("{name} generated {}", proposed.schema));
            assert_eq!(errors.len(), arrays, "{name}: {errors:?}");
            for error in errors {
                assert!(
                    error
                        .message
                        .starts_with(&format!("the {name} target doesn't generate arrays of")),
                    "{name}: {error:?}"
                );
            }
        }
    }
}

#[test]
fn a_target_that_rejects_arrays_of_strings_and_of_bools_points_at_each() {
    let Some(target) = Target::from_name("rust") else {
        return; // A build without the target's feature
    };
    let proposed = &PROPOSED_SCHEMAS[0];
    let source = proposed.source();
    let errors = compile(proposed.schema, &source, target, &Options::default()).unwrap_err();
    let rendered = Diagnostic::render_all(&errors, proposed.schema, &source);

    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/proposed/lists.rust.stderr");
    assert_no_differences(
        &compare_file(&expected, &rendered)
            .into_iter()
            .collect::<Vec<_>>(),
    );
}
