use pyo3::exceptions::{PyIndexError, PyTypeError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyList, PyMemoryView, PySlice, PyString, PyTuple};
use pyo3::{IntoPyObjectExt, ffi, intern};

use crate::elements::check_struct_size;
use crate::fixed_fields::{FixedFields, Plan};
use crate::layout::{Shape, TableLayout, shape_of};
use crate::little_endian_host;
use crate::python::{
    Int, contiguous_error, core_error, decode_error, objects, out_of_range, small, with_buffer,
};
use crate::values;

/// The buffer that a `ByteBuffer` reads, held while it lives. It's requested
/// as plain bytes, which C-contiguous buffers of any shape and format export,
/// and which leaves the request without pointers into itself, so it can move.
struct Held(ffi::Py_buffer);

// SAFETY: the exporter keeps the bytes in place until the buffer is released,
// and reads of them stay within its length, from any thread. Python code that
// writes to them as they're read races, as it would with a memoryview.
unsafe impl Send for Held {}
unsafe impl Sync for Held {}

impl Drop for Held {
    fn drop(&mut self) {
        // The ByteBuffer is dropped by its deallocation, with the thread
        // attached
        unsafe { ffi::PyBuffer_Release(&raw mut self.0) };
    }
}

/// Reads a table within the supplied bytes, in the table layout it was
/// written in, whose multi-byte values are little-endian. Array reads return
/// views of the bytes on a little-endian host, whatever their alignment, and
/// copies in the host's byte order on a big-endian host.
///
/// Every read checks that it stays within the bytes and raises ``DecodeError``
/// otherwise, at any position, however large. The bytes must be C-contiguous,
/// and bytes that aren't raise ``TypeError``.
///
/// Tables are written with ``TableBuilder``, whose ``finalize()`` returns a
/// ``ByteBuffer`` that reads the table, or with ``TableWriter``.
#[pyclass(frozen, module = "tafla._native", name = "ByteBuffer")]
pub struct ByteBuffer {
    held: Held,
    shape: Shape,
    /// The view of the bytes that `bytes()` returns and reads slice, created
    /// when first used. Bytes given as a memoryview are held through a view
    /// of its own, which it is when it's of bytes.
    view: PyOnceLock<Py<PyAny>>,
}

impl ByteBuffer {
    /// The bytes, which no Python code may run while they're borrowed, so
    /// that nothing changes them
    #[inline]
    fn bytes_slice(&self) -> &[u8] {
        unsafe { crate::python::buffer_bytes(&self.held.0) }
    }

    #[inline]
    fn available(&self) -> usize {
        self.bytes_slice().len()
    }

