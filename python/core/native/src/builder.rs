//! The native `TableBuilder`, with the pure-Python one's API, bytes and
//! errors. Each field is written into the table's bytes when it's set, so
//! each setter is one call into native code:
//!
//! - A field's offset is written when it's set. A vector writes its count and
//!   zeros for its ends, and each item's end once the item is written.
//! - A table nested in place is written where its parent ends, unpadded, and
//!   moved forward by more padding whenever a field raises its alignment,
//!   which is known only once its fields are set. The field raises the
//!   alignment of the tables that it's nested in too, and pads those nested
//!   in place, before it's written. So a large array moves the tables so
//!   far, which are often their slots alone, and its tables are never moved
//!   once it's written. Offsets count from their table's start, so none of
//!   them changes. A nested table that fails takes back the padding that its
//!   fields gave the tables it's nested in.
//! - Values are copied when they're set, and a nested builder as its bytes.
//! - A setter checks its field before it writes anything. A nested table or
//!   vector whose `fill` raises, or that's incomplete or too long, is undone
//!   by truncating the bytes to where it started.
//!
//! `bytes()` moves the bytes into a `_Table`, which exports them, so that the
//! builder holds no buffer of itself, which a `TableReader` that `finalize()`
//! keeps would hold in a cycle with it.

use std::cell::Cell;
use std::ffi::c_int;
use std::mem::{self, MaybeUninit};
use std::ptr;

use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};

use crate::bytes::{Bytes, memory_error};
use crate::elements::{self, struct_count};
use crate::layout::{Layout, Shape, shape_of};
use crate::little_endian_host;
use crate::python::{Int, buffer_bytes, contiguous_error, objects, utf8, with_bytes};
use crate::values::{self, Kind};
use crate::writer::{STR_VECTOR_MESSAGE, alignment, header_length};

/// The bytes of a written table, which ``TableBuilder.bytes()`` returns a
/// view of
#[pyclass(frozen, module = "tafla._native", name = "_Table")]
pub struct Table {
    data: Bytes,
}

#[pymethods]
impl Table {
    /// Exports the bytes, which never change their length, writable, as the
    /// pure-Python builder's `bytearray` is
    unsafe fn __getbuffer__(
        slf: Bound<'_, Self>,
        view: *mut ffi::Py_buffer,
        flags: c_int,
    ) -> PyResult<()> {
        let data = &slf.get().data;
        if unsafe {
            ffi::PyBuffer_FillInfo(
                view,
                slf.as_ptr(),
                data.pointer.as_ptr().cast(),
                data.len.cast_signed(),
                0,
                flags,
            )
        } != 0
        {
            return Err(PyErr::fetch(slf.py()));
        }
        Ok(())
    }
}

/// What an in-place setter restores: the end of the bytes where its table or
/// vector starts, and its parent's counters. The parent's positions move
/// with it whenever a field pads the tables it's nested in.
struct Checkpoint {
    start: usize,
    base: usize,
    slots: usize,
    fields: usize,
    index: usize,
    /// The parent's alignment from its own fields, which a nested table that
    /// fails restores, and as the nested table's fields raised it, which
    /// pads the parent if it's nested in place too, and which the parent
    /// takes once the nested table ends
    alignment: usize,
    raised: usize,
    in_place: Option<usize>,
    padding: usize,
}

/// The most checkpoints whose storage a builder leaves to the next
const SPARE_CHECKPOINTS: usize = 64;

thread_local! {
    /// The storage of the checkpoints of the last builder that this thread
    /// dropped, emptied, which the next builder to nest a table in place
    /// takes rather than allocating its own: a malloc and a free that took
    /// a few percent of a small table's time
    static SPARE: Cell<Vec<Checkpoint>> = const { Cell::new(Vec::new()) };
}

/// The table once written: its bytes, and the ``TableReader`` of them that
/// ``finalize()`` creates
struct Written {
    table: Py<Table>,
    reader: Option<Py<PyAny>>,
}

/// A vector's item as `set_vector()` takes it from the iterator: a builder,
/// not yet borrowed, or the bytes of a buffer
enum Taken<'py> {
    Builder(Bound<'py, TableBuilder>),
    Bytes(HeldBytes),
    Itself,
}

/// A table nested from a builder or from bytes, held while its parent's
/// setter writes it, as the pure-Python builder holds a view of it
enum Nested<'py> {
    Builder(PyRef<'py, TableBuilder>),
    Bytes(HeldBytes),
    /// The builder that a vector is set on, which can't hold all its fields
    /// and so raises once the items are taken
    Itself,
}

/// The bytes of a C-contiguous buffer, held until it's dropped. The buffer
/// is boxed, so that it stays where its exporter filled it.
struct HeldBytes(Box<ffi::Py_buffer>);

impl HeldBytes {
    /// Raises `TypeError` for a buffer that isn't C-contiguous, as appending
    /// it to a `bytearray` does
    fn of(object: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut view = Box::new(MaybeUninit::<ffi::Py_buffer>::uninit());
        if unsafe { ffi::PyObject_GetBuffer(object.as_ptr(), view.as_mut_ptr(), ffi::PyBUF_SIMPLE) }
            != 0
        {
            return Err(contiguous_error(object.py(), PyErr::fetch(object.py())));
        }
        Ok(Self(unsafe { view.assume_init() }))
    }

    fn bytes(&self) -> &[u8] {
        unsafe { buffer_bytes(&self.0) }
    }
}

impl Drop for HeldBytes {
    fn drop(&mut self) {
        // Dropped within the setter's call, with the thread attached
        unsafe { ffi::PyBuffer_Release(&raw mut *self.0) };
    }
}

