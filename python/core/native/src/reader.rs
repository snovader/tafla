use pyo3::exceptions::{PyIndexError, PyTypeError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyList, PyMemoryView, PySlice, PyString, PyTuple};
use pyo3::{IntoPyObjectExt, ffi, intern};

use crate::elements::check_struct_size;
use crate::fixed_fields::{FixedFields, Plan};
use crate::layout::{Layout, Shape, shape_of};
use crate::little_endian_host;
use crate::python::{
    Int, contiguous_error, core_error, decode_error, objects, out_of_range, small, with_buffer,
};
use crate::values;

/// The buffer that a `TableReader` reads, held while it lives. It's requested
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
        // The TableReader is dropped by its deallocation, with the thread
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
/// ``TableReader`` that reads the table, or with ``TableWriter``.
#[pyclass(frozen, module = "tafla._native", name = "TableReader")]
pub struct TableReader {
    held: Held,
    shape: Shape,
    /// The view of the bytes that `bytes()` returns and reads slice, created
    /// when first used. Bytes given as a memoryview are held through a view
    /// of its own, which it is when it's of bytes.
    view: PyOnceLock<Py<PyAny>>,
}

impl TableReader {
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
    fn core(&self) -> tafla::TableReader<'_> {
        tafla::TableReader::new(self.bytes_slice(), self.shape.core())
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
        let (start, _) = self.item_bounds(py, position, count, index)?;
        Ok(start)
    }

    /// Where item `index` of the vector of `count` items at `position`
    /// starts and ends, as `item_offset()` finds it: within the bytes, but
    /// before them for a negative position, which a caller may pass, as the
    /// pure-Python runtime finds it
    fn item_bounds<'py>(
        &self,
        py: Python<'py>,
        position: &Int<'py>,
        count: &Int<'py>,
        index: &Int<'py>,
    ) -> PyResult<(Int<'py>, Int<'py>)> {
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

        Ok((position.add(start)?, position.add(end)?))
    }

    /// Decodes the string from `start` to `end` in the bytes, which
    /// `item_bounds()` found, as the pure-Python runtime decodes a string of
    /// a vector of strings. A string that starts before the bytes, at a
    /// negative position, lies outside them.
    #[inline]
    fn decode_string<'py>(
        &self,
        py: Python<'py>,
        start: &Int<'py>,
        end: &Int<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        match (start.index(), end.index()) {
            (Some(from), Some(to)) => decode_utf8(py, &self.bytes_slice()[from..to], from),
            _ => Err(out_of_range(
                py,
                start,
                &end.sub(start.clone())?,
                self.available(),
            )),
        }
    }
}

/// Decodes `bytes`, a string at `start` in a reader's bytes, as UTF-8, or
/// raises the pure-Python runtime's `DecodeError` for them, whose cause is
/// the `UnicodeDecodeError`. Creating a `str` runs no Python code, so the
/// bytes stay borrowed.
#[inline]
fn decode_utf8<'py>(py: Python<'py>, bytes: &[u8], start: usize) -> PyResult<Bound<'py, PyAny>> {
    let string = unsafe {
        ffi::PyUnicode_DecodeUTF8(
            bytes.as_ptr().cast(),
            bytes.len().cast_signed(),
            c"strict".as_ptr(),
        )
    };
    if string.is_null() {
        return Err(string_error(py, start));
    }

    Ok(unsafe { Bound::from_owned_ptr(py, string) })
}

