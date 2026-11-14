//! The bytes that a `DirectTableBuilder` writes into, in the ways that the
//! evaluation compares: a `Vec` grown by `realloc()`, a `Vec` grown by
//! allocating anew and copying, and bytes within the builder that move to
//! the heap once they're full.

#![allow(clippy::inline_always)]

use std::{cmp, iter, mem};

use crate::{Element, element::extend_elements};

/// Bytes that a builder appends to, and patches the offsets and item ends
/// of, and the runs that it keeps as segments rather than copying into them
pub trait Buffer: 'static {
    /// Runs of this many bytes or more are kept as segments
    const SEGMENT_BYTES: usize;

    /// No bytes, with whatever room the buffer starts with
    fn new() -> Self;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn as_slice(&self) -> &[u8];

    fn as_mut_slice(&mut self) -> &mut [u8];

    /// Makes room for `additional` more bytes
    fn reserve(&mut self, additional: usize);

    fn push(&mut self, bytes: &[u8]);

    fn push_zeros(&mut self, count: usize);

    /// Appends `values`, little-endian
    fn push_elements<T: Element>(&mut self, values: &[T]);

    /// Removes the bytes past the first `len`
    fn truncate(&mut self, len: usize);

    /// Takes the bytes when they're a `Vec` of their own, leaving none
    fn take_vec(&mut self) -> Option<Vec<u8>>;
}

/// A `Vec` with room for `FIRST` bytes from the start.
///
/// It's grown to at least twice its capacity: by `Vec::reserve()`, which
/// reallocates, when `REALLOC`, or otherwise by allocating anew and copying.
/// It keeps runs of `SEGMENT` bytes or more as segments.
#[derive(Debug)]
pub struct VecBuffer<
    const FIRST: usize = 256,
    const REALLOC: bool = true,
    const SEGMENT: usize = { super::SEGMENT_BYTES },
> {
    bytes: Vec<u8>,
}

impl<const FIRST: usize, const REALLOC: bool, const SEGMENT: usize>
    VecBuffer<FIRST, REALLOC, SEGMENT>
{
    /// Makes room for `additional` more bytes, out of line, so that the
    /// setters that call it keep their state in registers
    #[cold]
    #[inline(never)]
    fn grow(&mut self, additional: usize) {
        if REALLOC {
            // At least twice the capacity, as `Vec` grows
            self.bytes.reserve(additional);
        } else {
            let needed = self.bytes.len() + additional;
            let capacity = cmp::max(needed, 2 * self.bytes.capacity()).max(FIRST);
            let mut grown = Vec::with_capacity(capacity);
            grown.extend_from_slice(&self.bytes);
            self.bytes = grown;
        }
    }
}

impl<const FIRST: usize, const REALLOC: bool, const SEGMENT: usize> Buffer
    for VecBuffer<FIRST, REALLOC, SEGMENT>
{
    const SEGMENT_BYTES: usize = SEGMENT;

    #[inline(always)]
    fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(FIRST),
        }
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.bytes.len()
    }

    #[inline(always)]
    fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    #[inline(always)]
    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    #[inline(always)]
    fn reserve(&mut self, additional: usize) {
        if additional > self.bytes.capacity() - self.bytes.len() {
            self.grow(additional);
        }
    }

    #[inline(always)]
    fn push(&mut self, bytes: &[u8]) {
        self.reserve(bytes.len());
        self.bytes.extend_from_slice(bytes);
    }

    #[inline(always)]
    fn push_zeros(&mut self, count: usize) {
        self.reserve(count);
        self.bytes.extend(iter::repeat_n(0, count));
    }

    #[inline(always)]
    fn push_elements<T: Element>(&mut self, values: &[T]) {
        self.reserve(size_of_val(values));
        extend_elements(values, &mut self.bytes);
    }

    #[inline(always)]
    fn truncate(&mut self, len: usize) {
        self.bytes.truncate(len);
    }

    #[inline(always)]
    fn take_vec(&mut self) -> Option<Vec<u8>> {
        Some(mem::take(&mut self.bytes))
    }
}

/// The bytes that an [`InlineBuffer`] holds within itself
const INLINE: usize = 128;

/// The room that an [`InlineBuffer`] makes on the heap when its inline
/// bytes are full, at least
const FIRST_HEAP: usize = 512;

