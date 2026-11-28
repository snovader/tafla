//! The buffer that a `TableBuilder` writes its table into, as its fields are
//! set

// What a setter writes is inlined into it, and growing is out of line, so
// that the setter keeps its state in registers
#![allow(clippy::inline_always)]

use std::iter;

use crate::{Element, element::extend_elements};

/// The least room that a builder's bytes start with, which holds a small
/// table whole. Started empty, they grew three times for the benchmarks'
/// record of four fields, which was written in 155 ns rather than 51 ns.
/// Started at 128 or 512 bytes, the record took 53 ns, and the other rows
/// were within 4% of their times from 256.
const FIRST_CAPACITY: usize = 256;

/// The bytes that a builder's bytes start with room for, for each field of
/// its table, beyond its header, up to [`MOST_ROOM_FOR_FIELDS`]. A table of
/// 32 fields of 128 and 192 bytes grew from 256 bytes five times, and with
/// this room and growing fourfold, once.
const ROOM_FOR_A_FIELD: usize = 64;

/// The most room that a builder's bytes start with beyond its header
const MOST_ROOM_FOR_FIELDS: usize = 64 * 1024;

/// The capacity below which the bytes grow fourfold rather than twofold,
/// which grows a table of many short fields fewer times, and above which
/// they waste no more than half their capacity
const FOURFOLD_BELOW: usize = 64 * 1024;

/// A builder's bytes, which it appends fields to, and patches offsets and
/// item ends in. They grow fourfold while they're short, and twofold after,
/// which reallocates: allocating anew and copying took as long.
#[derive(Debug)]
pub struct Bytes {
    vec: Vec<u8>,
}

impl Bytes {
    /// The bytes of `vec`, which `start_table()` clears
    #[inline(always)]
    pub const fn new(vec: Vec<u8>) -> Self {
        Self { vec }
    }

    /// Clears the bytes, and makes room for a table of `fields` fields after
    /// a header of `header` bytes when they have none. Bytes with room keep
    /// it, whether it's a reused buffer's or room that the caller chose.
    #[inline(always)]
    pub fn start_table(&mut self, header: usize, fields: usize) {
        self.vec.clear();

        if self.vec.capacity() == 0 {
            let room = fields
                .saturating_mul(ROOM_FOR_A_FIELD)
                .min(MOST_ROOM_FOR_FIELDS);
            self.vec
                .reserve_exact(FIRST_CAPACITY.max(header.saturating_add(room)));
        }
    }

    #[inline(always)]
    pub fn into_vec(self) -> Vec<u8> {
        self.vec
    }

    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.vec.len()
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[u8] {
        &self.vec
    }

    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.vec
    }

    /// Makes room for `additional` more bytes
    #[inline(always)]
    pub fn reserve(&mut self, additional: usize) {
        if additional > self.vec.capacity() - self.vec.len() {
            self.grow(additional);
        }
    }

    /// Makes room for `additional` more bytes if the allocator can, which
    /// is room that the bytes may need, rather than room they do
    pub fn try_reserve(&mut self, additional: usize) {
        if additional > self.vec.capacity() - self.vec.len() {
            let _ = self.vec.try_reserve(additional);
        }
    }

    #[inline(always)]
    pub fn push(&mut self, bytes: &[u8]) {
        self.reserve(bytes.len());
        self.vec.extend_from_slice(bytes);
    }

    /// Appends `count` zeros, of padding or of slots written later
    #[inline(always)]
    pub fn push_zeros(&mut self, count: usize) {
        self.reserve(count);
        // From an iterator, which is inlined, as `TableWriter` appends them
        self.vec.extend(iter::repeat_n(0, count));
    }

    /// Appends `values`, little-endian
    #[inline(always)]
    pub fn push_elements<T: Element>(&mut self, values: &[T]) {
        self.reserve(size_of_val(values));
        extend_elements(values, &mut self.vec);
    }

    /// Appends bools, as bytes of 1 and 0
    #[inline(always)]
    pub fn push_bools(&mut self, values: &[bool]) {
        self.reserve(values.len());
        // Converted with `u8::from()`, as `TableWriter` converts them
        self.vec.extend(values.iter().map(|&value| u8::from(value)));
    }

    /// Removes the bytes past the first `len`
    #[inline(always)]
    pub fn truncate(&mut self, len: usize) {
        self.vec.truncate(len);
    }

    /// Makes room for `additional` more bytes, out of line, so that the
    /// setters that call it keep their state in registers
    #[cold]
    #[inline(never)]
    fn grow(&mut self, additional: usize) {
        let capacity = self.vec.capacity();
        if capacity < FOURFOLD_BELOW {
            let needed = self.vec.len().saturating_add(additional);
            let grown = needed.max(capacity.saturating_mul(4));
            self.vec.reserve_exact(grown - self.vec.len());
        } else {
            // At least twice the capacity, as `Vec` grows
            self.vec.reserve(additional);
        }
    }
}
