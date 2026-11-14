use std::ffi::c_int;
use std::fmt::Display;
use std::mem::MaybeUninit;
use std::ptr::{self, NonNull};

use pyo3::exceptions::{
    PyBufferError, PyIndexError, PyMemoryError, PyStopIteration, PyUnicodeEncodeError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PySlice, PyString, PyTuple, PyType};
use pyo3::{IntoPyObjectExt, ffi};

use crate::fixed_fields::{Description, FixedFields, Format, Plan};
use crate::layout::{Layout, Shape, shape_of};
use crate::python::{Int, objects, small, utf8, with_buffer, with_bytes};
use crate::values::{self, Kind};
use crate::{elements, little_endian_host};

/// The bytes of the table being written, which Python's allocator holds, so
/// that `tracemalloc` counts them as it counts a `bytearray`'s. While a
/// buffer of them is exported, by `TableWriter.bytes()`, their length is
/// frozen, so that the exported pointer stays valid, as a `bytearray`'s is.
struct Bytes {
    pointer: NonNull<u8>,
    len: usize,
    capacity: usize,
    /// The buffers of the bytes that are exported and not yet released
    exports: usize,
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

    fn zeroed(len: usize) -> PyResult<Self> {
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

    #[inline]
    const fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.len) }
    }

    /// Raises `BufferError` if the bytes are exported and `len` isn't their
    /// length, as a `bytearray` raises for a change of its length while
    /// it's exported, and for nothing else
    #[inline]
    fn check_length(&self, len: usize) -> PyResult<()> {
        if self.exports > 0 && len != self.len {
            return Err(frozen());
        }
        Ok(())
    }

    /// Sets the length to `len`, with zeros in the bytes it adds. Raises
    /// `BufferError` if that changes the length while the bytes are
    /// exported, as a `bytearray` does.
    #[inline]
    fn resize(&mut self, len: usize) -> PyResult<()> {
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
    fn replace_from(&mut self, start: usize, bytes: &[u8]) -> PyResult<()> {
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
    fn extend_from_slice(&mut self, bytes: &[u8]) -> PyResult<()> {
        self.replace_from(self.len, bytes)
    }

    #[inline]
    fn extend_zeros(&mut self, count: usize) -> PyResult<()> {
        self.resize(self.len.checked_add(count).ok_or_else(memory_error)?)
    }

    /// The `length` bytes of the capacity from `start`, which is at least the
    /// length, grown to hold them, for bytes that `extend_written()` adds to
    /// the length once they're written. Returns `None` if they don't fit in
    /// memory, or if the bytes are exported and would have to grow.
    #[inline]
    fn spare(&mut self, start: usize, length: usize) -> Option<&mut [MaybeUninit<u8>]> {
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
    unsafe fn extend_written(&mut self, length: usize) -> PyResult<()> {
        let len = self.len + length;
        debug_assert!(len <= self.capacity);
        self.check_length(len)?;
        self.len = len;

        Ok(())
    }

    /// Grows the capacity to at least `required`, so that appending is
    /// amortized: by half again while it's small, and by an eighth, as a
    /// `bytearray` grows, once it's large, so that a large table holds
    /// little more than its bytes. A larger append, such as a large array's,
    /// grows it to `required` exactly.
    #[cold]
    fn grow(&mut self, required: usize) -> PyResult<()> {
        const LARGE: usize = 64 * 1024;
        let growth = if self.capacity < LARGE {
            self.capacity / 2
        } else {
            self.capacity / 8
        };
        let capacity = required.max(self.capacity + growth);
        self.pointer = allocate(self.pointer.as_ptr(), capacity)?;
        self.capacity = capacity;

        Ok(())
    }
}

impl Drop for Bytes {
    fn drop(&mut self) {
        // The writer is dropped by its deallocation, with the thread attached
        unsafe { ffi::PyMem_Free(self.pointer.as_ptr().cast()) };
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
fn memory_error() -> PyErr {
    PyMemoryError::new_err(())
}

#[cold]
fn frozen() -> PyErr {
    PyBufferError::new_err("Existing exports of data: object cannot be re-sized")
}

/// The start of a table, or the position of its first slot, as
/// `end_table()` takes it back: the writer's own from `start_table()`, or any
/// other `int`, after which the writer writes where the pure-Python writer
/// does, which keeps it as it's given
enum Mark {
    Small(i128),
    Big(Py<PyAny>),
}

impl Mark {
    const fn at(position: usize) -> Self {
        Self::Small(position as i128)
    }

    fn from_int(int: Int<'_>) -> Self {
        match int {
            Int::Small(value) => Self::Small(value),
            Int::Big(value) => Self::Big(value.unbind()),
        }
    }

    #[inline]
    fn int<'py>(&self, py: Python<'py>) -> Int<'py> {
        match self {
            Self::Small(value) => Int::Small(*value),
            Self::Big(value) => Int::Big(value.bind(py).clone()),
        }
    }

    fn clone_ref(&self, py: Python<'_>) -> Self {
        match self {
            Self::Small(value) => Self::Small(*value),
            Self::Big(value) => Self::Big(value.clone_ref(py)),
        }
    }

    /// The number of zeros that put `position` at a multiple of `alignment`
    /// from this start
    #[inline]
    fn padding(&self, py: Python<'_>, position: usize, alignment: usize) -> PyResult<usize> {
        // Modulo a power of two, which wrapping keeps, and which an int's
        // bits give, as they're its two's complement
        let mask = alignment - 1;
        match self {
            Self::Small(start) => {
                Ok((start.wrapping_sub(position as i128).cast_unsigned() as usize) & mask)
            }
            Self::Big(start) => start.bind(py).sub(position)?.bitand(mask)?.extract(),
        }
    }
}

/// A value or position of a slot that `struct` can't pack, whose error
/// `TableWriter::struct_error()` raises, as `struct` raises it
struct SlotFailure;

/// Writes a table forward, appending each field to its bytes as it's set,
/// for code that knows a table's fields as it writes them, such as the code
/// that tafla-gen generates. Unlike ``TableBuilder``, it doesn't record the
/// fields first, which takes most of a small table's time.
///
/// Fields are set in order, each with its index in its table. A nested table
/// follows its parent's ``set_table()``, between ``start_table()`` and
/// ``end_table()``; the items of a vector follow its ``set_vector()`` the same
/// way, each followed by ``set_item_end()``, which writes where it ends. Each
/// field's offset is written as the writer reaches the field.
///
/// ``set_fields()`` writes a table's leading fields whose offsets depend on no
/// value, which a ``FixedFields`` describes, in one call. ``write_table()``
/// writes a nested table of only those fields whole. Both write the bytes
/// that the setters would, and raise what they would.
///
/// It's the target of the code that tafla-gen generates, whose calls follow
/// from the schema, so it doesn't check them again: every field is set, in
/// index order, each once, and each vector item is followed by its
/// ``set_item_end()``. A field index outside its table, or an item outside its
/// vector, writes an offset or end over another slot or field, so the table's
/// bytes are undefined, though never outside them: a slot past them raises
/// ``ValueError``.
///
/// The bytes grow as fields are appended, and ``bytes()`` returns a view of
/// them, without copying them. While the view is held, a call that would
/// change their length raises ``BufferError``, as a ``bytearray``'s does,
/// and one that wouldn't writes as it would without the view, such as
/// ``start_table(0)``, an empty struct's ``set_struct()``, or
/// ``set_fields()`` for no leading fields. ``set_fields()`` and
/// ``write_table()`` pack every value before they write any, and raise
/// ``BufferError`` only once they have, so for values that can't be
/// written they raise what the calls that they replace would.
///
/// A table too long for the layout's ``offset_type`` raises ``ValueError``,
/// from the setter whose offset or count doesn't fit, from ``start_table()``
/// for slots that don't, or from ``bytes()``. A scalar setter that raised
/// wrote nothing, but another setter that raised may have written part of
/// its field, after which the writer is of no further use.
#[pyclass(module = "tafla._native", name = "TableWriter")]
pub struct TableWriter {
    data: Bytes,
    shape: Shape,
    /// The start of the table being written, which its offsets count from
    table: Mark,
    /// The position of the table's first offset, after the root's header
    slots: Mark,
}

impl TableWriter {
    fn create(shape: Shape, length: usize, version: Option<&Int<'_>>) -> PyResult<Self> {
        let mut writer = Self {
            data: Bytes::zeroed(length)?,
            shape,
            table: Mark::Small(0),
            slots: Mark::at(shape.header_size()),
        };

        if let Some(version) = version {
            let slot = Int::from(if shape.sized { shape.size() } else { 0 });
            // Only a negative number of fields leaves no room for it, and
            // struct raises its error
            if writer.pack_slot(&slot, version).is_err() {
                return Err(Python::attach(|py| writer.struct_error(py, &slot, version)));
            }
        }

        Ok(writer)
    }

    #[inline]
    const fn len(&self) -> usize {
        self.data.len
    }

    #[inline]
    const fn size(&self) -> usize {
        self.shape.size()
    }

    /// The offset of a field that starts at the end, from the table's start
    #[inline]
    fn offset<'py>(&self, py: Python<'py>) -> PyResult<Int<'py>> {
        Int::from(self.len()).sub(self.table.int(py))
    }

    /// The position of field `index`'s slot
    #[inline]
    fn slot_of<'py>(&self, py: Python<'py>, index: &Int<'py>) -> PyResult<Int<'py>> {
        index.mul(self.size())?.add(self.slots.int(py))
    }

    /// The error of a table whose fields make it `length` bytes, too long
    /// for its offset type, which an offset, count or item end then doesn't
    /// fit in
    #[cold]
    fn too_long(&self, length: impl Display) -> PyErr {
        PyValueError::new_err(format!(
            "The table's fields make it {length} bytes, too long for {} offsets",
            self.shape.offset_type
        ))
    }

    /// Writes `value` in the slot at `position`, as `struct` packs it into
    /// the bytes with the offset type: a negative position counts from the
    /// end. Fails, having written nothing, for a value or position that
    /// `struct` rejects, whose error `struct_error()` then raises.
    #[inline]
    fn pack_slot(&mut self, position: &Int<'_>, value: &Int<'_>) -> Result<(), SlotFailure> {
        let size = self.size();
        let len = self.len();
        let Int::Small(position) = *position else {
            return Err(SlotFailure);
        };
        let position = if position < 0 {
            position + len as i128
        } else {
            position
        };
        let Some(position) = usize::try_from(position)
            .ok()
            .filter(|&position| position <= len.saturating_sub(size) && size <= len)
        else {
            return Err(SlotFailure);
        };
        let Some(value) = value
            .index()
            .filter(|&value| value <= self.shape.max_length())
        else {
            return Err(SlotFailure);
        };

        self.data.as_mut_slice()[position..position + size].copy_from_slice(&slot(value)[..size]);
        Ok(())
    }

    /// The error that `struct` raises for packing `value` at `position` in
    /// the bytes, which `pack_slot()` refused. It's `struct.pack_into()`'s
    /// own, from packing into the bytes themselves, which zeroes the slot
    /// for a value that doesn't fit, as it does for the pure-Python writer.
    #[cold]
    fn struct_error(&mut self, py: Python<'_>, position: &Int<'_>, value: &Int<'_>) -> PyErr {
        let kind = match self.size() {
            1 => Kind::U8,
            2 => Kind::U16,
            _ => Kind::U32,
        };
        let pack = || -> PyResult<PyErr> {
            let packing = objects(py)?.struct_of(py, kind);
            // A view of the bytes, which is released before they can move
            let view = unsafe {
                Bound::from_owned_ptr_or_err(
                    py,
                    ffi::PyMemoryView_FromMemory(
                        self.data.pointer.as_ptr().cast(),
                        self.len().cast_signed(),
                        ffi::PyBUF_WRITE,
                    ),
                )?
            };
            let packed = packing.call_method1(
                "pack_into",
                (&view, position.to_object_in(py)?, value.to_object_in(py)?),
            );
            view.call_method0("release")?;

            Ok(match packed {
                Err(error) => error,
                Ok(_) => PyValueError::new_err("struct wrote a slot that the writer refused"),
            })
        };
        pack().unwrap_or_else(|error| error)
    }

    /// The error of a count that the offset type can't hold, which `struct`
    /// raises
    #[cold]
    fn count_error(&self, py: Python<'_>, count: &Int<'_>) -> PyErr {
        let kind = match self.size() {
            1 => Kind::U8,
            2 => Kind::U16,
            _ => Kind::U32,
        };
        let pack = || -> PyResult<PyErr> {
            let packing = objects(py)?.struct_of(py, kind);
            Ok(
                match packing.call_method1("pack", (count.to_object_in(py)?,)) {
                    Err(error) => error,
                    Ok(_) => PyValueError::new_err("struct packed a count that the writer refused"),
                },
            )
        };
        pack().unwrap_or_else(|error| error)
    }

    /// The error of an offset or item end that wasn't written at `position`:
    /// one too large for the offset type, or one whose slot lies outside the
    /// bytes, which only an index or item that isn't its own has
    #[cold]
    fn slot_error(&mut self, py: Python<'_>, position: &Int<'_>, value: &Int<'_>) -> PyErr {
        let cause = self.struct_error(py, position, value);
        let max = Int::from(self.shape.max_length());
        let raised = match max.lt(value.clone()) {
            Ok(true) => self.too_long(self.len()),
            Ok(false) => match value.describe() {
                Ok(described) => PyValueError::new_err(format!(
                    "Can't write {described} in a slot outside the table: {}",
                    cause.value(py)
                )),
                Err(error) => return error,
            },
            Err(error) => return error,
        };
        raised.set_cause(py, Some(cause));
        raised
    }

    /// The error of a scalar that wasn't written, from `struct`'s error for
    /// it or its offset: one too long for the offset type, if the offset is,
    /// as the pure-Python runtime raises it
    #[cold]
    fn scalar_error(&self, value: &Bound<'_, PyAny>, offset: &Int<'_>, cause: PyErr) -> PyErr {
        let py = value.py();
        let error = match Int::from(self.shape.max_length()).lt(offset.clone()) {
            Ok(true) => self.too_long(self.len()),
            Ok(false) => match value.repr() {
                Ok(repr) => PyValueError::new_err(format!(
                    "Can't write {repr} at {}: {}",
                    self.len(),
                    cause.value(py)
                )),
                Err(error) => return error,
            },
            Err(error) => return error,
        };
        error.set_cause(py, Some(cause));
        error
    }

    /// The error of a value that `struct` rejects for `kind`, which `struct`
    /// raises itself
    #[cold]
    fn value_error(&self, kind: Kind, value: &Bound<'_, PyAny>, offset: &Int<'_>) -> PyErr {
        let py = value.py();
        let objects = match objects(py) {
            Ok(objects) => objects,
            Err(error) => return error,
        };

        match objects.struct_of(py, kind).call_method1("pack", (value,)) {
            Err(error) if objects.is_pack_error(py, &error) => {
                self.scalar_error(value, offset, error)
            }
            Err(error) => error,
            // The conversion rejected a value that struct packs, which they
            // agree on for every value that the tests try
            Ok(_) => PyValueError::new_err(format!("Can't write the value as {kind:?}")),
        }
    }

    /// Writes a scalar field's packed bytes, after its offset. `value` is
    /// the value that they're packed from, which an error names.
    #[inline]
    fn set_packed(
        &mut self,
        index: &Bound<'_, PyAny>,
        packed: &[u8],
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let py = value.py();
        let offset = self.offset(py)?;
        let position = self.slot_of(py, &Int::new(index)?)?;

        if self.pack_slot(&position, &offset).is_err() {
            let cause = self.struct_error(py, &position, &offset);
            return Err(self.scalar_error(value, &offset, cause));
        }
        self.data.extend_from_slice(packed)
    }

    /// Writes a scalar, which packs before it writes its offset, so that a
    /// value that `struct` rejects writes nothing, and raises before its
    /// index is read, as the pure-Python runtime's setters do
    #[inline]
    fn set_scalar(
        &mut self,
        kind: Kind,
        index: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let mut packed = [0u8; 8];
        let packed = &mut packed[..kind.size()];
        if !values::write(kind, value, packed)? {
            let offset = self.offset(value.py())?;
            return Err(self.value_error(kind, value, &offset));
        }

        self.set_packed(index, packed, value)
    }

    /// Writes the offset of field `index`, which is the table's end
    #[inline]
    fn set_offset(&mut self, py: Python<'_>, index: &Int<'_>) -> PyResult<()> {
        let value = self.offset(py)?;
        let position = self.slot_of(py, index)?;

        if self.pack_slot(&position, &value).is_err() {
            return Err(self.slot_error(py, &position, &value));
        }
        Ok(())
    }

    /// Appends a count or length, as wide as the offsets, of the
    /// `byte_length` bytes that follow it
    #[inline]
    fn set_count(&mut self, count: &Int<'_>, byte_length: &Int<'_>) -> PyResult<()> {
        let size = self.size();
        if let Some(count) = count
            .index()
            .filter(|&count| count <= self.shape.max_length())
        {
            return self.data.extend_from_slice(&slot(count)[..size]);
        }

        let length = byte_length.add(self.len() + size)?;
        let error = self.too_long(length.display()?);
        // SAFETY: the thread is attached in every method
        let py = unsafe { Python::assume_attached() };
        error.set_cause(py, Some(self.count_error(py, count)));
        Err(error)
    }

    /// Pads so that `position` lies at a multiple of `alignment` from the
    /// start of the table
    #[inline]
    fn pad(&mut self, py: Python<'_>, position: usize, alignment: usize) -> PyResult<()> {
        let padding = self.table.padding(py, position, alignment)?;
        if padding > 0 {
            self.data.extend_zeros(padding)?;
        }
        Ok(())
    }

    /// Pads an array so that its `byte_length` bytes of elements, after its
    /// count, are aligned, and writes its offset and count
    fn start_array(
        &mut self,
        py: Python<'_>,
        index: &Int<'_>,
        count: &Int<'_>,
        byte_length: &Int<'_>,
        alignment: usize,
    ) -> PyResult<()> {
        self.pad(py, self.len() + self.size(), alignment)?;
        self.set_offset(py, index)?;
        self.set_count(count, byte_length)
    }

    /// Appends elements in the host's byte order as little-endian ones
    fn append_elements(&mut self, elements: &[u8], item_size: usize) -> PyResult<()> {
        let start = self.len();
        self.data.extend_from_slice(elements)?;
        self.make_little_endian(start, item_size);
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

    fn set_array(
        &mut self,
        py: Python<'_>,
        index: &Bound<'_, PyAny>,
        code: char,
        item_size: usize,
        values: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let index = Int::new(index)?;
        if self.set_sequence(py, &index, code, item_size, values)? {
            return Ok(());
        }

        elements::with_elements(values, code, item_size, |elements| {
            let length = elements.len();
            self.start_array(
                py,
                &index,
                &Int::from(length / item_size),
                &Int::from(length),
                item_size,
            )?;
            self.append_elements(elements, item_size)
        })
    }

    /// Writes an array from an exact list or tuple whose elements
    /// `elements::convert_sequence()` converts, straight into the spare
    /// capacity where they follow the array's padding and count, before
    /// either is written, rather than into an `array` that's copied again.
    /// Returns `false`, having written nothing, for elements that it doesn't
    /// convert, so that `with_elements()` converts them, and raises what the
    /// pure-Python runtime raises.
    fn set_sequence(
        &mut self,
        py: Python<'_>,
        index: &Int<'_>,
        code: char,
        item_size: usize,
        values: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        // The elements follow the padding and the count, which start_array()
        // writes once they're converted
        let unpadded = self.len() + self.size();
        let mut start = 0;
        let table = &self.table;
        let data = &mut self.data;
        let destination = |byte_length| {
            // Moved into the call, so that the capacity it returns outlives it
            let data = data;
            start = unpadded + table.padding(py, unpadded, item_size).ok()?;
            data.spare(start, byte_length)
        };
        let Some(byte_length) = elements::convert_sequence(values, code, destination) else {
            return Ok(false);
        };

        let count = Int::from(byte_length / item_size);
        self.start_array(py, index, &count, &Int::from(byte_length), item_size)?;
        assert_eq!(self.len(), start, "the elements follow the count");
        // SAFETY: convert_sequence() wrote the elements where they start
        unsafe { self.data.extend_written(byte_length)? };
        self.make_little_endian(start, item_size);

        Ok(true)
    }

    fn set_table_at(&mut self, py: Python<'_>, index: &Int<'_>, alignment: usize) -> PyResult<()> {
        self.pad(py, self.len(), alignment)?;
        self.set_offset(py, index)
    }

    /// Starts a table at the end, and returns its parent's start
    fn begin(&mut self) -> Mark {
        let len = self.len();
        self.slots = Mark::at(len);
        std::mem::replace(&mut self.table, Mark::at(len))
    }

    fn start(&mut self, fields: usize) -> PyResult<Mark> {
        let parent = self.begin();
        let slots = Int::from(fields).mul(self.size())?;
        self.data.extend_zeros(self.zeros(&slots)?)?;

        Ok(parent)
    }

    /// The number of zeros that the pure-Python writer appends for `slots`
    /// bytes of slots: a tuple's item, of zeros by their number up to 64,
    /// which a negative number indexes from its end, and `bytes(slots)`
    /// beyond, unless they're longer than the offset type addresses, which
    /// raises `ValueError`, as the table that they start is too long. Only a
    /// count of fields outside the contract, which is never negative, takes
    /// another than `slots`, so that the writers' bytes and errors agree.
    #[inline]
    fn zeros(&self, slots: &Int<'_>) -> PyResult<usize> {
        // The tuple's, which every offset type addresses, as the pure-Python
        // writer appends them without a check
        if let Int::Small(slots @ 0..ZEROS_TUPLE) = *slots {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            return Ok(slots as usize);
        }
        self.zeros_beyond(slots)
    }

    /// `zeros()` for slots that the tuple's zeros don't hold
    #[cold]
    fn zeros_beyond(&self, slots: &Int<'_>) -> PyResult<usize> {
        const BEYOND: &str = "cannot fit 'int' into an index-sized integer";

        if Int::from(self.shape.max_length()).lt(slots.clone())? {
            return Err(self.too_long(slots.add(self.len())?.display()?));
        }

        // Within the offset type, only a negative number passes an isize
        let slots = match *slots {
            Int::Small(slots) if isize::try_from(slots).is_ok() => slots,
            _ => return Err(PyIndexError::new_err(BEYOND)),
        };
        let count = if slots < 0 {
            ZEROS_TUPLE + slots
        } else {
            slots
        };
        usize::try_from(count).map_err(|_| PyIndexError::new_err("tuple index out of range"))
    }

    fn end(&mut self, py: Python<'_>, parent: Mark) {
        // Only the root starts at 0, and only its offsets follow a header
        self.slots = if matches!(parent, Mark::Small(0)) {
            Mark::at(self.shape.header_size())
        } else {
            parent.clone_ref(py)
        };
        self.table = parent;
    }

    /// Where `set_fields()` writes the fields, at the table's slots, as a
    /// slice of the bytes from them starts: from the end for a negative
    /// position, and at the end for one past it
    fn slots_start(&self, py: Python<'_>) -> PyResult<usize> {
        let len = self.len();
        let slots = self.slots.int(py);
        if slots.is_negative()? {
            return Ok(slots.add(len)?.index().unwrap_or(0));
        }
        Ok(slots.index().map_or(len, |slots| slots.min(len)))
    }

    fn set_string_at(&mut self, py: Python<'_>, index: &Int<'_>, encoded: &[u8]) -> PyResult<()> {
        let length = Int::from(encoded.len());
        self.set_offset(py, index)?;
        self.set_count(&length, &length)?;
        self.data.extend_from_slice(encoded)
    }

    fn set_struct_at(
        &mut self,
        py: Python<'_>,
        index: &Int<'_>,
        data: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.set_offset(py, index)?;
        with_bytes(data, |bytes| self.data.extend_from_slice(bytes))
    }

    /// Writes the fields that `plan` describes at `start`, which are the
    /// table's slots, from the first `count` of `values` and `string`, as
    /// `struct` packs them for the pure-Python runtime. Returns `false`,
    /// having written nothing, for fields that it rejects, which the setters
    /// write instead, raising what they raise.
    fn write_fixed(
        &mut self,
        plan: &Plan,
        start: usize,
        values: &Bound<'_, PyTuple>,
        count: usize,
        string: Option<&Bound<'_, PyString>>,
    ) -> PyResult<bool> {
        let py = values.py();
        if count != plan.values.len() || !plan.fits {
            return Ok(false);
        }

        let encoded = match (plan.string_length, string) {
            (Some(_), Some(string)) => match utf8(string) {
                Ok(encoded) => encoded,
                Err(error) if error.is_instance_of::<PyUnicodeEncodeError>(py) => {
                    return Ok(false);
                }
                Err(error) => return Err(error),
            },
            _ => &[],
        };
        if encoded.len() > self.shape.max_length() {
            return Ok(false);
        }

        // The fields are packed apart from the bytes, and replace them from
        // the slots only once every value has packed, as the pure-Python
        // runtime packs them before it writes any. So values that struct
        // rejects write nothing, and fall back to the setters, before a
        // view of the bytes that's held can fail the write, which it does
        // only if the write changes their length.
        let mut stack = [0u8; 64];
        let mut heap = Vec::new();
        let packed = if plan.len <= stack.len() {
            &mut stack[..plan.len]
        } else {
            heap.try_reserve_exact(plan.len)
                .map_err(|_| memory_error())?;
            heap.resize(plan.len, 0);
            &mut heap[..]
        };
        if !Self::pack_fixed(plan, packed, values)? {
            return Ok(false);
        }
        if let Some(at) = plan.string_length {
            let size = plan.size;
            packed[at..at + size].copy_from_slice(&slot(encoded.len())[..size]);
        }

        self.data.replace_from(start, packed)?;
        // Only a string's bytes follow the fields. Appending none for every
        // table made set_vector_tables() of 100 tables without strings 6%
        // slower.
        if !encoded.is_empty() {
            self.data.extend_from_slice(encoded)?;
        }

        Ok(true)
    }

    /// Packs the slots and values of `plan` into `packed`, its zeroed bytes.
    /// Returns `false` for a value that `struct` rejects.
    fn pack_fixed(plan: &Plan, packed: &mut [u8], values: &Bound<'_, PyTuple>) -> PyResult<bool> {
        let size = plan.size;

        for &(at, offset) in &plan.slots {
            packed[at..at + size].copy_from_slice(&slot(offset)[..size]);
        }

        for (index, &(at, kind)) in plan.values.iter().enumerate() {
            let value = tuple_item(values, index);
            if !values::write(kind, &value, &mut packed[at..at + kind.size()])? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Writes the fields that `description` describes with the setters,
    /// from `values`, as they're written without `FixedFields`
    fn set_each(
        &mut self,
        description: &Description,
        values: &mut Values<'_, '_>,
        string: Option<&Bound<'_, PyString>>,
    ) -> PyResult<()> {
        let py = values.tuple.py();
        for (index, format) in description.formats.iter().enumerate() {
            match format {
                Format::Table(nested) => {
                    // A table of fixed-size fields has no arrays to align
                    self.set_table_at(py, &Int::from(index), 1)?;
                    let parent = self.start(nested.fields)?;
                    self.set_each(nested, values, None)?;
                    self.end(py, parent);
                }
                Format::Value(Kind::Bool) => {
                    let truth = u8::from(values.next()?.is_truthy()?);
                    let value = truth.into_bound_py_any(py)?;
                    self.set_packed(&index.into_bound_py_any(py)?, &[truth], &value)?;
                }
                Format::Value(Kind::Struct(_)) => {
                    self.set_struct_at(py, &Int::from(index), &values.next()?)?;
                }
                Format::Value(kind) => {
                    let value = values.next()?;
                    self.set_scalar(*kind, &index.into_bound_py_any(py)?, &value)?;
                }
            }
        }

        if description.string {
            let index = Int::from(description.formats.len());
            let encoded = match string {
                Some(string) => utf8(string)?,
                None => &[],
            };
            self.set_string_at(py, &index, encoded)?;
        }

        Ok(())
    }

    /// Writes a nested table of `fields`, from the first `count` of `values`
    /// and `string`, as `write_table()` does
    fn write_item(
        &mut self,
        fields: &FixedFields,
        values: &Bound<'_, PyTuple>,
        count: usize,
        string: Option<&Bound<'_, PyString>>,
    ) -> PyResult<()> {
        let plan = fields.plan(self.size())?;
        if self.write_fixed(plan, self.len(), values, count, string)? {
            return Ok(());
        }

        let description = fields.description();
        let parent = self.start(description.fields)?;
        self.set_each(description, &mut Values::first(values, count), string)?;
        self.end(values.py(), parent);

        Ok(())
    }

    /// Writes an item of `set_vector_tables()`: its table's values, followed
    /// by its string when `has_string`. An exact tuple's values and string
    /// are written in place, borrowed from it, and any other sequence's are
    /// sliced as the pure-Python runtime slices them, with the same errors.
    fn write_vector_item(
        &mut self,
        fields: &FixedFields,
        values: &Bound<'_, PyAny>,
        has_string: bool,
    ) -> PyResult<()> {
        if let Ok(tuple) = values.cast_exact::<PyTuple>() {
            if !has_string {
                return self.write_item(fields, tuple, tuple.len(), None);
            }
            if let Some(count) = tuple.len().checked_sub(1) {
                let string = tuple_item(tuple, count);
                return self.write_item(fields, tuple, count, Some(&*string.cast::<PyString>()?));
            }
        }

        let (tuple, string) = if has_string {
            let table = values.get_item(PySlice::new(values.py(), 0, -1, 1))?;
            let string = values.get_item(-1)?.cast_into::<PyString>()?;
            (Self::fixed_values(&table)?, Some(string))
        } else {
            (Self::fixed_values(values)?, None)
        };
        self.write_item(fields, &tuple, tuple.len(), string.as_ref())
    }

    fn fixed_values<'py>(values: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyTuple>> {
        match values.cast::<PyTuple>() {
            Ok(tuple) => Ok(tuple.clone()),
            Err(_) => PyTuple::new(
                values.py(),
                values.try_iter()?.collect::<PyResult<Vec<_>>>()?,
            ),
        }
    }
}

/// The bytes of a message of only the fields of `fixed`, in the layout of
/// `shape`, packed into new bytes at once, or `None` when a value doesn't
/// fit, the version isn't as the layout needs or the message is longer than
/// it addresses, for the writer's calls to raise for
fn pack_message<'py>(
    fixed: &FixedFields,
    shape: Shape,
    values: &Bound<'py, PyTuple>,
    string: Option<&Bound<'py, PyString>>,
    version: Option<&Bound<'py, PyAny>>,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    let py = values.py();
    let size = shape.size();
    let plan = fixed.plan(size)?;
    if values.len() != plan.values.len() || !plan.fits || shape.versioned != version.is_some() {
        return Ok(None);
    }
    // Any version but an int that fits is the writer's to raise for
    let version = match version {
        None => None,
        Some(version) => match small(version)
            .ok()
            .flatten()
            .and_then(|version| usize::try_from(version).ok())
        {
            Some(version) if version <= shape.max_length() => Some(version),
            _ => return Ok(None),
        },
    };
    let encoded = match (plan.string_length, string) {
        (Some(_), Some(string)) => match utf8(string) {
            Ok(encoded) => encoded,
            Err(error) if error.is_instance_of::<PyUnicodeEncodeError>(py) => return Ok(None),
            Err(error) => return Err(error),
        },
        _ => &[],
    };
    let header = shape.header_size();
    let length = header + plan.len + encoded.len();
    if length > shape.max_length() {
        return Ok(None);
    }

    // new_with() zeroes the bytes, which the header, the plan and the
    // string fill, before Python sees them
    let mut fits = true;
    let bytes = PyBytes::new_with(py, length, |bytes| {
        if shape.sized {
            bytes[..size].copy_from_slice(&slot(length)[..size]);
        }
        if let Some(version) = version {
            let at = if shape.sized { size } else { 0 };
            bytes[at..at + size].copy_from_slice(&slot(version)[..size]);
        }

        let block = &mut bytes[header..];
        for &(at, offset) in &plan.slots {
            block[at..at + size].copy_from_slice(&slot(offset)[..size]);
        }
        for (index, &(at, kind)) in plan.values.iter().enumerate() {
            let value = tuple_item(values, index);
            if !values::write(kind, &value, &mut block[at..at + kind.size()])? {
                fits = false;
                return Ok(());
            }
        }
        if let Some(at) = plan.string_length {
            block[at..at + size].copy_from_slice(&slot(encoded.len())[..size]);
            block[plan.len..].copy_from_slice(encoded);
        }
        Ok(())
    })?;

    Ok(fits.then_some(bytes))
}

/// The length of the pure-Python writer's tuple of zeros by their number,
/// which holds the slots of a table of up to 16 uint32 fields
const ZEROS_TUPLE: i128 = 16 * 4 + 1;

/// The little-endian bytes of a value that fits in its offset type, the
/// first of which a slot holds
#[inline]
const fn slot(value: usize) -> [u8; 4] {
    debug_assert!(value <= u32::MAX as usize);
    #[allow(clippy::cast_possible_truncation)]
    (value as u32).to_le_bytes()
}

/// The order of a C-contiguous buffer, as `PyBuffer_IsContiguous()` takes it
#[allow(clippy::cast_possible_wrap)]
const C_ORDER: std::ffi::c_char = b'C' as std::ffi::c_char;

/// The values of fixed fields, the first `end` of a tuple, which the setters
/// take one by one, as the pure-Python runtime takes them from an iterator
struct Values<'a, 'py> {
    tuple: &'a Bound<'py, PyTuple>,
    next: usize,
    end: usize,
}

impl<'a, 'py> Values<'a, 'py> {
    fn all(tuple: &'a Bound<'py, PyTuple>) -> Self {
        Self::first(tuple, tuple.len())
    }

    const fn first(tuple: &'a Bound<'py, PyTuple>, end: usize) -> Self {
        Self {
            tuple,
            next: 0,
            end,
        }
    }

    fn next(&mut self) -> PyResult<Bound<'py, PyAny>> {
        if self.next >= self.end {
            return Err(PyStopIteration::new_err(()));
        }
        let value = tuple_item(self.tuple, self.next).to_owned();
        self.next += 1;
        Ok(value)
    }
}

