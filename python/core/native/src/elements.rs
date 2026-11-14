//! The elements of arrays and structs that callers pass, as the pure-Python
//! runtime's `tafla.elements` takes them

use std::ffi::{CStr, c_int, c_long};
use std::mem::MaybeUninit;

use pyo3::exceptions::PyValueError;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::sync::critical_section::with_critical_section;
use pyo3::types::{PyList, PyTuple};

use crate::python::{Int, buffer_bytes, objects, with_buffer, with_bytes};

/// Calls `f` with the bytes of `values` as elements of the array type code
/// `code`, in the host's byte order: those of a one-dimensional, C-contiguous
/// buffer of the code's elements, such as an `array` of the code, as they
/// are, and otherwise those that `tafla.elements.as_elements()` converts them
/// to, as `array()` converts an iterable, raising what it raises
pub fn with_elements<R>(
    values: &Bound<'_, PyAny>,
    code: char,
    item_size: usize,
    f: impl FnOnce(&[u8]) -> PyResult<R>,
) -> PyResult<R> {
    let py = values.py();
    let mut f = Some(f);

    if unsafe { ffi::PyObject_CheckBuffer(values.as_ptr()) } != 0 {
        let flags = ffi::PyBUF_C_CONTIGUOUS | ffi::PyBUF_FORMAT;
        let copied = with_buffer(values, flags, |view| {
            if view.ndim != 1
                || usize::try_from(view.itemsize) != Ok(item_size)
                || !is_host_format(format_of(view), code, item_size)
            {
                return Ok(None);
            }
            let f = f.take().expect("f is called once");
            f(unsafe { buffer_bytes(view) }).map(Some)
        });
        if f.is_none() {
            // f ran on the buffer, and this is what it returned
            return copied.map(|result| result.expect("f returned"));
        }
        // Otherwise the buffer isn't of the code's elements, or its exporter
        // refused the request, such as for bytes that aren't C-contiguous,
        // and as_elements() converts it
    }

    let elements = objects(py)?
        .as_elements
        .bind(py)
        .call1((code.to_string(), values))?;
    let f = f.take().expect("f is called once");
    with_bytes(&elements, f)
}

/// Converts the elements of an exact `list` or `tuple` as `array(code,
/// values)` converts them, into the bytes that `destination` returns for their
/// byte length, in the host's byte order, and returns that length. This is
/// `with_elements()`'s conversion without the `array`, which a writer
/// writes straight into its bytes.
///
/// Only `int`s, `float`s and `bool`s convert, whose conversions run no Python
/// code, so nothing changes the sequence while it's read; on a free-threaded
/// build, a list is locked for it, too. Returns `None` for any other values,
/// for an element of another type or one that `array()` rejects, such as an
/// `int` out of the code's range, and if `destination` returns `None`, so that
/// `with_elements()` converts them, with `array()`'s errors. `destination`
/// may hold elements converted before one that failed.
pub fn convert_sequence<'a>(
    values: &Bound<'_, PyAny>,
    code: char,
    destination: impl FnOnce(usize) -> Option<&'a mut [MaybeUninit<u8>]>,
) -> Option<usize> {
    let list = values.is_exact_instance_of::<PyList>();
    if !list && !values.is_exact_instance_of::<PyTuple>() {
        return None;
    }

    let convert = || {
        // SAFETY: an exact list or tuple, which nothing changes while it's
        // read, as no Python code runs and no other thread holds the list
        let items = unsafe { Items::of(values.as_ptr(), list) };
        match code {
            'b' => items.convert(destination, |item| narrow(item, i8::to_ne_bytes)),
            'B' => items.convert(destination, |item| narrow(item, u8::to_ne_bytes)),
            'h' => items.convert(destination, |item| narrow(item, i16::to_ne_bytes)),
            'H' => items.convert(destination, |item| narrow(item, u16::to_ne_bytes)),
            'i' => items.convert(destination, |item| narrow(item, i32::to_ne_bytes)),
            'I' => items.convert(destination, |item| narrow(item, u32::to_ne_bytes)),
            'q' => items.convert(destination, |item| narrow(item, i64::to_ne_bytes)),
            'Q' => items.convert(destination, |item| unsigned(item).map(u64::to_ne_bytes)),
            // As array() casts a double to a C float, which rounds one beyond
            // a float32's range to infinity
            #[allow(clippy::cast_possible_truncation)]
            'f' => items.convert(destination, |item| {
                real(item).map(|value| (value as f32).to_ne_bytes())
            }),
            'd' => items.convert(destination, |item| real(item).map(f64::to_ne_bytes)),
            _ => None,
        }
    };

    if list {
        with_critical_section(values, convert)
    } else {
        convert()
    }
}

