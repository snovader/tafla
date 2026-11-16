//! The bytes that a `TableBuilder` writes its table into, as its fields are
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
/// 32 fields of 128 and 192 bytes, each copied into the bytes when it's set
/// and again by `to_bytes()`, took 1.36 times as long to write as with the
/// builder that recorded its fields, in 21.5 KB from 7 allocations, as its
/// bytes grew from 256 five times, and with this room and growing fourfold,
/// 0.86 times as long, in 16.3 KB from 3. A table of 4 fields still starts
/// with 256 bytes or about, so the record took as long.
///
/// A builder of scalars holds that room too, while it's kept: 100,000
/// builders of 32 `uint32` fields, nested by reference, held 347 MB at once
/// rather than 189 MB, and took 1.13 times as long to write. With room for
/// 16 bytes a field, they held 194 MB and took 0.66 times as long, but the
/// table of 32 fields of 128 and 192 bytes took 0.98 times as long rather
/// than 0.88, of 64 and 96 bytes 0.86 rather than 0.72, and of 4 fields of
/// 255 bytes, whose bytes grew twice rather than once, 1.89 rather than
/// 1.35.
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
    /// Bytes with room for a table of `fields` fields after a header of
    /// `header` bytes
    #[inline(always)]
    pub fn for_table(header: usize, fields: usize) -> Self {
        let room = fields
            .saturating_mul(ROOM_FOR_A_FIELD)
            .min(MOST_ROOM_FOR_FIELDS);

        Self {
            vec: Vec::with_capacity(FIRST_CAPACITY.max(header.saturating_add(room))),
        }
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