    #[inline]
    fn core(&self) -> tafla::ByteBuffer<'_> {
        tafla::ByteBuffer::new(self.bytes_slice(), self.shape.core())
    }

    #[inline]
    const fn size(&self) -> usize {
        self.shape.size()
    }

    /// The position of a read of `length` bytes, which must be non-negative,
    /// as `struct` reads the scalars
    #[inline]
    fn position(&self, position: &Bound<'_, PyAny>, length: usize) -> PyResult<usize> {
        if let Some(position) = small(position)?
            && let Ok(position) = usize::try_from(position)
        {
            return Ok(position);
        }
        Err(self.outside(position, length))
    }

    #[cold]
    fn outside(&self, position: &Bound<'_, PyAny>, length: usize) -> PyErr {
        let py = position.py();
        match Int::new(position) {
            Ok(position) => out_of_range(py, &position, &Int::from(length), self.available()),
            Err(error) => error,
        }
    }

    /// Reads the length, offset or item end at `position`
    #[inline]
    fn read_size(&self, py: Python<'_>, position: &Int<'_>) -> PyResult<usize> {
        let size = self.size();
        let Some(at) = position.index() else {
            return Err(out_of_range(
                py,
                position,
                &Int::from(size),
                self.available(),
            ));
        };

        let core = self.core();
        let read = match size {
            1 => core.get_u8(at).map(usize::from),
            2 => core.get_u16(at).map(usize::from),
            _ => core.get_u32(at).map(|size| size as usize),
        };
        read.map_err(|error| core_error(py, &error))
    }

    /// The view of the bytes, as a one-dimensional view of bytes
    fn view<'py>(&self, py: Python<'py>) -> PyResult<&Bound<'py, PyAny>> {
        self.view
            .get_or_try_init(py, || -> PyResult<Py<PyAny>> {
                let source = unsafe { Bound::from_borrowed_ptr(py, self.held.0.obj) };
                Ok(byte_view(source)?.unbind())
            })
            .map(|view| view.bind(py))
    }

    /// A view of the bytes from `start` to `end`
    #[inline]
    fn slice<'py>(&self, py: Python<'py>, start: usize, end: usize) -> PyResult<Bound<'py, PyAny>> {
        let slice = PySlice::new(py, start.cast_signed(), end.cast_signed(), 1);
        self.view(py)?.get_item(slice)
    }

    /// The position in the bytes of `bytes`, which lie within them
    #[inline]
    fn offset_of(&self, bytes: &[u8]) -> usize {
        bytes.as_ptr() as usize - self.held.0.buf as usize
    }

    /// A view of the elements of `bytes` in the host's byte order, from
    /// little-endian ones: a view of the bytes on a little-endian host, and a
    /// copy on a big-endian host
    fn elements<'py>(
        &self,
        py: Python<'py>,
        code: &Bound<'py, PyString>,
        start: usize,
        end: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let view = self.slice(py, start, end)?;
        if little_endian_host() {
            return view.call_method1(intern!(py, "cast"), (code,));
        }

        let swapped = objects(py)?.array.bind(py).call1((code,))?;
        swapped.call_method1(intern!(py, "frombytes"), (view,))?;
        swapped.call_method0(intern!(py, "byteswap"))?;
        Ok(unsafe {
            Bound::from_owned_ptr_or_err(py, ffi::PyMemoryView_FromObject(swapped.as_ptr()))?
        })
    }

    fn array<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'_, PyAny>,
        code: &Bound<'py, PyString>,
        item_size: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let at = self.position(position, self.size())?;
        let bytes = self
            .core()
            .get_struct_array(at, item_size)
            .map_err(|error| core_error(py, &error))?;
        let start = self.offset_of(bytes);

        self.elements(py, code, start, start + bytes.len())
    }

    /// The position of item `index` of the vector of `count` items at
    /// `position`, as the pure-Python runtime finds it from the item's ends
    fn item_offset<'py>(
        &self,
        py: Python<'py>,
        position: &Int<'py>,
        count: &Int<'py>,
        index: &Int<'py>,
    ) -> PyResult<Int<'py>> {
        if index.is_negative()? || !index.lt(count.clone())? {
            return Err(PyIndexError::new_err(format!(
                "Item {} is outside the vector of {} at {}",
                index.describe()?,
                count.display()?,
                position.display()?
            )));
        }

        let size = self.size();
        let start = if matches!(index, Int::Small(0)) {
            count.add(1)?.mul(size)?
        } else {
            Int::from(self.read_size(py, &position.add(index.mul(size)?)?)?)
        };
        let end = Int::from(self.read_size(py, &position.add(index.add(1)?.mul(size)?)?)?);

        if end.lt(start.clone())? || Int::from(self.available()).lt(position.add(end.clone())?)? {
            return Err(item_error(py, position, &start, &end, self.available()));
        }
        position.add(start)
    }
}

/// The error of a vector item that starts after it ends, or ends past the
/// bytes
#[cold]
fn item_error(
    py: Python<'_>,
    position: &Int<'_>,
    start: &Int<'_>,
    end: &Int<'_>,
    available: usize,
) -> PyErr {
    let build = || -> PyResult<PyErr> {
        let from = position.add(start.clone())?;
        if end.lt(start.clone())? {
            let to = position.add(end.clone())?;
            return Ok(decode_error(
                py,
                format!(
                    "The vector item at {} ends before it starts, at {}",
                    from.display()?,
                    to.display()?
                ),
            ));
        }
        Ok(out_of_range(py, &from, &end.sub(start.clone())?, available))
    };
    build().unwrap_or_else(|error| error)
}

