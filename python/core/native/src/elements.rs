//! The elements of arrays and structs that callers pass, as the pure-Python
//! runtime's `tafla.elements` takes them

use std::ffi::{CStr, c_long};

use pyo3::exceptions::PyValueError;
use pyo3::ffi;
use pyo3::prelude::*;

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
