//! The Python objects that the classes take, call and raise: integers that
//! callers pass, buffers that objects export, the pure-Python runtime's
//! errors and conversions, and the standard library's `struct` and `array`.

use std::ffi::c_int;

use pyo3::exceptions::{PyBufferError, PyTypeError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyString, PyType};
use pyo3::{IntoPyObjectExt, ffi};

use crate::values::Kind;

/// An integer that a caller passed, such as a position, an index or a count,
/// or one computed from them: an `i128` when it fits in an `i64`, as every
/// position within bytes does, and the Python `int` otherwise. Arithmetic on
/// an `int` beyond an `i64` is Python's, so that an error reports the value
/// that the pure-Python runtime reports, however large.
#[derive(Clone)]
pub enum Int<'py> {
    Small(i128),
    Big(Bound<'py, PyAny>),
}

impl<'py> Int<'py> {
    /// Takes an integer argument, or an object with `__index__()`, as
    /// `struct` and slices take them. Raises `TypeError` for another object.
    #[inline]
    pub fn new(value: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(small(value)?.map_or_else(
            || Self::Big(value.clone()),
            |small| Self::Small(small.into()),
        ))
    }

    /// The value as a position or length within bytes, or `None` if it's
    /// negative or beyond any bytes
    #[inline]
    pub fn index(&self) -> Option<usize> {
        match self {
            Self::Small(value) => usize::try_from(*value).ok(),
            Self::Big(_) => None,
        }
    }

    #[inline]
    pub fn add(&self, other: impl Into<Self>) -> PyResult<Self> {
        let other = other.into();
        if let (Self::Small(a), Self::Small(b)) = (self, &other)
            && let Some(sum) = a.checked_add(*b)
        {
            return Ok(Self::Small(sum));
        }
        self.python(&other, |a, b| a.add(b))
    }

    #[inline]
    pub fn sub(&self, other: impl Into<Self>) -> PyResult<Self> {
        let other = other.into();
        if let (Self::Small(a), Self::Small(b)) = (self, &other)
            && let Some(difference) = a.checked_sub(*b)
        {
            return Ok(Self::Small(difference));
        }
        self.python(&other, |a, b| a.sub(b))
    }

    #[inline]
    pub fn mul(&self, other: impl Into<Self>) -> PyResult<Self> {
        let other = other.into();
        if let (Self::Small(a), Self::Small(b)) = (self, &other)
            && let Some(product) = a.checked_mul(*b)
        {
            return Ok(Self::Small(product));
        }
        self.python(&other, |a, b| a.mul(b))
    }

    /// Whether the value is less than `other`
    #[inline]
    pub fn lt(&self, other: impl Into<Self>) -> PyResult<bool> {
        let other = other.into();
        if let (Self::Small(a), Self::Small(b)) = (self, &other) {
            return Ok(a < b);
        }
        let py = self.py_of(&other);
        self.to_object(py)?.lt(other.to_object(py)?)
    }

    #[inline]
    pub fn is_negative(&self) -> PyResult<bool> {
        self.lt(0)
    }

    /// The value as Python formats it, as an f-string does
    pub fn display(&self) -> PyResult<String> {
        match self {
            Self::Small(value) => Ok(value.to_string()),
            Self::Big(value) => Ok(value.str()?.to_string()),
        }
    }

    /// The value as `tafla.errors.describe_int()` describes it: in decimal,
    /// or by its sign and bit length beyond 64 bits
    pub fn describe(&self) -> PyResult<String> {
        match self {
            Self::Small(value) => Ok(describe(*value)),
            Self::Big(value) => objects(value.py())?
                .describe_int
                .bind(value.py())
                .call1((value,))?
                .extract(),
        }
    }

    pub fn to_object(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        match self {
            Self::Small(value) => value.into_bound_py_any(py),
            Self::Big(value) => Ok(value.clone()),
        }
    }

