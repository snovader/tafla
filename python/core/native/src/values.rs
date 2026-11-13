//! Scalars and structs, converted between Python's objects and their
//! little-endian bytes as `struct` converts them, which the pure-Python
//! runtime packs and unpacks them with. A value that `struct` can't pack
//! is rejected, so that the caller can raise `struct`'s own error for it.

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::python::objects;

/// The type of a scalar or struct field, which a `FixedFields` describes by
/// its `struct` format code
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    /// A bool, written as 1 when its value is truthy and 0 otherwise
    Bool,
    /// A struct of this many bytes, written from its bytes
    Struct(usize),
}

impl Kind {
    /// The kind of a `struct` format code: a scalar's, `"?"` for a bool or
    /// `"{size}s"` for a struct
    pub fn parse(code: &str) -> Option<Self> {
        Some(match code {
            "b" => Self::I8,
            "B" => Self::U8,
            "h" => Self::I16,
            "H" => Self::U16,
            "i" => Self::I32,
            "I" => Self::U32,
            "q" => Self::I64,
            "Q" => Self::U64,
            "f" => Self::F32,
            "d" => Self::F64,
            "?" => Self::Bool,
            _ => {
                let size = code.strip_suffix('s')?;
                if size.is_empty() {
                    Self::Struct(1)
                } else if size.bytes().all(|digit| digit.is_ascii_digit()) {
                    // A size beyond a usize is too long for any plan, as
                    // struct rejects any beyond an isize
                    Self::Struct(size.parse().unwrap_or(usize::MAX))
                } else {
                    return None;
                }
            }
        })
    }

    pub const fn size(self) -> usize {
        match self {
            Self::I8 | Self::U8 | Self::Bool => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
            Self::Struct(size) => size,
        }
    }

    /// The inclusive range of an integer kind's values
    const fn range(self) -> (i64, i64) {
        match self {
            Self::I8 => (i8::MIN as i64, i8::MAX as i64),
            Self::U8 => (0, u8::MAX as i64),
            Self::I16 => (i16::MIN as i64, i16::MAX as i64),
            Self::U16 => (0, u16::MAX as i64),
            Self::I32 => (i32::MIN as i64, i32::MAX as i64),
            Self::U32 => (0, u32::MAX as i64),
            _ => (i64::MIN, i64::MAX),
        }
    }
}

/// Writes `value` as `kind` into `destination`, which is `kind.size()` bytes,
/// as `struct` packs it with the kind's little-endian format. Returns
/// `false`, having written nothing, for a value that `struct` rejects with
/// `struct.error` or `OverflowError`, and raises what `struct` raises
/// otherwise, such as an exception from a value's `__index__()`.
#[inline]
pub fn write(kind: Kind, value: &Bound<'_, PyAny>, destination: &mut [u8]) -> PyResult<bool> {
    let py = value.py();
    let object = value.as_ptr();

    match kind {
        Kind::F64 => Ok(float(object).is_some_and(|float| {
            destination.copy_from_slice(&float.to_le_bytes());
            true
        })),
        Kind::F32 => match float(object) {
            // struct packs a NaN's payload by rules that its version decides
            Some(float) if float.is_nan() => write_by_struct(kind, value, destination),
            Some(float) => {
                #[allow(clippy::cast_possible_truncation)]
                let single = float as f32;
                if single.is_infinite() && float.is_finite() {
                    return Ok(false);
                }
                destination.copy_from_slice(&single.to_le_bytes());
                Ok(true)
            }
            None => Ok(false),
        },
        Kind::Bool => {
            let truth = unsafe { ffi::PyObject_IsTrue(object) };
            if truth < 0 {
                return Err(PyErr::fetch(py));
            }
            destination[0] = u8::from(truth == 1);
            Ok(true)
        }
        Kind::Struct(size) => Ok(write_struct(value, size, destination)),
        Kind::U64 => integer(value, |object| {
            let mut overflow = 0;
            let small = unsafe { ffi::PyLong_AsLongLongAndOverflow(object, &raw mut overflow) };
            let unsigned = match overflow {
                0 => u64::try_from(small).ok(),
                1 => {
                    let large = unsafe { ffi::PyLong_AsUnsignedLongLong(object) };
                    if large == u64::MAX && unsafe { !ffi::PyErr_Occurred().is_null() } {
                        unsafe { ffi::PyErr_Clear() };
                        None
                    } else {
                        Some(large)
                    }
                }
                _ => None,
            };
            Ok(unsigned.map(|unsigned| destination.copy_from_slice(&unsigned.to_le_bytes())))
        }),
        _ => integer(value, |object| {
            let mut overflow = 0;
            let small = unsafe { ffi::PyLong_AsLongLongAndOverflow(object, &raw mut overflow) };
            let (min, max) = kind.range();
            if overflow != 0 || small < min || small > max {
                return Ok(None);
            }

            let bytes = small.to_le_bytes();
            destination.copy_from_slice(&bytes[..kind.size()]);
            Ok(Some(()))
        }),
    }
}

