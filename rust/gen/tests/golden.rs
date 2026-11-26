//! The code generated from each example schema in `schemas/`, for each
//! target, matches the files checked in under a directory of the schema's
//! name in the test tree of the target's language, as `GOLDEN_ROOTS` in
//! `tests/common/mod.rs` lists them. `TAFLA_GEN_UPDATE=1` rewrites them.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use common::{
    GOLDEN_ROOTS, Golden, PYTHON_CACHE, REPOSITORY, assert_no_differences, compare_tree,
    example_schemas, targets, update,
};
use tafla_gen::{Options, compile};

#[test]
fn generated_code_matches_the_golden_files() {
    let mut differences = Vec::new();

    for schema in example_schemas() {
        for (target_name, target) in targets() {
            let golden = Golden::of(&schema.name, target_name);
            let files = compile(&schema.file_name, &schema.source, target, &golden.options)
                .unwrap_or_else(|_| panic!("{} has errors", schema.file_name));

            differences.extend(compare_tree(&golden.directory, &golden.relative(files)));
        }
    }

    assert_no_differences(&differences);
}

/// The schemas that only some targets generate so far, with their golden
/// directories under those targets' roots: optional fields and fields added
/// `since` a version, which the targets whose
/// `Target::generates_missing_fields()` is true generate
const PARTIAL_SCHEMAS: [(&str, &str); 1] = [("presence", "typescript/core/test/presence.tafla")];

#[test]
fn generated_code_of_the_partial_schemas_matches_the_golden_files() {
    let mut differences = Vec::new();

    for (name, path) in PARTIAL_SCHEMAS {
        let source = fs::read_to_string(Path::new(REPOSITORY).join(path)).unwrap();

        for (target_name, target) in targets() {
            let result = compile(path, &source, target, &Options::default());
            if !target.generates_missing_fields() {
                // Every other target refuses the schema, rather than
                // generating code that reads its fields as required
                assert!(result.is_err(), "{target_name} generates {path}");
                continue;
            }

            let golden = Golden::of(name, target_name);
            let files = result.unwrap_or_else(|_| panic!("{path} has errors"));
            differences.extend(compare_tree(&golden.directory, &golden.relative(files)));
        }
    }

    assert_no_differences(&differences);
}

#[test]
fn every_golden_directory_has_a_schema_and_a_target() {
    let schemas: Vec<String> = example_schemas()
        .into_iter()
        .map(|schema| schema.name)
        .chain(PARTIAL_SCHEMAS.iter().map(|(name, _)| (*name).to_owned()))
        .collect();
    let roots: BTreeSet<&str> = GOLDEN_ROOTS.iter().map(|root| root.root).collect();
    let mut stale = Vec::new();

    for root in roots {
        let subdirectories: Vec<&str> = GOLDEN_ROOTS
            .iter()
            .filter(|golden| golden.root == root)
            .filter_map(|golden| golden.subdirectory)
            .collect();

        // The tests of the generated code lie beside its directories
        for schema in directories(&Path::new(REPOSITORY).join(root)) {
            let schema_name = schema.file_name().unwrap().to_str().unwrap();
            if !schemas.iter().any(|name| name == schema_name) {
                stale.push(schema);
                continue;
            }
            if subdirectories.is_empty() {
                continue;
            }

            for target in fs::read_dir(&schema).unwrap() {
                let target = target.unwrap().path();
                let target_name = target.file_name().unwrap().to_str().unwrap();
                if !subdirectories.contains(&target_name) {
                    stale.push(target);
                }
            }
        }
    }

    if update() {
        for path in &stale {
            if path.is_dir() {
                fs::remove_dir_all(path).unwrap();
            } else {
                fs::remove_file(path).unwrap();
            }
        }
        return;
    }
    assert!(stale.is_empty(), "stale golden directories: {stale:?}");
}

/// The directories in `directory`, but for Python's cache
fn directories(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir() && !path.ends_with(PYTHON_CACHE))
        .collect()
}
