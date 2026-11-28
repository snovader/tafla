//! The bytes of a table being written, which Python's allocator holds, as
//! `TableWriter` and `TableBuilder` write them

use std::mem::MaybeUninit;
use std::ptr::{self, NonNull};

use pyo3::exceptions::{PyBufferError, PyMemoryError};
use pyo3::ffi;
use pyo3::prelude::*;

/// The bytes of the table being written, which Python's allocator holds, so
/// that `tracemalloc` counts them as it counts a `bytearray`'s. While a
/// buffer of them is exported, by `TableWriter.bytes()`, their length is
/// frozen, so that the exported pointer stays valid, as a `bytearray`'s is.
pub struct Bytes {
    pub pointer: NonNull<u8>,
    pub len: usize,
    pub capacity: usize,
    /// The buffers of the bytes that are exported and not yet released
    pub exports: usize,
}

// SAFETY: the bytes are an allocation of their own, which only their
// writer's methods reach, and PyO3 borrows the writer mutably for each, so no
// two threads use them at once. Python's allocator frees them from any
// thread that's attached.
unsafe impl Send for Bytes {}
unsafe impl Sync for Bytes {}

impl Bytes {
    /// The least capacity, which holds a small table without growing
    const MIN_CAPACITY: usize = 64;

    pub fn zeroed(len: usize) -> PyResult<Self> {
        let capacity = len.max(Self::MIN_CAPACITY);
        let pointer = allocate(ptr::null_mut(), capacity)?;
        unsafe { ptr::write_bytes(pointer.as_ptr(), 0, len) };

        Ok(Self {
            pointer,
            len,
            capacity,
            exports: 0,
        })
    }

    /// No bytes and no capacity, which allocate nothing until they grow, as
    /// a builder holds once it has moved its bytes into its table
    pub const fn empty() -> Self {
        Self {
            pointer: NonNull::dangling(),
            len: 0,
            capacity: 0,
            exports: 0,
        }
    }

    /// The allocation, which is null for `empty()` bytes, to reallocate or
    /// free
    #[inline]
    const fn allocation(&self) -> *mut u8 {
        if self.capacity == 0 {
            ptr::null_mut()
        } else {
            self.pointer.as_ptr()
        }
    }

