//! The strings comparison's workloads: tables of 16 distinct strings of one
//! shape and nothing else, so that a call spends most of its time on their
//! UTF-8, as README.md describes.
//!
//! The generator ports the TypeScript strings suite's,
//! `typescript/benchmarks/strings/shapes.ts`, with its seeds, its alphabets
//! and its lengths in characters, so that the shapes that both suites have
//! hold the same strings. The shapes that the C++ suite has too,
//! `cpp/benchmarks/src/string_workloads.hpp`, take its seeds, and hold its
//! strings. `tests/string_workloads.rs` checks every shape against a hash of
//! the bytes that the TypeScript generator made of it.

use std::borrow::Cow;

use rkyv::with::{AsOwned, Map};

use crate::workloads::{CAPACITY_OVERHEAD, Shape, Xorshift};

/// The strings of each table
pub const COUNT: usize = 16;

const ASCII: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
/// Latin-1 letters, of two bytes in UTF-8
const TWO_BYTES: &str = "àáâãäåæçèéêëìíîïñòóôõöøùúûüý";
/// The first of the CJK ideographs, of three bytes in UTF-8
const THREE_BYTES: u32 = 0x4e00;
/// The first of the emoji, of four bytes in UTF-8
const FOUR_BYTES: u32 = 0x1_f600;

/// A table's 16 strings, as every codec writes and reads them.
///
/// Codecs write them from borrowed input, and read them back borrowed from
/// the bytes where they read them in place, and copied otherwise. rkyv
/// archives them through its derive, as an application's own type.
#[derive(Clone, Debug, PartialEq, Eq, rkyv::Archive, rkyv::Serialize)]
pub struct Strings<'a> {
    #[rkyv(with = Map<AsOwned>)]
    pub items: [Cow<'a, str>; COUNT],
}

/// The shape of [`Strings`]
pub struct StringsShape;

impl Shape for StringsShape {
    const NAME: &'static str = "strings";
    type Value<'a> = Strings<'a>;

    fn same(left: &Strings<'_>, right: &Strings<'_>) -> bool {
        left.items == right.items
    }

    /// The strings' bytes, and room for the length, offset, terminator and
    /// padding of each, of up to 12 bytes in flatbuffers
    fn capacity(value: &Strings<'_>) -> usize {
        let bytes: usize = value.items.iter().map(|string| string.len()).sum();
        CAPACITY_OVERHEAD + bytes + COUNT * 16
    }
}

/// One shape's strings, of the same length and mix of UTF-8 widths
pub struct StringWorkload {
    /// The shape's name, which gives its length in characters and its mix
    pub name: &'static str,
    pub strings: [String; COUNT],
}

impl StringWorkload {
    /// The UTF-8 length of each string, which every string of a shape has
    pub const fn bytes(&self) -> usize {
        self.strings[0].len()
    }

    /// The strings, borrowed, as the codecs write them
    pub fn input(&self) -> Strings<'_> {
        Strings {
            items: self
                .strings
                .each_ref()
                .map(|string| Cow::Borrowed(string.as_str())),
        }
    }
}

