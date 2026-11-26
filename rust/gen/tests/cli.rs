//! The `tafla-gen` binary: it writes the generated files under `--out`,
//! prints errors as the library renders them, and fails on wrong arguments
//! with exit code 2 and on a schema's errors with 1. Its subcommands print
//! a schema's description, and where a schema is incompatible with the
//! version before it.

mod common;

use std::path::Path;
use std::process::{Command, Output};

use common::{DIAGNOSTICS, Golden, REPOSITORY, read_tree, targets};
use tafla_gen::{DESCRIPTION_VERSION, describe};

const COMPATIBILITY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/compatibility");

fn tafla_gen(directory: &str, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tafla-gen"))
        .current_dir(directory)
        .args(arguments)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

/// A new directory of schemas, by their paths in it
fn write_schemas(schemas: &[(&str, &str)]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (path, source) in schemas {
        let path = directory.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    directory
}

#[test]
fn writes_the_golden_files_of_an_example_schema() {
    let out = tempfile::tempdir().unwrap();

    for (name, _) in targets() {
        let golden = Golden::of("shapes", name);
        let directory = out.path().join(name);
        let mut arguments = vec![
            name,
            "schemas/shapes.tafla",
            "--out",
            directory.to_str().unwrap(),
        ];
        if let Some(package) = &golden.options.package {
            arguments.extend(["--package", package.as_str()]);
        }
        let output = tafla_gen(REPOSITORY, &arguments);

        assert!(output.status.success(), "{}", stderr(&output));
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        assert_eq!(
            read_tree(&directory.join(&golden.prefix)),
            read_tree(&golden.directory),
            "{name}"
        );
    }
}

#[test]
fn writes_every_schema_it_is_given() {
    let out = tempfile::tempdir().unwrap();

    let output = tafla_gen(
        REPOSITORY,
        &[
            "py",
            "schemas/shapes.tafla",
            "--out",
            out.path().to_str().unwrap(),
            "schemas/layouts.tafla",
        ],
    );

    assert!(output.status.success(), "{}", stderr(&output));
    let files: Vec<String> = read_tree(out.path()).into_keys().collect();
    assert_eq!(files, ["layouts.py", "shapes.py"]);
}

#[test]
fn accepts_abbreviated_targets() {
    for (abbreviation, name) in [
        ("ts", "typescript"),
        ("js", "javascript"),
        ("py", "python"),
        ("rs", "rust"),
    ] {
        let out = tempfile::tempdir().unwrap();
        let output = tafla_gen(
            REPOSITORY,
            &[
                abbreviation,
                "schemas/shapes.tafla",
                "--out",
                out.path().to_str().unwrap(),
            ],
        );

        assert!(output.status.success(), "{}", stderr(&output));
        let golden = Golden::of("shapes", name);
        assert_eq!(
            read_tree(out.path()),
            read_tree(&golden.directory),
            "{abbreviation}"
        );
    }
}

#[test]
fn prints_the_errors_of_an_invalid_schema_and_fails() {
    let out = tempfile::tempdir().unwrap();

    let output = tafla_gen(
        DIAGNOSTICS,
        &[
            "rust",
            "unknown_types.tafla",
            "--out",
            out.path().to_str().unwrap(),
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let expected = std::fs::read_to_string(Path::new(DIAGNOSTICS).join("unknown_types.stderr"));
    assert_eq!(stderr(&output), expected.unwrap());
    assert!(read_tree(out.path()).is_empty());
}

#[test]
fn writes_the_valid_schemas_and_prints_the_errors_of_the_others() {
    let out = tempfile::tempdir().unwrap();
    let diagnostics = Path::new(DIAGNOSTICS);
    let schemas = [
        diagnostics.join("cycles.tafla"),
        Path::new(REPOSITORY).join("schemas/shapes.tafla"),
        diagnostics.join("naming.tafla"),
    ];

    let mut arguments = vec!["rust", "--out", out.path().to_str().unwrap()];
    arguments.extend(schemas.iter().map(|path| path.to_str().unwrap()));
    let output = tafla_gen(REPOSITORY, &arguments);

    assert_eq!(output.status.code(), Some(1));
    // Each schema's errors name it by its path, after a blank line
    let stderr = stderr(&output);
    assert!(stderr.contains("cycles.tafla:3:7\n"), "{stderr}");
    assert!(
        stderr.contains("in itself\n\nerror: `point` should be PascalCase\n"),
        "{stderr}"
    );
    let files: Vec<String> = read_tree(out.path()).into_keys().collect();
    assert_eq!(files, ["shapes.rs"]);
}

#[test]
fn writes_neither_of_two_schemas_that_write_the_same_file() {
    let point = "message Point { x: uint8; }\n";
    let directory = write_schemas(&[
        ("a/point.tafla", point),
        ("b/point.tafla", point),
        ("point.v2.tafla", point),
        ("pair.tafla", "message Pair { a: uint8; }\n"),
    ]);
    let out = tempfile::tempdir().unwrap();

    let output = tafla_gen(
        directory.path().to_str().unwrap(),
        &[
            "py",
            "a/point.tafla",
            "b/point.tafla",
            "point.v2.tafla",
            "pair.tafla",
            "--out",
            out.path().to_str().unwrap(),
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "a/point.tafla and b/point.tafla both write point.py\n\n\
         a/point.tafla and point.v2.tafla both write point.py\n"
    );
    let files: Vec<String> = read_tree(out.path()).into_keys().collect();
    assert_eq!(files, ["pair.py"]);
}

#[test]
fn writes_neither_of_two_schemas_that_declare_the_same_java_class() {
    let directory = write_schemas(&[
        (
            "a.tafla",
            "message Point { x: uint8; pair: Pair; }\ntable Pair { a: uint8; }\n",
        ),
        ("b.tafla", "message Pair { b: uint8; }\n"),
        ("c.tafla", "message Other { c: uint8; }\n"),
    ]);
    let out = tempfile::tempdir().unwrap();

    let output = tafla_gen(
        directory.path().to_str().unwrap(),
        &[
            "java",
            "a.tafla",
            "b.tafla",
            "c.tafla",
            "--out",
            out.path().to_str().unwrap(),
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "a.tafla and b.tafla both write generated/Pair.java\n"
    );
    let files: Vec<String> = read_tree(out.path()).into_keys().collect();
    assert_eq!(files, ["generated/Other.java"]);
}

#[test]
fn writes_no_files_whose_paths_differ_only_in_case() {
    let directory = write_schemas(&[
        ("a/Point.tafla", "message Point { x: uint8; }\n"),
        ("b/point.tafla", "message Point { x: uint8; }\n"),
        (
            "points.tafla",
            "message Point { x: uint8; }\nmessage POINT { x: uint8; }\n",
        ),
    ]);

    for (schemas, expected) in [
        (
            &["py", "a/Point.tafla", "b/point.tafla"][..],
            "a/Point.tafla writes Point.py and b/point.tafla writes point.py, \
             which are one file where file names ignore case\n",
        ),
        // Classes of one schema
        (
            &["java", "points.tafla"],
            "points.tafla writes generated/Point.java and points.tafla writes \
             generated/POINT.java, which are one file where file names ignore case\n",
        ),
    ] {
        let out = tempfile::tempdir().unwrap();
        let mut arguments = schemas.to_vec();
        arguments.extend(["--out", out.path().to_str().unwrap()]);

        let output = tafla_gen(directory.path().to_str().unwrap(), &arguments);

        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stderr(&output), expected);
        assert!(read_tree(out.path()).is_empty());
    }
}

#[test]
fn generates_a_schema_given_twice_once() {
    let out = tempfile::tempdir().unwrap();

    let output = tafla_gen(
        REPOSITORY,
        &[
            "py",
            "schemas/shapes.tafla",
            "schemas/shapes.tafla",
            "--out",
            out.path().to_str().unwrap(),
        ],
    );

    assert!(output.status.success(), "{}", stderr(&output));
    let files: Vec<String> = read_tree(out.path()).into_keys().collect();
    assert_eq!(files, ["shapes.py"]);
}

#[test]
fn prints_its_help_to_stdout() {
    for arguments in [
        &["--help"][..],
        &["-h"],
        &["rust", "schema.tafla", "--help"],
    ] {
        let output = tafla_gen(REPOSITORY, arguments);

        assert!(output.status.success(), "{arguments:?}");
        assert_eq!(stderr(&output), "");
        let help = String::from_utf8(output.stdout).unwrap();
        let version = env!("CARGO_PKG_VERSION");
        assert!(
            help.starts_with(&format!("tafla-gen {version}\n")),
            "{help}"
        );
        assert!(help.contains(&format!("\n{USAGE}\n")), "{help}");
        assert!(help.contains("\n  rust, rs\n"), "{help}");
        assert!(help.contains("\n  csharp, cs\n"), "{help}");
        assert!(help.contains("\n  2  The arguments are wrong\n"), "{help}");
    }
}

#[test]
fn prints_its_version_to_stdout() {
    for argument in ["--version", "-V"] {
        let output = tafla_gen(REPOSITORY, &[argument]);

        assert!(output.status.success());
        assert_eq!(stderr(&output), "");
        let version = format!("tafla-gen {}\n", env!("CARGO_PKG_VERSION"));
        assert_eq!(String::from_utf8(output.stdout).unwrap(), version);
    }
}

const USAGE: &str = "usage: tafla-gen <typescript|javascript|python|java|rust|go|cpp|swift|csharp> <schema>... \
                     [--out <dir>] [--runtime <module>] [--package <package>]\n       \
                     tafla-gen describe <schema>...\n       \
                     tafla-gen check <schema> --against <old schema>";

/// Fails as wrong arguments do: with the error, the usage and exit code 2,
/// without writing a file. A target's arguments are given an `--out`, which
/// a subcommand's aren't.
fn assert_usage_error(directory: &str, arguments: &[&str], error: &str) {
    let out = tempfile::tempdir().unwrap();
    let mut arguments = arguments.to_vec();
    let subcommand = matches!(arguments.first(), Some(&("describe" | "check")));
    if !subcommand && !arguments.contains(&"--out") {
        arguments.extend(["--out", out.path().to_str().unwrap()]);
    }

    let output = tafla_gen(directory, &arguments);

    assert_eq!(output.status.code(), Some(2), "{arguments:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        stderr(&output),
        format!("error: {error}\n{USAGE}\nRun `tafla-gen --help` for more.\n"),
        "{arguments:?}"
    );
    assert!(read_tree(out.path()).is_empty());
}

#[test]
fn rejects_wrong_arguments() {
    for (arguments, error) in [
        (&[][..], "no target is given"),
        (&["rust"], "no schema is given"),
        (
            &["cobol", "schemas/shapes.tafla"],
            "unknown target `cobol`: this build generates typescript, javascript, python, java, rust, go, cpp, swift, csharp",
        ),
        (
            &["rust", "schemas/shapes.tafla", "--outdir", "out"],
            "unknown option `--outdir`",
        ),
        (
            &["rust", "-o", "schemas/shapes.tafla"],
            "unknown option `-o`",
        ),
        (
            &["rust", "schemas/shapes.tafla", "--out"],
            "`--out` needs a value",
        ),
        (
            &[
                "rust",
                "schemas/shapes.tafla",
                "--runtime",
                "a",
                "--runtime",
                "b",
            ],
            "`--runtime` is given twice",
        ),
    ] {
        assert_usage_error(REPOSITORY, arguments, error);
    }
}

#[test]
fn rejects_a_package_that_isnt_a_java_package() {
    assert_usage_error(
        REPOSITORY,
        &["java", "schemas/shapes.tafla", "--package", ".generated"],
        "`.generated` isn't a Java package, which is identifiers in ASCII separated by dots, \
         such as `com.example.shapes`",
    );
}

#[test]
fn rejects_two_schemas_for_go_whose_packages_would_share_a_directory() {
    assert_usage_error(
        REPOSITORY,
        &["go", "schemas/shapes.tafla", "schemas/layouts.tafla"],
        "go generates a package of each schema, which needs a directory of its own: \
         give one schema, and another `--out` for each other",
    );
}

#[test]
fn generates_one_schema_given_twice_for_go() {
    let out = tempfile::tempdir().unwrap();

    let output = tafla_gen(
        REPOSITORY,
        &[
            "go",
            "schemas/shapes.tafla",
            "schemas/shapes.tafla",
            "--out",
            out.path().to_str().unwrap(),
        ],
    );

    assert!(output.status.success(), "{}", stderr(&output));
    let files: Vec<String> = read_tree(out.path()).into_keys().collect();
    assert_eq!(files, ["shapes.go"]);
}

#[cfg(unix)]
#[test]
fn rejects_an_argument_that_isnt_utf8() {
    use std::os::unix::ffi::OsStrExt;

    let out = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tafla-gen"))
        .current_dir(REPOSITORY)
        .args(["rust", "--out", out.path().to_str().unwrap()])
        .arg(std::ffi::OsStr::from_bytes(b"caf\xe9.tafla"))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).starts_with("error: `caf\u{FFFD}.tafla` isn't UTF-8\n"));
    assert!(read_tree(out.path()).is_empty());
}

#[test]
fn takes_options_before_the_target_and_schemas_after_a_double_dash() {
    let directory = write_schemas(&[("-point.tafla", "message Point { x: uint8; }\n")]);
    let out = tempfile::tempdir().unwrap();

    let output = tafla_gen(
        directory.path().to_str().unwrap(),
        &[
            "--out",
            out.path().to_str().unwrap(),
            "py",
            "--",
            "-point.tafla",
        ],
    );

    assert!(output.status.success(), "{}", stderr(&output));
    let files: Vec<String> = read_tree(out.path()).into_keys().collect();
    assert_eq!(files, ["-point.py"]);
}

#[test]
fn reads_a_schema_with_a_byte_order_mark_and_crlf_line_endings() {
    let source = "/// A point\n/// on a plane\nmessage Point {\n    x: float64;\n}\n";
    let windows = format!("\u{FEFF}{}", source.replace('\n', "\r\n"));
    let directory = write_schemas(&[
        ("unix/point.tafla", source),
        ("windows/point.tafla", &windows),
    ]);

    for (name, _) in targets() {
        let [unix, windows] = ["unix", "windows"].map(|system| {
            let out = tempfile::tempdir().unwrap();
            let schema = format!("{system}/point.tafla");
            let output = tafla_gen(
                directory.path().to_str().unwrap(),
                &[name, &schema, "--out", out.path().to_str().unwrap()],
            );
            assert!(output.status.success(), "{}", stderr(&output));
            read_tree(out.path())
        });

        // The same files, but for the schema's path in their header
        let windows: Vec<String> = windows
            .into_values()
            .map(|contents| contents.replace("windows/point.tafla", "unix/point.tafla"))
            .collect();
        assert_eq!(unix.into_values().collect::<Vec<_>>(), windows, "{name}");
    }
}

#[test]
fn rejects_a_schema_that_isnt_utf8() {
    let directory = tempfile::tempdir().unwrap();
    // A doc comment in Latin-1 rather than UTF-8
    let source = b"message Point {\n    /// caf\xe9 au lait\n    x: float64;\n}\n";
    std::fs::write(directory.path().join("point.tafla"), source).unwrap();
    let out = tempfile::tempdir().unwrap();

    let output = tafla_gen(
        directory.path().to_str().unwrap(),
        &["rust", "point.tafla", "--out", out.path().to_str().unwrap()],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "error: the schema isn't UTF-8\n \
         --> point.tafla:2:12\n  \
         |\n\
         2 |     /// caf\u{FFFD} au lait\n  \
         |            ^ this isn't a character in UTF-8\n"
    );
    assert!(read_tree(out.path()).is_empty());
}

#[test]
fn reports_a_schema_it_cannot_read_and_writes_the_others() {
    let out = tempfile::tempdir().unwrap();

    let output = tafla_gen(
        REPOSITORY,
        &[
            "rust",
            "schemas/missing.tafla",
            "schemas/shapes.tafla",
            "--out",
            out.path().to_str().unwrap(),
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).starts_with("schemas/missing.tafla: "));
    let files: Vec<String> = read_tree(out.path()).into_keys().collect();
    assert_eq!(files, ["shapes.rs"]);
}

#[test]
fn describes_each_schema_once_to_stdout() {
    let output = tafla_gen(
        REPOSITORY,
        &[
            "describe",
            "schemas/shapes.tafla",
            "schemas/layouts.tafla",
            "schemas/shapes.tafla",
        ],
    );

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stderr(&output), "");
    let description: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(description["description_version"], DESCRIPTION_VERSION);
    let schemas = description["schemas"].as_array().unwrap();
    let paths: Vec<&serde_json::Value> = schemas.iter().map(|schema| &schema["path"]).collect();
    assert_eq!(paths, ["schemas/shapes.tafla", "schemas/layouts.tafla"]);

    // Each schema is described as the library describes it alone
    let source = std::fs::read_to_string(Path::new(REPOSITORY).join("schemas/shapes.tafla"));
    let alone: serde_json::Value =
        serde_json::from_str(&describe("schemas/shapes.tafla", &source.unwrap()).unwrap()).unwrap();
    assert_eq!(schemas[0], alone["schemas"][0]);
}

#[test]
fn describes_nothing_when_a_schema_has_errors() {
    let output = tafla_gen(
        DIAGNOSTICS,
        &[
            "describe",
            "unknown_types.tafla",
            "missing.tafla",
            "cycles.tafla",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    let read = |name: &str| std::fs::read_to_string(Path::new(DIAGNOSTICS).join(name)).unwrap();
    let expected = [
        read("unknown_types.stderr"),
        format!("missing.tafla: {}\n", not_found()),
        read("cycles.stderr"),
    ];
    assert_eq!(stderr(&output), expected.join("\n"));
}

/// Why a file that doesn't exist can't be read, as the command prints it:
/// in Rust's words, with the operating system's code
fn not_found() -> String {
    let directory = tempfile::tempdir().unwrap();
    std::fs::read(directory.path().join("missing.tafla"))
        .unwrap_err()
        .to_string()
}

#[test]
fn checks_a_schema_against_the_one_before_it() {
    for (case, code) in [("fields", 1), ("versions_back", 0)] {
        let new = format!("{case}.tafla");
        let old = format!("{case}.old.tafla");

        let output = tafla_gen(COMPATIBILITY, &["check", &new, "--against", &old]);

        assert_eq!(output.status.code(), Some(code), "{case}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        let expected =
            std::fs::read_to_string(Path::new(COMPATIBILITY).join(format!("{case}.stderr")));
        assert_eq!(stderr(&output), expected.unwrap(), "{case}");
    }
}

#[test]
fn checks_a_compatible_schema_without_printing() {
    let output = tafla_gen(
        REPOSITORY,
        &[
            "check",
            "--against",
            "schemas/shapes.tafla",
            "schemas/shapes.tafla",
        ],
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn checks_nothing_when_a_schema_has_errors_or_cannot_be_read() {
    let output = tafla_gen(
        DIAGNOSTICS,
        &["check", "cycles.tafla", "--against", "unknown_types.tafla"],
    );
    let missing = tafla_gen(
        REPOSITORY,
        &[
            "check",
            "schemas/shapes.tafla",
            "--against",
            "missing.tafla",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let read = |name: &str| std::fs::read_to_string(Path::new(DIAGNOSTICS).join(name)).unwrap();
    assert_eq!(
        stderr(&output),
        format!(
            "{}\n{}",
            read("cycles.stderr"),
            read("unknown_types.stderr")
        )
    );
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(
        stderr(&missing),
        format!("missing.tafla: {}\n", not_found())
    );
}

#[test]
fn rejects_wrong_arguments_of_subcommands() {
    for (arguments, error) in [
        (&["describe"][..], "no schema is given"),
        (&["check", "--against", "a.tafla"], "no schema is given"),
        (
            &["check", "b.tafla"],
            "`check` needs `--against` and the schema to compare with",
        ),
        (
            &["check", "b.tafla", "c.tafla", "--against", "a.tafla"],
            "`check` compares one schema at a time",
        ),
        (
            &["describe", "a.tafla", "--out", "out"],
            "`--out` isn't an option of `describe`",
        ),
        (
            &["check", "b.tafla", "--against", "a.tafla", "--package", "p"],
            "`--package` isn't an option of `check`",
        ),
        (
            &["describe", "a.tafla", "--against", "b.tafla"],
            "`--against` is an option of `check`",
        ),
        (
            &["rust", "a.tafla", "--against", "b.tafla"],
            "`--against` is an option of `check`",
        ),
    ] {
        assert_usage_error(REPOSITORY, arguments, error);
    }
}
