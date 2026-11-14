//! A prototype of a `TableBuilder` that writes each field into a `Vec<u8>`
//! when it's set, as a native class, `DirectTableBuilder`, with the API,
//! bytes and errors of `tafla.TableBuilder`, which records each field and
//! writes them all through a `TableWriter` in `bytes()`. Only a build with
//! the feature `prototype` has it, and no wheel's. See
//! `python/prototype/README.md`.
//!
//! It writes as the pure-Python prototype `DirectTableBuilder` does:
//!
//! - A field's offset is written when it's set. A vector writes its count and
//!   zeros for its ends, and each item's end once the item is written.
//! - A table nested in place is written where its parent ends, unpadded, and
//!   moved forward by its padding once `fill` has set its fields.
//! - Runs of 256 bytes or more are kept as segments, references to the
//!   caller's buffer or string, which `bytes()` copies into the table. On a
//!   big-endian host, arrays are always copied, since their elements are
//!   swapped.
//! - A nested builder is copied into its parent as its bytes and segments.
//! - A setter checks its field before it writes anything. A nested table or
//!   vector whose `fill` raises, or that's incomplete or too long, is undone
//!   by truncating the bytes and segments to a checkpoint.
//!
//! `bytes()` moves the bytes into a `_Table`, which exports them read-only,
//! so that the builder holds no view of itself.

use std::ffi::{CStr, c_int, c_long};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use pyo3::exceptions::{PyMemoryError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};

use crate::elements::struct_count;
use crate::layout::{Layout, Shape, shape_of};
use crate::little_endian_host;
use crate::python::{Int, buffer_bytes, objects, utf8, with_buffer, with_bytes};
use crate::values::{self, Kind};

/// The shortest run that a builder keeps as a segment, unless
/// `_set_prototype_defaults()` changes it
const SEGMENT_BYTES: usize = 256;

/// The room that a builder allocates first, and the least that it grows to
const INITIAL_CAPACITY: usize = 128;
const LEAST_GROWTH: usize = 512;

/// The defaults of new builders, which a benchmark's worker process sets for
/// the variant that it times: the shortest segment, and whether the bytes
/// grow by allocating new room and copying them, rather than by `realloc()`
static DEFAULT_SEGMENT_BYTES: AtomicUsize = AtomicUsize::new(SEGMENT_BYTES);
static DEFAULT_COPY_GROWTH: AtomicBool = AtomicBool::new(false);

/// Sets the defaults of builders created after it, for benchmarks and tests:
/// ``segment_bytes``, the shortest run kept as a segment, and
/// ``copy_growth``, whether the bytes grow by allocating new room and copying
/// them rather than by ``realloc()``. Returns the previous defaults.
#[pyfunction]
pub fn _set_prototype_defaults(segment_bytes: usize, copy_growth: bool) -> (usize, bool) {
    (
        DEFAULT_SEGMENT_BYTES.swap(segment_bytes, Ordering::Relaxed),
        DEFAULT_COPY_GROWTH.swap(copy_growth, Ordering::Relaxed),
    )
}

/// The bytes of a built table, which ``DirectTableBuilder.bytes()`` returns
/// a read-only view of
#[pyclass(frozen, module = "tafla._native", name = "_Table")]
pub struct Table {
    data: Vec<u8>,
}

#[pymethods]
impl Table {
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
                data.as_ptr().cast_mut().cast(),
                data.len().cast_signed(),
                1,
                flags,
            )
        } != 0
        {
            return Err(PyErr::fetch(slf.py()));
        }
        Ok(())
    }
}

/// What a segment's bytes are read from when the table is written
enum Held {
    /// A buffer, such as an array, whose length is checked again
    Buffer(Py<PyAny>),
    /// A string, whose UTF-8 `CPython` keeps with it
    String(Py<PyString>),
    /// A nested builder's table
    Table(Py<Table>),
}

impl Held {
    fn with_bytes<R>(&self, py: Python<'_>, f: impl FnOnce(&[u8]) -> PyResult<R>) -> PyResult<R> {
        match self {
            Self::Buffer(buffer) => with_bytes(buffer.bind(py), f),
            Self::String(string) => f(utf8(string.bind(py))?),
            Self::Table(table) => f(&table.get().data),
        }
    }

    fn clone_ref(&self, py: Python<'_>) -> Self {
        match self {
            Self::Buffer(buffer) => Self::Buffer(buffer.clone_ref(py)),
            Self::String(string) => Self::String(string.clone_ref(py)),
            Self::Table(table) => Self::Table(table.clone_ref(py)),
        }
    }
}

