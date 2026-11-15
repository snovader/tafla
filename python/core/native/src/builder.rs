//! The native `TableBuilder`, with the pure-Python one's API, bytes and
//! errors. Each field is written into the table's bytes when it's set, so
//! each setter is one call into native code:
//!
//! - A field's offset is written when it's set. A vector writes its count and
//!   zeros for its ends, and each item's end once the item is written.
//! - A table nested in place is written where its parent ends, unpadded, and
//!   moved forward by its padding once `fill` has set its fields, since only
//!   then is its alignment known. Its offsets count from its own start, so
//!   none of them changes.
//! - Values are copied when they're set, and a nested builder as its bytes.
//! - A setter checks its field before it writes anything. A nested table or
//!   vector whose `fill` raises, or that's incomplete or too long, is undone
//!   by truncating the bytes to where it started.
//!
//! `bytes()` moves the bytes into a `_Table`, which exports them, so that the
//! builder holds no buffer of itself, which a `ByteBuffer` that `finalize()`
//! keeps would hold in a cycle with it.

use std::ffi::c_int;
use std::mem::MaybeUninit;
use std::ptr;

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};

use crate::bytes::{Bytes, memory_error};
use crate::elements::{self, struct_count};
use crate::layout::{Layout, Shape, shape_of};
use crate::little_endian_host;
use crate::python::{Int, buffer_bytes, contiguous_error, objects, utf8, with_bytes};
use crate::values::{self, Kind};
use crate::writer::{alignment, header_length};

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

/// What an in-place setter restores: the end of the bytes before it started,
/// and its parent's counters
struct Checkpoint {
    start: usize,
    base: usize,
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
    shape: Shape,
    layout: Option<Py<PyAny>>,
    written: Option<Written>,
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
    fn set_sequence(
        &mut self,
        code: char,
        item_size: usize,
        values: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let index = self.index;
        let size = self.size();
        let offset = self.offset();
        let padding = (offset + size).wrapping_neg() & (item_size - 1);
        let start = self.data.len + padding + size;

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

        self.data.zeros_within(padding);
        self.write_slot(self.slots + index * size, offset + padding);
        self.push_count(byte_length / item_size);
        debug_assert_eq!(self.data.len, start, "the elements follow the count");
        // SAFETY: convert_sequence() wrote the elements where they start
        unsafe { self.data.extend_written(byte_length)? };
        self.make_little_endian(start, item_size);

        self.index = index + 1;
        self.alignment = self.alignment.max(item_size);
        Ok(true)
    }

