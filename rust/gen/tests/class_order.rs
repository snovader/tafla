//! TypeScript and JavaScript declare each class after the classes that its
//! code reads, so that V8 reads those in its methods without a check of the
//! temporal dead zone, which keeps the methods' receivers on the heap. In a
//! cycle of tables, which passes through a vector, the vector's view comes
//! before its items.

use tafla_gen::{Options, Target, compile};

fn classes(target: &str, schema: &str) -> Vec<String> {
    let target = Target::from_name(target).unwrap();
    let files = compile("order.tafla", schema, target, &Options::default()).unwrap();

    files
        .iter()
        .flat_map(|file| file.contents.lines())
        .filter_map(|line| line.strip_prefix("export class "))
        .map(|rest| rest.trim_end_matches(" {").to_owned())
        .collect()
}

fn assert_order(schema: &str, expected: &[&str]) {
    for target in ["typescript", "javascript"] {
        assert_eq!(classes(target, schema), expected, "{target}");
    }
}

#[test]
fn nested_tables_come_before_the_tables_that_hold_them() {
    assert_order(
        "\
message ItemList {
    items: [Item];
}

table Item {
    value: float32;
    pair: Pair;
}

table Pair {
    first: float32;
    second: float32;
}
",
        &["Pair", "Item", "ItemVector", "ItemList"],
    );
}

#[test]
fn structs_and_arrays_of_structs_come_before_their_holders() {
    assert_order(
        "\
message Path {
    start: Point;
    points: [Point];
}

struct Point {
    x: float32;
    y: float32;
}
",
        &["Point", "PointArray", "Path"],
    );
}

#[test]
fn definitions_that_need_no_other_order_keep_the_schema_order() {
    assert_order(
        "\
message First {
    a: uint8;
}

message Second {
    b: uint8;
}
",
        &["First", "Second"],
    );
}

#[test]
fn a_vector_of_a_table_in_itself_comes_before_the_table() {
    assert_order(
        "\
message Forest {
    trees: [Tree];
}

table Tree {
    value: int32;
    children: [Tree];
}
",
        &["TreeVector", "Tree", "Forest"],
    );
}

#[test]
fn a_cycle_through_a_nested_table_breaks_at_the_vector() {
    assert_order(
        "\
message Root {
    a: A;
}

table A {
    b: B;
}

table B {
    as: [A];
}
",
        &["AVector", "B", "A", "Root"],
    );
}

// A table's static `size()` and `write()` read the items of its vectors,
// which, in a cycle, can come after it
#[test]
fn a_cycle_of_vectors_breaks_at_each_vector() {
    assert_order(
        "\
message Root {
    a: A;
}

table A {
    bs: [B];
}

table B {
    as: [A];
}
",
        &["BVector", "A", "Root", "AVector", "B"],
    );
}
