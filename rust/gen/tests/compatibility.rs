//! The compatibility of a schema with the version of it before: each pair of
//! schemas in `tests/compatibility`, `<case>.old.tafla` and `<case>.tafla`,
//! is compared with the errors and notes in `<case>.stderr`, rendered as
//! `tafla-gen check` prints them, and the rules hold for each direction in
//! which a reader of one version reads the other's tables.
//! `TAFLA_GEN_UPDATE=1` rewrites the `.stderr` files.

mod common;

use std::path::Path;

use common::{assert_no_differences, compare_file, example_schemas, files_with_extension, update};
use tafla_gen::{Comparison, Severity, compatibility};

const COMPATIBILITY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/compatibility");

#[test]
fn changed_schemas_fail_with_the_expected_errors_and_notes() {
    let mut differences = Vec::new();

    for (name, path) in new_schemas() {
        let new = std::fs::read_to_string(&path).unwrap();
        let old = std::fs::read_to_string(path.with_extension("old.tafla")).unwrap();

        let comparison = compatibility(&old, &new).unwrap();
        let rendered = comparison.render(&format!("{name}.tafla"), &new);
        differences.extend(compare_file(&path.with_extension("stderr"), &rendered));
    }

    assert_no_differences(&differences);
}

#[test]
fn every_case_has_an_old_schema_and_expected_output() {
    let directory = Path::new(COMPATIBILITY);
    let cases: Vec<String> = new_schemas().into_iter().map(|(name, _)| name).collect();

    let old: Vec<String> = files_with_extension(directory, "tafla")
        .into_iter()
        .filter_map(|(name, _)| name.strip_suffix(".old").map(str::to_owned))
        .collect();
    assert_eq!(old, cases);

    let stale: Vec<_> = files_with_extension(directory, "stderr")
        .into_iter()
        .filter(|(name, _)| !cases.contains(name))
        .map(|(_, path)| path)
        .collect();
    if update() {
        for path in &stale {
            std::fs::remove_file(path).unwrap();
        }
        return;
    }
    assert!(stale.is_empty(), "outputs without a case: {stale:?}");
}

/// The new schema of each case, by the case's name
fn new_schemas() -> Vec<(String, std::path::PathBuf)> {
    files_with_extension(Path::new(COMPATIBILITY), "tafla")
        .into_iter()
        .filter(|(name, _)| name.strip_suffix(".old").is_none())
        .collect()
}

#[test]
fn every_example_schema_is_compatible_with_itself() {
    for schema in example_schemas() {
        let source = &schema.source;

        let comparison = compatibility(source, source).unwrap();

        assert_eq!(comparison.diagnostics, [], "{}", schema.name);
        assert_eq!(comparison.undeclared, Vec::<String>::new());
    }
}

/// The messages of each diagnostic, with `error: ` or `note: ` before them
fn messages(comparison: &Comparison) -> Vec<String> {
    comparison
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let severity = match diagnostic.severity {
                Severity::Error => "error",
                Severity::Note => "note",
            };
            format!("{severity}: {}", diagnostic.message)
        })
        .collect()
}

fn compare(old: &str, new: &str) -> Comparison {
    compatibility(old, new).unwrap()
}

#[test]
fn a_field_appended_in_a_new_version_is_compatible() {
    let old = "message Point (version: 1) { x: float64; }\n\
               message PointV0 (version: 0) { x: float32; }";
    let new = "message PointV1 (version: 1) { x: float64; }\n\
               message Point (version: 2) { x: float64; y: float64; }\n\
               message PointV0 (version: 0) { x: float32; }";

    let comparison = compare(old, new);

    assert_eq!(messages(&comparison), Vec::<String>::new());
    assert!(!comparison.has_errors());
}

#[test]
fn a_new_version_without_a_reader_of_the_old_one_is_a_note() {
    // A reader of version 2 rejects the tables of version 1 with its
    // decode error, rather than misreading them
    let comparison = compare(
        "message Point (version: 1) { x: float64; }",
        "message Point (version: 2) { x: float64; y: float64; }",
    );

    assert_eq!(
        messages(&comparison),
        ["note: no message reads version 1 of `Point` any more"]
    );
    assert!(!comparison.has_errors());
}

#[test]
fn a_reader_of_the_old_version_must_have_its_fields() {
    // `PointV1` has the old version's layout, but not its fields
    let comparison = compare(
        "message Point (version: 1) { x: float64; }",
        "message PointV1 (version: 1) { x: float32; }\n\
         message Point (version: 2) { x: float64; y: float64; }",
    );

    assert_eq!(
        messages(&comparison),
        ["note: no message reads version 1 of `Point` any more"]
    );
}