/// A one-dimensional view of the bytes of `source`, which a `ByteBuffer`
/// holds, as the pure-Python runtime views them: `memoryview(source)`, cast
/// to bytes if it's of another format or shape. A memoryview is one that
/// the `ByteBuffer` created of its own, which it views them through.
fn byte_view(source: Bound<'_, PyAny>) -> PyResult<Bound<'_, PyAny>> {
    let py = source.py();
    if source.is_exact_instance_of::<pyo3::types::PyBytes>()
        || source.is_exact_instance_of::<pyo3::types::PyByteArray>()
    {
        return unsafe {
            Bound::from_owned_ptr_or_err(py, ffi::PyMemoryView_FromObject(source.as_ptr()))
        };
    }

    let view = if source.is_exact_instance_of::<PyMemoryView>() {
        source
    } else {
        unsafe { Bound::from_owned_ptr_or_err(py, ffi::PyMemoryView_FromObject(source.as_ptr()))? }
    };

    let format = view.getattr(intern!(py, "format"))?;
    let ndim: usize = view.getattr(intern!(py, "ndim"))?.extract()?;
    if ndim == 1 && format.eq("B")? {
        return Ok(view);
    }
    view.call_method1(intern!(py, "cast"), ("B",))
}

macro_rules! scalar_getters {
    ($($name:ident: $core:ident -> $type:ty),* $(,)?) => {
        #[pymethods]
        impl ByteBuffer {
            $(
                fn $name(&self, py: Python<'_>, position: &Bound<'_, PyAny>) -> PyResult<$type> {
                    let at = self.position(position, size_of::<$type>())?;
                    self.core().$core(at).map_err(|error| core_error(py, &error))
                }
            )*
        }
    };
}

scalar_getters!(
    get_int8: get_i8 -> i8,
    get_uint8: get_u8 -> u8,
    get_int16: get_i16 -> i16,
    get_uint16: get_u16 -> u16,
    get_int32: get_i32 -> i32,
    get_uint32: get_u32 -> u32,
    get_int64: get_i64 -> i64,
    get_uint64: get_u64 -> u64,
    get_float64: get_f64 -> f64,
);

macro_rules! array_getters {
    ($($name:ident: $code:literal, $size:literal),* $(,)?) => {
        #[pymethods]
        impl ByteBuffer {
            $(
                /// Reads the array at ``position``: a view of the buffer on a
                /// little-endian host, and a copy on a big-endian host.
                fn $name<'py>(&self, py: Python<'py>, position: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyAny>> {
                    self.array(py, position, intern!(py, $code), $size)
                }
            )*
        }
    };
}

array_getters!(
    get_int8_array: "b", 1,
    get_int16_array: "h", 2,
    get_uint16_array: "H", 2,
    get_int32_array: "i", 4,
    get_uint32_array: "I", 4,
    get_int64_array: "q", 8,
    get_uint64_array: "Q", 8,
    get_float32_array: "f", 4,
    get_float64_array: "d", 8,
);

#[pymethods]
impl ByteBuffer {
    /// Reads ``bytes`` in ``layout``, which must be the layout that the
    /// table was written in.
    ///
    /// Raises ``TypeError`` if ``bytes`` isn't C-contiguous, such as a slice
    /// with a step. Reads return views of the bytes, which a copy of them
    /// wouldn't be.
    #[new]
    #[pyo3(signature = (bytes, layout = None))]
    fn new(bytes: &Bound<'_, PyAny>, layout: Option<&Bound<'_, TableLayout>>) -> PyResult<Self> {
        let py = bytes.py();

        // A memoryview is viewed by one of its own, as memoryview() views it,
        // so that its caller can release theirs while this reads its bytes
        let own;
        let source = if bytes.is_exact_instance_of::<PyMemoryView>() {
            own = unsafe {
                Bound::from_owned_ptr_or_err(py, ffi::PyMemoryView_FromObject(bytes.as_ptr()))?
            };
            &own
        } else {
            bytes
        };

        let mut held = std::mem::MaybeUninit::<ffi::Py_buffer>::uninit();
        if unsafe { ffi::PyObject_GetBuffer(source.as_ptr(), held.as_mut_ptr(), ffi::PyBUF_SIMPLE) }
            != 0
        {
            return Err(contiguous_error(py, PyErr::fetch(py)));
        }
        let held = Held(unsafe { held.assume_init() });

        Ok(Self {
            held,
            shape: shape_of(layout),
            view: PyOnceLock::new(),
        })
    }

    fn bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.view(py).cloned()
    }

    fn get_size(&self, py: Python<'_>) -> PyResult<usize> {
        self.core().size().map_err(|error| core_error(py, &error))
    }

    /// Returns the version that the table's header carries after its
    /// length. It's as wide as the header's other slots.
    ///
    /// Raises ``TypeError`` if the buffer's layout isn't versioned.
    fn get_version(&self, py: Python<'_>) -> PyResult<u32> {
        if !self.shape.versioned {
            return Err(PyTypeError::new_err("The table has no version"));
        }
        tafla::ByteBuffer::new(self.bytes_slice(), self.shape.core().versioned())
            .version()
            .map_err(|error| core_error(py, &error))
    }

    /// Returns the position of field ``index`` of the root table.
    fn root_offset(&self, py: Python<'_>, index: &Bound<'_, PyAny>) -> PyResult<usize> {
        let slot = Int::new(index)?
            .mul(self.size())?
            .add(self.shape.header_size())?;
        self.read_size(py, &slot)
    }

    /// Returns the position of field ``index`` of a nested table, which has
    /// no header and stores its field offsets from its own start.
    /// ``table_position`` is the nested table's position, which is the offset
    /// of the field that holds it.
    fn field_offset<'py>(
        &self,
        py: Python<'py>,
        table_position: &Bound<'py, PyAny>,
        index: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Without an int's arithmetic in the common case, of a position and
        // an index within the bytes
        if let (Some(table), Some(index)) = (small(table_position)?, small(index)?)
            && let (Ok(table), Ok(index)) = (usize::try_from(table), usize::try_from(index))
            && let Some(slot) = index
                .checked_mul(self.size())
                .and_then(|slot| slot.checked_add(table))
        {
            let offset = self.read_size(py, &Int::from(slot))?;
            return (table as u64 + offset as u64).into_bound_py_any(py);
        }

        let table = Int::new(table_position)?;
        let slot = Int::new(index)?.mul(self.size())?.add(table.clone())?;
        let offset = self.read_size(py, &slot)?;
        table.add(offset)?.to_object(py)
    }

    fn get_float32(&self, py: Python<'_>, position: &Bound<'_, PyAny>) -> PyResult<f64> {
        let at = self.position(position, 4)?;
        let bits = self
            .core()
            .get_u32(at)
            .map_err(|error| core_error(py, &error))?;
        values::read_f32(py, bits.to_le_bytes())
    }

    /// Reads the array at ``position``, as a view of the buffer.
    fn get_uint8_array<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let at = self.position(position, self.size())?;
        let bytes = self
            .core()
            .get_u8_array(at)
            .map_err(|error| core_error(py, &error))?;
        let start = self.offset_of(bytes);

        self.slice(py, start, start + bytes.len())
    }

    /// Returns a view of the ``size`` bytes of the struct at ``position``,
    /// whose scalars lie at fixed offsets from its start.
    fn get_struct<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'_, PyAny>,
        size: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let position = Int::new(position)?;
        let size = Int::new(size)?;
        let available = self.available();
        if let (Some(at), Some(length)) = (position.index(), size.index())
            && let Some(end) = at.checked_add(length)
            && end <= available
        {
            return self.slice(py, at, end);
        }

        Err(out_of_range(py, &position, &size, available))
    }

    /// Returns a view of the structs of the array at ``position``, each of
    /// ``size`` bytes, one after another after the count. Their number is the
    /// view's length over ``size``.
    ///
    /// Raises ``ValueError`` if ``size`` is less than 1.
    fn get_struct_array<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'_, PyAny>,
        size: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let size = Int::new(size)?;
        check_struct_size(&size)?;

        let at = self.position(position, self.size())?;
        let count = self.read_size(py, &Int::from(at))?;
        let byte_length = size.mul(count)?;
        let start = at + self.size();
        let available = self.available();
        if let Some(length) = byte_length.index()
            && let Some(end) = start.checked_add(length)
            && end <= available
        {
            return self.slice(py, start, end);
        }

        Err(out_of_range(py, &Int::from(start), &byte_length, available))
    }

    /// Raises ``DecodeError`` if the string isn't valid UTF-8.
    fn get_string<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let at = self.position(position, self.size())?;
        let bytes = self
            .core()
            .get_u8_array(at)
            .map_err(|error| core_error(py, &error))?;

        let string = unsafe {
            ffi::PyUnicode_DecodeUTF8(
                bytes.as_ptr().cast(),
                bytes.len().cast_signed(),
                c"strict".as_ptr(),
            )
        };
        if string.is_null() {
            let cause = PyErr::fetch(py);
            let error = decode_error(py, format!("The string at {at} isn't valid UTF-8"));
            error.set_cause(py, Some(cause));
            return Err(error);
        }

        Ok(unsafe { Bound::from_owned_ptr(py, string) })
    }

    /// Returns the values of the fields that ``fields`` describes, of the
    /// table at ``position``, read in one call, in the order that
    /// ``TableWriter.set_fields()`` takes them, with the string's last. It's
    /// for a table whose fields it describes whole, when its offsets are the
    /// constants that ``TableWriter`` writes them at. Otherwise, or when the
    /// fields don't lie within the buffer or the string isn't valid UTF-8,
    /// it returns ``None``, for the field getters to read them, which then
    /// raise ``DecodeError`` for whatever they can't read. Each value is what
    /// its getter returns, but a struct's, which is its bytes.
    fn get_fields<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'_, PyAny>,
        fields: &Bound<'_, FixedFields>,
    ) -> PyResult<Option<Bound<'py, PyTuple>>> {
        let plan = fields.get().plan(self.size())?;
        let Some(start) = small(position)?
            .and_then(|position| usize::try_from(position).ok())
            .and_then(|position| position.checked_add(plan.header))
        else {
            return Ok(None);
        };

        let Some(Checked { string: text }) = self.check_fields(py, plan, start) else {
            return Ok(None);
        };

        // The tuple is created first, since creating it may run the garbage
        // collector, and with it Python code, and the values create no
        // objects that it tracks
        let count = plan.values.len() + usize::from(text.is_some());
        let tuple =
            unsafe { Bound::from_owned_ptr_or_err(py, ffi::PyTuple_New(count.cast_signed()))? };
        for (index, &(at, kind)) in plan.values.iter().enumerate() {
            // Within the fields, which check_fields() found within the bytes
            let fields = &self.bytes_slice()[start..start + plan.len];
            let value = values::read(py, kind, &fields[at..at + kind.size()])?;
            tuple_set(&tuple, index, value)?;
        }
        if let Some(text) = text {
            tuple_set(&tuple, plan.values.len(), text)?;
        }

        Ok(Some(unsafe { tuple.cast_into_unchecked() }))
    }

    /// Returns the number of items of the vector at ``position``.
    ///
    /// Raises ``DecodeError`` if the count or the ends after it don't lie
    /// within the buffer. Every item has an end, so a count that the bytes
    /// can't hold fails before anything is allocated for it.
    fn get_vector_length(&self, py: Python<'_>, position: &Bound<'_, PyAny>) -> PyResult<usize> {
        let at = self.position(position, self.size())?;
        let vector = self
            .core()
            .get_vector(at)
            .map_err(|error| core_error(py, &error))?;
        Ok(vector.len())
    }

    /// Returns the position of item ``index`` of the vector at
    /// ``position``. It's read from the ends of that item and the one before
    /// it, without reading any other item.
    ///
    /// Raises ``DecodeError`` if the count or the ends after it don't lie
    /// within the buffer, which is checked before ``index``, or the item
    /// doesn't, and ``IndexError`` if the vector has no item ``index``.
    fn get_vector_offset<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'py, PyAny>,
        index: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let count = self.get_vector_length(py, position)?;
        let position = Int::new(position)?;
        self.item_offset(py, &position, &Int::from(count), &Int::new(index)?)?
            .to_object(py)
    }

    /// Returns the position of item ``index`` of the vector at
    /// ``position``, as ``get_vector_offset()`` does, for a vector whose
    /// ``count`` ``get_vector_length()`` returned, which it doesn't read or
    /// check again. A view of the vector checks its count once, and passes it
    /// for each item.
    ///
    /// Raises ``IndexError`` if ``index`` isn't below ``count``, and
    /// ``DecodeError`` if the item's ends don't lie within the buffer, or the
    /// item doesn't.
    fn get_vector_item_offset<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'py, PyAny>,
        count: &Bound<'py, PyAny>,
        index: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let position = Int::new(position)?;
        self.item_offset(py, &position, &Int::new(count)?, &Int::new(index)?)?
            .to_object(py)
    }

    /// Returns the position of every item of the vector at ``position``,
    /// as 64-bit unsigned integers, since the buffer may be longer than
    /// 4 GiB, as a memory map of many tables is.
    fn get_vector_offsets<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let at = self.position(position, self.size())?;
        let count = self.get_vector_length(py, position)?;

        // Created before the items are read, since creating it may run
        // Python code
        let offsets = objects(py)?.zero_offset.bind(py).mul(count)?;
        let read = with_buffer(&offsets, ffi::PyBUF_WRITABLE, |view| {
            let slots = unsafe {
                std::slice::from_raw_parts_mut(view.buf.cast::<u8>(), view.len.unsigned_abs())
            };
            Ok(self.fill_offsets(at, slots))
        })?;
        read.map_err(|error| core_error(py, &error))?;

        Ok(offsets)
    }

    /// Returns the bytes of every item of the vector at ``position``, as
    /// views of the buffer.
    fn get_vector<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyList>> {
        let at = self.position(position, self.size())?;
        let vector = self
            .core()
            .get_vector(at)
            .map_err(|error| core_error(py, &error))?;
        let items = vector
            .iter()
            .map(|item| item.map(|item| (self.offset_of(item), item.len())))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| core_error(py, &error))?;

        let list = PyList::empty(py);
        for (start, length) in items {
            list.append(self.slice(py, start, start + length)?)?;
        }
        Ok(list)
    }
}