    /// Writes an array of the elements' `bytes`, padded so that its
    /// elements, after its count, are aligned
    fn write_array(&mut self, bytes: &[u8], item_size: usize) -> PyResult<()> {
        let index = self.next_index()?;
        let size = self.size();
        let byte_length = bytes.len();
        let offset = self.offset();
        let padding = (offset + size).wrapping_neg() & (item_size - 1);
        self.check_length(index, offset + padding + size + byte_length)?;

        self.room(padding + size + byte_length)?;
        self.data.zeros_within(padding);
        self.write_slot(self.slots + index * size, offset + padding);
        self.push_count(byte_length / item_size);
        let start = self.data.len;
        self.data.push_within(bytes);
        self.make_little_endian(start, item_size);

        self.index = index + 1;
        self.alignment = self.alignment.max(item_size);
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
        }
    }

    /// Starts a table of `fields` fields at the end of the bytes, with zeros
    /// for its `slot_bytes` bytes of slots, for `fill` to set its fields
    fn start_table(&mut self, fields: usize, slot_bytes: usize) -> PyResult<()> {
        self.room(slot_bytes)?;
        let start = self.data.len;
        self.base = start;
        self.slots = start;
        self.fields = fields;
        self.index = 0;
        self.alignment = 1;
        self.data.zeros_within(slot_bytes);
        Ok(())
    }

    fn roll_back(&mut self, checkpoint: &Checkpoint) {
        self.data.truncate(checkpoint.start);
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

        // Padded from its parent's start, as a nested builder is
        let alignment = self.alignment;
        let offset = checkpoint.start - checkpoint.base;
        let padding = offset.wrapping_neg() & (alignment - 1);
        let length = offset + padding + self.data.len - checkpoint.start;
        if length > self.shape.max_length() {
            self.roll_back(checkpoint);
            return Err(self.too_long(checkpoint.index, length));
        }

        if padding > 0 {
            if let Err(error) = self.room(padding) {
                self.roll_back(checkpoint);
                return Err(error);
            }
            self.data.insert_zeros_within(checkpoint.start, padding);
        }
        self.write_slot(
            checkpoint.slots + checkpoint.index * self.size(),
            offset + padding,
        );

        self.base = checkpoint.base;
        self.slots = checkpoint.slots;
        self.fields = checkpoint.fields;
        self.index = checkpoint.index + 1;
        self.alignment = alignment.max(checkpoint.alignment);
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
        Ok(())
    }

    /// Pads a nested table of `byte_length` bytes to `alignment` and writes
    /// its offset, after checking that it fits, and makes room for it
    fn place_table(&mut self, byte_length: usize, alignment: usize) -> PyResult<usize> {
        let offset = self.offset();
        let padding = offset.wrapping_neg() & (alignment - 1);
        let index = self.next_index()?;
        self.check_length(index, offset + padding + byte_length)?;

        self.room(padding + byte_length)?;
        self.data.zeros_within(padding);
        self.write_slot(self.slots + index * self.size(), offset + padding);
        Ok(index)
    }

    const fn finish_table(&mut self, index: usize, alignment: usize) {
        self.index = index + 1;
        if alignment > self.alignment {
            self.alignment = alignment;
        }
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
            buffer: None,
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
fn borrow_to_finish<'py>(slf: &Bound<'py, TableBuilder>) -> PyRefMut<'py, TableBuilder> {
    loop {
        if let Ok(builder) = slf.try_borrow_mut() {
            return builder;
        }
        slf.py()
            .detach(|| std::thread::sleep(std::time::Duration::from_micros(50)));
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
    /// ``layout`` its header and offset type, which its readers must use too.
    /// ``version`` is written to the header, and required when the layout is
    /// versioned.
    #[new]
    #[pyo3(signature = (fields, layout = None, version = None))]
    fn new(
        fields: &Bound<'_, PyAny>,
        layout: Option<&Bound<'_, PyAny>>,
        version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let fields = Int::new(fields)?;
        let parsed = layout.map(PyAnyMethods::extract::<Layout>).transpose()?;
        let shape = shape_of(parsed.as_ref());
        let version = version.map(Int::new).transpose()?;
        let length = header_length(shape, &fields, version.as_ref())?;
        // A negative number of fields that a header's slots outnumber, as
        // bytearray() rejects a negative length
        let Some(field_count) = fields.index() else {
            return Err(PyValueError::new_err("negative count"));
        };

        let mut data = Bytes::zeroed(length)?;
        if let Some(version) = version.and_then(|version| version.index()) {
            let slot = if shape.sized { shape.size() } else { 0 };
            #[allow(clippy::cast_possible_truncation)]
            let bytes = (version as u32).to_le_bytes();
            data.as_mut_slice()[slot..slot + shape.size()].copy_from_slice(&bytes[..shape.size()]);
        }

        Ok(Self {
            data,
            base: 0,
            slots: shape.header_size(),
            fields: field_count,
            index: 0,
            alignment: 1,
            shape,
            layout: layout.map(|layout| layout.clone().unbind()),
            written: None,
        })
    }

    /// Finishes the table, once, and returns a ``ByteBuffer`` that reads it;
    /// later calls return the same buffer.
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

    /// Finishes the table, once, as ``finalize()`` does, and returns its
    /// bytes, without the buffer that ``finalize()`` creates
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
            let padding = (offset + offset_size).wrapping_neg() & (alignment - 1);

            let index = self.next_index()?;
            self.check_length(index, offset + padding + offset_size + byte_length)?;

            self.room(padding + offset_size + byte_length)?;
            self.data.zeros_within(padding);
            self.write_slot(self.slots + index * offset_size, offset + padding);
            self.push_count(count);
            self.data.push_within(bytes);
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
            builder.finish_table(index, alignment);
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
            builder.finish_table(index, alignment);
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
        let (checkpoint, fields) = {
            let mut builder = slf.try_borrow_mut()?;
            let index = builder.next_index()?;
            let offset = builder.offset();
            let (fields, slot_bytes) = builder.slots_of(index, offset, &Int::new(fields)?)?;
            let checkpoint = builder.checkpoint(index);
            if let Err(error) = builder.start_table(fields, slot_bytes) {
                builder.roll_back(&checkpoint);
                return Err(error);
            }
            (checkpoint, fields)
        };

        let filled = call_fill(fill, slf.as_any(), args.iter_borrowed());

        let mut builder = borrow_to_finish(slf);
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
        fields: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let count = values.len()?;
        let fields = Int::new(fields)?;
        let (checkpoint, fields, slot_bytes) =
            slf.try_borrow_mut()?.start_vector(count, &fields)?;

        let filled = (|| -> PyResult<()> {
            let mut item = 0;
            for value in values.try_iter()? {
                let value = value?;
                if item == count {
                    return Err(count_error(count));
                }

                // Each item counts its offsets and alignment from its own
                // start, as a builder does
                borrow_to_finish(slf).start_table(fields, slot_bytes)?;
                call_fill(fill, slf.as_any(), std::iter::once(value.as_borrowed()))?;
                borrow_to_finish(slf).end_item(&checkpoint, item, fields)?;
                item += 1;
            }

            if item != count {
                return Err(count_error(count));
            }
            Ok(())
        })();

        let mut builder = borrow_to_finish(slf);
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