#[test]
fn a_field_appended_in_the_same_version_misreads_the_old_tables() {
    // A reader of the new fields reads the old table's missing offset from
    // the first value's bytes
    let comparison = compare(
        "message Point (version: 1) { x: float64; }",
        "message Point (version: 1) { x: float64; y: float64; }",
    );

    assert_eq!(
        messages(&comparison),
        ["error: `Point` changes, and keeps version 1"]
    );
    assert!(comparison.has_errors());
}

#[test]
fn a_message_without_a_version_changes_nothing() {
    let comparison = compare(
        "message Point { x: float64; }",
        "message Point { x: float64; y: float64; }",
    );

    assert_eq!(
        messages(&comparison),
        ["error: `Point` changes, and has no version"]
    );
}

#[test]
fn a_field_removed_misreads_the_new_tables() {
    // A reader of the old fields reads the offset of `y` from the bytes of
    // `x`'s value
    let comparison = compare(
        "message Point (version: 1) { x: float64; y: float64; }",
        "message Point (version: 2) { x: float64; }",
    );

    assert_eq!(
        messages(&comparison),
        [
            "error: `Point` loses its field `y`",
            "note: no message reads version 1 of `Point` any more"
        ]
    );
}

#[test]
fn no_scalar_type_changes_into_another() {
    let scalars = [
        "bool", "int8", "uint8", "int16", "uint16", "int32", "uint32", "int64", "uint64",
        "float32", "float64",
    ];

    for old in scalars {
        for new in scalars.iter().filter(|&&new| new != old) {
            let comparison = compare(
                &format!("message Value (version: 1) {{ value: {old}; }}"),
                &format!(
                    "message Value (version: 2) {{ value: {new}; }}\nmessage ValueV1 (version: 1) {{ value: {old}; }}"
                ),
            );

            assert_eq!(
                messages(&comparison),
                [format!(
                    "error: `value` changes its type from `{old}` to `{new}`"
                )]
            );
        }
    }
}

#[test]
fn a_string_is_not_an_array_of_bytes() {
    // An array of bytes needn't be UTF-8, which a string's reader rejects
    for (old, new) in [("string", "[uint8]"), ("[uint8]", "string")] {
        let comparison = compare(
            &format!("message Text {{ value: {old}; }}"),
            &format!("message Text {{ value: {new}; }}"),
        );

        assert!(comparison.has_errors(), "{old} to {new}");
    }
}

#[test]
fn a_renamed_field_is_a_note() {
    let comparison = compare(
        "message Point { x: float64; y: float64; }",
        "message Point { x: float64; height: float64; }",
    );

    assert_eq!(messages(&comparison), ["note: field 1 was named `y`"]);
    assert!(!comparison.has_errors());
}

#[test]
fn a_field_of_another_type_of_the_same_bytes_is_compatible() {
    // Fields are compared by their bytes, whatever names their types have
    let comparison = compare(
        "message Shape { origin: Point; corner: Pair; }\n\
         table Point { x: float64; y: float64; }\n\
         struct Pair { a: int32; b: int32; }",
        "message Shape { origin: Origin; corner: Corner; }\n\
         table Origin { x: float64; y: float64; }\n\
         struct Corner { a: int32; b: int32; }",
    );

    assert_eq!(messages(&comparison), Vec::<String>::new());
    assert_eq!(comparison.undeclared, ["Point", "Pair"]);
}

#[test]
fn a_nested_table_appended_to_changes_the_version_of_its_messages() {
    let old = "message Tree (version: 1) { root: Node; }\n\
               table Node { value: int32; children: [Node]; }";

    let kept = compare(
        old,
        "message Tree (version: 1) { root: Node; }\n\
         table Node { value: int32; children: [Node]; label: string; }",
    );
    let bumped = compare(
        old,
        "message TreeV1 (version: 1) { root: NodeV1; }\n\
         table NodeV1 { value: int32; children: [NodeV1]; }\n\
         message Tree (version: 2) { root: Node; }\n\
         table Node { value: int32; children: [Node]; label: string; }",
    );

    assert_eq!(
        messages(&kept),
        ["error: `Tree` changes, and keeps version 1"]
    );
    assert_eq!(
        kept.diagnostics[0].help.as_deref(),
        Some(
            "`Node` changes, which `root` holds, and only the version tells its old tables from its new ones"
        )
    );
    assert_eq!(messages(&bumped), Vec::<String>::new());
}