impl ByteBuffer {
    /// Checks that the fields of `plan` lie within the bytes at `start`,
    /// with the offsets that the writer writes, and decodes their string.
    /// Returns `None` otherwise.
    fn check_fields<'py>(
        &self,
        py: Python<'py>,
        plan: &Plan,
        start: usize,
    ) -> Option<Checked<'py>> {
        let bytes = self.bytes_slice();
        let size = plan.size;
        let end = start.checked_add(plan.len)?;
        let fields = bytes.get(start..end)?;

        for &(at, offset) in &plan.slots {
            if read_slot(&fields[at..], size) != offset {
                return None;
            }
        }

        let Some(at) = plan.string_length else {
            return Some(Checked { string: None });
        };
        let length = read_slot(&fields[at..], size);
        let text = bytes.get(end..end.checked_add(length)?)?;
        let string = unsafe {
            ffi::PyUnicode_DecodeUTF8(
                text.as_ptr().cast(),
                text.len().cast_signed(),
                c"strict".as_ptr(),
            )
        };
        if string.is_null() {
            // Invalid UTF-8, which the getter raises for
            drop(PyErr::take(py));
            return None;
        }

        let string = unsafe { Bound::from_owned_ptr(py, string) };
        Some(Checked {
            string: Some(string),
        })
    }

    /// Writes the position of each item of the vector at `at` into `slots`,
    /// one host-order u64 each, checking every item
    fn fill_offsets(&self, at: usize, slots: &mut [u8]) -> Result<(), tafla::DecodeError> {
        let vector = self.core().get_vector(at)?;
        for (slot, item) in slots
            .as_chunks_mut::<8>()
            .0
            .iter_mut()
            .zip(vector.iter_in_place())
        {
            let offset = self.offset_of(item?) as u64;
            slot.copy_from_slice(&offset.to_ne_bytes());
        }
        Ok(())
    }
}

/// Fixed fields that lie within the bytes with the offsets that the writer
/// writes, and their string, decoded
struct Checked<'py> {
    string: Option<Bound<'py, PyAny>>,
}

/// The length, offset or item end of `size` bytes at the start of `bytes`,
/// which hold it
#[inline]
fn read_slot(bytes: &[u8], size: usize) -> usize {
    match size {
        1 => usize::from(bytes[0]),
        2 => usize::from(u16::from_le_bytes([bytes[0], bytes[1]])),
        _ => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize,
    }
}

/// Sets item `index` of a tuple that was just created
#[inline]
fn tuple_set(tuple: &Bound<'_, PyAny>, index: usize, value: Bound<'_, PyAny>) -> PyResult<()> {
    #[cfg(not(Py_LIMITED_API))]
    unsafe {
        ffi::PyTuple_SET_ITEM(tuple.as_ptr(), index.cast_signed(), value.into_ptr());
    }
    #[cfg(Py_LIMITED_API)]
    if unsafe { ffi::PyTuple_SetItem(tuple.as_ptr(), index.cast_signed(), value.into_ptr()) } != 0 {
        return Err(PyErr::fetch(tuple.py()));
    }
    Ok(())
}