/// A run that `bytes()` copies in: where it goes among the builder's own
/// bytes, what holds it, and its length when it was set
struct Segment {
    at: usize,
    held: Held,
    len: usize,
}

/// What an in-place setter restores: the end of the bytes and the segments
/// before it started, and its parent's counters
struct Checkpoint {
    start: usize,
    shift: usize,
    held: usize,
    base: isize,
    slots: usize,
    fields: usize,
    index: usize,
    alignment: usize,
}

/// The table once written: its bytes, and the ``ByteBuffer`` of them that
/// ``finalize()`` creates
struct Written {
    table: Py<Table>,
    buffer: Option<Py<PyAny>>,
}

/// Builds a table as ``tafla.TableBuilder`` does, writing each field into
/// its bytes when it's set. A prototype; see the module's documentation.
#[pyclass(module = "tafla._native", name = "DirectTableBuilder")]
pub struct DirectTableBuilder {
    /// The bytes written so far, without the segments
    data: Vec<u8>,
    /// The segments' bytes so far, which come before the end of the bytes
    shift: usize,
    segments: Vec<Segment>,
    /// Where the table being written starts, less the segments' bytes so
    /// far, so that a field at the end of the bytes has the offset
    /// `data.len() - base`
    base: isize,
    /// The position in the bytes of its first slot, its number of fields,
    /// the number set so far, and the largest alignment that a field needs
    slots: usize,
    fields: usize,
    index: usize,
    alignment: usize,
    shape: Shape,
    layout: Option<Py<PyAny>>,
    written: Option<Written>,
    segment_bytes: usize,
    copy_growth: bool,
}

impl DirectTableBuilder {
    /// The offset from the table's start of a field at the end of the bytes
    #[inline]
    const fn offset(&self) -> usize {
        (self.data.len().cast_signed() - self.base).cast_unsigned()
    }

    #[inline]
    const fn size(&self) -> usize {
        self.shape.size()
    }

    /// Makes room for `additional` bytes at the end
    #[inline]
    fn room(&mut self, additional: usize) -> PyResult<()> {
        if self.data.capacity() - self.data.len() < additional {
            self.grow(additional)?;
        }
        Ok(())
    }

