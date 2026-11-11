//! The library's `compile()`: where generated files go, what they import, and
//! targets by name.

use std::fmt::Write;

use tafla_gen::{GeneratedFile, JavaPackage, Options, Target, compile};

const SCHEMA: &str = "message Point { x: float64; pair: Pair; }\ntable Pair { a: uint8; }\n";

fn generate(file_name: &str, source: &str, target: &str, options: &Options) -> Vec<GeneratedFile> {
    let target = Target::from_name(target).unwrap();
    compile(file_name, source, target, options).unwrap()
}

fn paths(files: &[GeneratedFile]) -> Vec<&str> {
    files.iter().map(|file| file.path.as_str()).collect()
}

#[test]
fn imports_the_runtime_that_the_options_name() {
    for (target, runtime, import) in [
        ("typescript", "./tafla.ts", "} from './tafla.ts';\n"),
        ("javascript", "./tafla.js", "} from './tafla.js';\n"),
        (
            "python",
            "tables.tafla",
            "from tables.tafla import ByteBuffer",
        ),
        (
            "java",
            "org.example.tafla",
            "import org.example.tafla.ByteBuffer;\n",
        ),
        ("rust", "crate::tafla", "use crate::tafla::{"),
        ("cpp", "vendor/tafla.hpp", "\n#include <vendor/tafla.hpp>\n"),
    ] {
        let options = Options {
            runtime: Some(runtime.to_owned()),
            package: None,
        };

        for file in generate("point.tafla", SCHEMA, target, &options) {
            assert!(
                file.contents.contains(import),
                "{target}: {}",
                file.contents
            );
        }
    }
}

#[test]
fn imports_taflas_runtimes_by_default() {
    for (target, import) in [
        ("typescript", "} from 'tafla';\n"),
        ("javascript", "} from 'tafla';\n"),
        ("python", "from tafla import ByteBuffer"),
        ("java", "import io.github.snovader.tafla.ByteBuffer;\n"),
        ("rust", "use tafla::{"),
        ("cpp", "\n#include <tafla/tafla.hpp>\n"),
    ] {
        for file in generate("point.tafla", SCHEMA, target, &Options::default()) {
            assert!(
                file.contents.contains(import),
                "{target}: {}",
                file.contents
            );
        }
    }
}

/// The Rust module's `use` statement of the runtime
fn rust_import(runtime: &str) -> String {
    let options = Options {
        runtime: Some(runtime.to_owned()),
        package: None,
    };
    let files = generate("point.tafla", SCHEMA, "rust", &options);
    let contents = &files[0].contents;

    let start = contents.find(&format!("use {runtime}::")).unwrap();
    let end = start + contents[start..].find(";\n").unwrap();
    contents[start..=end].to_owned()
}

#[test]
fn rust_imports_break_where_rustfmt_breaks_them() {
    // rustfmt keeps a `use` statement with a list on one line up to 98
    // columns, and breaks one of 99 into a line for each brace
    let width_without_runtime = rust_import("r").len() - 1;

    for (width, lines) in [(98, 1), (99, 3)] {
        let runtime = "r".repeat(width - width_without_runtime);
        let import = rust_import(&runtime);
        assert_eq!(import.lines().count(), lines, "{import}");
    }
}

#[test]
fn java_classes_are_in_the_generated_package_by_default() {
    let files = generate("point.tafla", SCHEMA, "java", &Options::default());

    assert_eq!(
        paths(&files),
        ["generated/Point.java", "generated/Pair.java"]
    );
    for file in &files {
        assert!(file.contents.contains("\npackage generated;\n"));
    }
}

#[test]
fn java_classes_are_in_the_package_that_the_options_name() {
    let options = Options {
        runtime: None,
        package: Some("org.example.tables".parse().unwrap()),
    };

    let files = generate("point.tafla", SCHEMA, "java", &options);

    assert_eq!(
        paths(&files),
        [
            "org/example/tables/Point.java",
            "org/example/tables/Pair.java"
        ]
    );
    for file in &files {
        assert!(file.contents.contains("\npackage org.example.tables;\n"));
    }
}

#[test]
fn other_targets_ignore_the_package() {
    let options = Options {
        runtime: None,
        package: Some("org.example.tables".parse().unwrap()),
    };

    for target in ["typescript", "javascript", "python", "rust"] {
        let with_package = generate("point.tafla", SCHEMA, target, &options);
        let without = generate("point.tafla", SCHEMA, target, &Options::default());
        assert_eq!(with_package, without, "{target}");
    }
}

#[test]
fn the_cpp_namespace_is_the_package_or_the_schemas_name() {
    let namespace = |file_name: &str, package: Option<&str>| {
        let options = Options {
            runtime: None,
            package: package.map(|package| package.parse().unwrap()),
        };
        let files = generate(file_name, SCHEMA, "cpp", &options);
        let contents = &files[0].contents;

        let start = contents.find("\nnamespace ").unwrap() + "\nnamespace ".len();
        let end = start + contents[start..].find(" {\n").unwrap();
        contents[start..end].to_owned()
    };

    assert_eq!(
        namespace("point.tafla", Some("org.example.tables")),
        "org::example::tables"
    );
    assert_eq!(
        namespace("point.tafla", Some("std.tafla.delete")),
        "std_::tafla_::delete_"
    );
    for (file_name, expected) in [
        ("schemas/point_cloud.v1.tafla", "point_cloud"),
        ("point-cloud.tafla", "point_cloud"),
        ("my--points.tafla", "my_points"),
        ("1st.tafla", "schema_1st"),
        ("_points.tafla", "schema_points"),
        ("tafla.tafla", "tafla_"),
        ("namespace.tafla", "namespace_"),
        ("errno.tafla", "errno_"),
        ("punktwolke_ä.tafla", "punktwolke_"),
    ] {
        assert_eq!(namespace(file_name, None), expected, "{file_name}");
    }
}