/// Builds a table, writing each field into its bytes when it's set, as
/// ``tafla.table_builder.TableBuilder`` does.
#[pyclass(module = "tafla._native", name = "TableBuilder")]
pub struct TableBuilder {
    /// The table's bytes so far, the root's and those of any table being
    /// written in place in it
    data: Bytes,
    /// Where the table being written starts in the bytes, which its offsets
    /// count from: the root's 0, or a table nested in place
    base: usize,
    /// The position in the bytes of the table's first slot, its number of
    /// fields, the number set so far, and the largest alignment that a
    /// field needs from its start
    slots: usize,
    fields: usize,
    index: usize,
    alignment: usize,
    /// For a table nested in place, its offset in its parent before its
    /// padding, and the padding before it so far, which aligns it to its
    /// alignment so far
    in_place: Option<usize>,
    padding: usize,
    /// The checkpoints of the tables being written in place, from the
    /// outermost, each of which holds its parent's counters
    open: Vec<Checkpoint>,
    shape: Shape,
    layout: Option<Py<PyAny>>,
    written: Option<Written>,
}

impl Drop for TableBuilder {
    /// Leaves the storage of the checkpoints to the thread's next builder
    fn drop(&mut self) {
        let capacity = self.open.capacity();
        if capacity != 0 && capacity <= SPARE_CHECKPOINTS {
            let mut open = mem::take(&mut self.open);
            open.clear();
            // Dropped instead while the thread exits
            let _ = SPARE.try_with(|spare| spare.set(open));
        }
    }
}

/// The zeros that pad a table `offset` bytes into its parent to `alignment`
#[inline]
const fn padding_to(offset: usize, alignment: usize) -> usize {
    offset.wrapping_neg() & (alignment - 1)
}

impl TableBuilder {
    /// The offset from the table's start of a field at the end of the bytes
    #[inline]
    const fn offset(&self) -> usize {
        self.data.len - self.base
    }

    #[inline]
    const fn size(&self) -> usize {
        self.shape.size()
    }

    /// Makes room for `additional` bytes at the end, growing the bytes as
    /// `TableWriter`'s grow
    #[inline]
    fn room(&mut self, additional: usize) -> PyResult<()> {
        if self.data.capacity - self.data.len < additional {
            let required = self
                .data
                .len
                .checked_add(additional)
                .ok_or_else(memory_error)?;
            self.data.grow(required)?;
        }
        Ok(())
    }

    /// Writes `value`, which the offset type holds, in the slot at
    /// `position`
    #[inline]
    fn write_slot(&mut self, position: usize, value: usize) {
        let size = self.size();
        #[allow(clippy::cast_possible_truncation)]
        let bytes = (value as u32).to_le_bytes();
        self.data.as_mut_slice()[position..position + size].copy_from_slice(&bytes[..size]);
    }

    /// Appends a count or length, which the offset type holds, into room
    /// made for it
    #[inline]
    fn push_count(&mut self, count: usize) {
        let size = self.size();
        #[allow(clippy::cast_possible_truncation)]
        let bytes = (count as u32).to_le_bytes();
        self.data.push_within(&bytes[..size]);
    }

    /// Checks that the table has room for another field, and returns its
    /// index
    #[inline]
    fn next_index(&self) -> PyResult<usize> {
        let index = self.index;
        if index == self.fields {
            return Err(self.full());
        }
        Ok(index)
    }

    /// Checks that a field that makes the table `length` bytes fits
    #[inline]
    fn check_length(&self, index: usize, length: usize) -> PyResult<()> {
        if length > self.shape.max_length() {
            return Err(self.too_long(index, length));
        }
        Ok(())
    }

    /// The zeros that `raise_alignment(alignment)` inserts, for a field that
    /// needs `alignment`: before this table, if it's nested in place, and
    /// before each table that it's nested in, out to the first that's
    /// aligned to `alignment` already or isn't nested in place. None for the
    /// root, a vector item, or an alignment this table has already.
    #[inline]
    fn padding_to_raise(&self, alignment: usize) -> usize {
        let Some(offset) = self.in_place else {
            return 0;
        };
        if alignment <= self.alignment {
            return 0;
        }

        let mut padding = padding_to(offset, alignment) - self.padding;
        for parent in self.open.iter().rev() {
            let Some(offset) = parent.in_place else {
                break;
            };
            if parent.raised >= alignment {
                break;
            }
            padding += padding_to(offset, alignment) - parent.padding;
        }
        padding
    }

    /// Raises the table's alignment to `alignment`, if it's more, and that of
    /// the tables it's nested in, as far as it's more than theirs, and pads
    /// each that's nested in place to it, by inserting the
    /// `padding_to_raise(alignment)` zeros that there's room for. The bytes
    /// move once: from the innermost table out, each moves forward by its
    /// own padding and that of the tables it's nested in. Tables with no
    /// padding before them or the tables they're nested in stay where they
    /// are, rather than moving onto themselves.
    fn raise_alignment(&mut self, alignment: usize, padding: usize) {
        if alignment <= self.alignment {
            return;
        }
        self.alignment = alignment;
        let Some(offset) = self.in_place else {
            return;
        };

        let mut shift = padding;
        let more = padding_to(offset, alignment) - self.padding;
        if shift > 0 {
            self.data
                .move_forward_within(self.base, self.data.len, shift, more);
        }
        let mut end = self.base;
        self.base += shift;
        self.slots += shift;
        self.padding += more;
        shift -= more;

        for parent in self.open.iter_mut().rev() {
            // Where the nested table starts, in its parent's bytes
            parent.start += shift;
            if parent.raised >= alignment {
                break;
            }
            parent.raised = alignment;
            let Some(offset) = parent.in_place else {
                break;
            };

            let more = padding_to(offset, alignment) - parent.padding;
            if shift > 0 {
                self.data.move_forward_within(parent.base, end, shift, more);
            }
            end = parent.base;
            parent.base += shift;
            parent.slots += shift;
            parent.padding += more;
            shift -= more;
        }
        debug_assert_eq!(shift, 0, "each table moved by the padding before it");
        self.data.extend_moved(padding);
    }