    /// Computes with Python's integers, for operands or a result beyond an
    /// `i128`, and takes the result back as an `i128` when it fits
    #[cold]
    fn python(
        &self,
        other: &Self,
        operation: impl FnOnce(&Bound<'py, PyAny>, &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>>,
    ) -> PyResult<Self> {
        let py = self.py_of(other);
        let result = operation(&self.to_object(py)?, &other.to_object(py)?)?;

        Ok(small(&result)?.map_or_else(
            || Self::Big(result.clone()),
            |small| Self::Small(small.into()),
        ))
    }

    /// The `Python` of whichever operand is an `int`, or of the thread, which
    /// is attached in every method, when two `i128`s overflow
    fn py_of(&self, other: &Self) -> Python<'py> {
        match (self, other) {
            (Self::Big(value), _) | (_, Self::Big(value)) => value.py(),
            // SAFETY: only the classes' methods compute, with the thread
            // attached
            (Self::Small(_), Self::Small(_)) => unsafe { Python::assume_attached() },
        }
    }
}

impl From<i128> for Int<'_> {
    fn from(value: i128) -> Self {
        Self::Small(value)
    }
}

impl From<usize> for Int<'_> {
    fn from(value: usize) -> Self {
        Self::Small(value as i128)
    }
}

/// The value of an integer argument when it fits in an `i64`, and `None` for
/// a larger one. Raises `TypeError` for an object that isn't an integer and
/// has no `__index__()`.
#[inline]
pub fn small(value: &Bound<'_, PyAny>) -> PyResult<Option<i64>> {
    let mut overflow: c_int = 0;
    let small = unsafe { ffi::PyLong_AsLongLongAndOverflow(value.as_ptr(), &raw mut overflow) };
    if overflow != 0 {
        return Ok(None);
    }
    if small == -1
        && let Some(error) = PyErr::take(value.py())
    {
        return Err(error);
    }

    Ok(Some(small))
}

/// `value` in decimal, or by its sign and bit length beyond 64 bits, as
/// `tafla.errors.describe_int()` describes an `int`
pub fn describe(value: i128) -> String {
    let bits = 128 - value.unsigned_abs().leading_zeros();
    if bits <= 64 {
        return value.to_string();
    }

    let sign = if value < 0 { "negative " } else { "" };
    format!("<{sign}{bits}-bit integer>")
}

/// The Python objects that the classes use, imported once when first used:
/// the pure-Python runtime's error and conversions, and the standard
/// library's `struct` and `array`, whose errors and conversions the classes
/// share with it
pub struct Objects {
    /// `tafla.errors.DecodeError`
    pub decode_error: Py<PyType>,
    /// `DecodeError.out_of_range`
    pub out_of_range: Py<PyAny>,
    /// `tafla.errors.describe_int`
    pub describe_int: Py<PyAny>,
    /// `tafla.elements.as_elements`, which converts what isn't a buffer of
    /// an array's element type
    pub as_elements: Py<PyAny>,
    /// `array.array`
    pub array: Py<PyAny>,
    /// `array("Q", [0])`, which `get_vector_offsets()` repeats
    pub zero_offset: Py<PyAny>,
    /// `struct.error`
    pub struct_error: Py<PyType>,
    /// The `struct.Struct` of each scalar, `tafla.structs`', which raise the
    /// errors of values that a setter can't write
    structs: [Py<PyAny>; 10],
}