/// The items of an exact list or tuple, borrowed from it
struct Items {
    #[cfg(not(Py_LIMITED_API))]
    items: *const *mut ffi::PyObject,
    #[cfg(Py_LIMITED_API)]
    sequence: *mut ffi::PyObject,
    #[cfg(Py_LIMITED_API)]
    list: bool,
    len: usize,
}

impl Items {
    /// # Safety
    ///
    /// `sequence` must be an exact list if `list`, and an exact tuple
    /// otherwise, which nothing changes while its items are read.
    #[inline]
    unsafe fn of(sequence: *mut ffi::PyObject, list: bool) -> Self {
        #[cfg(not(Py_LIMITED_API))]
        unsafe {
            let len = if list {
                ffi::PyList_GET_SIZE(sequence)
            } else {
                ffi::PyTuple_GET_SIZE(sequence)
            };
            Self {
                items: ffi::PySequence_Fast_ITEMS(sequence),
                len: len.cast_unsigned(),
            }
        }
        #[cfg(Py_LIMITED_API)]
        unsafe {
            let len = if list {
                ffi::PyList_Size(sequence)
            } else {
                ffi::PyTuple_Size(sequence)
            };
            Self {
                sequence,
                list,
                len: len.cast_unsigned(),
            }
        }
    }

    /// Item `index`, which is less than their number
    #[inline]
    fn get(&self, index: usize) -> *mut ffi::PyObject {
        debug_assert!(index < self.len);
        #[cfg(not(Py_LIMITED_API))]
        unsafe {
            *self.items.add(index)
        }
        #[cfg(Py_LIMITED_API)]
        unsafe {
            let index = index.cast_signed();
            if self.list {
                ffi::PyList_GetItem(self.sequence, index)
            } else {
                ffi::PyTuple_GetItem(self.sequence, index)
            }
        }
    }

    /// Converts each item with `element` into the bytes that `destination`
    /// returns for their byte length, and returns that length, or `None` if
    /// an item doesn't convert or `destination` returns `None`
    #[inline]
    fn convert<'a, const N: usize>(
        &self,
        destination: impl FnOnce(usize) -> Option<&'a mut [MaybeUninit<u8>]>,
        element: impl Fn(*mut ffi::PyObject) -> Option<[u8; N]>,
    ) -> Option<usize> {
        let byte_length = self.len.checked_mul(N)?;
        let destination = destination(byte_length)?;
        debug_assert_eq!(destination.len(), byte_length);

        let (elements, _) = destination.as_chunks_mut::<N>();
        for (index, bytes) in elements.iter_mut().enumerate() {
            *bytes = element(self.get(index))?.map(MaybeUninit::new);
        }
        Some(byte_length)
    }
}

/// Whether `object` is an `int` or a `bool`, whose conversions run no Python
/// code, unlike those of their subclasses
#[inline]
fn is_int(object: *mut ffi::PyObject) -> bool {
    unsafe { ffi::PyLong_CheckExact(object) != 0 || ffi::PyBool_Check(object) != 0 }
}

/// The bytes of an `int` or `bool` within `T`'s range, as `array()` converts
/// it, or `None` for another object or one out of range
#[inline]
fn narrow<T: TryFrom<i64>, const N: usize>(
    object: *mut ffi::PyObject,
    bytes: impl Fn(T) -> [u8; N],
) -> Option<[u8; N]> {
    if !is_int(object) {
        return None;
    }
    let mut overflow: c_int = 0;
    // An int's conversion fails only by overflowing
    let value = unsafe { ffi::PyLong_AsLongLongAndOverflow(object, &raw mut overflow) };
    if overflow != 0 {
        return None;
    }
    T::try_from(value).ok().map(bytes)
}