    #[inline]
    pub const fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.pointer.as_ptr(), self.len) }
    }

    #[inline]
    pub const fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.len) }
    }

    /// Raises `BufferError` if the bytes are exported and `len` isn't their
    /// length, as a `bytearray` raises for a change of its length while
    /// it's exported, and for nothing else
    #[inline]
    pub fn check_length(&self, len: usize) -> PyResult<()> {
        if self.exports > 0 && len != self.len {
            return Err(frozen());
        }
        Ok(())
    }

    /// Sets the length to `len`, with zeros in the bytes it adds. Raises
    /// `BufferError` if that changes the length while the bytes are
    /// exported, as a `bytearray` does.
    #[inline]
    pub fn resize(&mut self, len: usize) -> PyResult<()> {
        self.check_length(len)?;
        if len > self.capacity {
            self.grow(len)?;
        }
        if len > self.len {
            unsafe { ptr::write_bytes(self.pointer.as_ptr().add(self.len), 0, len - self.len) };
        }
        self.len = len;

        Ok(())
    }

    /// Replaces the bytes from `start`, which is at most the length, with
    /// `bytes`, as a `bytearray`'s slice assignment `data[start:] = bytes`
    /// does. Raises `BufferError` if that changes the length while the bytes
    /// are exported, having written nothing.
    #[inline]
    pub fn replace_from(&mut self, start: usize, bytes: &[u8]) -> PyResult<()> {
        debug_assert!(start <= self.len);
        let len = start.checked_add(bytes.len()).ok_or_else(memory_error)?;
        self.check_length(len)?;
        if len > self.capacity {
            self.grow(len)?;
        }
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.pointer.as_ptr().add(start),
                bytes.len(),
            );
        }
        self.len = len;

        Ok(())
    }

    #[inline]
    pub fn extend_from_slice(&mut self, bytes: &[u8]) -> PyResult<()> {
        self.replace_from(self.len, bytes)
    }

    #[inline]
    pub fn extend_zeros(&mut self, count: usize) -> PyResult<()> {
        self.resize(self.len.checked_add(count).ok_or_else(memory_error)?)
    }

    /// The `length` bytes of the capacity from `start`, which is at least the
    /// length, grown to hold them, for bytes that `extend_written()` adds to
    /// the length once they're written. Returns `None` if they don't fit in
    /// memory, or if the bytes are exported and would have to grow.
    #[inline]
    pub fn spare(&mut self, start: usize, length: usize) -> Option<&mut [MaybeUninit<u8>]> {
        assert!(
            start >= self.len,
            "the spare capacity starts after the length"
        );
        let end = start.checked_add(length)?;
        if end > self.capacity {
            if self.exports > 0 {
                return None;
            }
            self.grow(end).ok()?;
        }

        let spare = unsafe { self.pointer.as_ptr().add(start) };
        Some(unsafe { std::slice::from_raw_parts_mut(spare.cast(), length) })
    }

    /// Adds the `length` bytes after the length to it, as `extend_from_slice()`
    /// would append them. Raises `BufferError` if the bytes are exported, as
    /// it would.
    ///
    /// # Safety
    ///
    /// The bytes must be written, into the capacity that `spare()` returned.
    #[inline]
    pub unsafe fn extend_written(&mut self, length: usize) -> PyResult<()> {
        let len = self.len + length;
        debug_assert!(len <= self.capacity);
        self.check_length(len)?;
        self.len = len;

        Ok(())
    }

    /// Gives back the capacity beyond `capacity`, which is at least the
    /// length, such as what `spare()` grew for bytes that weren't written
    /// after all. Keeps it if the allocator can't.
    #[cold]
    pub fn shrink_to(&mut self, capacity: usize) {
        debug_assert!(capacity >= self.len && capacity > 0);
        if capacity < self.capacity
            && let Ok(pointer) = allocate(self.allocation(), capacity)
        {
            self.pointer = pointer;
            self.capacity = capacity;
        }
    }

    /// Grows the capacity to at least `required`, so that appending is
    /// amortized: by half again while it's small, and by an eighth, as a
    /// `bytearray` grows, once it's large, so that a large table holds
    /// little more than its bytes. A larger append, such as a large array's,
    /// grows it to `required` exactly.
    #[cold]
    pub fn grow(&mut self, required: usize) -> PyResult<()> {
        const LARGE: usize = 64 * 1024;
        let growth = if self.capacity < LARGE {
            self.capacity / 2
        } else {
            self.capacity / 8
        };
        let capacity = required.max(self.capacity + growth);
        self.pointer = allocate(self.allocation(), capacity)?;
        self.capacity = capacity;

        Ok(())
    }

    // The methods below write within the capacity, which their owner made
    // room for, and so neither grow the bytes nor fail, and the bytes mustn't
    // be exported, as a `TableBuilder`'s never are. Each checks that it stays
    // within the capacity.

    /// Appends `bytes` within the capacity
    #[inline]
    pub fn push_within(&mut self, bytes: &[u8]) {
        assert!(
            bytes.len() <= self.capacity - self.len,
            "the bytes fit in the capacity"
        );
        debug_assert_eq!(self.exports, 0);
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.pointer.as_ptr().add(self.len),
                bytes.len(),
            );
        }
        self.len += bytes.len();
    }

    /// Appends `count` zeros within the capacity
    #[inline]
    pub fn zeros_within(&mut self, count: usize) {
        assert!(
            count <= self.capacity - self.len,
            "the zeros fit in the capacity"
        );
        debug_assert_eq!(self.exports, 0);
        unsafe { ptr::write_bytes(self.pointer.as_ptr().add(self.len), 0, count) };
        self.len += count;
    }

    /// Moves the bytes from `start` to `end`, which is at most the length,
    /// forward by `by`, within the capacity, and writes `zeros` zeros, at
    /// most `by`, just before where they then start. The length stays: the
    /// bytes after `end`, `by` of them, are overwritten, so zeros inserted in
    /// several places move the last bytes first, and `extend_moved()` then
    /// adds them to the length.
    pub fn move_forward_within(&mut self, start: usize, end: usize, by: usize, zeros: usize) {
        assert!(
            start <= end && end <= self.len && by <= self.capacity - end && zeros <= by,
            "the bytes move within the capacity"
        );
        debug_assert_eq!(self.exports, 0);
        unsafe {
            let from = self.pointer.as_ptr().add(start);
            ptr::copy(from, from.add(by), end - start);
            ptr::write_bytes(from.add(by - zeros), 0, zeros);
        }
    }

    /// Adds `count` bytes after the length to it, which `move_forward_within()`
    /// wrote
    pub fn extend_moved(&mut self, count: usize) {
        assert!(
            count <= self.capacity - self.len,
            "the moved bytes are within the capacity"
        );
        self.len += count;
    }

    /// Moves the bytes from `start` to `end`, which is at most the length,
    /// back by `by`, at most `start`, over the bytes before them. The length
    /// stays, for `truncate()` to shorten once bytes are removed from several
    /// places, the first bytes first.
    pub fn move_back_within(&mut self, start: usize, end: usize, by: usize) {
        assert!(
            start <= end && end <= self.len && by <= start,
            "the bytes move within the length"
        );
        debug_assert_eq!(self.exports, 0);
        unsafe {
            let from = self.pointer.as_ptr().add(start);
            ptr::copy(from, from.sub(by), end - start);
        }
    }

    /// Shortens the bytes to `len`, which is at most their length, keeping
    /// the capacity
    #[inline]
    pub fn truncate(&mut self, len: usize) {
        assert!(
            len <= self.len,
            "the bytes are truncated to at most their length"
        );
        debug_assert_eq!(self.exports, 0);
        self.len = len;
    }
}

impl Drop for Bytes {
    fn drop(&mut self) {
        // The owner is dropped by its deallocation, with the thread attached.
        // PyMem_Free() takes the null of empty bytes.
        unsafe { ffi::PyMem_Free(self.allocation().cast()) };
    }
}

/// Allocates `capacity` bytes, or reallocates `pointer` to them
fn allocate(pointer: *mut u8, capacity: usize) -> PyResult<NonNull<u8>> {
    if isize::try_from(capacity).is_err() {
        return Err(memory_error());
    }
    let pointer = unsafe { ffi::PyMem_Realloc(pointer.cast(), capacity) };
    NonNull::new(pointer.cast::<u8>()).ok_or_else(memory_error)
}

#[cold]
pub fn memory_error() -> PyErr {
    PyMemoryError::new_err(())
}

#[cold]
pub fn frozen() -> PyErr {
    PyBufferError::new_err("Existing exports of data: object cannot be re-sized")
}