impl Objects {
    pub fn struct_of<'py>(&self, py: Python<'py>, kind: Kind) -> &Bound<'py, PyAny> {
        let index = match kind {
            Kind::I8 => 0,
            Kind::U8 | Kind::Bool | Kind::Struct(_) => 1,
            Kind::I16 => 2,
            Kind::U16 => 3,
            Kind::I32 => 4,
            Kind::U32 => 5,
            Kind::I64 => 6,
            Kind::U64 => 7,
            Kind::F32 => 8,
            Kind::F64 => 9,
        };
        self.structs[index].bind(py)
    }

    /// Whether `error` is one that `struct` raises for a value or position
    /// that it can't pack, which a setter raises as its own `ValueError`:
    /// `struct.error`, `OverflowError` for a float too large for a float32,
    /// and `IndexError` for a position beyond a C `ssize_t`
    pub fn is_pack_error(&self, py: Python<'_>, error: &PyErr) -> bool {
        error.is_instance(py, self.struct_error.bind(py))
            || error.is_instance_of::<pyo3::exceptions::PyOverflowError>(py)
            || error.is_instance_of::<pyo3::exceptions::PyIndexError>(py)
    }
}

static OBJECTS: PyOnceLock<Objects> = PyOnceLock::new();

pub fn objects(py: Python<'_>) -> PyResult<&Objects> {
    OBJECTS.get_or_try_init(py, || {
        let errors = py.import("tafla.errors")?;
        let decode_error = errors.getattr("DecodeError")?.cast_into::<PyType>()?;
        let structs = py.import("tafla.structs")?;
        let array = py.import("array")?.getattr("array")?;
        let zero_offset = array.call1(("Q", [0]))?;

        let names = [
            "int8", "uint8", "int16", "uint16", "int32", "uint32", "int64", "uint64", "float32",
            "float64",
        ];
        let mut scalars = Vec::with_capacity(names.len());
        for name in names {
            scalars.push(structs.getattr(name)?.unbind());
        }

        Ok::<_, PyErr>(Objects {
            out_of_range: decode_error.getattr("out_of_range")?.unbind(),
            decode_error: decode_error.unbind(),
            describe_int: errors.getattr("describe_int")?.unbind(),
            as_elements: py
                .import("tafla.elements")?
                .getattr("as_elements")?
                .unbind(),
            zero_offset: zero_offset.unbind(),
            array: array.unbind(),
            struct_error: py
                .import("struct")?
                .getattr("error")?
                .cast_into::<PyType>()?
                .unbind(),
            structs: scalars.try_into().expect("a Struct of each scalar"),
        })
    })
}

/// A `DecodeError` with `message`
#[cold]
pub fn decode_error(py: Python<'_>, message: String) -> PyErr {
    match objects(py) {
        Ok(objects) => PyErr::from_type(objects.decode_error.bind(py).clone(), message),
        Err(error) => error,
    }
}

/// The `DecodeError` of a read of `byte_length` bytes at `position` that
/// leaves the `available` bytes, built by `DecodeError.out_of_range()`
#[cold]
pub fn out_of_range(
    py: Python<'_>,
    position: &Int<'_>,
    byte_length: &Int<'_>,
    available: usize,
) -> PyErr {
    let build = || -> PyResult<PyErr> {
        let error = objects(py)?.out_of_range.bind(py).call1((
            position.to_object_in(py)?,
            byte_length.to_object_in(py)?,
            available,
        ))?;
        Ok(PyErr::from_value(error))
    };

    build().unwrap_or_else(|error| error)
}

/// The `DecodeError` of a core read that failed
#[cold]
pub fn core_error(py: Python<'_>, error: &tafla::DecodeError) -> PyErr {
    match *error {
        tafla::DecodeError::OutOfRange {
            position,
            byte_length,
            available,
        } => out_of_range(
            py,
            &Int::from(position),
            &Int::Small(byte_length.into()),
            available,
        ),
        _ => decode_error(py, error.to_string()),
    }
}

impl Int<'_> {
    /// The value as a Python `int`, for an error, in any `Python`'s lifetime
    fn to_object_in<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        match self {
            Self::Small(value) => value.into_bound_py_any(py),
            Self::Big(value) => Ok(value.clone().unbind().into_bound(py)),
        }
    }
}

/// The error of bytes that aren't C-contiguous, such as a slice with a step,
/// as the pure-Python runtime raises it
#[cold]
pub fn not_contiguous() -> PyErr {
    PyTypeError::new_err(
        "Can't view the bytes of a buffer that isn't C-contiguous, such as a slice with a step",
    )
}