/// 128 bytes within the builder, and then a `Vec` of at least 512 bytes,
/// grown by `Vec::reserve()`, as C++'s prototype holds its bytes. It keeps
/// runs of `SEGMENT` bytes or more as segments.
///
/// The inline bytes are zeroed when it's created, since safe code can't
/// leave them uninitialized, and every write first asks whether the bytes
/// are on the heap.
#[derive(Debug)]
pub struct InlineBuffer<const SEGMENT: usize = { super::SEGMENT_BYTES }> {
    /// The bytes until they're on the heap, of which the first `len` are
    /// written
    inline: [u8; INLINE],
    len: usize,
    /// The bytes once they're on the heap, which they are when it has
    /// capacity
    heap: Vec<u8>,
}

impl<const SEGMENT: usize> InlineBuffer<SEGMENT> {
    #[inline]
    const fn on_heap(&self) -> bool {
        self.heap.capacity() != 0
    }

    /// The inline bytes for `count` more, if they fit, which then count as
    /// written
    #[inline(always)]
    fn inline_room(&mut self, count: usize) -> Option<&mut [u8]> {
        let start = self.len;
        let room = self.inline.get_mut(start..start + count)?;
        self.len = start + count;

        Some(room)
    }

    /// Moves the bytes to the heap, with room for `additional` more
    #[cold]
    #[inline(never)]
    fn spill(&mut self, additional: usize) {
        let capacity = cmp::max(FIRST_HEAP, self.len + additional);
        let mut heap = Vec::with_capacity(capacity);
        heap.extend_from_slice(&self.inline[..self.len]);
        self.heap = heap;
    }
}

impl<const SEGMENT: usize> Buffer for InlineBuffer<SEGMENT> {
    const SEGMENT_BYTES: usize = SEGMENT;

    #[inline(always)]
    fn new() -> Self {
        Self {
            inline: [0; INLINE],
            len: 0,
            heap: Vec::new(),
        }
    }

    #[inline(always)]
    fn len(&self) -> usize {
        if self.on_heap() {
            self.heap.len()
        } else {
            self.len
        }
    }

    #[inline(always)]
    fn as_slice(&self) -> &[u8] {
        if self.on_heap() {
            &self.heap
        } else {
            &self.inline[..self.len]
        }
    }

    #[inline(always)]
    fn as_mut_slice(&mut self) -> &mut [u8] {
        if self.on_heap() {
            &mut self.heap
        } else {
            &mut self.inline[..self.len]
        }
    }

    #[inline(always)]
    fn reserve(&mut self, additional: usize) {
        if self.on_heap() {
            self.heap.reserve(additional);
        } else if additional > INLINE - self.len {
            self.spill(additional);
        }
    }

    #[inline(always)]
    fn push(&mut self, bytes: &[u8]) {
        if !self.on_heap() {
            if let Some(room) = self.inline_room(bytes.len()) {
                room.copy_from_slice(bytes);
                return;
            }
            self.spill(bytes.len());
        }

        self.heap.extend_from_slice(bytes);
    }

    #[inline(always)]
    fn push_zeros(&mut self, count: usize) {
        if !self.on_heap() {
            if let Some(room) = self.inline_room(count) {
                room.fill(0);
                return;
            }
            self.spill(count);
        }

        self.heap.extend(iter::repeat_n(0, count));
    }

    #[inline(always)]
    fn push_elements<T: Element>(&mut self, values: &[T]) {
        let byte_length = size_of_val(values);
        if !self.on_heap() {
            if let Some(room) = self.inline_room(byte_length) {
                let elements = room.chunks_exact_mut(size_of::<T>());
                for (bytes, value) in elements.zip(values) {
                    bytes.copy_from_slice(value.to_le().as_ref());
                }
                return;
            }
            self.spill(byte_length);
        }

        self.heap.reserve(byte_length);
        extend_elements(values, &mut self.heap);
    }

    #[inline(always)]
    fn truncate(&mut self, len: usize) {
        if self.on_heap() {
            self.heap.truncate(len);
        } else {
            self.len = self.len.min(len);
        }
    }

    #[inline(always)]
    fn take_vec(&mut self) -> Option<Vec<u8>> {
        if !self.on_heap() {
            return None;
        }

        self.len = 0;
        Some(mem::take(&mut self.heap))
    }
}