    /// Lowers the alignment of the tables that this table is nested in, once
    /// a table nested in it fails, to what their own fields and the tables
    /// still nested in them need, and removes the padding that they no
    /// longer need from before each that's nested in place, and from before
    /// this table. The bytes move once: from the outermost table in, each
    /// moves back by its own padding removed and that of the tables it's
    /// nested in.
    #[cold]
    fn lower_alignment(&mut self) {
        let Some(offset) = self.in_place else {
            return;
        };

        let mut alignment = self.alignment;
        let mut outermost = self.open.len();
        for parent in self.open.iter_mut().rev() {
            let lowered = parent.alignment.max(alignment);
            if lowered == parent.raised {
                break;
            }
            parent.raised = lowered;
            if parent.in_place.is_none() {
                break;
            }
            alignment = lowered;
            outermost -= 1;
        }

        let mut shift = 0;
        for level in outermost..self.open.len() {
            let end = self.open.get(level + 1).map_or(self.base, |next| next.base);
            let parent = &mut self.open[level];
            let needed = parent
                .in_place
                .map_or(0, |offset| padding_to(offset, parent.raised));
            let less = parent.padding - needed;
            shift += less;
            self.data.move_back_within(parent.base, end, shift);
            parent.base -= shift;
            parent.slots -= shift;
            parent.padding -= less;
            parent.start -= shift;
        }

        let less = self.padding - padding_to(offset, self.alignment);
        shift += less;
        self.data.move_back_within(self.base, self.data.len, shift);
        self.base -= shift;
        self.slots -= shift;
        self.padding -= less;
        self.data.truncate(self.data.len - shift);
    }

    #[inline]
    fn set_scalar(&mut self, kind: Kind, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let index = self.next_index()?;
        let size = kind.size();
        let offset = self.offset();
        self.check_length(index, offset + size)?;

        let mut packed = [0u8; 8];
        let packed = &mut packed[..size];
        if !values::write(kind, value, packed)? {
            return Err(self.value_error(kind, value));
        }

        self.room(size)?;
        self.write_slot(self.slots + index * self.size(), offset);
        self.data.push_within(packed);
        self.index = index + 1;
        Ok(())
    }

    fn set_array(
        &mut self,
        code: char,
        item_size: usize,
        values: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        // A builder that holds all its fields, or is written, raises once the
        // values are converted, as the pure-Python one does
        if self.index < self.fields && self.set_sequence(code, item_size, values)? {
            return Ok(());
        }

        elements::with_elements(values, code, item_size, |bytes| {
            self.write_array(bytes, item_size)
        })
    }

    /// Writes an array from an exact list or tuple whose elements
    /// `elements::convert_sequence()` converts, straight into the room where
    /// they follow the array's padding and count, before either is written,
    /// as `TableWriter` does, rather than into an `array` that's copied
    /// again. Returns `false`, having written nothing, for elements that it
    /// doesn't convert, which `with_elements()` then converts, raising what
    /// the pure-Python builder raises.
    ///
    /// The room is after the zeros that the array's alignment inserts before
    /// the tables it's nested in, which are inserted only once the elements
    /// are converted, moving the tables up to the room.
    fn set_sequence(
        &mut self,
        code: char,
        item_size: usize,
        values: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let index = self.index;
        let size = self.size();
        let offset = self.offset();
        let padding = padding_to(offset + size, item_size);
        let raise = self.padding_to_raise(item_size);
        let start = self.data.len + raise + padding + size;

        let capacity = self.data.capacity;
        let data = &mut self.data;
        let destination = |byte_length| {
            // Moved into the call, so that the room it returns outlives it
            let data = data;
            data.spare(start, byte_length)
        };
        // SAFETY: the destination only grows the bytes, which runs no Python
        // code and keeps the thread attached
        let converted = unsafe { elements::convert_sequence(values, code, destination) };
        let Some(byte_length) = converted else {
            // What the elements grew, which with_elements() now converts
            // into an array of their own
            self.data.shrink_to(capacity);
            return Ok(false);
        };

        let length = offset + padding + size + byte_length;
        if length > self.shape.max_length() {
            self.data.shrink_to(capacity);
            return Err(self.too_long(index, length));
        }

        // Within the room that the destination made
        self.raise_alignment(item_size, raise);
        self.data.zeros_within(padding);
        self.write_slot(self.slots + index * size, offset + padding);
        self.push_count(byte_length / item_size);
        debug_assert_eq!(self.data.len, start, "the elements follow the count");
        // SAFETY: convert_sequence() wrote the elements where they start
        unsafe { self.data.extend_written(byte_length)? };
        self.make_little_endian(start, item_size);

        self.index = index + 1;
        Ok(true)
    }

    /// Writes an array of the elements' `bytes`, padded so that its
    /// elements, after its count, are aligned
    fn write_array(&mut self, bytes: &[u8], item_size: usize) -> PyResult<()> {
        let index = self.next_index()?;
        let size = self.size();
        let byte_length = bytes.len();
        let offset = self.offset();
        let padding = padding_to(offset + size, item_size);
        self.check_length(index, offset + padding + size + byte_length)?;

        let raise = self.padding_to_raise(item_size);
        self.room(raise + padding + size + byte_length)?;
        self.raise_alignment(item_size, raise);
        self.data.zeros_within(padding);
        self.write_slot(self.slots + index * size, offset + padding);
        self.push_count(byte_length / item_size);
        let start = self.data.len;
        self.data.push_within(bytes);
        self.make_little_endian(start, item_size);

        self.index = index + 1;
        Ok(())
    }

    /// Swaps the bytes of the elements from `start` to the end, which are in
    /// the host's byte order, to little-endian ones
    fn make_little_endian(&mut self, start: usize, item_size: usize) {
        if !little_endian_host() && item_size > 1 {
            for element in self.data.as_mut_slice()[start..].chunks_exact_mut(item_size) {
                element.reverse();
            }
        }
    }

    /// The bytes of a builder to nest, which it has once all its fields are
    /// set. Raises `ValueError` if it has fields left to set, or is writing
    /// a table in place.
    fn nested_bytes(&self) -> PyResult<&[u8]> {
        if let Some(written) = &self.written {
            return Ok(written.table.get().data.as_slice());
        }
        if self.slots != self.shape.header_size() {
            return Err(written_in_place_error());
        }
        if self.index != self.fields {
            return Err(unset_fields_error(self.index, self.fields, "the table's"));
        }
        Ok(self.data.as_slice())
    }