/// Every shape, by the length of each string in bytes.
///
/// Beside 16 empty strings, which time what a call takes without any, are
/// the TypeScript suite's shapes but those either side of V8's lengths, and
/// 64 and about 1,024 bytes of characters of 2, 3 and 4 bytes, and 256 and
/// 1,024 bytes of ASCII with one character of 2 bytes first or last, which
/// part a check of UTF-8 that skips ASCII from one that checks every
/// character. A shape that the TypeScript or the C++ suite has takes that
/// suite's seed, so that it holds that suite's strings. The others, of 22,
/// 342 and 1,023 characters, which neither has, take seeds of their own.
pub fn string_workloads() -> Vec<StringWorkload> {
    let none = |_| false;
    let every = |_| true;

    vec![
        StringWorkload {
            name: "empty",
            strings: [const { String::new() }; COUNT],
        },
        shape("8 chars of ASCII", 8, 1, none, ascii),
        shape("8 chars, one of 2 bytes", 8, 2, |i| i == 3, two_bytes),
        shape("16 chars of ASCII", 16, 17, none, ascii),
        shape(
            "16 chars, two of 2 bytes",
            16,
            4,
            |i| i == 2 || i == 11,
            two_bytes,
        ),
        shape("15 chars, one of 4 bytes", 15, 5, |i| i == 7, four_bytes),
        shape("8 chars of 3 bytes", 8, 3, every, three_bytes),
        shape("64 chars of ASCII", 64, 7, none, ascii),
        shape("32 chars of 2 bytes", 32, 25, every, two_bytes),
        shape("16 chars of 4 bytes", 16, 27, every, four_bytes),
        shape("22 chars of 3 bytes", 22, 22, every, three_bytes),
        shape("32 chars of 3 bytes", 32, 6, every, three_bytes),
        shape("256 chars of ASCII", 256, 8, none, ascii),
        shape(
            "255 chars, first of 2 bytes",
            255,
            28,
            |i| i == 0,
            two_bytes,
        ),
        shape(
            "255 chars, last of 2 bytes",
            255,
            29,
            |i| i == 254,
            two_bytes,
        ),
        shape("256 chars of 3 bytes", 256, 9, every, three_bytes),
        shape("1,024 chars of ASCII", 1024, 10, none, ascii),
        shape(
            "1,023 chars, first of 2 bytes",
            1023,
            33,
            |i| i == 0,
            two_bytes,
        ),
        shape(
            "1,023 chars, last of 2 bytes",
            1023,
            34,
            |i| i == 1022,
            two_bytes,
        ),
        shape("512 chars of 2 bytes", 512, 30, every, two_bytes),
        shape("256 chars of 4 bytes", 256, 32, every, four_bytes),
        shape("342 chars of 3 bytes", 342, 23, every, three_bytes),
    ]
}

/// Picks a character of a shape's mix
type Pick = fn(&mut Xorshift) -> char;

/// 16 distinct strings of `length` characters, of which those at the indexes
/// that `special` selects come from `pick`, and the others from ASCII, as
/// the TypeScript suite's `shape()` makes them
fn shape(
    name: &'static str,
    length: usize,
    seed: usize,
    special: impl Fn(usize) -> bool,
    pick: Pick,
) -> StringWorkload {
    let mut random = Xorshift::new(seed);
    let strings: [String; COUNT] = std::array::from_fn(|_| {
        (0..length)
            .map(|i| {
                if special(i) {
                    pick(&mut random)
                } else {
                    ascii(&mut random)
                }
            })
            .collect()
    });

    let mut distinct = strings.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        COUNT,
        "The strings of {name} aren't distinct"
    );

    StringWorkload { name, strings }
}

fn ascii(random: &mut Xorshift) -> char {
    char::from(ASCII[index(random, ASCII.len())])
}

fn two_bytes(random: &mut Xorshift) -> char {
    let letters = TWO_BYTES.chars().count();
    TWO_BYTES
        .chars()
        .nth(index(random, letters))
        .expect("an index below the letters' count")
}

fn three_bytes(random: &mut Xorshift) -> char {
    code_point(THREE_BYTES, index(random, 0x5000))
}

fn four_bytes(random: &mut Xorshift) -> char {
    code_point(FOUR_BYTES, index(random, 0x50))
}

/// `Math.floor(random() * length)`, an index below `length`
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn index(random: &mut Xorshift, length: usize) -> usize {
    (random.next() * length as f64) as usize
}

/// The character `offset` code points from `first`, within a block of
/// characters that has no surrogates
fn code_point(first: u32, offset: usize) -> char {
    let offset = u32::try_from(offset).expect("an offset within the block");
    char::from_u32(first + offset).expect("a character of the block")
}