/// Converts an integer as `struct` does: an `int` as it is, another object
/// through its `__index__()`, whose errors propagate, and anything else is
/// rejected. `convert` returns `None` for an `int` out of the kind's range.
#[inline]
fn integer(
    value: &Bound<'_, PyAny>,
    convert: impl FnOnce(*mut ffi::PyObject) -> PyResult<Option<()>>,
) -> PyResult<bool> {
    let object = value.as_ptr();
    if unsafe { ffi::PyLong_Check(object) } != 0 {
        return Ok(convert(object)?.is_some());
    }
    if unsafe { ffi::PyIndex_Check(object) } == 0 {
        return Ok(false);
    }

    let index = unsafe { Bound::from_owned_ptr_or_err(value.py(), ffi::PyNumber_Index(object))? };
    Ok(convert(index.as_ptr())?.is_some())
}

/// A float as `struct` converts it, or `None` for an object that
/// `PyFloat_AsDouble()` can't convert, which `struct` rejects whatever the
/// error
#[inline]
fn float(object: *mut ffi::PyObject) -> Option<f64> {
    let float = unsafe { ffi::PyFloat_AsDouble(object) };
    // -1.0 is the C API's sentinel of an error, which only an error sets
    #[allow(clippy::float_cmp)]
    let failed = float == -1.0 && unsafe { !ffi::PyErr_Occurred().is_null() };
    if failed {
        unsafe { ffi::PyErr_Clear() };
        return None;
    }
    Some(float)
}

/// Writes a struct field from bytes or a `bytearray`, as `struct`'s `"s"`
/// format packs them: truncated or padded with zeros to its size. Other
/// buffers are rejected, which `TableWriter.set_struct()` writes whole.
fn write_struct(value: &Bound<'_, PyAny>, size: usize, destination: &mut [u8]) -> bool {
    let object = value.as_ptr();
    let (data, length) = unsafe {
        if ffi::PyBytes_Check(object) != 0 {
            (ffi::PyBytes_AsString(object), ffi::PyBytes_Size(object))
        } else if ffi::PyByteArray_Check(object) != 0 {
            (
                ffi::PyByteArray_AsString(object),
                ffi::PyByteArray_Size(object),
            )
        } else {
            return false;
        }
    };

    let length = usize::try_from(length).unwrap_or(0).min(size);
    if length > 0 {
        let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length) };
        destination[..length].copy_from_slice(bytes);
    }
    destination[length..].fill(0);

    true
}

/// Writes `value` with `struct` itself, for a value whose conversion its
/// version decides
#[cold]
fn write_by_struct(kind: Kind, value: &Bound<'_, PyAny>, destination: &mut [u8]) -> PyResult<bool> {
    let py = value.py();
    let packed = objects(py)?
        .struct_of(py, kind)
        .call_method1("pack", (value,))?;
    destination.copy_from_slice(packed.cast::<PyBytes>()?.as_bytes());

    Ok(true)
}