    /// Appends a nested builder's bytes, with their length in a sized header
    /// that isn't written yet, into room made for them
    fn push_builder(&mut self, nested: &Self) -> PyResult<()> {
        let bytes = nested.nested_bytes()?;
        let start = self.data.len;
        self.data.push_within(bytes);
        if nested.written.is_none() && nested.shape.sized {
            let size = nested.size();
            #[allow(clippy::cast_possible_truncation)]
            let length = (bytes.len() as u32).to_le_bytes();
            self.data.as_mut_slice()[start..start + size].copy_from_slice(&length[..size]);
        }
        Ok(())
    }

    /// The fields of a table nested in place, and the bytes of their slots,
    /// for a table that starts `length` bytes into its parent. Raises
    /// `ValueError` for slots that make the table too long, before they're
    /// written, or for a negative number of fields, as `bytearray()` does
    /// for a negative length.
    fn slots_of(&self, index: usize, length: usize, fields: &Int<'_>) -> PyResult<(usize, usize)> {
        let slot_bytes = fields.mul(self.size())?;
        if Int::from(self.shape.max_length()).lt(slot_bytes.clone())? {
            let length = slot_bytes.add(length)?;
            return Err(self.too_long(index, length.describe()?));
        }
        match (fields.index(), slot_bytes.index()) {
            (Some(fields), Some(slot_bytes)) => Ok((fields, slot_bytes)),
            _ => Err(PyValueError::new_err("negative count")),
        }
    }

    const fn checkpoint(&self, index: usize) -> Checkpoint {
        Checkpoint {
            start: self.data.len,
            base: self.base,
            slots: self.slots,
            fields: self.fields,
            index,
            alignment: self.alignment,
            raised: self.alignment,
            in_place: self.in_place,
            padding: self.padding,
        }
    }

    /// Starts a table of `fields` fields, unpadded, at the end of the bytes,
    /// with zeros for its `slot_bytes` bytes of slots, for `fill` to set its
    /// fields
    fn begin(&mut self, fields: usize, slot_bytes: usize, in_place: Option<usize>) {
        let start = self.data.len;
        self.base = start;
        self.slots = start;
        self.fields = fields;
        self.index = 0;
        self.alignment = 1;
        self.in_place = in_place;
        self.padding = 0;
        self.data.zeros_within(slot_bytes);
    }

    /// Starts a table nested in place as field `index`, and keeps this
    /// table's checkpoint until it ends
    fn start_table(&mut self, index: usize, fields: usize, slot_bytes: usize) -> PyResult<()> {
        self.room(slot_bytes)?;
        let offset = self.offset();
        if self.open.capacity() == 0 {
            // None while the thread exits
            self.open = SPARE.try_with(Cell::take).unwrap_or_default();
        }
        self.open.push(self.checkpoint(index));
        self.begin(fields, slot_bytes, Some(offset));
        Ok(())
    }

    /// Starts a vector's item, which isn't padded, and counts its offsets
    /// and alignment from its own start, as a builder does
    fn start_item(&mut self, fields: usize, slot_bytes: usize) -> PyResult<()> {
        self.room(slot_bytes)?;
        self.begin(fields, slot_bytes, None);
        Ok(())
    }

    /// Takes the checkpoint of the table that the in-place setter of `depth`
    /// started, once its `fill` returns, with whether a table nested in it
    /// is unfinished, which the setter then rolls back with its own: one
    /// whose setter was interrupted while it waited for another thread's
    /// call, or that another thread's call started. Raises `RuntimeError` if
    /// another thread's call took the checkpoint, leaving the builder as it
    /// is.
    fn take_checkpoint(&mut self, depth: usize) -> PyResult<(Checkpoint, bool)> {
        if self.open.len() < depth {
            return Err(changed_error());
        }
        let unfinished = self.open.len() > depth;
        self.open.truncate(depth);
        let checkpoint = self.open.pop().ok_or_else(changed_error)?;
        Ok((checkpoint, unfinished))
    }

    fn roll_back(&mut self, checkpoint: &Checkpoint) {
        self.data.truncate(checkpoint.start);
        self.base = checkpoint.base;
        self.slots = checkpoint.slots;
        self.fields = checkpoint.fields;
        self.index = checkpoint.index;
        self.alignment = checkpoint.alignment;
        self.in_place = checkpoint.in_place;
        self.padding = checkpoint.padding;
    }

    /// Rolls back a table nested in place that failed, and the padding that
    /// its fields gave the tables it's nested in
    fn roll_back_table(&mut self, checkpoint: &Checkpoint) {
        self.roll_back(checkpoint);
        self.lower_alignment();
    }

    /// Ends a table nested in place, once `fill` has set its fields, which
    /// padded it, and the tables it's nested in, as they raised their
    /// alignment: restores its parent's counters, with the alignment that
    /// the table's fields raised it to, and writes the table's offset. Rolls
    /// it back if it's incomplete or makes its parent too long.
    fn end_table(&mut self, checkpoint: &Checkpoint, fields: usize) -> PyResult<()> {
        if self.index != fields {
            let error = unset_fields_error(self.index, fields, "the nested table's");
            self.roll_back_table(checkpoint);
            return Err(error);
        }

        // Padded from its parent's start, as a nested builder is
        let padding = self.padding;
        let offset = checkpoint.start - checkpoint.base;
        debug_assert_eq!(padding, padding_to(offset, self.alignment));
        debug_assert!(checkpoint.raised >= self.alignment);
        let length = offset + self.data.len - checkpoint.start;
        if length > self.shape.max_length() {
            self.roll_back_table(checkpoint);
            return Err(self.too_long(checkpoint.index, length));
        }

        self.base = checkpoint.base;
        self.slots = checkpoint.slots;
        self.fields = checkpoint.fields;
        self.alignment = checkpoint.raised;
        self.in_place = checkpoint.in_place;
        self.padding = checkpoint.padding;
        self.write_slot(
            self.slots + checkpoint.index * self.size(),
            offset + padding,
        );
        self.index = checkpoint.index + 1;
        Ok(())
    }

