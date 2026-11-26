//! The `tafla-gen` binary: it writes the generated files under `--out`,
//! prints errors as the library renders them, and fails on wrong arguments
//! with exit code 2 and on a schema's errors with 1. `decode` and `encode`
//! read a table from a file or stdin and print its JSON or its bytes.

mod common;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use common::{DIAGNOSTICS, Golden, REPOSITORY, read_tree, targets};

fn tafla_gen(directory: &str, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tafla-gen"))
        .current_dir(directory)
        .args(arguments)
        .output()
        .unwrap()
}

/// Runs the command with `stdin` as its standard input
fn tafla_gen_with_stdin(directory: &str, arguments: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tafla-gen"))
        .current_dir(directory)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
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

const USAGE: &str =
    "usage: tafla-gen <typescript|javascript|python|java|rust|go|cpp|swift|csharp> <schema>... \
                     [--out <dir>] [--runtime <module>] [--package <package>]
       tafla-gen decode <schema> <name> [<file>] [--layout] [--any-version] [--ascii]
       tafla-gen encode <schema> <name> [<file>]";

/// Fails as wrong arguments do: with the error, the usage and exit code 2,
/// without writing a file
fn assert_usage_error(directory: &str, arguments: &[&str], error: &str) {
    let out = tempfile::tempdir().unwrap();
    let mut arguments = arguments.to_vec();
    if !arguments.contains(&"--out") {
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
        (
            &["rust", "-"],
            "a schema is read from a file, not from stdin, `-`",
        ),
        (
            &["rust", "schemas/shapes.tafla", "--layout"],
            "`--layout` is an option of `decode`",
        ),
    ] {
        assert_usage_error(REPOSITORY, arguments, error);
    }
}

#[test]
fn rejects_wrong_arguments_of_decode_and_encode() {
    for (arguments, error) in [
        (&["decode"][..], "no schema is given"),
        (
            &["decode", "schemas/shapes.tafla"],
            "no message or struct is given, by its name in the schema",
        ),
        (
            &[
                "encode",
                "schemas/shapes.tafla",
                "Point",
                "a.json",
                "b.json",
            ],
            "`encode` reads one file, `a.json`, and `b.json` is another",
        ),
        (
            &["decode", "schemas/shapes.tafla", "Point", "--out", "out"],
            "`decode` doesn't take `--out`",
        ),
        (
            &["encode", "schemas/shapes.tafla", "Point", "--ascii"],
            "`encode` doesn't take `--ascii`",
        ),
        (
            &["decode", "-", "Point", "point.bin"],
            "a schema is read from a file, not from stdin, `-`",
        ),
        (
            &["decode", "--layout", "--layout"],
            "`--layout` is given twice",
        ),
    ] {
        let output = tafla_gen(REPOSITORY, arguments);

        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        assert_eq!(
            stderr(&output),
            format!("error: {error}\n{USAGE}\nRun `tafla-gen --help` for more.\n"),
            "{arguments:?}"
        );
    }
}

const POINT: &str = "../conformance/generated/shapes/point.bin";

#[test]
fn decodes_a_table_from_a_file_or_stdin_and_encodes_it_back() {
    let bytes = std::fs::read(Path::new(REPOSITORY).join(&POINT[3..])).unwrap();
    let directory = format!("{REPOSITORY}/schemas");

    let from_file = tafla_gen(&directory, &["decode", "shapes.tafla", "Point", POINT]);
    let from_stdin = tafla_gen_with_stdin(&directory, &["decode", "shapes.tafla", "Point"], &bytes);
    let from_dash = tafla_gen_with_stdin(
        &directory,
        &["decode", "shapes.tafla", "Point", "-"],
        &bytes,
    );

    assert!(from_file.status.success(), "{}", stderr(&from_file));
    assert_eq!(stderr(&from_file), "");
    let json = String::from_utf8(from_file.stdout).unwrap();
    assert_eq!(
        json,
        "{\n    \"x\": \"-0\",\n    \"y\": \"NaN\",\n    \"id\": \"-9223372036854775808\",\n    \
         \"name\": \"a\u{e5}\u{4e16}\u{1f980}\"\n}\n"
    );
    assert_eq!(from_stdin.stdout, json.as_bytes());
    assert_eq!(from_dash.stdout, json.as_bytes());

    let encoded = tafla_gen_with_stdin(
        &directory,
        &["encode", "shapes.tafla", "Point"],
        json.as_bytes(),
    );
    assert!(encoded.status.success(), "{}", stderr(&encoded));
    assert_eq!(encoded.stdout, bytes);

    let ascii = tafla_gen(
        &directory,
        &["decode", "--ascii", "shapes.tafla", "Point", POINT],
    );
    assert!(
        String::from_utf8(ascii.stdout)
            .unwrap()
            .contains("\"a\\u00e5\\u4e16\\ud83e\\udd80\"")
    );
}

#[test]
fn prints_the_layout_of_a_table() {
    let output = tafla_gen(
        REPOSITORY,
        &[
            "decode",
            "schemas/shapes.tafla",
            "Sample",
            "conformance/generated/shapes/sample.bin",
            "--layout",
        ],
    );

    assert!(output.status.success(), "{}", stderr(&output));
    let layout = String::from_utf8(output.stdout).unwrap();
    assert!(
        layout.starts_with(
            "position    length  bytes                      field  value\n       \
             0         4  "
        ),
        "{layout}"
    );
    assert!(layout.contains("  key    uint32 "), "{layout}");
}

#[test]
fn fails_on_a_table_that_isnt_the_message_and_prints_its_layout_up_to_the_failure() {
    let truncated = &std::fs::read(Path::new(REPOSITORY).join(&POINT[3..])).unwrap()[..30];
    let arguments = ["decode", "schemas/shapes.tafla", "Point"];

    let output = tafla_gen_with_stdin(REPOSITORY, &arguments, truncated);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        stderr(&output),
        "error: <stdin> isn't a `Point`: `y`: Read exceeds the supplied buffer: 8 bytes at 24 of 30\n"
    );

    let output = tafla_gen_with_stdin(
        REPOSITORY,
        &[&arguments[..], &["--layout"]].concat(),
        truncated,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stdout.clone())
            .unwrap()
            .contains("  x      float64 -0\n")
    );
    assert!(stderr(&output).starts_with("error: <stdin> isn't a `Point`: `y`: "));
}

