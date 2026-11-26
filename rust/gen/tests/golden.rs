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
use tafla_gen::compile;

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

#[test]
fn every_golden_directory_has_a_schema_and_a_target() {
    let schemas: Vec<String> = example_schemas()
        .into_iter()
        .map(|schema| schema.name)
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