    /// Writes a vector's count and zeros for its ends, after checking that
    /// they and the first item's slots fit, and returns the checkpoint that
    /// the items roll back to, and the items' fields and slot bytes.
    ///
    /// Until the first item starts, and between items, the builder holds the
    /// vector's state, in which any field raises, as a table that holds all
    /// its fields does, and writing the builder raises, as within a fill. The
    /// values' iterator runs then, and in its parent's state could set the
    /// vector's own field or write the builder over the vector's bytes.
    fn start_vector(
        &mut self,
        count: usize,
        fields: &Int<'_>,
    ) -> PyResult<(Checkpoint, usize, usize)> {
        let index = self.next_index()?;
        let size = self.size();
        let offset = self.offset();
        let ends = count.saturating_mul(size);
        let length = offset.saturating_add(size).saturating_add(ends);
        self.check_length(index, length)?;
        let (fields, slot_bytes) = if count > 0 {
            self.slots_of(index, length, fields)?
        } else {
            (0, 0)
        };

        let checkpoint = self.checkpoint(index);
        self.room(size + ends)?;
        self.push_count(count);
        self.data.zeros_within(ends);

        self.base = checkpoint.start;
        self.slots = checkpoint.start;
        self.fields = fields;
        self.index = fields;
        self.in_place = None;
        self.padding = 0;
        Ok((checkpoint, fields, slot_bytes))
    }

    /// Ends item `item` of a vector at `checkpoint.start`, once `fill` has
    /// set its fields, by writing its end. An end that its slot can't hold
    /// makes the table too long, which `end_vector()` raises.
    fn end_item(&mut self, checkpoint: &Checkpoint, item: usize, fields: usize) -> PyResult<()> {
        if self.index != fields {
            return Err(unset_fields_error(
                self.index,
                fields,
                &format!("item {item}'s"),
            ));
        }

        let last = self.data.len - checkpoint.start;
        if last <= self.shape.max_length() {
            self.write_slot(checkpoint.start + (item + 1) * self.size(), last);
        }
        Ok(())
    }

    fn end_vector(&mut self, checkpoint: &Checkpoint) -> PyResult<()> {
        let length = self.data.len - checkpoint.base;
        if length > self.shape.max_length() {
            self.roll_back(checkpoint);
            return Err(self.too_long(checkpoint.index, length));
        }

        let offset = checkpoint.start - checkpoint.base;
        self.write_slot(checkpoint.slots + checkpoint.index * self.size(), offset);
        self.base = checkpoint.base;
        self.slots = checkpoint.slots;
        self.fields = checkpoint.fields;
        self.index = checkpoint.index + 1;
        self.alignment = checkpoint.alignment;
        self.in_place = checkpoint.in_place;
        self.padding = checkpoint.padding;
        Ok(())
    }

    /// Pads a nested table of `byte_length` bytes to `alignment` and writes
    /// its offset, after checking that it fits, and makes room for it, having
    /// raised this table's alignment
    fn place_table(&mut self, byte_length: usize, alignment: usize) -> PyResult<usize> {
        let offset = self.offset();
        let padding = padding_to(offset, alignment);
        let index = self.next_index()?;
        self.check_length(index, offset + padding + byte_length)?;

        let raise = self.padding_to_raise(alignment);
        self.room(raise + padding + byte_length)?;
        self.raise_alignment(alignment, raise);
        self.data.zeros_within(padding);
        self.write_slot(self.slots + index * self.size(), offset + padding);
        Ok(index)
    }

    const fn finish_table(&mut self, index: usize) {
        self.index = index + 1;
    }

    /// Writes the table's length into a sized header, and moves its bytes
    /// into a `_Table`, once
    fn write(&mut self, py: Python<'_>) -> PyResult<Py<Table>> {
        if let Some(written) = &self.written {
            return Ok(written.table.clone_ref(py));
        }
        // Only a table being written in place has its slots elsewhere than
        // after the root's header
        if self.slots != self.shape.header_size() {
            return Err(written_in_place_error());
        }
        if self.index != self.fields {
            return Err(unset_fields_error(self.index, self.fields, "the table's"));
        }

        if self.shape.sized {
            self.write_slot(0, self.data.len);
        }
        let data = std::mem::replace(&mut self.data, Bytes::empty());
        let table = Py::new(py, Table { data })?;
        self.written = Some(Written {
            table: table.clone_ref(py),
            reader: None,
        });
        Ok(table)
    }

    /// The error of a field that the table has no room for: it's finalized,
    /// or holds all its fields
    #[cold]
    fn full(&self) -> PyErr {
        if self.written.is_some() {
            return PyTypeError::new_err("The table was already finalized");
        }
        PyValueError::new_err(format!(
            "Can't add field {} to a table of {}",
            self.index + 1,
            self.index
        ))
    }

    #[cold]
    fn too_long(&self, index: usize, length: impl std::fmt::Display) -> PyErr {
        PyValueError::new_err(format!(
            "Field {} makes the table {length} bytes, too long for {} offsets",
            index + 1,
            self.shape.offset_type.name()
        ))
    }

    /// The error that `TableWriter` raises for a scalar that its type can't
    /// hold, from `struct`'s error for it, at the position where it would be
    /// written
    #[cold]
    fn value_error(&self, kind: Kind, value: &Bound<'_, PyAny>) -> PyErr {
        let py = value.py();
        let objects = match objects(py) {
            Ok(objects) => objects,
            Err(error) => return error,
        };

        match objects.struct_of(py, kind).call_method1("pack", (value,)) {
            Err(error) if objects.is_pack_error(py, &error) => {
                let repr = match value.repr() {
                    Ok(repr) => repr,
                    Err(error) => return error,
                };
                let raised = PyValueError::new_err(format!(
                    "Can't write {repr} at {}: {}",
                    self.data.len,
                    error.value(py)
                ));
                raised.set_cause(py, Some(error));
                raised
            }
            Err(error) => error,
            Ok(_) => PyValueError::new_err(format!("Can't write the value as {kind:?}")),
        }
    }
}