#[test]
fn a_change_deep_in_a_vector_reaches_every_message_that_holds_it() {
    let old = "message Catalog { items: [Item]; }\n\
               table Item { inner: Inner; }\n\
               table Inner { more: [Leaf]; }\n\
               table Leaf { value: uint8; }";
    let new = "message Catalog { items: [Item]; }\n\
               table Item { inner: Inner; }\n\
               table Inner { more: [Leaf]; }\n\
               table Leaf { value: uint8; weight: uint8; }";

    let comparison = compare(old, new);

    assert_eq!(
        messages(&comparison),
        ["error: `Catalog` changes, and has no version"]
    );
}

#[test]
fn a_struct_appended_to_at_the_end_of_a_table_changes_its_version() {
    let old = "message Path (version: 1) { last: Vec2; }\n\
               struct Vec2 { x: float32; y: float32; }";
    let new = "message Path (version: 2) { last: Vec2; }\n\
               message PathV1 (version: 1) { last: Vec2V1; }\n\
               struct Vec2 { x: float32; y: float32; z: float32; }\n\
               struct Vec2V1 { x: float32; y: float32; }";

    assert_eq!(messages(&compare(old, new)), Vec::<String>::new());
}

#[test]
fn a_struct_read_on_its_own_changes_nothing() {
    let comparison = compare(
        "struct Sample { time: uint64; }",
        "struct Sample { time: uint64; value: float32; }",
    );

    assert_eq!(
        messages(&comparison),
        ["error: `Sample` grows, and no message holds it"]
    );
}

#[test]
fn an_array_of_structs_keeps_their_size() {
    let comparison = compare(
        "message Path (version: 1) { points: [Vec2]; }\n\
         struct Vec2 { x: float32; y: float32; }",
        "message Path (version: 2) { points: [Vec2]; }\n\
         message PathV1 (version: 1) { points: [Vec2V1]; }\n\
         struct Vec2 { x: float32; y: float32; z: float32; }\n\
         struct Vec2V1 { x: float32; y: float32; }",
    );

    assert_eq!(
        messages(&comparison),
        ["error: `points` is an array of `Vec2`, whose size changes"]
    );
}

#[test]
fn a_struct_in_a_struct_keeps_its_size() {
    let comparison = compare(
        "message Path { segment: Segment; }\n\
         struct Segment { start: Vec2; end: Vec2; }\n\
         struct Vec2 { x: float32; y: float32; }",
        "message Path { segment: Segment; }\n\
         struct Segment { start: Vec2; end: Vec2; }\n\
         struct Vec2 { x: float32; y: float32; z: float32; }",
    );

    assert_eq!(
        messages(&comparison),
        [
            "error: `Path` changes, and has no version",
            "error: `Vec2` grows, which moves the fields after it in `Segment`"
        ]
    );
}

#[test]
fn declarations_are_matched_by_name() {
    let comparison = compare(
        "message Point { x: float64; }\nmessage Gone { x: uint8; }",
        "message Location { x: float64; }\nmessage Point { x: float64; }",
    );

    assert_eq!(messages(&comparison), Vec::<String>::new());
    assert_eq!(comparison.undeclared, ["Gone"]);
    assert_eq!(
        comparison.render("point.tafla", ""),
        "note: `Gone` isn't declared in point.tafla, so nothing is compared with it\n"
    );
}

#[test]
fn a_table_and_a_message_of_the_default_layout_have_the_same_bytes() {
    let comparison = compare(
        "table Point { x: float64; }",
        "message Point { x: float64; }",
    );

    assert_eq!(messages(&comparison), Vec::<String>::new());
}

#[test]
fn schemas_with_errors_are_not_compared() {
    let valid = "message Point { x: float64; }";
    let invalid = "message Point { x: flaot64; }";

    let both = compatibility(invalid, invalid).unwrap_err();
    let new = compatibility(valid, invalid).unwrap_err();
    let old = compatibility(invalid, valid).unwrap_err();

    assert_eq!((both.old.len(), both.new.len()), (1, 1));
    assert_eq!((new.old.len(), new.new.len()), (0, 1));
    assert_eq!((old.old.len(), old.new.len()), (1, 0));
    assert_eq!(new.new[0].message, "unknown type `flaot64`");
}
