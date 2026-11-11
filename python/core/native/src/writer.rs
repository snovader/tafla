use std::ffi::c_int;
use std::fmt::Display;
use std::ptr::{self, NonNull};

use pyo3::exceptions::{PyBufferError, PyIndexError, PyMemoryError, PyStopIteration, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple, PyType};
use pyo3::{IntoPyObjectExt, ffi};

use crate::fixed_fields::{Description, FixedFields, Format, Plan};
use crate::layout::{Shape, TableLayout, shape_of};
use crate::python::{Int, objects, utf8, with_buffer, with_bytes};
use crate::values::{self, Kind};
use crate::{elements, little_endian_host};

/// The bytes of the table being written, which Python's allocator holds, so
/// that `tracemalloc` counts them as it counts a `bytearray`'s. Once a
/// buffer of them is exported, by `TableWriter.bytes()`, their length is
/// frozen, so that the exported pointer stays valid.
struct Bytes {
    pointer: NonNull<u8>,
    len: usize,
    capacity: usize,
    frozen: bool,
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
            frozen: false,
        })
    }

    #[inline]
    const fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.pointer.as_ptr(), self.len) }
    }

    #[inline]
    const fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.len) }
    }

    /// Sets the length to `len`, with zeros in the bytes it adds. Raises
    /// `BufferError` once the bytes are exported, as a `bytearray` does.
    #[inline]
    fn resize(&mut self, len: usize) -> PyResult<()> {
        if self.frozen {
            return Err(frozen());
        }
        if len > self.capacity {
            self.grow(len)?;
        }
        if len > self.len {
            unsafe { ptr::write_bytes(self.pointer.as_ptr().add(self.len), 0, len - self.len) };
        }
        self.len = len;

        Ok(())
    }

    /// Sets the length to `len`, with zeros from `start`, which is at most
    /// the length
    #[inline]
    fn zero_from(&mut self, start: usize, len: usize) -> PyResult<()> {
        if self.frozen {
            return Err(frozen());
        }
        if len > self.capacity {
            self.grow(len)?;
        }
        unsafe { ptr::write_bytes(self.pointer.as_ptr().add(start), 0, len - start) };
        self.len = len;

        Ok(())
    }

    #[inline]
    fn extend_from_slice(&mut self, bytes: &[u8]) -> PyResult<()> {
        let start = self.len;
        let len = start.checked_add(bytes.len()).ok_or_else(memory_error)?;
        if self.frozen {
            return Err(frozen());
        }
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
    fn extend_zeros(&mut self, count: usize) -> PyResult<()> {
        self.resize(self.len.checked_add(count).ok_or_else(memory_error)?)
    }

    /// Grows the capacity to at least `required`, by half again at least,
    /// so that appending is amortized, and to `required` exactly for a
    /// larger append, such as a large array's
    #[cold]
    fn grow(&mut self, required: usize) -> PyResult<()> {
        let capacity = required.max(self.capacity + self.capacity / 2);
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

/// Why an offset, count or item end couldn't be written in its slot: the
/// message of the error that `struct` raises for it, and whether that's an
/// `IndexError`, as `struct` raises for a position beyond a C `ssize_t`
struct SlotError {
    message: String,
    index_error: bool,
}

impl SlotError {
    /// The error itself, which the setter's error has as its cause
    fn into_cause(self, py: Python<'_>) -> PyErr {
        if self.index_error {
            return PyIndexError::new_err(self.message);
        }
        match objects(py) {
            Ok(objects) => PyErr::from_type(objects.struct_error.bind(py).clone(), self.message),
            Err(error) => error,
        }
    }
}

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
/// them, without copying them, after which the writer takes no more fields:
/// one that would change their length raises ``BufferError``. A table too
/// long for the layout's ``offset_type`` raises ``ValueError``, from the
/// setter whose offset or count doesn't fit or from ``bytes()``. A scalar
/// setter that raised wrote nothing, but another setter that raised may have
/// written part of its field, after which the writer is of no further use.
#[pyclass(module = "tafla._native", name = "TableWriter")]
pub struct TableWriter {
    data: Bytes,
    shape: Shape,
    /// The start of the table being written, which its offsets count from
    table: usize,
    /// The position of the table's first offset, after the root's header
    slots: usize,
}

impl TableWriter {
    fn create(shape: Shape, length: usize, version: Option<&Int<'_>>) -> PyResult<Self> {
        let mut writer = Self {
            data: Bytes::zeroed(length)?,
            shape,
            table: 0,
            slots: shape.header_size(),
        };

        if let Some(version) = version {
            let slot = if shape.sized { shape.size() } else { 0 };
            // Only a negative number of fields leaves no room for it, and
            // struct raises its error
            if let Err(error) = writer.pack_slot(&Int::from(slot), version) {
                return Err(Python::attach(|py| error.into_cause(py)));
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
    /// end. Fails with `struct`'s message for a value or position that it
    /// rejects, and zeroes the slot for a value, as `struct` does before it
    /// packs.
    #[inline]
    fn pack_slot(&mut self, position: &Int<'_>, value: &Int<'_>) -> Result<(), SlotError> {
        let size = self.size();
        let position = self.slot_position(position)?;

        let Some(value) = value
            .index()
            .filter(|&value| value <= self.shape.max_length())
        else {
            self.data.as_mut_slice()[position..position + size].fill(0);
            return Err(self.range_error());
        };

        self.data.as_mut_slice()[position..position + size].copy_from_slice(&slot(value)[..size]);

        Ok(())
    }

    /// The error of a value that the offset type can't hold, as `struct`
    /// words it
    #[cold]
    fn range_error(&self) -> SlotError {
        let code = match self.size() {
            1 => 'B',
            2 => 'H',
            _ => 'I',
        };
        SlotError {
            message: format!(
                "'{code}' format requires 0 <= number <= {}",
                self.shape.max_length()
            ),
            index_error: false,
        }
    }

    /// Where `struct.pack_into()` writes at `position` in the bytes
    #[inline]
    fn slot_position(&self, position: &Int<'_>) -> Result<usize, SlotError> {
        let size = self.size() as i128;
        let len = self.len() as i128;
        let error = |message: String| SlotError {
            message,
            index_error: false,
        };

        let Int::Small(mut at) = *position else {
            return Err(SlotError {
                message: String::from("cannot fit 'int' into an index-sized integer"),
                index_error: true,
            });
        };
        if isize::try_from(at).is_err() {
            return Err(SlotError {
                message: String::from("cannot fit 'int' into an index-sized integer"),
                index_error: true,
            });
        }

        if at < 0 {
            if at + size > 0 {
                return Err(error(format!(
                    "no space to pack {size} bytes at offset {at}"
                )));
            }
            if at + len < 0 {
                return Err(error(format!(
                    "offset {at} out of range for {len}-byte buffer"
                )));
            }
            at += len;
        }
        if len - at < size {
            return Err(error(format!(
                "pack_into requires a buffer of at least {} bytes for packing {size} bytes at \
                 offset {at} (actual buffer size is {len})",
                size + at
            )));
        }

        // It lies within the bytes
        Ok(usize::try_from(at).unwrap_or(usize::MAX))
    }

    /// The error of an offset or item end that wasn't written: one too large
    /// for the offset type, or one whose slot lies outside the bytes, which
    /// only an index or item that isn't its own has
    #[cold]
    fn slot_error(&self, py: Python<'_>, value: &Int<'_>, error: SlotError) -> PyErr {
        let max = Int::from(self.shape.max_length());
        let raised = match max.lt(value.clone()) {
            Ok(true) => self.too_long(self.len()),
            Ok(false) => match value.describe() {
                Ok(described) => PyValueError::new_err(format!(
                    "Can't write {described} in a slot outside the table: {}",
                    error.message
                )),
                Err(error) => return error,
            },
            Err(error) => return error,
        };
        raised.set_cause(py, Some(error.into_cause(py)));
        raised
    }

    /// The error of a scalar that wasn't written, from `struct`'s error for
    /// it or its offset: one too long for the offset type, if the offset is,
    /// as the pure-Python runtime raises it
    #[cold]
    fn scalar_error(&self, value: &Bound<'_, PyAny>, offset: i128, cause: PyErr) -> PyErr {
        let py = value.py();
        let error = if offset > self.shape.max_length() as i128 {
            self.too_long(self.len())
        } else {
            match value.repr() {
                Ok(repr) => PyValueError::new_err(format!(
                    "Can't write {repr} at {}: {}",
                    self.len(),
                    cause.value(py)
                )),
                Err(error) => return error,
            }
        };
        error.set_cause(py, Some(cause));
        error
    }

    /// The error of a value that `struct` rejects for `kind`, which `struct`
    /// raises itself
    #[cold]
    fn value_error(&self, kind: Kind, value: &Bound<'_, PyAny>, offset: i128) -> PyErr {
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
        let offset = self.len() as i128 - self.table as i128;
        let position = Int::new(index)?.mul(self.size())?.add(self.slots)?;

        if let Err(error) = self.pack_slot(&position, &Int::from(offset)) {
            let py = value.py();
            return Err(self.scalar_error(value, offset, error.into_cause(py)));
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
            let offset = self.len() as i128 - self.table as i128;
            return Err(self.value_error(kind, value, offset));
        }

        self.set_packed(index, packed, value)
    }

    /// Writes the offset of field `index`, which is the table's end
    #[inline]
    fn set_offset(&mut self, py: Python<'_>, index: &Int<'_>) -> PyResult<()> {
        let value = Int::from(self.len() as i128 - self.table as i128);
        let position = index.mul(self.size())?.add(self.slots)?;

        self.pack_slot(&position, &value)
            .map_err(|error| self.slot_error(py, &value, error))
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
        error.set_cause(py, Some(self.range_error().into_cause(py)));
        Err(error)
    }

    /// Pads so that `position` lies at a multiple of `alignment` from the
    /// start of the table
    #[inline]
    fn pad(&mut self, position: usize, alignment: usize) -> PyResult<()> {
        // Modulo a power of two, which wrapping keeps
        let padding = self.table.wrapping_sub(position) & (alignment - 1);
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
        self.pad(self.len() + self.size(), alignment)?;
        self.set_offset(py, index)?;
        self.set_count(count, byte_length)
    }

    /// Appends elements in the host's byte order as little-endian ones
    fn append_elements(&mut self, elements: &[u8], item_size: usize) -> PyResult<()> {
        let start = self.len();
        self.data.extend_from_slice(elements)?;
        if !little_endian_host() && item_size > 1 {
            for element in self.data.as_mut_slice()[start..].chunks_exact_mut(item_size) {
                element.reverse();
            }
        }
        Ok(())
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

    fn set_table_at(&mut self, py: Python<'_>, index: &Int<'_>, alignment: usize) -> PyResult<()> {
        self.pad(self.len(), alignment)?;
        self.set_offset(py, index)
    }

    fn start(&mut self, fields: usize) -> PyResult<usize> {
        let parent = self.table;
        let len = self.len();
        self.table = len;
        self.slots = len;

        let slots = fields.checked_mul(self.size()).ok_or_else(memory_error)?;
        self.data.extend_zeros(slots)?;

        Ok(parent)
    }

    const fn end(&mut self, parent: usize) {
        self.table = parent;
        // Only the root starts at 0, and only its offsets follow a header
        self.slots = if parent == 0 {
            self.shape.header_size()
        } else {
            parent
        };
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
    /// table's slots, from their `values` and `string`, as `struct` packs
    /// them for the pure-Python runtime. Returns `false`, having written
    /// nothing, for fields that it rejects, which the setters write instead,
    /// raising what they raise.
    fn write_fixed(
        &mut self,
        plan: &Plan,
        start: usize,
        values: &Bound<'_, PyTuple>,
        string: Option<&Bound<'_, PyString>>,
    ) -> PyResult<bool> {
        let py = values.py();
        if values.len() != plan.values.len() || !plan.fits {
            return Ok(false);
        }

        let encoded = match (plan.string_length, string) {
            (Some(_), Some(string)) => match utf8(string) {
                Ok(encoded) => encoded,
                Err(error)
                    if error.is_instance_of::<pyo3::exceptions::PyUnicodeEncodeError>(py) =>
                {
                    return Ok(false);
                }
                Err(error) => return Err(error),
            },
            _ => &[],
        };
        if encoded.len() > self.shape.max_length() {
            return Ok(false);
        }

        // The bytes from the slots to the end, which the constructor or
        // start_table() zeroed, and which a write that fails restores, as
        // the pure-Python runtime packs every field before it writes any
        let original = self.len();
        let start = start.min(original);
        let mut stack = [0u8; 64];
        let heap: Vec<u8>;
        let saved = &self.data.as_slice()[start..];
        let saved: &[u8] = if saved.len() <= stack.len() {
            stack[..saved.len()].copy_from_slice(saved);
            &stack[..saved.len()]
        } else {
            heap = saved.to_vec();
            &heap
        };
        let end = start + plan.len;
        let size = plan.size;

        self.data.zero_from(start, end)?;
        let written = self.fill_fixed(plan, start, values);
        if !matches!(written, Ok(true)) {
            self.data.resize(start)?;
            self.data.extend_from_slice(saved)?;
            return written;
        }

        if let Some(at) = plan.string_length {
            self.data.as_mut_slice()[start + at..start + at + size]
                .copy_from_slice(&slot(encoded.len())[..size]);
            self.data.extend_from_slice(encoded)?;
        }

        Ok(true)
    }

    /// Writes the slots and values of `plan` at `start`, over zeros
    fn fill_fixed(
        &mut self,
        plan: &Plan,
        start: usize,
        values: &Bound<'_, PyTuple>,
    ) -> PyResult<bool> {
        let size = plan.size;
        let block = &mut self.data.as_mut_slice()[start..];

        for &(at, offset) in &plan.slots {
            block[at..at + size].copy_from_slice(&slot(offset)[..size]);
        }

        for (index, &(at, kind)) in plan.values.iter().enumerate() {
            let value = tuple_item(values, index);
            if !values::write(kind, &value, &mut block[at..at + kind.size()])? {
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
                    self.end(parent);
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

/// The values of fixed fields, which the setters take one by one, as the
/// pure-Python runtime takes them from an iterator
struct Values<'a, 'py> {
    tuple: &'a Bound<'py, PyTuple>,
    next: usize,
}

impl<'py> Values<'_, 'py> {
    fn next(&mut self) -> PyResult<Bound<'py, PyAny>> {
        if self.next >= self.tuple.len() {
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
        layout: Option<&Bound<'_, TableLayout>>,
        version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let shape = shape_of(layout);
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
        layout: &Bound<'_, TableLayout>,
        version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let shape = layout.get().shape();
        let length = fields
            .checked_mul(shape.size())
            .and_then(|slots| slots.checked_add(shape.header_size()))
            .ok_or_else(memory_error)?;
        let version = version.map(Int::new).transpose()?;

        Self::create(shape, length, version.as_ref())
    }

    /// Returns a view of the table's bytes, after which it takes no more
    /// fields.
    ///
    /// Raises ``ValueError`` if the table is longer than its layout addresses.
    fn bytes<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        {
            let mut writer = slf.try_borrow_mut()?;
            let len = writer.len();
            if len > writer.shape.max_length() {
                return Err(writer.too_long(len));
            }
            // Only a negative number of fields leaves no room for the length,
            // and struct raises its error
            if writer.shape.sized
                && let Err(error) = writer.pack_slot(&Int::from(0), &Int::from(len))
            {
                return Err(error.into_cause(slf.py()));
            }
        }

        // Exports the bytes through __getbuffer__, which freezes their length
        unsafe {
            Bound::from_owned_ptr_or_err(slf.py(), ffi::PyMemoryView_FromObject(slf.as_ptr()))
        }
    }

    unsafe fn __getbuffer__(
        slf: Bound<'_, Self>,
        view: *mut ffi::Py_buffer,
        flags: c_int,
    ) -> PyResult<()> {
        let mut writer = slf.try_borrow_mut()?;
        writer.data.frozen = true;

        let length = writer.len().cast_signed();
        let buffer = writer.data.pointer.as_ptr().cast();
        if unsafe { ffi::PyBuffer_FillInfo(view, slf.as_ptr(), buffer, length, 0, flags) } != 0 {
            return Err(PyErr::fetch(slf.py()));
        }
        Ok(())
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
    fn start_table(&mut self, fields: usize) -> PyResult<usize> {
        self.start(fields)
    }

    /// Ends the fields of the table that ``start_table()`` started
    const fn end_table(&mut self, parent: usize) {
        self.end(parent);
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

        if self.write_fixed(plan, self.slots, &values, string)? {
            return Ok(());
        }

        let mut values = Values {
            tuple: &values,
            next: 0,
        };
        self.set_each(fields.description(), &mut values, string)
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
        let fields = fields.get();
        let plan = fields.plan(self.size())?;
        let values = Self::fixed_values(values)?;

        if self.write_fixed(plan, self.len(), &values, string)? {
            return Ok(());
        }

        let description = fields.description();
        let parent = self.start(description.fields)?;
        let mut values = Values {
            tuple: &values,
            next: 0,
        };
        self.set_each(description, &mut values, string)?;
        self.end(parent);

        Ok(())
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

        self.pack_slot(&position, &value)
            .map_err(|error| self.slot_error(py, &value, error))
    }
}

impl From<i32> for Int<'_> {
    fn from(value: i32) -> Self {
        Self::Small(value.into())
    }
}