/// Calls `fill(builder, *args)`, with the arguments on the stack, without
/// allocating, unless there are more than `MAX_ARGS`. Collecting them into a
/// `Vec` first took an allocation for every table nested in place.
fn call_fill<'a, 'py>(
    fill: &Bound<'py, PyAny>,
    builder: &Bound<'py, PyAny>,
    args: impl ExactSizeIterator<Item = Borrowed<'a, 'py, PyAny>>,
) -> PyResult<()> {
    const MAX_ARGS: usize = 8;
    let py = fill.py();
    let count = args.len();

    if count >= MAX_ARGS {
        let mut all = Vec::with_capacity(count + 1);
        all.push(builder.clone());
        all.extend(args.map(Borrowed::to_owned));
        fill.call1(PyTuple::new(py, all)?)?;
        return Ok(());
    }

    // The first slot is free for the callee, as PY_VECTORCALL_ARGUMENTS_OFFSET
    // allows
    let mut stack = [ptr::null_mut::<ffi::PyObject>(); MAX_ARGS + 1];
    stack[1] = builder.as_ptr();
    for (slot, arg) in stack[2..].iter_mut().zip(args) {
        *slot = arg.as_ptr();
    }
    let result = unsafe {
        ffi::PyObject_Vectorcall(
            fill.as_ptr(),
            stack.as_ptr().add(1),
            (count + 1) | ffi::PY_VECTORCALL_ARGUMENTS_OFFSET,
            ptr::null_mut(),
        )
    };
    unsafe { Bound::from_owned_ptr_or_err(py, result) }.map(drop)
}

/// Borrows the builder to end or roll back the nested table or vector that
/// an in-place setter started, once Python code has run: waiting, with the
/// thread detached, while another thread's call holds it, since returning
/// that thread's `RuntimeError` would leave the builder in the nested
/// table's state. Its call holds the builder only while it runs.
///
/// Raises what a signal's handler raises while it waits, such as
/// `KeyboardInterrupt` for Ctrl-C, rather than wait for a call that may be
/// waiting for this thread. That leaves the nested table unfinished: writing
/// the builder raises then, as within a fill, and an in-place setter that
/// it's nested in rolls it back once its own fill returns.
fn borrow_to_finish<'py>(slf: &Bound<'py, TableBuilder>) -> PyResult<PyRefMut<'py, TableBuilder>> {
    let py = slf.py();
    loop {
        if let Ok(builder) = slf.try_borrow_mut() {
            return Ok(builder);
        }
        py.check_signals()?;
        py.detach(|| std::thread::sleep(std::time::Duration::from_micros(50)));
    }
}

#[cold]
fn unset_fields_error(set_fields: usize, fields: usize, whose: &str) -> PyErr {
    PyValueError::new_err(format!(
        "Only {set_fields} of {whose} {fields} fields were set"
    ))
}

#[cold]
fn count_error(count: usize) -> PyErr {
    PyValueError::new_err(format!(
        "Iterating the values gave other than the {count} of len()"
    ))
}

#[cold]
fn written_in_place_error() -> PyErr {
    PyValueError::new_err("The builder was written while it recorded a nested table")
}

#[cold]
fn unfinished_error() -> PyErr {
    PyRuntimeError::new_err("A table nested in this one was left unfinished")
}

#[cold]
fn changed_error() -> PyErr {
    PyRuntimeError::new_err("Another thread's call ended the nested table")
}

macro_rules! scalar_setters {
    ($($name:ident: $kind:expr),* $(,)?) => {
        #[pymethods]
        impl TableBuilder {
            $(
                fn $name(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
                    self.set_scalar($kind, value)
                }
            )*
        }
    };
}

scalar_setters!(
    set_int8: Kind::I8,
    set_uint8: Kind::U8,
    set_int16: Kind::I16,
    set_uint16: Kind::U16,
    set_int32: Kind::I32,
    set_uint32: Kind::U32,
    set_int64: Kind::I64,
    set_uint64: Kind::U64,
    set_float32: Kind::F32,
    set_float64: Kind::F64,
);

macro_rules! array_setters {
    ($($name:ident: $code:literal, $size:literal),* $(,)?) => {
        #[pymethods]
        impl TableBuilder {
            $(
                fn $name(&mut self, values: &Bound<'_, PyAny>) -> PyResult<()> {
                    self.set_array($code, $size, values)
                }
            )*
        }
    };
}

array_setters!(
    set_int8_array: 'b', 1,
    set_uint8_array: 'B', 1,
    set_int16_array: 'h', 2,
    set_uint16_array: 'H', 2,
    set_int32_array: 'i', 4,
    set_uint32_array: 'I', 4,
    set_int64_array: 'q', 8,
    set_uint64_array: 'Q', 8,
    set_float32_array: 'f', 4,
    set_float64_array: 'd', 8,
);

#[pymethods]
impl TableBuilder {
    /// ``fields`` is the number of fields that the table will hold, and
    /// ``layout`` its header and offset type, which its readers must use too,
    /// with the version to write to the header when it has one.
    #[new]
    #[pyo3(signature = (fields, layout = None))]
    fn new(fields: &Bound<'_, PyAny>, layout: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let fields = Int::new(fields)?;
        let parsed = layout.map(PyAnyMethods::extract::<Layout>).transpose()?;
        let shape = shape_of(parsed.as_ref());
        let length = header_length(shape, &fields)?;
        // A negative number of fields that a header's slots outnumber, as
        // bytearray() rejects a negative length
        let Some(field_count) = fields.index() else {
            return Err(PyValueError::new_err("negative count"));
        };

        let mut data = Bytes::zeroed(length)?;
        if let Some(version) = shape.version {
            let slot = if shape.sized { shape.size() } else { 0 };
            let bytes = version.to_le_bytes();
            data.as_mut_slice()[slot..slot + shape.size()].copy_from_slice(&bytes[..shape.size()]);
        }

        Ok(Self {
            data,
            base: 0,
            slots: shape.header_size(),
            fields: field_count,
            index: 0,
            alignment: 1,
            in_place: None,
            padding: 0,
            open: Vec::new(),
            shape,
            layout: layout.map(|layout| layout.clone().unbind()),
            written: None,
        })
    }

