//! Files that the tests compare the generator's output with: the example
//! schemas at the root of the repository, their generated code in the test
//! tree of each target's language, and the invalid schemas in
//! `tests/diagnostics`.
//!
//! Setting `TAFLA_GEN_UPDATE=1` rewrites the expected files from what the
//! generator writes, rather than comparing them.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tafla_gen::{GeneratedFile, Options, Target};

/// The root of the repository, where the generator's tests run the CLI so
/// that it names the example schemas as `schemas/<name>.tafla`
pub const REPOSITORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

pub const DIAGNOSTICS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/diagnostics");

/// Where a target's golden files lie: a directory of each example schema
/// under `root`, relative to the repository, and `subdirectory` of it when
/// targets share the root
pub struct GoldenRoot {
    pub target: &'static str,
    pub root: &'static str,
    pub subdirectory: Option<&'static str>,
}

/// Every target's golden files, including those of targets that a build
/// without their features doesn't generate. They lie in the test tree of the
/// target's language, whose tests run them against its runtime and whose
/// checks type-check, lint and format-check them; `tests/wasm/check.mjs`
/// keeps the same table.
pub const GOLDEN_ROOTS: [GoldenRoot; 6] = [
    GoldenRoot {
        target: "typescript",
        root: "typescript/core/test/generated",
        subdirectory: Some("typescript"),
    },
    GoldenRoot {
        target: "javascript",
        root: "typescript/core/test/generated",
        subdirectory: Some("javascript"),
    },
    GoldenRoot {
        target: "python",
        root: "python/core/tests/generated",
        subdirectory: None,
    },
    GoldenRoot {
        target: "java",
        root: "java/core/src/test/java/io/github/snovader/tafla/test/generated",
        subdirectory: None,
    },
    // tests/generated_<schema>.rs compile and run these
    GoldenRoot {
        target: "rust",
        root: "rust/gen/tests/golden",
        subdirectory: Some("rust"),
    },
    // A package of each schema, which the tests beside them run
    GoldenRoot {
        target: "go",
        root: "go/internal/generated",
        subdirectory: None,
    },
];

/// The package of the Java test module that holds the classes generated from
/// each example schema, in a package of the schema's name
const JAVA_PACKAGE: &str = "io.github.snovader.tafla.test.generated";

/// The golden files of an example schema's code for a target
pub struct Golden {
    /// The directory that holds the files and nothing else
    pub directory: PathBuf,
    /// The options that generate them
    pub options: Options,
    /// The directory of the generated paths that `directory` stands for,
    /// such as the directory of a Java package, or empty
    pub prefix: String,
}

impl Golden {
    pub fn of(schema: &str, target: &str) -> Self {
        let root = GOLDEN_ROOTS
            .iter()
            .find(|root| root.target == target)
            .unwrap_or_else(|| panic!("no golden files for {target}"));
        let mut directory = Path::new(REPOSITORY).join(root.root).join(schema);
        if let Some(subdirectory) = root.subdirectory {
            directory.push(subdirectory);
        }

        if target != "java" {
            return Self {
                directory,
                options: Options::default(),
                prefix: String::new(),
            };
        }

        let package = format!("{JAVA_PACKAGE}.{schema}");
        Self {
            directory,
            prefix: format!("{}/", package.replace('.', "/")),
            options: Options {
                runtime: None,
                package: Some(package.parse().unwrap()),
            },
        }
    }

    /// The generated files, by their paths relative to `directory`
    pub fn relative(&self, files: Vec<GeneratedFile>) -> Vec<GeneratedFile> {
        files
            .into_iter()
            .map(|file| GeneratedFile {
                path: file
                    .path
                    .strip_prefix(&self.prefix)
                    .unwrap_or_else(|| panic!("{} isn't under {}", file.path, self.prefix))
                    .to_owned(),
                contents: file.contents,
            })
            .collect()
    }
}

/// A comparison suite's schema of its workloads, and the directory under
/// which its codec's generated code lies, both relative to the repository
pub struct BenchmarkSchema {
    pub target: &'static str,
    pub schema: &'static str,
    pub directory: &'static str,
    /// The package of the Java classes, which are in its directory under
    /// `directory`
    pub package: Option<&'static str>,
}

/// Every suite's schema, which are copies of one schema, since the suites
/// run the same workloads. Each directory holds the generated code and
/// nothing else.
pub const BENCHMARK_SCHEMAS: [BenchmarkSchema; 5] = [
    BenchmarkSchema {
        target: "typescript",
        schema: "typescript/benchmarks/comparison/workloads.tafla",
        directory: "typescript/benchmarks/comparison/generated/tafla",
        package: None,
    },
    BenchmarkSchema {
        target: "python",
        schema: "python/benchmarks/src/tafla_benchmarks/generated/workloads.tafla",
        directory: "python/benchmarks/src/tafla_benchmarks/generated/tafla",
        package: None,
    },
    BenchmarkSchema {
        target: "java",
        schema: "java/benchmarks/src/main/tafla/workloads.tafla",
        directory: "java/benchmarks/src/main/java",
        package: Some("io.github.snovader.tafla.benchmarks.generated.tafla"),
    },
    BenchmarkSchema {
        target: "rust",
        schema: "rust/benchmarks/workloads.tafla",
        directory: "rust/benchmarks/src/codecs/tafla_gen",
        package: None,
    },
    BenchmarkSchema {
        target: "go",
        schema: "go/benchmarks/workloads.tafla",
        directory: "go/benchmarks/generated/tafla",
        package: None,
    },
];

