//! The bytes that a `TableBuilder` writes its table into, as its fields are
//! set

// What a setter writes is inlined into it, and growing is out of line, so
// that the setter keeps its state in registers
#![allow(clippy::inline_always)]

use std::iter;

use crate::{Element, element::extend_elements};

/// The room that a builder's bytes start with, which holds a small table
/// whole. Started empty, they grew three times for the benchmarks' record
/// of four fields, which was written in 155 ns rather than 51 ns. Started
/// at 128 or 512 bytes, the record took 53 ns, and the other rows were
/// within 4% of their times from 256.
const FIRST_CAPACITY: usize = 256;

/// A builder's bytes, which it appends fields to, and patches offsets and
/// item ends in. They grow to at least twice their capacity, by
/// `Vec::reserve()`, which reallocates: allocating anew and copying took as
/// long, since a table of the benchmarks grows twice at most.
#[derive(Debug)]
pub struct Bytes {
    vec: Vec<u8>,
}

impl Bytes {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            vec: Vec::with_capacity(FIRST_CAPACITY),
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
        self.vec.reserve(additional);
    }
}