    /// Grows the room to at least twice its length, by `realloc()`, or by
    /// allocating new room and copying the bytes into it
    #[cold]
    fn grow(&mut self, additional: usize) -> PyResult<()> {
        let required = self
            .data
            .len()
            .checked_add(additional)
            .ok_or_else(memory_error)?;
        let capacity = required
            .max(self.data.capacity().saturating_mul(2))
            .max(LEAST_GROWTH);

        if self.copy_growth {
            let mut grown = Vec::new();
            grown
                .try_reserve_exact(capacity)
                .map_err(|_| memory_error())?;
            grown.extend_from_slice(&self.data);
            self.data = grown;
        } else {
            self.data
                .try_reserve_exact(capacity - self.data.len())
                .map_err(|_| memory_error())?;
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
        self.data[position..position + size].copy_from_slice(&bytes[..size]);
    }

    /// Appends a count or length, which the offset type holds, after making
    /// room for it
    #[inline]
    fn push_count(&mut self, count: usize) {
        let size = self.size();
        #[allow(clippy::cast_possible_truncation)]
        let bytes = (count as u32).to_le_bytes();
        self.data.extend_from_slice(&bytes[..size]);
    }

    #[inline]
    fn push_zeros(&mut self, count: usize) {
        self.data.resize(self.data.len() + count, 0);
    }

    /// Keeps a run of `len` bytes as a segment at the end of the bytes
    fn hold(&mut self, held: Held, len: usize) {
        self.segments.push(Segment {
            at: self.data.len(),
            held,
            len,
        });
        self.shift += len;
        self.base -= len.cast_signed();
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

    /// Checks that a field of `length` bytes from the table's start fits
    #[inline]
    fn check_length(&self, index: usize, length: usize) -> PyResult<()> {
        if length > self.shape.max_length() {
            return Err(self.too_long(index, length));
        }
        Ok(())
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
        self.data.extend_from_slice(packed);
        self.index = index + 1;
        Ok(())
    }

    fn set_array(
        &mut self,
        code: char,
        item_size: usize,
        values: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let py = values.py();

        // A buffer of the code's elements is written as it is, as
        // elements::with_elements() takes it, and anything else as
        // tafla.elements.as_elements() converts it
        if unsafe { ffi::PyObject_CheckBuffer(values.as_ptr()) } != 0 {
            let mut wrote = None;
            let flags = ffi::PyBUF_C_CONTIGUOUS | ffi::PyBUF_FORMAT;
            let exported = with_buffer(values, flags, |view| {
                if view.ndim == 1
                    && usize::try_from(view.itemsize) == Ok(item_size)
                    && is_host_format(format_of(view), code, item_size)
                {
                    let bytes = unsafe { buffer_bytes(view) };
                    wrote = Some(self.write_array(values, bytes, item_size));
                }
                Ok(())
            });
            if let Some(result) = wrote {
                return result;
            }
            // Otherwise the buffer isn't of the code's elements, or its
            // exporter refused, as for bytes that aren't C-contiguous
            drop(exported);
        }

        let elements = objects(py)?
            .as_elements
            .bind(py)
            .call1((code.to_string(), values))?;
        with_bytes(&elements, |bytes| {
            self.write_array(&elements, bytes, item_size)
        })
    }

    /// Writes an array of `bytes`, which `elements` holds, padded so that
    /// its elements, after its count, are aligned
    fn write_array(
        &mut self,
        elements: &Bound<'_, PyAny>,
        bytes: &[u8],
        item_size: usize,
    ) -> PyResult<()> {
        let index = self.next_index()?;
        let size = self.size();
        let byte_length = bytes.len();
        let offset = self.offset();
        let padding = (offset + size).wrapping_neg() & (item_size - 1);
        self.check_length(index, offset + padding + size + byte_length)?;

        let little_endian = little_endian_host();
        let held = byte_length >= self.segment_bytes && little_endian;
        self.room(padding + size + if held { 0 } else { byte_length })?;
        self.push_zeros(padding);
        self.write_slot(self.slots + index * size, offset + padding);
        self.push_count(byte_length / item_size);
        if held {
            self.hold(Held::Buffer(elements.clone().unbind()), byte_length);
        } else {
            let start = self.data.len();
            self.data.extend_from_slice(bytes);
            if !little_endian && item_size > 1 {
                for element in self.data[start..].chunks_exact_mut(item_size) {
                    element.reverse();
                }
            }
        }

        self.index = index + 1;
        self.alignment = self.alignment.max(item_size);
        Ok(())
    }

    /// Writes the bytes of a run that starts at the end, as a segment if
    /// it's long enough
    fn push_run(&mut self, held: impl FnOnce() -> Held, bytes: &[u8]) -> PyResult<()> {
        if bytes.len() >= self.segment_bytes {
            self.hold(held(), bytes.len());
        } else {
            self.room(bytes.len())?;
            self.data.extend_from_slice(bytes);
        }
        Ok(())
    }

    /// The byte length of a nested builder, which it knows once all its
    /// fields are set
    fn length_of(&self) -> PyResult<usize> {
        if let Some(written) = &self.written {
            return Ok(written.table.get().data.len());
        }
        if self.index != self.fields {
            return Err(unset_fields_error(self.index, self.fields, "the table's"));
        }
        Ok(self.data.len() + self.shift)
    }

    /// Appends a complete builder's bytes, with its length in a sized
    /// header, and its segments
    fn append_builder(&mut self, py: Python<'_>, nested: &Self) -> PyResult<()> {
        if let Some(written) = &nested.written {
            let table = written.table.clone_ref(py);
            return self.push_run(|| Held::Table(table), &written.table.get().data);
        }

        let start = self.data.len();
        self.room(nested.data.len())?;
        self.data.extend_from_slice(&nested.data);
        if nested.shape.sized {
            let size = nested.size();
            #[allow(clippy::cast_possible_truncation)]
            let length = ((nested.data.len() + nested.shift) as u32).to_le_bytes();
            self.data[start..start + size].copy_from_slice(&length[..size]);
        }

        for segment in &nested.segments {
            self.segments.push(Segment {
                at: start + segment.at,
                held: segment.held.clone_ref(py),
                len: segment.len,
            });
        }
        self.shift += nested.shift;
        self.base -= nested.shift.cast_signed();
        Ok(())
    }

    /// Writes zeros for a nested table's slots. Slots longer than the offset
    /// type addresses make every field too long, so they're counted as bytes
    /// of the table without being written.
    fn start_slots(&mut self, fields: usize) -> PyResult<()> {
        match fields.checked_mul(self.size()) {
            Some(slots) if slots <= self.shape.max_length() => {
                self.room(slots)?;
                self.push_zeros(slots);
            }
            slots => {
                let slots = slots.unwrap_or(usize::MAX / 4);
                self.shift += slots;
                self.base -= slots.cast_signed();
            }
        }
        Ok(())
    }

    const fn checkpoint(&self, index: usize) -> Checkpoint {
        Checkpoint {
            start: self.data.len(),
            shift: self.shift,
            held: self.segments.len(),
            base: self.base,
            slots: self.slots,
            fields: self.fields,
            index,
            alignment: self.alignment,
        }
    }

    /// Starts a table at the end of the bytes, for `fill` to set its fields
    const fn start_table(&mut self, fields: usize) {
        let start = self.data.len();
        self.base = start.cast_signed();
        self.slots = start;
        self.fields = fields;
        self.index = 0;
        self.alignment = 1;
    }

    fn roll_back(&mut self, checkpoint: &Checkpoint) {
        self.data.truncate(checkpoint.start);
        self.segments.truncate(checkpoint.held);
        self.shift = checkpoint.shift;
        self.base = checkpoint.base;
        self.slots = checkpoint.slots;
        self.fields = checkpoint.fields;
        self.index = checkpoint.index;
        self.alignment = checkpoint.alignment;
    }

    /// Ends a table nested in place, once `fill` has set its fields: moves
    /// it forward by its padding, writes its offset and restores its
    /// parent's counters. Rolls it back if it's incomplete or makes its
    /// parent too long.
    fn end_table(&mut self, checkpoint: &Checkpoint, fields: usize) -> PyResult<()> {
        if self.index != fields {
            let error = unset_fields_error(self.index, fields, "the nested table's");
            self.roll_back(checkpoint);
            return Err(error);
        }

        let alignment = self.alignment;
        let shift = self.shift;
        let end = self.data.len();
        // Padded from its parent's start, as a nested builder is
        let offset = (checkpoint.start.cast_signed() - checkpoint.base).cast_unsigned();
        let padding = offset.wrapping_neg() & (alignment - 1);
        let length = offset + padding + end + shift - checkpoint.start - checkpoint.shift;
        if length > self.shape.max_length() {
            self.roll_back(checkpoint);
            return Err(self.too_long(checkpoint.index, length));
        }

        if padding > 0 {
            if let Err(error) = self.room(padding) {
                self.roll_back(checkpoint);
                return Err(error);
            }
            self.push_zeros(padding);
            self.data
                .copy_within(checkpoint.start..end, checkpoint.start + padding);
            self.data[checkpoint.start..checkpoint.start + padding].fill(0);
            for segment in &mut self.segments[checkpoint.held..] {
                segment.at += padding;
            }
        }
        self.write_slot(
            checkpoint.slots + checkpoint.index * self.size(),
            offset + padding,
        );

        self.base = checkpoint.base + checkpoint.shift.cast_signed() - shift.cast_signed();
        self.slots = checkpoint.slots;
        self.fields = checkpoint.fields;
        self.index = checkpoint.index + 1;
        self.alignment = alignment.max(checkpoint.alignment);
        Ok(())
    }

    /// Writes a vector's count and zeros for its ends, after checking that
    /// they fit, and returns the checkpoint that the items roll back to
    fn start_vector(&mut self, count: usize) -> PyResult<Checkpoint> {
        let index = self.next_index()?;
        let size = self.size();
        let offset = self.offset();
        let ends = count.saturating_mul(size);
        self.check_length(index, offset.saturating_add(size).saturating_add(ends))?;

        let checkpoint = self.checkpoint(index);
        self.room(size + ends)?;
        self.push_count(count);
        self.push_zeros(ends);
        Ok(checkpoint)
    }

    /// Ends item `item` of a vector at `checkpoint.start`, once `fill` has
    /// set its fields, by writing its end. An end that its slot can't hold
    /// makes the table too long, which `end_vector()` raises.
    fn end_item(
        &mut self,
        checkpoint: &Checkpoint,
        item: usize,
        count: usize,
        fields: usize,
        item_start: usize,
    ) -> PyResult<()> {
        if self.index != fields {
            return Err(unset_fields_error(
                self.index,
                fields,
                &format!("item {item}'s"),
            ));
        }

        let end = self.data.len();
        if item == 0 && count > 1 {
            // Room for the other items, each as long as the first, up to
            // what the offset type addresses
            let room = (count - 1)
                .saturating_mul(end - item_start)
                .min(self.shape.max_length());
            self.room(room)?;
        }

        let last = end + self.shift - checkpoint.start - checkpoint.shift;
        if last <= self.shape.max_length() {
            self.write_slot(checkpoint.start + (item + 1) * self.size(), last);
        }
        Ok(())
    }

    fn end_vector(&mut self, checkpoint: &Checkpoint) -> PyResult<()> {
        let shift = self.shift;
        let parent_start = (checkpoint.base + checkpoint.shift.cast_signed()).cast_unsigned();
        let length = self.data.len() + shift - parent_start;
        if length > self.shape.max_length() {
            self.roll_back(checkpoint);
            return Err(self.too_long(checkpoint.index, length));
        }

        let offset = (checkpoint.start.cast_signed() - checkpoint.base).cast_unsigned();
        self.write_slot(checkpoint.slots + checkpoint.index * self.size(), offset);
        self.base = checkpoint.base + checkpoint.shift.cast_signed() - shift.cast_signed();
        self.slots = checkpoint.slots;
        self.fields = checkpoint.fields;
        self.index = checkpoint.index + 1;
        self.alignment = checkpoint.alignment;
        Ok(())
    }

    /// Writes the table: its length in a sized header, and its bytes joined
    /// with the segments, into a `_Table`
    fn write(&mut self, py: Python<'_>) -> PyResult<Py<Table>> {
        if let Some(written) = &self.written {
            return Ok(written.table.clone_ref(py));
        }
        // Only a table nested in place has its slots elsewhere than after
        // the root's header
        if self.slots != self.shape.header_size() {
            return Err(PyValueError::new_err(
                "The builder was written while it recorded a nested table",
            ));
        }
        if self.index != self.fields {
            return Err(unset_fields_error(self.index, self.fields, "the table's"));
        }

        let total = self.data.len() + self.shift;
        if self.shape.sized {
            self.write_slot(0, total);
        }
        let data = if self.segments.is_empty() {
            std::mem::take(&mut self.data)
        } else {
            self.join(py, total)?
        };

        let table = Py::new(py, Table { data })?;
        self.segments.clear();
        self.written = Some(Written {
            table: table.clone_ref(py),
            buffer: None,
        });
        Ok(table)
    }

    /// Copies the bytes and the segments between them into bytes of the
    /// table's length. Raises `ValueError` if a segment's length changed
    /// since it was set.
    fn join(&self, py: Python<'_>, total: usize) -> PyResult<Vec<u8>> {
        let mut joined = Vec::new();
        joined
            .try_reserve_exact(total)
            .map_err(|_| memory_error())?;

        let mut changed = 0isize;
        let mut position = 0;
        for segment in &self.segments {
            joined.extend_from_slice(&self.data[position..segment.at]);
            segment.held.with_bytes(py, |bytes| {
                changed += bytes.len().cast_signed() - segment.len.cast_signed();
                joined.extend_from_slice(bytes);
                Ok(())
            })?;
            position = segment.at;
        }
        joined.extend_from_slice(&self.data[position..]);

        if changed != 0 {
            return Err(PyValueError::new_err(format!(
                "The fields wrote {} bytes rather than the {total} they counted when they were set",
                total.cast_signed() + changed
            )));
        }
        Ok(joined)
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
    fn too_long(&self, index: usize, length: usize) -> PyErr {
        PyValueError::new_err(format!(
            "Field {} makes the table {length} bytes, too long for {} offsets",
            index + 1,
            self.shape.offset_type.name()
        ))
    }

    /// The error that `TableWriter` raises for a scalar that its type can't
    /// hold, from `struct`'s error for it
    #[cold]
    fn value_error(&self, kind: Kind, value: &Bound<'_, PyAny>) -> PyErr {
        let py = value.py();
        let objects = match objects(py) {
            Ok(objects) => objects,
            Err(error) => return error,
        };

        match objects.struct_of(py, kind).call_method1("pack", (value,)) {
            Err(error) if objects.is_pack_error(py, &error) => {
                let position = self.data.len() + self.shift;
                let repr = match value.repr() {
                    Ok(repr) => repr,
                    Err(error) => return error,
                };
                let raised = PyValueError::new_err(format!(
                    "Can't write {repr} at {position}: {}",
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

/// Calls `fill(builder, *args)`
fn call_fill(
    fill: &Bound<'_, PyAny>,
    builder: &Bound<'_, PyAny>,
    args: &[Borrowed<'_, '_, PyAny>],
) -> PyResult<()> {
    const MAX_ARGS: usize = 8;
    let py = fill.py();

    if args.len() >= MAX_ARGS {
        let mut all = Vec::with_capacity(args.len() + 1);
        all.push(builder.clone());
        all.extend(args.iter().map(|arg| Borrowed::to_owned(*arg)));
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
            (args.len() + 1) | ffi::PY_VECTORCALL_ARGUMENTS_OFFSET,
            ptr::null_mut(),
        )
    };
    unsafe { Bound::from_owned_ptr_or_err(py, result) }.map(drop)
}

#[cold]
fn memory_error() -> PyErr {
    PyMemoryError::new_err(())
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

/// The alignment of a struct array or nested table, which must be the int
/// 1, 2, 4 or 8, or `ValueError`, as `TableWriter`'s
fn alignment_of(alignment: &Bound<'_, PyAny>) -> PyResult<usize> {
    if unsafe { ffi::PyLong_CheckExact(alignment.as_ptr()) } != 0
        && let Ok(Some(value @ (1 | 2 | 4 | 8))) = crate::python::small(alignment)
    {
        return Ok(value.unsigned_abs().try_into().expect("an alignment fits"));
    }

    Err(PyValueError::new_err(format!(
        "An alignment of {} isn't 1, 2, 4 or 8 bytes",
        alignment.repr()?
    )))
}

/// The byte length of the header, as `tafla.layout.header_length()` checks
/// it
fn header_length(shape: Shape, fields: usize, version: Option<&Int<'_>>) -> PyResult<usize> {
    let name = shape.offset_type.name();
    if shape.versioned != version.is_some() {
        return Err(PyTypeError::new_err(if shape.versioned {
            "A versioned layout needs a version"
        } else {
            "The layout has no version slot"
        }));
    }

    if let Some(version) = version
        && version
            .index()
            .is_none_or(|version| version > shape.max_length())
    {
        return Err(PyValueError::new_err(format!(
            "Version {} doesn't fit in a {name} slot",
            version.display()?
        )));
    }

    match fields
        .checked_mul(shape.size())
        .and_then(|slots| slots.checked_add(shape.header_size()))
    {
        Some(length) if length <= shape.max_length() => Ok(length),
        _ => Err(PyValueError::new_err(format!(
            "A header of {fields} fields doesn't fit in {name} offsets"
        ))),
    }
}

/// The format of a buffer, which is bytes without one: `elements.rs`'s,
/// copied so that the prototype leaves that file as it is
const fn format_of(view: &ffi::Py_buffer) -> &[u8] {
    if view.format.is_null() {
        return b"B";
    }
    unsafe { CStr::from_ptr(view.format) }.to_bytes()
}

/// Whether a buffer `format` holds elements of the same kind and size as the
/// array type code `code`, in the host's byte order: `elements.rs`'s, copied
fn is_host_format(format: &[u8], code: char, item_size: usize) -> bool {
    let host_order = if cfg!(target_endian = "little") {
        b'<'
    } else {
        b'>'
    };
    let (prefix, letter) = match *format {
        [prefix @ (b'@' | b'='), letter] => (Some(prefix), letter),
        [prefix, letter] if prefix == host_order => (Some(prefix), letter),
        [letter] => (None, letter),
        _ => return false,
    };

    let family: &[u8] = match code {
        'f' => b"f",
        'd' => b"d",
        'b' | 'h' | 'i' | 'q' => b"bhilqn",
        _ => b"BHILQN",
    };
    if !family.contains(&letter) {
        return false;
    }

    let native = matches!(prefix, None | Some(b'@'));
    let size = match letter.to_ascii_lowercase() {
        b'b' => 1,
        b'h' => 2,
        b'l' if native => size_of::<c_long>(),
        b'i' | b'f' | b'l' => 4,
        b'q' | b'd' => 8,
        b'n' if native => size_of::<isize>(),
        _ => return false,
    };
    size == item_size
}

macro_rules! scalar_setters {
    ($($name:ident: $kind:expr),* $(,)?) => {
        #[pymethods]
        impl DirectTableBuilder {
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
        impl DirectTableBuilder {
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
impl DirectTableBuilder {
    #[new]
    #[allow(clippy::used_underscore_binding)]
    #[pyo3(signature = (fields, layout = None, version = None, *, _segment_bytes = None, _copy_growth = None))]
    fn new(
        fields: usize,
        layout: Option<&Bound<'_, PyAny>>,
        version: Option<&Bound<'_, PyAny>>,
        _segment_bytes: Option<usize>,
        _copy_growth: Option<bool>,
    ) -> PyResult<Self> {
        let parsed = layout.map(PyAnyMethods::extract::<Layout>).transpose()?;
        let shape = shape_of(parsed.as_ref());
        let version = version.map(Int::new).transpose()?;
        let length = header_length(shape, fields, version.as_ref())?;

        let mut data = Vec::new();
        data.try_reserve_exact(length.max(INITIAL_CAPACITY))
            .map_err(|_| memory_error())?;
        data.resize(length, 0);

        let mut builder = Self {
            data,
            shift: 0,
            segments: Vec::new(),
            base: 0,
            slots: shape.header_size(),
            fields,
            index: 0,
            alignment: 1,
            shape,
            layout: layout.map(|layout| layout.clone().unbind()),
            written: None,
            segment_bytes: _segment_bytes
                .unwrap_or_else(|| DEFAULT_SEGMENT_BYTES.load(Ordering::Relaxed)),
            copy_growth: _copy_growth
                .unwrap_or_else(|| DEFAULT_COPY_GROWTH.load(Ordering::Relaxed)),
        };
        if let Some(version) = version.and_then(|version| version.index()) {
            let slot = if shape.sized { shape.size() } else { 0 };
            builder.write_slot(slot, version);
        }
        Ok(builder)
    }

    fn finalize<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let mut builder = slf.try_borrow_mut()?;
        if let Some(Written {
            buffer: Some(buffer),
            ..
        }) = &builder.written
        {
            return Ok(buffer.bind(py).clone());
        }

        let table = builder.write(py)?;
        let view = memoryview(table.bind(py))?;
        let byte_buffer = py.get_type::<crate::reader::ByteBuffer>();
        let buffer = match &builder.layout {
            Some(layout) => byte_buffer.call1((view, layout.bind(py)))?,
            None => byte_buffer.call1((view,))?,
        };
        if let Some(written) = &mut builder.written {
            written.buffer = Some(buffer.clone().unbind());
        }
        Ok(buffer)
    }

    fn bytes<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let table = slf.try_borrow_mut()?.write(py)?;
        memoryview(table.bind(py))
    }

    fn set_string(&mut self, string: &Bound<'_, PyAny>) -> PyResult<()> {
        let string = string.cast::<PyString>()?;
        let encoded = utf8(string)?;
        let index = self.next_index()?;
        let size = self.size();
        let offset = self.offset();
        self.check_length(index, offset + size + encoded.len())?;

        self.room(size)?;
        self.write_slot(self.slots + index * size, offset);
        self.push_count(encoded.len());
        self.push_run(|| Held::String(string.clone().unbind()), encoded)?;
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
            self.data.extend_from_slice(bytes);
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
            let alignment = alignment_of(alignment)?;
            let padding = (offset + offset_size).wrapping_neg() & (alignment - 1);

            let index = self.next_index()?;
            self.check_length(index, offset + padding + offset_size + byte_length)?;

            self.room(padding + offset_size)?;
            self.push_zeros(padding);
            self.write_slot(self.slots + index * offset_size, offset + padding);
            self.push_count(count);
            self.push_run(|| Held::Buffer(data.clone().unbind()), bytes)?;
            self.index = index + 1;
            self.alignment = self.alignment.max(alignment);
            Ok(())
        })
    }

    #[pyo3(signature = (table, alignment = None))]
    fn set_table(
        slf: &Bound<'_, Self>,
        table: &Bound<'_, PyAny>,
        alignment: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let py = slf.py();

        if let Ok(nested) = table.cast::<Self>() {
            if nested.is(slf) {
                let builder = slf.try_borrow()?;
                builder.length_of()?;
                return Err(builder.full());
            }

            let nested = nested.try_borrow()?;
            let byte_length = nested.length_of()?;
            let mut builder = slf.try_borrow_mut()?;
            let alignment = match alignment {
                Some(alignment) => alignment_of(alignment)?,
                None => nested.alignment,
            };
            let index = builder.place_table(byte_length, alignment)?;
            builder.append_builder(py, &nested)?;
            builder.finish_table(index, alignment);
            return Ok(());
        }

        with_bytes(table, |bytes| {
            let mut builder = slf.try_borrow_mut()?;
            let alignment = match alignment {
                Some(alignment) => alignment_of(alignment)?,
                None => 4,
            };
            let index = builder.place_table(bytes.len(), alignment)?;
            builder.push_run(|| Held::Buffer(table.clone().unbind()), bytes)?;
            builder.finish_table(index, alignment);
            Ok(())
        })
    }

    fn set_vector(slf: &Bound<'_, Self>, vector: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let items: Vec<Bound<'_, PyAny>> = vector.try_iter()?.collect::<PyResult<_>>()?;

        let mut lengths = Vec::with_capacity(items.len());
        for item in &items {
            lengths.push(if let Ok(nested) = item.cast::<Self>() {
                if nested.is(slf) {
                    slf.try_borrow()?.length_of()?
                } else {
                    nested.try_borrow()?.length_of()?
                }
            } else {
                with_bytes(item, |bytes| Ok(bytes.len()))?
            });
        }

        let mut builder = slf.try_borrow_mut()?;
        let index = builder.next_index()?;
        let size = builder.size();
        let byte_length = lengths
            .iter()
            .fold(size.saturating_mul(1 + items.len()), |sum, length| {
                sum.saturating_add(*length)
            });
        let offset = builder.offset();
        builder.check_length(index, offset.saturating_add(byte_length))?;

        let ends = items.len() * size;
        builder.room(size + ends)?;
        let slot = builder.slots + index * size;
        builder.write_slot(slot, offset);
        let vector_at = builder.data.len();
        let start = vector_at + builder.shift;
        builder.push_count(items.len());
        builder.push_zeros(ends);

        for (item, table) in items.iter().enumerate() {
            if let Ok(nested) = table.cast::<Self>() {
                builder.append_builder(py, &*nested.try_borrow()?)?;
            } else {
                with_bytes(table, |bytes| {
                    builder.push_run(|| Held::Buffer(table.clone().unbind()), bytes)
                })?;
            }
            let end = builder.data.len() + builder.shift - start;
            builder.write_slot(vector_at + (item + 1) * size, end);
        }
        builder.index = index + 1;
        Ok(())
    }

    #[pyo3(signature = (fill, *args, fields))]
    fn set_table_in_place(
        slf: &Bound<'_, Self>,
        fill: &Bound<'_, PyAny>,
        args: &Bound<'_, PyTuple>,
        fields: usize,
    ) -> PyResult<()> {
        let checkpoint = {
            let mut builder = slf.try_borrow_mut()?;
            let index = builder.next_index()?;
            let checkpoint = builder.checkpoint(index);
            builder.start_table(fields);
            if let Err(error) = builder.start_slots(fields) {
                builder.roll_back(&checkpoint);
                return Err(error);
            }
            checkpoint
        };

        let args: Vec<_> = args.iter_borrowed().collect();
        let filled = call_fill(fill, slf.as_any(), &args);

        let mut builder = slf.try_borrow_mut()?;
        if let Err(error) = filled {
            builder.roll_back(&checkpoint);
            return Err(error);
        }
        builder.end_table(&checkpoint, fields)
    }

    #[pyo3(signature = (values, fill, *, fields))]
    fn set_vector_in_place(
        slf: &Bound<'_, Self>,
        values: &Bound<'_, PyAny>,
        fill: &Bound<'_, PyAny>,
        fields: usize,
    ) -> PyResult<()> {
        let count = values.len()?;
        let checkpoint = slf.try_borrow_mut()?.start_vector(count)?;

        let filled = (|| -> PyResult<()> {
            let mut item = 0;
            for value in values.try_iter()? {
                let value = value?;
                if item == count {
                    return Err(count_error(count));
                }

                // Each item counts its length and alignment from its own
                // start, as a builder does
                let item_start = {
                    let mut builder = slf.try_borrow_mut()?;
                    let item_start = builder.data.len();
                    builder.start_table(fields);
                    builder.start_slots(fields)?;
                    item_start
                };
                call_fill(fill, slf.as_any(), &[value.as_borrowed()])?;
                slf.try_borrow_mut()?
                    .end_item(&checkpoint, item, count, fields, item_start)?;
                item += 1;
            }

            if item != count {
                return Err(count_error(count));
            }
            Ok(())
        })();

        let mut builder = slf.try_borrow_mut()?;
        if let Err(error) = filled {
            builder.roll_back(&checkpoint);
            return Err(error);
        }
        builder.end_vector(&checkpoint)
    }
}

impl DirectTableBuilder {
    /// Pads a nested table of `byte_length` bytes to `alignment` and writes
    /// its offset, after checking that it fits, and returns its index
    fn place_table(&mut self, byte_length: usize, alignment: usize) -> PyResult<usize> {
        let offset = self.offset();
        let padding = offset.wrapping_neg() & (alignment - 1);
        let index = self.next_index()?;
        self.check_length(index, offset + padding + byte_length)?;

        self.room(padding)?;
        self.push_zeros(padding);
        self.write_slot(self.slots + index * self.size(), offset + padding);
        Ok(index)
    }

    const fn finish_table(&mut self, index: usize, alignment: usize) {
        self.index = index + 1;
        if alignment > self.alignment {
            self.alignment = alignment;
        }
    }
}

fn memoryview<'py>(table: &Bound<'py, Table>) -> PyResult<Bound<'py, PyAny>> {
    unsafe {
        Bound::from_owned_ptr_or_err(table.py(), ffi::PyMemoryView_FromObject(table.as_ptr()))
    }
}
