//! The lists of strings that the lists suite writes and reads: for each kind
//! of string, 10, 100 and 10,000 distinct strings of the same length and mix
//! of UTF-8 widths. They're the TypeScript suite's, which
//! `typescript/benchmarks/strings/lists/shapes.ts` makes, from the same
//! generator and seeds, for three of its eight kinds.

use crate::workloads::{Xorshift, thousands};

/// A list of strings, and its name, such as `10 × 16 chars of ASCII`
#[derive(Clone, Debug)]
pub struct ListShape {
    pub name: String,
    pub strings: Vec<String>,
}

impl ListShape {
    /// The strings, as a list that the generated code writes
    pub fn strs(&self) -> Vec<&str> {
        self.strings.iter().map(String::as_str).collect()
    }
}

/// The number of strings in each kind's lists
pub const COUNTS: [usize; 3] = [10, 100, 10_000];

/// The characters of the ASCII strings, from which each is picked at random
const ASCII: &[u8; 62] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// The first of the CJK ideographs, of three bytes in UTF-8, from which the
/// next 0x5000 are picked at random
const THREE_BYTES: u32 = 0x4e00;

/// The number of TypeScript's kinds, which its seeds count
const TYPESCRIPT_KINDS: usize = 8;

/// A kind of string: its name, its length in characters, the characters it
/// picks, and its index among TypeScript's kinds, which seeds its lists
struct Kind {
    name: &'static str,
    length: usize,
    three_bytes: bool,
    index: usize,
}

const KINDS: [Kind; 3] = [
    Kind {
        name: "16 chars of ASCII",
        length: 16,
        three_bytes: false,
        index: 1,
    },
    Kind {
        name: "256 chars of ASCII",
        length: 256,
        three_bytes: false,
        index: 2,
    },
    Kind {
        name: "8 chars of 3 bytes",
        length: 8,
        three_bytes: true,
        index: 6,
    },
];

/// Every shape, of each count and then each kind, in TypeScript's order
pub fn list_shapes() -> Vec<ListShape> {
    let mut shapes = Vec::with_capacity(COUNTS.len() * KINDS.len());
    for (count_index, count) in COUNTS.into_iter().enumerate() {
        for kind in &KINDS {
            // A seed of its own for each shape
            let seed = 1 + kind.index + TYPESCRIPT_KINDS * count_index;
            shapes.push(shape(kind, count, seed));
        }
    }

    shapes
}

/// `count` strings of `kind`, each of its length in characters, picked in
/// order by a generator of `seed`
fn shape(kind: &Kind, count: usize, seed: usize) -> ListShape {
    let mut random = Xorshift::new(seed);
    let strings = (0..count)
        .map(|_| (0..kind.length).map(|_| pick(kind, &mut random)).collect())
        .collect();

    ListShape {
        name: format!("{} × {}", thousands(count), kind.name),
        strings,
    }
}

/// A character of `kind`, which takes a number from `random`
// The products are below the tables' lengths, so they truncate to their
// floors, as `Math.floor()` does, and 62 converts exactly
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn pick(kind: &Kind, random: &mut Xorshift) -> char {
    if kind.three_bytes {
        let offset = (random.next() * f64::from(0x5000)) as u32;
        char::from_u32(THREE_BYTES + offset).expect("A CJK ideograph is a character")
    } else {
        char::from(ASCII[(random.next() * ASCII.len() as f64) as usize])
    }
}