#[test]
fn reads_another_version_only_with_any_version() {
    let mut bytes =
        std::fs::read(Path::new(REPOSITORY).join("conformance/generated/shapes/document.bin"))
            .unwrap();
    bytes[4] = 2;
    let arguments = ["decode", "schemas/shapes.tafla", "Document"];

    let output = tafla_gen_with_stdin(REPOSITORY, &arguments, &bytes);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "error: <stdin> isn't a `Document`: Unsupported format version 2; the reader reads \
         version 1\n  = help: `--any-version` reads it whatever its version\n"
    );

    let output = tafla_gen_with_stdin(
        REPOSITORY,
        &[&arguments[..], &["--any-version"]].concat(),
        &bytes,
    );
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn prints_the_errors_of_json_that_doesnt_encode() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("point.json"),
        r#"{"x": 1, "y": true}"#,
    )
    .unwrap();
    std::fs::write(
        directory.path().join("latin1.json"),
        b"{\"name\": \"caf\xe9\"}",
    )
    .unwrap();
    let schema = format!("{REPOSITORY}/schemas/shapes.tafla");
    let directory = directory.path().to_str().unwrap();

    let output = tafla_gen(directory, &["encode", &schema, "Point", "point.json"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        stderr(&output),
        "\
error: `y` isn't a float64
 --> point.json:1:15
  |
1 | {\"x\": 1, \"y\": true}
  |               ^^^^ a float64 is a number within its range, or \"NaN\", \"Infinity\", \"-Infinity\" or \"-0\"

error: missing the fields `id` and `name`
 --> point.json:1:1
  |
1 | {\"x\": 1, \"y\": true}
  | ^ every field is required
"
    );

    let output = tafla_gen(directory, &["encode", &schema, "Point", "latin1.json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).starts_with("error: the JSON isn't UTF-8\n --> latin1.json:1:14\n"));

    let output = tafla_gen(directory, &["encode", &schema, "Point", "missing.json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).starts_with("missing.json: "));
}

#[test]
fn names_the_messages_and_structs_of_a_schema_for_another_name() {
    let output = tafla_gen_with_stdin(
        REPOSITORY,
        &["decode", "schemas/shapes.tafla", "Columns"],
        b"",
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "error: `Columns` is a table, which is read within a message, not on its own\n \
         --> schemas/shapes.tafla\n  = help: the schema's messages and structs are `Point`, \
         `Document`, `ItemList` and `Sample`\n"
    );
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