/// The error of a string of a vector of strings, at `start`, that isn't
/// valid UTF-8, whose `UnicodeDecodeError` `CPython` has just raised
#[cold]
fn string_error(py: Python<'_>, start: usize) -> PyErr {
    let cause = PyErr::fetch(py);
    let error = decode_error(py, format!("The string at {start} isn't valid UTF-8"));
    error.set_cause(py, Some(cause));
    error
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

/// The bytes of `source`, held as plain bytes, or `TypeError` if they
/// aren't C-contiguous
fn hold(source: &Bound<'_, PyAny>) -> PyResult<Held> {
    let py = source.py();
    let mut held = std::mem::MaybeUninit::<ffi::Py_buffer>::uninit();
    if unsafe { ffi::PyObject_GetBuffer(source.as_ptr(), held.as_mut_ptr(), ffi::PyBUF_SIMPLE) }
        != 0
    {
        return Err(contiguous_error(py, PyErr::fetch(py)));
    }
    Ok(Held(unsafe { held.assume_init() }))
}

/// A one-dimensional view of the bytes of `source`, which a `TableReader`
/// holds, as the pure-Python runtime views them: `memoryview(source)`, cast
/// to bytes if it's of another format or shape. A memoryview is one that
/// the `TableReader` created of its own, which it views them through.
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
        impl TableReader {
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
        impl TableReader {
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
impl TableReader {
    /// Reads ``bytes`` in ``layout``, which must be the layout that the
    /// table was written in.
    ///
    /// Raises ``TypeError`` if ``bytes`` isn't C-contiguous, such as a slice
    /// with a step. Reads return views of the bytes, which a copy of them
    /// wouldn't be.
    #[new]
    #[pyo3(signature = (bytes, layout = None))]
    fn new(bytes: &Bound<'_, PyAny>, layout: Option<Layout>) -> PyResult<Self> {
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

        Ok(Self {
            held: hold(source)?,
            shape: shape_of(layout.as_ref()),
            view: PyOnceLock::new(),
        })
    }

    /// Returns the values of the fields that ``fields`` describes whole, a
    /// message's, of the message in ``data``, in the layout that ``fields``
    /// was created with, as ``TableReader(data, layout).get_fields(0,
    /// fields)`` returns them, or ``None`` as it does, without creating a
    /// ``TableReader``. It returns ``None`` too if ``version`` isn't the one
    /// that the header holds, for the reader's constructor to raise for.
    ///
    /// Raises ``TypeError`` if ``data`` isn't C-contiguous, if ``fields`` has
    /// no layout, as a nested table's doesn't, and if ``version`` is given
    /// for a layout without a version.
    ///
    /// Generated code calls it to read every field of a message of only such
    /// fields at once, without a reader.
    #[staticmethod]
    #[pyo3(signature = (data, fields, version = None))]
    fn read_fields<'py>(
        data: &Bound<'py, PyAny>,
        fields: &Bound<'_, FixedFields>,
        version: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Option<Bound<'py, PyTuple>>> {
        let py = data.py();
        // The constructor's checks of the bytes, before the fields'
        let held = hold(data)?;
        let fixed = fields.get();
        let shape = fixed.shape()?;
        if version.is_some() && !shape.versioned() {
            return Err(PyTypeError::new_err("The table has no version"));
        }
        let reader = Self {
            held,
            shape,
            view: PyOnceLock::new(),
        };

        if let Some(version) = version {
            let layout = shape.core().with_version(0);
            let Ok(found) = tafla::TableReader::new(reader.bytes_slice(), layout).version() else {
                return Ok(None);
            };
            if !version.eq(found)? {
                return Ok(None);
            }
        }
        reader.fields_at(py, fixed.plan(shape.size())?, 0)
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
    /// Raises ``TypeError`` if the reader's layout isn't versioned.
    fn get_version(&self, py: Python<'_>) -> PyResult<u32> {
        if !self.shape.versioned() {
            return Err(PyTypeError::new_err("The table has no version"));
        }
        tafla::TableReader::new(self.bytes_slice(), self.shape.core().with_version(0))
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
        let Some(position) = small(position)?.and_then(|position| usize::try_from(position).ok())
        else {
            return Ok(None);
        };

        self.fields_at(py, plan, position)
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

    /// Returns the values of every item of the vector at ``position``, a
    /// table whose fields ``fields`` describes whole, each as
    /// ``get_fields()`` returns them, or ``None`` when it returns ``None``
    /// for any item, for the field getters to read them.
    ///
    /// It's all the items or none: every implementation's writer writes such
    /// a table at the offsets that ``get_fields()`` expects, so an item that
    /// it can't read is corrupted or was written by hand, and the getters,
    /// which check each read, then read the whole vector.
    ///
    /// Raises ``DecodeError`` if the vector's count or an item's ends don't
    /// lie within the buffer, as ``get_vector_offsets()`` does.
    ///
    /// Generated code calls it to read every field of every item at once.
    fn get_vector_fields<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'_, PyAny>,
        fields: &Bound<'_, FixedFields>,
    ) -> PyResult<Option<Bound<'py, PyList>>> {
        // Every item's position first, as get_vector_offsets() reads them
        let at = self.position(position, self.size())?;
        let offsets = self
            .core()
            .get_vector(at)
            .and_then(|vector| {
                vector
                    .iter_in_place()
                    .map(|item| item.map(|item| self.offset_of(item)))
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|error| core_error(py, &error))?;

        let plan = fields.get().plan(self.size())?;
        let mut items = Vec::with_capacity(offsets.len());
        for offset in offsets {
            match self.fields_at(py, plan, offset)? {
                Some(values) => items.push(values),
                None => return Ok(None),
            }
        }

        Ok(Some(PyList::new(py, items)?))
    }

    /// Returns an iterator over the values of every item of the vector at
    /// ``position``, a table whose fields ``fields`` describes whole: each
    /// item's as ``get_fields()`` returns them, or, for an item whose offsets
    /// aren't those that its writer writes, what ``read`` returns for the
    /// item's position, which the item's getters read.
    ///
    /// It reads each item's end when it reaches the item, after the end of
    /// the item before it, so it raises ``DecodeError`` for an item that
    /// doesn't lie within the buffer when it reaches it, as
    /// ``get_vector_offsets()`` raises for it, after the items before it.
    ///
    /// Raises ``DecodeError`` at once if the vector's count or the ends after
    /// it don't lie within the buffer, as ``get_vector_length()`` does.
    ///
    /// Generated code calls it to iterate over the values of a vector's
    /// items, without reading them all at once.
    fn iter_vector_fields(
        slf: &Bound<'_, Self>,
        position: &Bound<'_, PyAny>,
        fields: &Bound<'_, FixedFields>,
        read: Bound<'_, PyAny>,
    ) -> PyResult<VectorFields> {
        let py = slf.py();
        let reader = slf.get();
        let count = reader.get_vector_length(py, position)?;
        let at = reader.position(position, reader.size())?;
        fields.get().plan(reader.size())?;

        Ok(VectorFields {
            reader: slf.clone().unbind(),
            fields: fields.clone().unbind(),
            read: read.unbind(),
            position: at,
            count,
            index: 0,
            start: (count + 1) * reader.size(),
        })
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

    /// Reads every string of the vector of strings at ``position``, in
    /// order.
    ///
    /// A vector of strings is laid out as a vector of tables is: its count,
    /// the end of each string from the vector's position, and then the
    /// strings' UTF-8, one after another, so the ends are read and checked
    /// as ``get_vector()`` reads a vector's. Each string reads as reading it
    /// on its own with ``get_string_vector_item()`` would.
    ///
    /// Raises ``DecodeError`` if the count, the ends or the strings don't
    /// lie within the buffer, a string ends before it starts, or one isn't
    /// valid UTF-8: the error that reading the strings one after another
    /// with ``get_string_vector_item()`` raises first.
    fn get_string_vector<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyList>> {
        let at = self.position(position, self.size())?;
        let vector = self
            .core()
            .get_vector(at)
            .map_err(|error| core_error(py, &error))?;

        // The list first, since creating it may run the garbage collector,
        // and with it Python code, which creating the strings doesn't. Each
        // string is decoded by CPython, which checks it as UTF-8 on its own,
        // from the bounds that the Rust runtime reads and checks one after
        // another.
        let list = unsafe {
            Bound::from_owned_ptr_or_err(py, ffi::PyList_New(vector.len().cast_signed()))?
                .cast_into_unchecked::<PyList>()
        };
        for (index, item) in vector.iter().enumerate() {
            let bytes = item.map_err(|error| core_error(py, &error))?;
            let string = decode_utf8(py, bytes, self.offset_of(bytes))?;
            list_set(&list, index, string)?;
        }

        Ok(list)
    }

    /// Reads string ``index`` of the vector of strings at ``position``,
    /// from its end and the end of the string before it, without reading
    /// any other string. It takes the vector's ``count``, which
    /// ``get_vector_length()`` returned, and doesn't read or check it again,
    /// as ``get_vector_item_offset()`` does, so that reading many strings,
    /// as a ``StringVector`` does, reads and checks the count once.
    ///
    /// Raises ``IndexError`` if ``index`` isn't below ``count``, and
    /// ``DecodeError`` if the string's ends or the string don't lie within
    /// the buffer, it ends before it starts, or it isn't valid UTF-8.
    fn get_string_vector_item<'py>(
        &self,
        py: Python<'py>,
        position: &Bound<'py, PyAny>,
        count: &Bound<'py, PyAny>,
        index: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let position = Int::new(position)?;
        let (start, end) = self.item_bounds(py, &position, &Int::new(count)?, &Int::new(index)?)?;
        self.decode_string(py, &start, &end)
    }
}

impl TableReader {
    /// The values of the fields of `plan`, of the table at `position`, as
    /// `get_fields()` returns them, or `None`
    fn fields_at<'py>(
        &self,
        py: Python<'py>,
        plan: &Plan,
        position: usize,
    ) -> PyResult<Option<Bound<'py, PyTuple>>> {
        let Some(start) = position.checked_add(plan.header) else {
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

/// An iterator over the values of a vector's items, which
/// ``TableReader.iter_vector_fields()`` returns. It's for one thread at a
/// time: in free-threaded Python, a call while another thread's call to it
/// runs raises ``RuntimeError``.
#[pyclass(module = "tafla._native", name = "VectorFieldsIterator")]
pub struct VectorFields {
    reader: Py<TableReader>,
    fields: Py<FixedFields>,
    /// What reads an item that `get_fields()` can't, from its position
    read: Py<PyAny>,
    /// The vector's position, and its count, whose ends lie within the bytes
    position: usize,
    count: usize,
    /// The next item, and where it starts, from the vector's position
    index: usize,
    start: usize,
}

#[pymethods]
impl VectorFields {
    const fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__<'py>(&mut self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        if self.index >= self.count {
            return Ok(None);
        }

        let next = self.next_item(py);
        // An iterator that raised is done, as a generator that raised is
        if next.is_err() {
            self.index = self.count;
        }
        next.map(Some)
    }
}

impl VectorFields {
    /// The values of the next item, after reading its end
    fn next_item<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let reader = self.reader.get();
        let size = reader.size();
        let available = reader.available();

        // The ends lie within the bytes, which the iterator checked first
        let slot = self.position + (self.index + 1) * size;
        let end = read_slot(&reader.bytes_slice()[slot..], size);
        let start = self.start;
        if start > end
            || self
                .position
                .checked_add(end)
                .is_none_or(|end| end > available)
        {
            let position = Int::from(self.position);
            return Err(item_error(
                py,
                &position,
                &Int::from(start),
                &Int::from(end),
                available,
            ));
        }
        self.index += 1;
        self.start = end;

        let offset = self.position + start;
        let plan = self.fields.get().plan(size)?;
        match reader.fields_at(py, plan, offset)? {
            Some(values) => Ok(values.into_any()),
            None => self.read.bind(py).call1((offset,)),
        }
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

/// Sets item `index` of a list that was just created
#[inline]
fn list_set(list: &Bound<'_, PyList>, index: usize, value: Bound<'_, PyAny>) -> PyResult<()> {
    #[cfg(not(Py_LIMITED_API))]
    unsafe {
        ffi::PyList_SET_ITEM(list.as_ptr(), index.cast_signed(), value.into_ptr());
    }
    #[cfg(Py_LIMITED_API)]
    if unsafe { ffi::PyList_SetItem(list.as_ptr(), index.cast_signed(), value.into_ptr()) } != 0 {
        return Err(PyErr::fetch(list.py()));
    }
    Ok(())
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