/// Calls `f` with the buffer that `object` exports for `flags`, and releases
/// it after. The buffer stays in this frame, since an exporter may point its
/// shape at its own length within it.
#[inline]
pub fn with_buffer<R>(
    object: &Bound<'_, PyAny>,
    flags: c_int,
    f: impl FnOnce(&ffi::Py_buffer) -> PyResult<R>,
) -> PyResult<R> {
    let mut view = std::mem::MaybeUninit::<ffi::Py_buffer>::uninit();
    if unsafe { ffi::PyObject_GetBuffer(object.as_ptr(), view.as_mut_ptr(), flags) } != 0 {
        return Err(PyErr::fetch(object.py()));
    }
    let mut view = unsafe { view.assume_init() };

    let result = f(&view);
    unsafe { ffi::PyBuffer_Release(&raw mut view) };

    result
}

/// Calls `f` with the bytes of a C-contiguous buffer, which raises
/// `TypeError` for one that isn't, as appending it to a `bytearray` does
#[inline]
pub fn with_bytes<R>(
    object: &Bound<'_, PyAny>,
    f: impl FnOnce(&[u8]) -> PyResult<R>,
) -> PyResult<R> {
    let mut view = std::mem::MaybeUninit::<ffi::Py_buffer>::uninit();
    if unsafe { ffi::PyObject_GetBuffer(object.as_ptr(), view.as_mut_ptr(), ffi::PyBUF_SIMPLE) }
        != 0
    {
        return Err(contiguous_error(object.py(), PyErr::fetch(object.py())));
    }
    let mut view = unsafe { view.assume_init() };

    let result = f(unsafe { buffer_bytes(&view) });
    unsafe { ffi::PyBuffer_Release(&raw mut view) };

    result
}

/// `error`, as `TypeError` if it's the `BufferError` of a buffer that isn't
/// C-contiguous
#[cold]
pub fn contiguous_error(py: Python<'_>, error: PyErr) -> PyErr {
    if error.is_instance_of::<PyBufferError>(py) {
        return not_contiguous();
    }
    error
}

/// The bytes of a buffer, which must stay held while they're used
///
/// # Safety
///
/// `view` must be a buffer that an exporter filled and hasn't released.
#[inline]
pub unsafe fn buffer_bytes(view: &ffi::Py_buffer) -> &[u8] {
    let length = usize::try_from(view.len).unwrap_or(0);
    if length == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(view.buf.cast::<u8>(), length) }
}

/// The UTF-8 of a string, which `CPython` keeps with it: an ASCII string's own
/// characters, and a copy that it caches for any other. Raises
/// `UnicodeEncodeError` for a lone surrogate, as `str.encode()` does.
#[inline]
pub fn utf8<'a>(string: &'a Bound<'_, PyString>) -> PyResult<&'a [u8]> {
    let mut length: ffi::Py_ssize_t = 0;
    let data = unsafe { ffi::PyUnicode_AsUTF8AndSize(string.as_ptr(), &raw mut length) };
    if data.is_null() {
        return Err(PyErr::fetch(string.py()));
    }

    let length = usize::try_from(length).unwrap_or(0);
    Ok(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length) })
}

#[cfg(test)]
mod tests {
    use super::describe;

    #[test]
    fn describes_values_beyond_64_bits_by_their_bit_length() {
        assert_eq!(describe(0), "0");
        assert_eq!(describe(-1), "-1");
        assert_eq!(describe(i128::from(u64::MAX)), "18446744073709551615");
        assert_eq!(describe(-i128::from(u64::MAX)), "-18446744073709551615");
        assert_eq!(describe(1 << 64), "<65-bit integer>");
        assert_eq!(describe(-(1 << 64)), "<negative 65-bit integer>");
    }
}
