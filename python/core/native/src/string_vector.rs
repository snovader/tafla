//! The native `StringVector`, which reads a vector of strings through a
//! native `TableReader`, each string in one call.
//!
//! The pure-Python class, given the native reader, makes a Python call for
//! each string that then calls the reader. In `CPython` 3.14, with the lists
//! suite's tables of 10 to 10,000 strings, this one read the last string in
//! 0.78–0.87 times the time that it took, about 230 ns rather than 290 ns,
//! each string in turn in 0.64–0.79 times, and every string with `tolist()`
//! in 0.93–1.03 times, since both make one call of the reader for those.

use pyo3::prelude::*;
use pyo3::types::{PyIterator, PyList};

use crate::python::Int;
use crate::reader::TableReader;

/// A vector of strings in a table, which reads each string when it's asked
/// for, from its end and the end of the string before it, so that a string
/// whose bytes are corrupt raises ``DecodeError`` when it's read, rather than
/// when the vector is viewed. The code that tafla-gen generates returns one
/// from the getter of a ``[string]`` field.
///
/// It reads through the ``TableReader`` that it's given, which is native, as
/// the native classes never mix with the pure-Python ones.
#[pyclass(frozen, module = "tafla._native", name = "StringVector")]
pub struct StringVector {
    reader: Py<TableReader>,
    /// The vector's position, and its count, whose ends lie within the bytes
    position: usize,
    length: usize,
}

#[pymethods]
impl StringVector {
    /// Views the vector of strings at ``position``, as ``root_offset()``
    /// or ``field_offset()`` finds it.
    ///
    /// Raises ``DecodeError`` if the vector's count or the ends of its
    /// strings lie outside the bytes.
    #[new]
    fn new(reader: &Bound<'_, TableReader>, position: &Bound<'_, PyAny>) -> PyResult<Self> {
        let table = reader.get();
        let length = table.get_vector_length(reader.py(), position)?;
        // The position that the count was read at
        let position = table.position(position, table.size())?;

        Ok(Self {
            reader: reader.clone().unbind(),
            position,
            length,
        })
    }

    /// The number of strings
    const fn __len__(&self) -> usize {
        self.length
    }

    /// Reads string ``index``, without reading the others, counting from
    /// the end if it's negative.
    ///
    /// Raises ``IndexError`` if the vector has no string ``index``, and
    /// ``DecodeError`` if the string lies outside the bytes or isn't valid
    /// UTF-8.
    fn __getitem__<'py>(
        &self,
        py: Python<'py>,
        index: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut index = Int::new(index)?;
        if index.is_negative()? {
            index = index.add(self.length)?;
        }

        // The count that the constructor checked, rather than reading and
        // checking it for each string
        let reader = self.reader.get();
        let (start, end) = reader.item_bounds(
            py,
            &Int::from(self.position),
            &Int::from(self.length),
            &index,
        )?;
        reader.decode_string(py, &start, &end)
    }

    /// Reads every string, as ``tolist()`` does, and then iterates over
    /// them.
    ///
    /// Raises ``DecodeError`` if a string lies outside the bytes or isn't
    /// valid UTF-8, before returning any.
    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.tolist(py)?.try_iter()
    }

    /// Reads every string into a list, from the first to the last, as
    /// ``TableReader.get_string_vector()`` reads them, which a list of
    /// many strings takes less time to than reading each.
    ///
    /// Raises ``DecodeError`` if a string lies outside the bytes or isn't
    /// valid UTF-8.
    fn tolist<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        self.reader.get().strings_at(py, self.position)
    }
}