/// Item `index` of `tuple`, which has it, borrowed from it
#[inline]
fn tuple_item<'a, 'py>(tuple: &'a Bound<'py, PyTuple>, index: usize) -> Borrowed<'a, 'py, PyAny> {
    #[cfg(not(Py_LIMITED_API))]
    unsafe {
        tuple.get_borrowed_item_unchecked(index)
    }
    #[cfg(Py_LIMITED_API)]
    tuple
        .get_borrowed_item(index)
        .expect("the tuple has the item")
}

/// The alignment of a struct array or nested table, which must be the int
/// 1, 2, 4 or 8, or `ValueError`
fn alignment(alignment: &Bound<'_, PyAny>) -> PyResult<usize> {
    // The type comes first, as in the pure-Python runtime, since 2.0 and
    // True compare equal to alignments
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

/// The byte length of the header of a table of `fields` fields in `shape`,
/// whose writer puts `version` in the header, as the pure-Python runtime's
/// `header_length()` checks it
fn header_length(shape: Shape, fields: &Int<'_>, version: Option<&Int<'_>>) -> PyResult<usize> {
    let name = shape.offset_type.name();
    if shape.versioned != version.is_some() {
        return Err(pyo3::exceptions::PyTypeError::new_err(if shape.versioned {
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

    let length = fields.mul(shape.size())?.add(shape.header_size())?;
    if Int::from(shape.max_length()).lt(length.clone())? {
        return Err(PyValueError::new_err(format!(
            "A header of {} fields doesn't fit in {name} offsets",
            fields.display()?
        )));
    }

    // A negative number of fields, as bytearray() rejects it
    match length {
        Int::Small(length) if isize::try_from(length).is_ok() => {
            usize::try_from(length).map_err(|_| PyValueError::new_err("negative count"))
        }
        _ => Err(pyo3::exceptions::PyOverflowError::new_err(
            "cannot fit 'int' into an index-sized integer",
        )),
    }
}

macro_rules! scalar_setters {
    ($($name:ident: $kind:expr),* $(,)?) => {
        #[pymethods]
        impl TableWriter {
            $(
                fn $name(&mut self, index: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<()> {
                    self.set_scalar($kind, index, value)
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
        impl TableWriter {
            $(
                fn $name(
                    &mut self,
                    py: Python<'_>,
                    index: &Bound<'_, PyAny>,
                    values: &Bound<'_, PyAny>,
                ) -> PyResult<()> {
                    self.set_array(py, index, $code, $size, values)
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
impl TableWriter {
    /// ``fields`` is the number of fields of the table, and ``layout`` its
    /// header and offset type, which its readers must use too. ``version`` is
    /// written to the header, and required when the layout is versioned.
    ///
    /// Raises ``TypeError`` if ``version`` is given for a layout without a
    /// version slot, or missing for one with it. Raises ``ValueError`` if
    /// ``version`` doesn't fit in the layout's offset type, or the header alone
    /// is longer than the layout addresses.
    #[new]
    #[pyo3(signature = (fields, layout = None, version = None))]
    fn new(
        fields: &Bound<'_, PyAny>,
        layout: Option<Layout>,
        version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let shape = shape_of(layout.as_ref());
        let version = version.map(Int::new).transpose()?;
        let length = header_length(shape, &Int::new(fields)?, version.as_ref())?;

        Self::create(shape, length, version.as_ref())
    }

    /// Returns a writer as the constructor does, for arguments that
    /// ``TableBuilder``'s constructor checked already, without checking them
    /// again.
    #[classmethod]
    #[pyo3(signature = (fields, layout, version))]
    fn _checked(
        _cls: &Bound<'_, PyType>,
        fields: usize,
        layout: Layout,
        version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let shape = layout.shape();
        let length = fields
            .checked_mul(shape.size())
            .and_then(|slots| slots.checked_add(shape.header_size()))
            .ok_or_else(memory_error)?;
        let version = version.map(Int::new).transpose()?;

        Self::create(shape, length, version.as_ref())
    }

    /// Returns a view of the table's bytes, without copying them. While
    /// it's held, a field that would change their length raises
    /// ``BufferError``, as a ``bytearray``'s does. ``memoryview(writer)``
    /// returns the same view.
    ///
    /// Raises ``ValueError`` if the table is longer than its layout addresses.
    fn bytes<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        // Exports the bytes through __getbuffer__, which finishes the header
        unsafe {
            Bound::from_owned_ptr_or_err(slf.py(), ffi::PyMemoryView_FromObject(slf.as_ptr()))
        }
    }

    /// Exports the table's bytes, as ``bytes()`` returns them, writing its
    /// length in the header first, and freezes their length until the
    /// buffer is released
    unsafe fn __getbuffer__(
        slf: Bound<'_, Self>,
        view: *mut ffi::Py_buffer,
        flags: c_int,
    ) -> PyResult<()> {
        let py = slf.py();
        let mut writer = slf.try_borrow_mut()?;
        let len = writer.len();
        if len > writer.shape.max_length() {
            return Err(writer.too_long(len));
        }
        // Only a negative number of fields leaves no room for the length,
        // and struct raises its error
        if writer.shape.sized && writer.pack_slot(&Int::from(0), &Int::from(len)).is_err() {
            return Err(writer.struct_error(py, &Int::from(0), &Int::from(len)));
        }

        let buffer = writer.data.pointer.as_ptr().cast();
        if unsafe {
            ffi::PyBuffer_FillInfo(view, slf.as_ptr(), buffer, len.cast_signed(), 0, flags)
        } != 0
        {
            return Err(PyErr::fetch(py));
        }
        writer.data.exports += 1;
        Ok(())
    }

    /// Releases a buffer that ``__getbuffer__()`` exported. A release that
    /// can't borrow the writer, since another thread's call is using it,
    /// leaves the length frozen, which is safe.
    unsafe fn __releasebuffer__(mut slf: PyRefMut<'_, Self>, _view: *mut ffi::Py_buffer) {
        slf.data.exports = slf.data.exports.saturating_sub(1);
    }

    /// Writes a table that was written already, from its bytes as they are,
    /// in place of the fields that ``start_table()`` and ``end_table()``
    /// would enclose. ``TableBuilder`` copies the tables that it can't write
    /// in place with it.
    fn _copy_table(&mut self, table: &Bound<'_, PyAny>) -> PyResult<()> {
        with_bytes(table, |bytes| self.data.extend_from_slice(bytes))
    }

    /// Starts the fields of a nested table or vector item of ``fields``
    /// fields, after its parent's ``set_table()`` or ``set_vector()``, and
    /// returns the parent's start, which ``end_table()`` takes back.
    ///
    /// Raises ``ValueError`` if the table's slots alone are longer than the
    /// layout addresses, as the constructor raises for the root's header,
    /// rather than allocating them.
    fn start_table<'py>(
        &mut self,
        py: Python<'py>,
        fields: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let slots = Int::new(fields)?.mul(self.size())?;
        let parent = self.begin().int(py).to_object(py)?;
        self.data.extend_zeros(self.zeros(&slots)?)?;

        Ok(parent)
    }

    /// Ends the fields of the table that ``start_table()`` started
    fn end_table(&mut self, py: Python<'_>, parent: &Bound<'_, PyAny>) -> PyResult<()> {
        let parent = Mark::from_int(Int::new(parent)?);
        self.end(py, parent);
        Ok(())
    }

    /// Writes the leading fields that ``fields`` describes, from their
    /// ``values`` in order, with a nested table's values in place of it, and
    /// ``string`` when they end with a string, as their setters would. It's
    /// called first, after the constructor or ``start_table()``, in place of
    /// those fields' setters, and the table's other fields follow with
    /// theirs.
    #[pyo3(signature = (fields, values, string = None))]
    fn set_fields(
        &mut self,
        fields: &Bound<'_, FixedFields>,
        values: &Bound<'_, PyAny>,
        string: Option<&Bound<'_, PyString>>,
    ) -> PyResult<()> {
        let fields = fields.get();
        let plan = fields.plan(self.size())?;
        let values = Self::fixed_values(values)?;

        let start = self.slots_start(values.py())?;
        if self.write_fixed(plan, start, &values, values.len(), string)? {
            return Ok(());
        }

        let mut values = Values::all(&values);
        self.set_each(fields.description(), &mut values, string)
    }

    /// Writes a table of only the fields that ``fields`` describes, a
    /// message's, in the layout that it was created with, from their
    /// ``values`` as ``set_fields()`` takes them, and ``string`` when they
    /// end with one, and returns its bytes, in one call. They're the bytes
    /// that ``TableWriter(fields, layout, version)``, ``set_fields()`` and
    /// ``bytes()`` write, and it raises as they do.
    ///
    /// Raises ``TypeError`` if ``fields`` has no layout, as a nested table's
    /// doesn't.
    ///
    /// Generated code calls it to write a message of only such fields.
    #[staticmethod]
    #[pyo3(signature = (fields, values, string = None, version = None))]
    fn write_fields<'py>(
        fields: &Bound<'py, FixedFields>,
        values: &Bound<'py, PyAny>,
        string: Option<&Bound<'py, PyString>>,
        version: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let py = fields.py();
        let fixed = fields.get();
        let shape = fixed.shape()?;
        let tuple = Self::fixed_values(values)?;
        if let Some(bytes) = pack_message(fixed, shape, &tuple, string, version)? {
            return Ok(bytes);
        }

        // The writer's calls, which raise for what doesn't fit
        let version = version.map(Int::new).transpose()?;
        let fields_count = Int::from(fixed.description().fields);
        let length = header_length(shape, &fields_count, version.as_ref())?;
        let writer = Bound::new(py, Self::create(shape, length, version.as_ref())?)?;
        writer.borrow_mut().set_fields(fields, &tuple, string)?;
        let view = Self::bytes(&writer)?;
        unsafe {
            Ok(
                Bound::from_owned_ptr_or_err(py, ffi::PyBytes_FromObject(view.as_ptr()))?
                    .cast_into_unchecked(),
            )
        }
    }

    /// Writes a nested table whose fields ``fields`` describes whole, from
    /// their ``values`` as ``set_fields()`` takes them, in place of
    /// ``start_table()``, their setters and ``end_table()``: after its
    /// parent's ``set_table()``, or as a vector item before its
    /// ``set_item_end()``.
    #[pyo3(signature = (fields, values, string = None))]
    fn write_table(
        &mut self,
        fields: &Bound<'_, FixedFields>,
        values: &Bound<'_, PyAny>,
        string: Option<&Bound<'_, PyString>>,
    ) -> PyResult<()> {
        let values = Self::fixed_values(values)?;
        self.write_item(fields.get(), &values, values.len(), string)
    }

    /// Writes ``string`` as UTF-8. Raises ``UnicodeEncodeError``, a
    /// ``ValueError``, if it has a lone surrogate, which UTF-8 can't encode.
    fn set_string(
        &mut self,
        py: Python<'_>,
        index: &Bound<'_, PyAny>,
        string: &Bound<'_, PyString>,
    ) -> PyResult<()> {
        let encoded = utf8(string)?;
        self.set_string_at(py, &Int::new(index)?, encoded)
    }

    /// Writes a string that's encoded as UTF-8 already, as ``set_string()``
    /// writes it. ``TableBuilder`` encodes a string outside ASCII when it's
    /// set, to count its bytes, and writes them with it.
    fn _set_encoded_string(
        &mut self,
        py: Python<'_>,
        index: &Bound<'_, PyAny>,
        encoded: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let index = Int::new(index)?;
        with_bytes(encoded, |bytes| self.set_string_at(py, &index, bytes))
    }

    /// Writes a struct field from its bytes: its scalars, little-endian, one
    /// after another, without padding. A ``struct.Struct`` with a ``<`` format
    /// packs them.
    fn set_struct(
        &mut self,
        py: Python<'_>,
        index: &Bound<'_, PyAny>,
        data: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.set_struct_at(py, &Int::new(index)?, data)
    }

    /// Writes an array of structs of ``size`` bytes each, from their bytes
    /// one after another, padded so that they start at a multiple of
    /// ``alignment`` from the start of the table.
    ///
    /// Raises ``ValueError`` if ``size`` is less than 1, ``data`` isn't a
    /// whole number of structs, or ``alignment`` isn't 1, 2, 4 or 8.
    fn set_struct_array(
        &mut self,
        py: Python<'_>,
        index: &Bound<'_, PyAny>,
        data: &Bound<'_, PyAny>,
        size: &Bound<'_, PyAny>,
        alignment: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let index = Int::new(index)?;
        let size = Int::new(size)?;

        // Any buffer, as memoryview() takes it to count its bytes, which must
        // be C-contiguous to be written
        with_buffer(data, ffi::PyBUF_FULL_RO, |view| {
            let byte_length = view.len.unsigned_abs();
            let count = crate::elements::struct_count(byte_length, &size)?;
            let alignment = self::alignment(alignment)?;

            self.start_array(py, &index, &count, &Int::from(byte_length), alignment)?;
            if unsafe { ffi::PyBuffer_IsContiguous(view, C_ORDER) } == 0 {
                return Err(crate::python::not_contiguous());
            }
            self.data
                .extend_from_slice(unsafe { crate::python::buffer_bytes(view) })
        })
    }

    /// Writes the offset of a nested table, padded to start at a multiple
    /// of ``alignment``, whose fields then follow between ``start_table()``
    /// and ``end_table()``.
    ///
    /// Raises ``ValueError`` if ``alignment`` isn't 1, 2, 4 or 8.
    fn set_table(
        &mut self,
        py: Python<'_>,
        index: &Bound<'_, PyAny>,
        alignment: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let alignment = self::alignment(alignment)?;
        self.set_table_at(py, &Int::new(index)?, alignment)
    }

    /// Writes the offset and count of a vector of ``count`` tables, whose
    /// items then follow, each between ``start_table()`` and ``end_table()``
    /// and then ``set_item_end()``. Returns the vector's position, which
    /// ``set_item_end()`` takes.
    fn set_vector(
        &mut self,
        py: Python<'_>,
        index: &Bound<'_, PyAny>,
        count: &Bound<'_, PyAny>,
    ) -> PyResult<usize> {
        let index = Int::new(index)?;
        let count = Int::new(count)?;
        self.set_offset(py, &index)?;

        let vector = self.len();
        let ends = count.mul(self.size())?;
        self.set_count(&count, &ends)?;
        // The count fits in the offset type, so its ends do in a usize
        self.data.extend_zeros(ends.index().unwrap_or(0))?;

        Ok(vector)
    }

    /// Writes the end of item ``item`` of the vector, which was just
    /// written
    fn set_item_end(
        &mut self,
        py: Python<'_>,
        vector: &Bound<'_, PyAny>,
        item: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let vector = Int::new(vector)?;
        let position = Int::new(item)?
            .add(1)?
            .mul(self.size())?
            .add(vector.clone())?;
        let value = Int::from(self.len()).sub(vector)?;

        if self.pack_slot(&position, &value).is_err() {
            return Err(self.slot_error(py, &position, &value));
        }
        Ok(())
    }

    /// Writes field ``index`` as a vector of tables whose fields ``fields``
    /// describes whole, one for each of ``items``, as ``set_vector()``,
    /// ``write_table()`` for each item and ``set_item_end()`` write it. Each
    /// item is a table's values as ``write_table()`` takes them, followed by
    /// its string when ``fields`` ends with one: the tuple that
    /// ``ByteBuffer.get_fields()`` returns.
    ///
    /// Raises ``ValueError`` if iterating ``items`` gives more or fewer than
    /// their ``len()``, after which the table is undefined.
    ///
    /// Generated code calls it to write a vector of such tables.
    fn set_vector_tables(
        &mut self,
        py: Python<'_>,
        index: &Bound<'_, PyAny>,
        fields: &Bound<'_, FixedFields>,
        items: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let length = items.len()?;
        let vector = self.set_vector(py, index, &length.into_bound_py_any(py)?)?;
        let fields = fields.get();
        let has_string = fields.description().string;

        let mut iterated = 0;
        for (item, values) in items.try_iter()?.enumerate() {
            iterated = item + 1;
            self.write_vector_item(fields, &values?, has_string)?;

            let position = Int::from(vector + (item + 1) * self.size());
            let end = Int::from(self.len() - vector);
            if self.pack_slot(&position, &end).is_err() {
                return Err(self.slot_error(py, &position, &end));
            }
        }

        // A sequence whose len() isn't its number of items has ends over its
        // items, or ends left at 0, and the table is undefined
        if iterated != length {
            return Err(PyValueError::new_err(format!(
                "The items' len() is {length}, but iterating them gave {iterated}"
            )));
        }
        Ok(())
    }
}

impl From<i32> for Int<'_> {
    fn from(value: i32) -> Self {
        Self::Small(value.into())
    }
}