    /// Finishes the table, once, and returns a ``TableReader`` that reads it;
    /// later calls return the same reader.
    fn finalize<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let mut builder = slf.try_borrow_mut()?;
        if let Some(Written {
            reader: Some(reader),
            ..
        }) = &builder.written
        {
            return Ok(reader.bind(py).clone());
        }

        let table = builder.write(py)?;
        let view = memoryview(table.bind(py))?;
        let table_reader = py.get_type::<crate::reader::TableReader>();
        let reader = match &builder.layout {
            Some(layout) => table_reader.call1((view, layout.bind(py)))?,
            None => table_reader.call1((view,))?,
        };
        if let Some(written) = &mut builder.written {
            written.reader = Some(reader.clone().unbind());
        }
        Ok(reader)
    }

    /// Finishes the table, once, as ``finalize()`` does, and returns its
    /// bytes, without the reader that ``finalize()`` creates
    fn bytes<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let table = slf.try_borrow_mut()?.write(py)?;
        memoryview(table.bind(py))
    }

    fn set_string(&mut self, string: &Bound<'_, PyAny>) -> PyResult<()> {
        let encoded = utf8(string.cast::<PyString>()?)?;
        let index = self.next_index()?;
        let size = self.size();
        let offset = self.offset();
        self.check_length(index, offset + size + encoded.len())?;

        self.room(size + encoded.len())?;
        self.write_slot(self.slots + index * size, offset);
        self.push_count(encoded.len());
        self.data.push_within(encoded);
        self.index = index + 1;
        Ok(())
    }

    /// Writes an array of bools as a ``uint8`` array: 1 for each value
    /// that's true, as ``bool()`` takes it, and 0 for each that's false
    fn set_bool_array(&mut self, py: Python<'_>, values: &Bound<'_, PyAny>) -> PyResult<()> {
        // An exact list or tuple of bools straight into the bytes, as the
        // writer converts it, and any other values as bool() takes them, as
        // the pure-Python builder converts them. A builder that holds all its
        // fields raises once they're converted.
        if self.index < self.fields && self.set_sequence('?', 1, values)? {
            return Ok(());
        }
        let elements = objects(py)?.bool_bytes.bind(py).call1((values,))?;
        with_bytes(&elements, |bytes| self.write_array(bytes, 1))
    }

    /// Writes a vector of strings, each as UTF-8, after the end of each,
    /// which lets readers find any string without reading the others.
    /// Raises ``TypeError`` for a ``str``, which is a sequence of its
    /// characters.
    fn set_string_vector(&mut self, strings: &Bound<'_, PyAny>) -> PyResult<()> {
        if strings.is_instance_of::<PyString>() {
            return Err(PyTypeError::new_err(STR_VECTOR_MESSAGE));
        }

        // Each string is checked and encoded in order, which raises the
        // error of the first that can't be, as the pure-Python builder's
        // encoding does. CPython keeps the UTF-8 of a string outside ASCII
        // with it, so that it's encoded once.
        let mut items = Vec::new();
        let mut byte_length = 0_usize;
        for item in strings.try_iter()? {
            let string = item?.cast_into::<PyString>()?;
            byte_length = byte_length.saturating_add(utf8(&string)?.len());
            items.push(string);
        }

        let index = self.next_index()?;
        let size = self.size();
        let head = size.saturating_mul(items.len().saturating_add(1));
        let offset = self.offset();
        self.check_length(
            index,
            offset.saturating_add(head).saturating_add(byte_length),
        )?;

        self.room(head + byte_length)?;
        self.write_slot(self.slots + index * size, offset);
        self.push_count(items.len());
        let mut end = head;
        for string in &items {
            end += utf8(string)?.len();
            self.push_count(end);
        }
        for string in &items {
            self.data.push_within(utf8(string)?);
        }
        self.index = index + 1;
        Ok(())
    }

    fn set_struct(&mut self, data: &Bound<'_, PyAny>) -> PyResult<()> {
        with_bytes(data, |bytes| {
            let index = self.next_index()?;
            let offset = self.offset();
            self.check_length(index, offset + bytes.len())?;

            self.room(bytes.len())?;
            self.write_slot(self.slots + index * self.size(), offset);
            self.data.push_within(bytes);
            self.index = index + 1;
            Ok(())
        })
    }

    fn set_struct_array(
        &mut self,
        data: &Bound<'_, PyAny>,
        size: &Bound<'_, PyAny>,
        alignment: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        with_bytes(data, |bytes| {
            let byte_length = bytes.len();
            let count = struct_count(byte_length, &Int::new(size)?)?
                .index()
                .unwrap_or(0);
            let offset_size = self.size();
            let offset = self.offset();
            let alignment = self::alignment(alignment)?;
            let padding = padding_to(offset + offset_size, alignment);

            let index = self.next_index()?;
            self.check_length(index, offset + padding + offset_size + byte_length)?;

            let raise = self.padding_to_raise(alignment);
            self.room(raise + padding + offset_size + byte_length)?;
            self.raise_alignment(alignment, raise);
            self.data.zeros_within(padding);
            self.write_slot(self.slots + index * offset_size, offset + padding);
            self.push_count(count);
            self.data.push_within(bytes);
            self.index = index + 1;
            Ok(())
        })
    }

    #[pyo3(signature = (table, alignment = None))]
    fn set_table(
        slf: &Bound<'_, Self>,
        table: &Bound<'_, PyAny>,
        alignment: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        if let Ok(nested) = table.cast::<Self>() {
            if nested.is(slf) {
                // A builder that can be nested holds all its fields
                let builder = slf.try_borrow()?;
                builder.nested_bytes()?;
                if let Some(alignment) = alignment {
                    self::alignment(alignment)?;
                }
                return Err(builder.full());
            }

            let nested = nested.try_borrow()?;
            let byte_length = nested.nested_bytes()?.len();
            let mut builder = slf.try_borrow_mut()?;
            let alignment = match alignment {
                Some(alignment) => self::alignment(alignment)?,
                None => nested.alignment,
            };
            let index = builder.place_table(byte_length, alignment)?;
            builder.push_builder(&nested)?;
            builder.finish_table(index);
            return Ok(());
        }

        with_bytes(table, |bytes| {
            let mut builder = slf.try_borrow_mut()?;
            let alignment = match alignment {
                Some(alignment) => self::alignment(alignment)?,
                None => 4,
            };
            let index = builder.place_table(bytes.len(), alignment)?;
            builder.data.push_within(bytes);
            builder.finish_table(index);
            Ok(())
        })
    }

    fn set_vector(slf: &Bound<'_, Self>, vector: &Bound<'_, PyAny>) -> PyResult<()> {
        // Each builder is checked as it's taken, as the pure-Python builder
        // checks it, but borrowed only once the iterator is done, since the
        // iterator may call an item that it gave already
        let mut taken = Vec::new();
        for item in vector.try_iter()? {
            let item = item?;
            taken.push(if let Ok(nested) = item.cast::<Self>() {
                if nested.is(slf) {
                    slf.try_borrow()?.nested_bytes()?;
                    Taken::Itself
                } else {
                    nested.try_borrow()?.nested_bytes()?;
                    Taken::Builder(nested.clone())
                }
            } else {
                Taken::Bytes(HeldBytes::of(&item)?)
            });
        }
        let items = taken
            .into_iter()
            .map(|item| {
                Ok(match item {
                    Taken::Builder(nested) => Nested::Builder(nested.try_borrow()?),
                    Taken::Bytes(held) => Nested::Bytes(held),
                    Taken::Itself => Nested::Itself,
                })
            })
            .collect::<PyResult<Vec<_>>>()?;

        let mut builder = slf.try_borrow_mut()?;
        let index = builder.next_index()?;
        let size = builder.size();
        let mut byte_length = size.saturating_mul(1 + items.len());
        for item in &items {
            let length = match item {
                Nested::Builder(nested) => nested.nested_bytes()?.len(),
                Nested::Bytes(held) => held.bytes().len(),
                Nested::Itself => unreachable!("a builder that holds itself holds all its fields"),
            };
            byte_length = byte_length.saturating_add(length);
        }
        let offset = builder.offset();
        builder.check_length(index, offset.saturating_add(byte_length))?;

        builder.room(byte_length)?;
        let slot = builder.slots + index * size;
        builder.write_slot(slot, offset);
        let start = builder.data.len;
        builder.push_count(items.len());
        builder.data.zeros_within(items.len() * size);

        for (item, table) in items.iter().enumerate() {
            match table {
                Nested::Builder(nested) => builder.push_builder(nested)?,
                Nested::Bytes(held) => builder.data.push_within(held.bytes()),
                Nested::Itself => unreachable!("a builder that holds itself holds all its fields"),
            }
            let end = builder.data.len - start;
            builder.write_slot(start + (item + 1) * size, end);
        }
        builder.index = index + 1;
        Ok(())
    }

    #[pyo3(signature = (fill, *args, fields))]
    fn set_table_in_place(
        slf: &Bound<'_, Self>,
        fill: &Bound<'_, PyAny>,
        args: &Bound<'_, PyTuple>,
        fields: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let (depth, fields) = {
            let mut builder = slf.try_borrow_mut()?;
            let index = builder.next_index()?;
            let offset = builder.offset();
            let (fields, slot_bytes) = builder.slots_of(index, offset, &Int::new(fields)?)?;
            builder.start_table(index, fields, slot_bytes)?;
            (builder.open.len(), fields)
        };

        let filled = call_fill(fill, slf.as_any(), args.iter_borrowed());

        let mut builder = borrow_to_finish(slf)?;
        let (checkpoint, unfinished) = builder.take_checkpoint(depth)?;
        match filled {
            Ok(()) if !unfinished => builder.end_table(&checkpoint, fields),
            filled => {
                builder.roll_back_table(&checkpoint);
                Err(filled.err().unwrap_or_else(unfinished_error))
            }
        }
    }

    #[pyo3(signature = (values, fill, *, fields))]
    fn set_vector_in_place(
        slf: &Bound<'_, Self>,
        values: &Bound<'_, PyAny>,
        fill: &Bound<'_, PyAny>,
        fields: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let count = values.len()?;
        let fields = Int::new(fields)?;
        // The vector's checkpoint stays here, rather than among those of the
        // tables nested in place, since its items' fields don't pad the
        // tables it's nested in
        let (checkpoint, fields, slot_bytes, depth) = {
            let mut builder = slf.try_borrow_mut()?;
            let (checkpoint, fields, slot_bytes) = builder.start_vector(count, &fields)?;
            (checkpoint, fields, slot_bytes, builder.open.len())
        };

        let filled = (|| -> PyResult<()> {
            let mut item = 0;
            for value in values.try_iter()? {
                let value = value?;
                if item == count {
                    return Err(count_error(count));
                }

                borrow_to_finish(slf)?.start_item(fields, slot_bytes)?;
                call_fill(fill, slf.as_any(), std::iter::once(value.as_borrowed()))?;
                let mut builder = borrow_to_finish(slf)?;
                if builder.open.len() != depth {
                    return Err(unfinished_error());
                }
                builder.end_item(&checkpoint, item, fields)?;
                item += 1;
            }

            if item != count {
                return Err(count_error(count));
            }
            Ok(())
        })();

        let mut builder = borrow_to_finish(slf)?;
        if builder.open.len() < depth {
            return Err(changed_error());
        }
        builder.open.truncate(depth);
        if let Err(error) = filled {
            builder.roll_back(&checkpoint);
            return Err(error);
        }
        builder.end_vector(&checkpoint)
    }
}

fn memoryview<'py>(table: &Bound<'py, Table>) -> PyResult<Bound<'py, PyAny>> {
    unsafe {
        Bound::from_owned_ptr_or_err(table.py(), ffi::PyMemoryView_FromObject(table.as_ptr()))
    }
}