/// An `int` or `bool` within a `u64`, or `None` for another object or one
/// out of range
#[inline]
fn unsigned(object: *mut ffi::PyObject) -> Option<u64> {
    if !is_int(object) {
        return None;
    }
    let mut overflow: c_int = 0;
    let value = unsafe { ffi::PyLong_AsLongLongAndOverflow(object, &raw mut overflow) };
    match overflow {
        0 => u64::try_from(value).ok(),
        // Beyond an i64, which may still fit in a u64
        1 => {
            let value = unsafe { ffi::PyLong_AsUnsignedLongLong(object) };
            if value == u64::MAX && unsafe { !ffi::PyErr_Occurred().is_null() } {
                unsafe { ffi::PyErr_Clear() };
                return None;
            }
            Some(value)
        }
        _ => None,
    }
}

/// A `float`, `int` or `bool` as a double, as `array()` converts it for `f`
/// and `d`, or `None` for another object, or an `int` too large for a double
#[inline]
fn real(object: *mut ffi::PyObject) -> Option<f64> {
    #[cfg(not(Py_LIMITED_API))]
    if unsafe { ffi::PyFloat_CheckExact(object) } != 0 {
        return Some(unsafe { ffi::PyFloat_AS_DOUBLE(object) });
    }
    if unsafe { ffi::PyFloat_CheckExact(object) } == 0 && !is_int(object) {
        return None;
    }

    let value = unsafe { ffi::PyFloat_AsDouble(object) };
    // -1.0 is the C API's sentinel of an error, which only an error sets
    #[allow(clippy::float_cmp)]
    let failed = value == -1.0 && unsafe { !ffi::PyErr_Occurred().is_null() };
    if failed {
        unsafe { ffi::PyErr_Clear() };
        return None;
    }
    Some(value)
}

/// The format of a buffer, which is bytes without one
const fn format_of(view: &ffi::Py_buffer) -> &[u8] {
    if view.format.is_null() {
        return b"B";
    }
    unsafe { CStr::from_ptr(view.format) }.to_bytes()
}

/// Whether a buffer `format` holds elements of the same kind and size as the
/// array type code `code`, in the host's byte order, as
/// `tafla.elements._host_formats()` lists them. They include other letters of
/// that size, such as `l` for `q` where a C long has 8 bytes, which is the
/// format of numpy's int64 arrays.
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

    // The sizes that struct gives each letter: native without a prefix or
    // with "@", and standard with "=" or the byte order, which has no "n"
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

/// The number of structs of `size` bytes in `byte_length` bytes. Raises
/// `ValueError` if `size` is less than 1, or the bytes aren't a whole number
/// of structs, as `tafla.table_writer.struct_count()` does.
pub fn struct_count<'py>(byte_length: usize, size: &Int<'py>) -> PyResult<Int<'py>> {
    check_struct_size(size)?;

    // A size beyond any bytes holds none of them
    let whole = size
        .index()
        .map_or(byte_length == 0, |size| byte_length.is_multiple_of(size));
    if !whole {
        return Err(PyValueError::new_err(format!(
            "{byte_length} bytes aren't a whole number of {}-byte structs",
            size.describe()?
        )));
    }

    Ok(Int::from(size.index().map_or(0, |size| byte_length / size)))
}

/// Raises `ValueError` if `size` is less than 1, which no struct's size is
pub fn check_struct_size(size: &Int<'_>) -> PyResult<()> {
    if size.lt(1)? {
        return Err(PyValueError::new_err(format!(
            "A struct's size must be a positive integer, not {}",
            size.describe()?
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_host_format;

    #[test]
    fn takes_the_formats_of_the_codes_kind_and_size_in_the_hosts_order() {
        let host = if cfg!(target_endian = "little") {
            "<"
        } else {
            ">"
        };
        let other = if cfg!(target_endian = "little") {
            ">"
        } else {
            "<"
        };

        for format in ["q", "@q", "=q", &format!("{host}q")] {
            assert!(is_host_format(format.as_bytes(), 'q', 8), "{format}");
        }
        assert!(is_host_format(b"=l", 'i', 4));
        assert!(is_host_format(b"n", 'q', 8));
        assert!(!is_host_format(b"=n", 'q', 8));
        assert!(!is_host_format(format!("{other}q").as_bytes(), 'q', 8));
        assert!(!is_host_format(b"Q", 'q', 8));
        assert!(!is_host_format(b"f", 'd', 8));
        assert!(!is_host_format(b"!i", 'i', 4));
        assert!(is_host_format(b"B", 'B', 1));
        assert!(!is_host_format(b"c", 'B', 1));
    }
}