#[test]
fn a_java_package_is_identifiers_separated_by_dots() {
    for name in ["generated", "org.example.tables", "a1.$b._c", "org.Example"] {
        let package: JavaPackage = name.parse().unwrap();
        assert_eq!(package.as_str(), name);
    }
}

#[test]
fn a_java_package_names_no_other_directory_than_its_own() {
    for name in [
        "",
        ".generated",
        "generated.",
        "org..example",
        "/generated",
        "../generated",
        "org/example",
        r"org\example",
        "C:generated",
        "my-tables",
        "1st",
        "org.class.tables",
        "_",
        "éxample",
    ] {
        let error = name.parse::<JavaPackage>().unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "`{name}` isn't a Java package, which is identifiers in ASCII separated by \
                 dots, such as `com.example.shapes`"
            )
        );
    }
}

#[test]
fn files_are_named_after_the_schema_and_name_it() {
    let file_name = "schemas/v1/point_cloud.v1.tafla";

    for (target, path) in [
        ("typescript", "pointCloud.ts"),
        ("javascript", "pointCloud.js"),
        ("python", "point_cloud.py"),
        ("rust", "point_cloud.rs"),
        ("cpp", "point_cloud.hpp"),
    ] {
        let files = generate(file_name, SCHEMA, target, &Options::default());

        assert_eq!(paths(&files), [path]);
        assert!(
            files[0].contents.contains(
                "Generated by tafla-gen from schemas/v1/point_cloud.v1.tafla. Do not edit."
            )
        );
    }
}

#[test]
fn files_are_named_after_the_schema_without_its_windows_directory() {
    for file_name in [
        r"schemas\v1\point_cloud.v1.tafla",
        r"C:\schemas\point_cloud.tafla",
        "C:point_cloud.tafla",
    ] {
        for (target, path) in [
            ("typescript", "pointCloud.ts"),
            ("python", "point_cloud.py"),
            ("rust", "point_cloud.rs"),
            ("cpp", "point_cloud.hpp"),
        ] {
            let files = generate(file_name, SCHEMA, target, &Options::default());
            assert_eq!(paths(&files), [path], "{file_name}");
        }
    }
}

#[test]
fn a_header_names_any_path_in_one_comment() {
    // A Windows directory that Java would read as a Unicode escape, a newline
    // that would end the comment, and a control of the direction of text,
    // which rustc rejects in comments
    let file_name = "schemas\\util\\a\nimport os\u{202E}\\point.tafla";
    let shown = "schemas\\util\\a\u{FFFD}import os\u{FFFD}\\point.tafla";

    for (target, comment, shown) in [
        ("typescript", "//", shown.to_owned()),
        ("javascript", "//", shown.to_owned()),
        ("python", "#", shown.to_owned()),
        ("java", "//", shown.replace("\\u", "\\u005cu")),
        ("rust", "//", shown.to_owned()),
        ("cpp", "//", shown.to_owned()),
    ] {
        for file in generate(file_name, SCHEMA, target, &Options::default()) {
            let header = file.contents.lines().next().unwrap();
            assert_eq!(
                header,
                format!("{comment} Generated by tafla-gen from {shown}. Do not edit."),
                "{target}"
            );
        }
    }
}

#[test]
fn a_schema_without_definitions_generates_modules_without_imports() {
    let source = "// Nothing yet\n";
    let header = "Generated by tafla-gen from empty.tafla. Do not edit.";

    for (target, contents) in [
        ("typescript", format!("// {header}\n")),
        ("javascript", format!("// {header}\n")),
        ("python", format!("# {header}\n")),
        ("rust", format!("// {header}\n")),
        ("cpp", format!("// {header}\n")),
    ] {
        let files = generate("empty.tafla", source, target, &Options::default());
        assert_eq!(files[0].contents, contents, "{target}");
    }
    // Java has a file per definition, so none for an empty schema
    let files = generate("empty.tafla", source, "java", &Options::default());
    assert_eq!(files, Vec::new());
}

#[test]
fn tables_nested_in_many_fields_of_each_other_generate() {
    // Each table nests the next in three fields, so a generator that walks
    // every path through them would take 3^40 steps
    let mut source = String::new();
    for index in 0..40 {
        let next = index + 1;
        writeln!(
            source,
            "table Level{index} {{ a: Level{next}; b: Level{next}; c: Level{next}; }}"
        )
        .unwrap();
    }
    source.push_str("table Level40 { value: uint8; }\n");

    for name in Target::names() {
        let files = generate("levels.tafla", &source, name, &Options::default());
        assert_ne!(files, Vec::new(), "{name}");
    }
}

#[test]
fn targets_are_found_by_name_or_abbreviation() {
    let names: Vec<&str> = Target::names().collect();
    assert_eq!(
        names,
        ["typescript", "javascript", "python", "java", "rust", "cpp"]
    );

    for (abbreviation, name) in [
        ("ts", "typescript"),
        ("js", "javascript"),
        ("py", "python"),
        ("rs", "rust"),
    ] {
        assert_eq!(Target::from_name(abbreviation), Target::from_name(name));
    }
    assert_eq!(Target::from_name("TypeScript"), None);
    assert_eq!(Target::from_name("go"), None);
}

#[test]
fn empty_attributes_give_the_default_layout() {
    let with_attributes = "message Point () { x: uint8; }";
    let without = "message Point { x: uint8; }";

    for name in Target::names() {
        assert_eq!(
            generate("point.tafla", with_attributes, name, &Options::default()),
            generate("point.tafla", without, name, &Options::default()),
            "{name}"
        );
    }
}
