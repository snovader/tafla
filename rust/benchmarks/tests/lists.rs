//! The lists suite's shapes are the TypeScript suite's, whose strings these
//! were printed from, so that the languages time the same work, and each of
//! its codecs reads back what it wrote

use std::collections::HashSet;

use tafla::{TableBuilder, TableLayout};
use tafla_benchmarks::for_each_list_codec;
use tafla_benchmarks::lists::{self, COUNTS, ListCodec, TaflaGenStrings, list_shapes};

/// Each shape's name, count, and the start of its first string, the start
/// of the string at half its count, and the end of its last, as TypeScript's
/// `shapes.ts` makes them
const TYPESCRIPT: [(&str, usize, &str, &str, &str); 9] = [
    (
        "10 × 16 chars of ASCII",
        10,
        "2FsyeXxp1ewxJijA",
        "F4x68tnjpw96TrsH",
        "CcfRsv23GwWuZIqm",
    ),
    (
        "10 × 256 chars of ASCII",
        10,
        "2GYBM2rvPSVojjhNm2gyRuqB",
        "ougk7NU4vvXFq6xdOmgnfvsn",
        "FfuwjY0KIhN6s9a8gdduArZS",
    ),
    (
        "10 × 8 chars of 3 bytes",
        10,
        "鑡絞眙蜩榩眳矝愞",
        "朥兄霈郑浙縳辛痁",
        "琑翘趠谎衬耂竎刓",
    ),
    (
        "100 × 16 chars of ASCII",
        100,
        "2N45gJLJhyrBp817",
        "VUGfD2AupqXxQzfI",
        "GuM8hMtpdCgpm337",
    ),
    (
        "100 × 256 chars of ASCII",
        100,
        "2OC9KSUHzW0k790oiuLHZp5H",
        "gAjy2fcw9X5QV8auS48ih5UG",
        "APQeHcpfAoeTloKTbgWtfA2R",
    ),
    (
        "100 × 8 chars of 3 bytes",
        100,
        "鑗蝝姘徃栲鈞甕顽",
        "慤璛娂份纉睃行磄",
        "鱵髩痧剻熉五憓荜",
    ),
    (
        "10,000 × 16 chars of ASCII",
        10_000,
        "2VGBSOLG6JrrCjwD",
        "P2nj9r4wyrMNAZm7",
        "FzBrmc8Y0jcGbWah",
    ),
    (
        "10,000 × 256 chars of ASCII",
        10_000,
        "2WgxnHUMGp0eUivRrOyb04Qa",
        "3yYTVq3YMNykH6HwUvEnb6py",
        "5y4csgj8XOGtNlXbtpkzqZR0",
    ),
    (
        "10,000 × 8 chars of 3 bytes",
        10_000,
        "鑶鄷攴臚鷑靆畖鱯",
        "銆牉楑快鮒喙傾螩",
        "淟坓惗聳侞忝鞂姩",
    ),
];

#[test]
fn generates_the_typescript_lists_of_distinct_strings() {
    let shapes = list_shapes();
    assert_eq!(shapes.len(), TYPESCRIPT.len());
    assert_eq!(COUNTS.len() * 3, shapes.len());

    for (shape, (name, count, first, middle, last)) in shapes.iter().zip(TYPESCRIPT) {
        let strings = &shape.strings;
        assert_eq!(shape.name, name);
        assert_eq!(strings.len(), count, "{name}");
        assert!(strings[0].starts_with(first), "{name}: {}", strings[0]);
        assert!(strings[count / 2].starts_with(middle), "{name}");
        assert!(strings[count - 1].ends_with(last), "{name}");

        let lengths: HashSet<usize> = strings
            .iter()
            .map(|string| string.chars().count())
            .collect();
        assert_eq!(lengths.len(), 1, "{name}: strings of one length");
        let distinct: HashSet<&String> = strings.iter().collect();
        assert_eq!(distinct.len(), count, "{name}: distinct strings");
    }
}

#[test]
fn every_codec_reads_back_every_list() {
    for shape in list_shapes() {
        for_each_list_codec!(|codec, name| {
            let checked = lists::check(codec, &shape);
            assert!(checked.is_ok(), "{name}, {}: {checked:?}", shape.name);
        });
    }
}

#[test]
fn the_vector_of_strings_is_the_table_that_table_builder_writes() {
    for shape in list_shapes() {
        let strings = shape.strs();
        let mut builder = TableBuilder::new(1, TableLayout::DEFAULT).unwrap();
        builder.set_str_vector(strings.as_slice()).unwrap();

        let written = TaflaGenStrings.serialize(&strings).unwrap();
        assert!(written == builder.to_bytes().unwrap(), "{}", shape.name);
    }
}