/// Reads the value of `kind` that `source` holds, as a new reference, as
/// `struct` unpacks it. Creates no object that the garbage collector tracks,
/// so no Python code runs while `source` is borrowed, except for a float32
/// NaN, which `struct` reads.
///
/// # Panics
///
/// If `source` is shorter than `kind.size()`, which a plan's values never
/// are within the bytes that it covers.
#[inline]
pub fn read<'py>(py: Python<'py>, kind: Kind, source: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    let source = &source[..kind.size()];
    let object = unsafe {
        match kind {
            Kind::F64 => ffi::PyFloat_FromDouble(f64::from_le_bytes(chunk(source))),
            Kind::F32 => {
                let single = f32::from_le_bytes(chunk(source));
                if single.is_nan() {
                    return read_by_struct(py, kind, source);
                }
                ffi::PyFloat_FromDouble(f64::from(single))
            }
            Kind::I64 => ffi::PyLong_FromLongLong(i64::from_le_bytes(chunk(source))),
            Kind::U64 => ffi::PyLong_FromUnsignedLongLong(u64::from_le_bytes(chunk(source))),
            Kind::I32 => ffi::PyLong_FromLongLong(i32::from_le_bytes(chunk(source)).into()),
            Kind::U32 => ffi::PyLong_FromLongLong(u32::from_le_bytes(chunk(source)).into()),
            Kind::I16 => ffi::PyLong_FromLongLong(i16::from_le_bytes(chunk(source)).into()),
            Kind::U16 => ffi::PyLong_FromLongLong(u16::from_le_bytes(chunk(source)).into()),
            Kind::I8 => ffi::PyLong_FromLongLong(i8::from_le_bytes(chunk(source)).into()),
            Kind::U8 => ffi::PyLong_FromLongLong(source[0].into()),
            Kind::Bool => {
                let truth = if source[0] == 0 {
                    ffi::Py_False()
                } else {
                    ffi::Py_True()
                };
                ffi::Py_IncRef(truth);
                truth
            }
            // A plan's struct is at most isize::MAX bytes, as is any slice
            Kind::Struct(size) => {
                ffi::PyBytes_FromStringAndSize(source.as_ptr().cast(), size.cast_signed())
            }
        }
    };

    unsafe { Bound::from_owned_ptr_or_err(py, object) }
}

/// The float of a float32's bits, as `struct` unpacks them
#[inline]
pub fn read_f32(py: Python<'_>, bytes: [u8; 4]) -> PyResult<f64> {
    let single = f32::from_le_bytes(bytes);
    if single.is_nan() {
        return read_by_struct(py, Kind::F32, &bytes)?.extract();
    }
    Ok(f64::from(single))
}

/// Reads a value with `struct` itself, for bits whose conversion its version
/// decides
#[cold]
fn read_by_struct<'py>(py: Python<'py>, kind: Kind, source: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    let bytes = PyBytes::new(py, source);
    objects(py)?
        .struct_of(py, kind)
        .call_method1("unpack", (bytes,))?
        .get_item(0)
}

/// The first `N` bytes of `source`, which holds them
#[inline]
fn chunk<const N: usize>(source: &[u8]) -> [u8; N] {
    *source.first_chunk().expect("the source holds the value")
}

#[cfg(test)]
mod tests {
    use super::Kind;

    #[test]
    fn parses_the_formats_of_fixed_fields() {
        assert_eq!(Kind::parse("d"), Some(Kind::F64));
        assert_eq!(Kind::parse("?"), Some(Kind::Bool));
        assert_eq!(Kind::parse("12s"), Some(Kind::Struct(12)));
        assert_eq!(Kind::parse("s"), Some(Kind::Struct(1)));
        assert_eq!(Kind::parse("2h"), None);
        assert_eq!(Kind::parse("-1s"), None);
        assert_eq!(
            Kind::parse("99999999999999999999999999s"),
            Some(Kind::Struct(usize::MAX))
        );
        assert_eq!(Kind::parse("e"), None);
    }
}