impl BenchmarkSchema {
    pub fn source(&self) -> String {
        read(&Path::new(REPOSITORY).join(self.schema))
    }

    pub fn golden(&self) -> Golden {
        let Some(package) = self.package else {
            return Golden {
                directory: Path::new(REPOSITORY).join(self.directory),
                options: Options::default(),
                prefix: String::new(),
            };
        };

        let prefix = format!("{}/", package.replace('.', "/"));
        Golden {
            directory: Path::new(REPOSITORY).join(self.directory).join(&prefix),
            options: Options {
                runtime: None,
                package: Some(package.parse().unwrap()),
            },
            prefix,
        }
    }
}

pub fn update() -> bool {
    std::env::var_os("TAFLA_GEN_UPDATE").is_some_and(|value| value == "1")
}

/// An example schema at the root of the repository
pub struct Schema {
    /// The file name without its extension
    pub name: String,
    /// The path from the root of the repository, which generated files name
    pub file_name: String,
    pub source: String,
}

pub fn example_schemas() -> Vec<Schema> {
    let directory = Path::new(REPOSITORY).join("schemas");
    files_with_extension(&directory, "tafla")
        .into_iter()
        .map(|(name, path)| Schema {
            file_name: format!("schemas/{name}.tafla"),
            source: read(&path),
            name,
        })
        .collect()
}

/// The targets that this build generates, with their full names
pub fn targets() -> Vec<(&'static str, Target)> {
    Target::names()
        .map(|name| (name, Target::from_name(name).unwrap()))
        .collect()
}

/// The files with `extension` in `directory`, by name without the extension
pub fn files_with_extension(directory: &Path, extension: &str) -> Vec<(String, PathBuf)> {
    let mut files: Vec<(String, PathBuf)> = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|found| found == extension))
        .map(|path| (path.file_stem().unwrap().to_str().unwrap().to_owned(), path))
        .collect();
    files.sort();
    files
}

/// Python's cache of compiled modules, which it writes beside the generated
/// modules that its tests import
pub const PYTHON_CACHE: &str = "__pycache__";

/// Every file under `directory`, by its path from there with `/`
/// separators, or nothing if the directory doesn't exist
pub fn read_tree(directory: &Path) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    if directory.exists() {
        read_tree_into(directory, "", &mut files);
    }
    files
}

fn read_tree_into(directory: &Path, prefix: &str, files: &mut BTreeMap<String, String>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        let relative = format!("{prefix}{name}");

        if name == PYTHON_CACHE {
            continue;
        }
        if path.is_dir() {
            read_tree_into(&path, &format!("{relative}/"), files);
        } else {
            files.insert(relative, read(&path));
        }
    }
}

/// Compares generated files with the files under `directory`, or replaces
/// those with them when updating. Returns a description of each difference.
pub fn compare_tree(directory: &Path, files: &[GeneratedFile]) -> Vec<String> {
    let generated: BTreeMap<String, String> = files
        .iter()
        .map(|file| (file.path.clone(), file.contents.clone()))
        .collect();

    if update() {
        if directory.exists() {
            fs::remove_dir_all(directory).unwrap();
        }
        for (path, contents) in &generated {
            write(&directory.join(path), contents);
        }
        return Vec::new();
    }

    let expected = read_tree(directory);
    let mut differences = Vec::new();
    for (path, contents) in &generated {
        match expected.get(path) {
            None => differences.push(format!("{}/{path} is missing", directory.display())),
            Some(expected) => {
                if let Some(difference) = first_difference(expected, contents) {
                    differences.push(format!("{}/{path}: {difference}", directory.display()));
                }
            }
        }
    }
    for path in expected.keys() {
        if !generated.contains_key(path) {
            differences.push(format!(
                "{}/{path} isn't generated any more",
                directory.display()
            ));
        }
    }

    differences
}

/// Compares a file with what was expected, or replaces it when updating
pub fn compare_file(path: &Path, actual: &str) -> Option<String> {
    if update() {
        write(path, actual);
        return None;
    }

    let expected = fs::read_to_string(path).unwrap_or_default();
    first_difference(&expected, actual)
        .map(|difference| format!("{}: {difference}", path.display()))
}

/// The first line where `actual` differs from `expected`, if it does
pub fn first_difference(expected: &str, actual: &str) -> Option<String> {
    if expected == actual {
        return None;
    }

    let mut expected_lines = expected.split('\n');
    let mut actual_lines = actual.split('\n');
    for line in 1.. {
        let (expected_line, actual_line) = (expected_lines.next(), actual_lines.next());
        if expected_line != actual_line {
            return Some(format!(
                "line {line} is {actual_line:?} rather than {expected_line:?}"
            ));
        }
    }
    unreachable!("the strings differ")
}

/// Fails with every difference, and how to accept them
pub fn assert_no_differences(differences: &[String]) {
    assert!(
        differences.is_empty(),
        "{}\n\nRun the tests with TAFLA_GEN_UPDATE=1 to accept the generator's output",
        differences.join("\n")
    );
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}
