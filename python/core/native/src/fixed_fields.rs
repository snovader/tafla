use std::sync::Arc;

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};

use crate::layout::Layout;
use crate::python::{Int, objects};
use crate::values::Kind;

/// A table's leading fields as a `FixedFields` describes them
#[derive(Debug)]
pub struct Description {
    /// The number of fields of the table
    pub fields: usize,
    pub formats: Vec<Format>,
    /// Whether a string follows the formats
    pub string: bool,
}

#[derive(Debug)]
pub enum Format {
    Value(Kind),
    /// A nested table, which its fields describe whole
    Table(Arc<Description>),
}

/// Where a table's fixed fields lie for one offset size, from its first
/// slot: the slots that they fill, with the offsets that they hold, their
/// values, in the order that `set_fields()` takes them, and the string's
/// length. Nested tables lie in place of their field, their slots and then
/// their values.
#[derive(Debug)]
pub struct Plan {
    /// The byte length of the header before the slots, which the slots'
    /// offsets count, as they count from the table's start
    pub header: usize,
    /// The bytes that the plan covers from the first slot: the slots, the
    /// values and the string's length, which the string's bytes follow
    pub len: usize,
    pub slots: Vec<(usize, usize)>,
    pub values: Vec<(usize, Kind)>,
    pub string_length: Option<usize>,
    /// Whether every offset fits in the offset type. One that doesn't is
    /// written by the setters, which raise for it.
    pub fits: bool,
    /// The offset size
    pub size: usize,
}

impl Plan {
    /// The plan of `description` for offset size `size`, after a header of
    /// `header` bytes, or `None` for fields whose bytes from the first slot
    /// pass a C `ssize_t`, which `struct` can't describe
    fn new(description: &Description, size: usize, header: usize) -> Option<Self> {
        let mut plan = Self {
            header,
            len: 0,
            slots: Vec::new(),
            values: Vec::new(),
            string_length: None,
            fits: true,
            size,
        };
        plan.len = plan.describe(description, header, 0)? - header;
        if isize::try_from(plan.len).is_err() {
            return None;
        }

        let max = (1u64 << (8 * size)) - 1;
        plan.fits = plan.slots.iter().all(|&(_, offset)| offset as u64 <= max);

        Some(plan)
    }

    /// Adds the table that `description` describes, whose slots lie at
    /// `start` from its own start and at `base` from the plan's first slot,
    /// and returns the table's byte length up to its string's bytes, from
    /// its own start, or `None` if it's beyond a `usize`
    fn describe(&mut self, description: &Description, start: usize, base: usize) -> Option<usize> {
        let size = self.size;
        let mut position = description.fields.checked_mul(size)?.checked_add(start)?;

        // The slots lie within the fields' slots, before position
        for (index, format) in description.formats.iter().enumerate() {
            self.slots.push((base + index * size, position));
            let at = base.checked_add(position - start)?;
            let length = match format {
                Format::Value(kind) => {
                    self.values.push((at, *kind));
                    kind.size()
                }
                Format::Table(nested) => self.describe(nested, 0, at)?,
            };
            position = position.checked_add(length)?;
        }

        if description.string {
            self.slots
                .push((base + description.formats.len() * size, position));
            self.string_length = Some(base.checked_add(position - start)?);
            position = position.checked_add(size)?;
        }

        Some(position)
    }
}

/// The leading fields of a table whose offsets don't depend on any value,
/// which ``TableWriter.set_fields()`` and ``write_table()`` write with the
/// table's offsets in one call, rather than with a setter for each field, and
/// ``ByteBuffer.get_fields()`` reads. The code that tafla-gen generates
/// creates one for each table that starts with such fields.
///
/// Those fields are scalars, structs and nested tables whose own fields are
/// all such fields, since none of them is aligned, and then a string, whose
/// length is written with them and whose bytes follow. ``fields`` is the
/// number of fields of the table, and each of ``formats`` describes one of
/// its leading fields, in order:
///
/// - a scalar's ``struct`` format code, such as ``"d"`` for a float64 or
///   ``"?"`` for a bool, which is written as 1 or 0, as it's truthy;
/// - ``"{size}s"`` for a struct of ``size`` bytes, written from its bytes;
/// - the ``FixedFields`` of a nested table that ``formats`` describe whole,
///   whose values follow in place of it.
///
/// ``string`` adds the string that follows them. ``layout`` is a message's
/// layout, whose header its offsets follow. A nested table's fields omit it,
/// and are prepared for each offset type, since the root's sets theirs.
///
/// Like ``TableWriter``, it's the target of generated code, which describes
/// a table's fields as the schema declares them, so it checks only that it
/// can describe them: it raises ``ValueError`` for fewer ``fields`` than it
/// describes, and ``struct.error`` for fields whose bytes from the first slot
/// pass a C ``ssize_t``, as ``struct`` does.
#[pyclass(frozen, module = "tafla._native", name = "FixedFields")]
pub struct FixedFields {
    description: Arc<Description>,
    /// A plan for each offset size, 1, 2 and 4: the root's for its layout's
    /// alone, and a nested table's for each
    plans: [Option<Plan>; 3],
}

impl FixedFields {
    /// The plan for offset size `size`, or `TypeError` for fields of a
    /// layout of another offset size
    #[inline]
    pub fn plan(&self, size: usize) -> PyResult<&Plan> {
        self.plans[size.trailing_zeros() as usize]
            .as_ref()
            .ok_or_else(|| {
                PyTypeError::new_err("The fields are those of a layout of another offset type")
            })
    }

    /// The fields, as the setters write them when the plan can't
    pub fn description(&self) -> &Description {
        &self.description
    }
}

#[pymethods]
impl FixedFields {
    #[new]
    #[pyo3(signature = (fields, *formats, string = None, layout = None))]
    fn new(
        fields: &Bound<'_, PyAny>,
        formats: &Bound<'_, PyTuple>,
        string: Option<&Bound<'_, PyAny>>,
        layout: Option<Layout>,
    ) -> PyResult<Self> {
        let py = formats.py();
        let string = string.map_or(Ok(false), PyAnyMethods::is_truthy)?;
        let leading = formats.len() + usize::from(string);
        let count = Int::new(fields)?;
        if count.lt(leading)? {
            return Err(PyValueError::new_err(format!(
                "A table of {} fields can't start with {leading} of them",
                count.describe()?
            )));
        }
        // At least the leading fields, so only a count beyond a usize is
        // left, whose slots no struct describes
        let fields = count.index().ok_or_else(|| too_long(py))?;

        let mut parsed = Vec::with_capacity(formats.len());
        for format in formats.iter() {
            parsed.push(parse(&format)?);
        }
        let description = Arc::new(Description {
            fields,
            formats: parsed,
            string,
        });

        let mut plans = [None, None, None];
        match layout {
            None => {
                for (slot, size) in plans.iter_mut().zip([1, 2, 4]) {
                    *slot = Some(Plan::new(&description, size, 0).ok_or_else(|| too_long(py))?);
                }
            }
            Some(layout) => {
                let shape = layout.shape();
                let plan = Plan::new(&description, shape.size(), shape.header_size())
                    .ok_or_else(|| too_long(py))?;
                plans[shape.size().trailing_zeros() as usize] = Some(plan);
            }
        }

        Ok(Self { description, plans })
    }
}

/// The error of fields too long for a `struct` to describe, which
/// `struct.Struct()` raises for the pure-Python runtime's
#[cold]
fn too_long(py: Python<'_>) -> PyErr {
    match objects(py) {
        Ok(objects) => PyErr::from_type(
            objects.struct_error.bind(py).clone(),
            "total struct size too long",
        ),
        Err(error) => error,
    }
}

/// A format of `FixedFields()`: a scalar's or struct's `struct` format code,
/// or a nested table's `FixedFields`, whose fields are fixed, without a
/// string
fn parse(format: &Bound<'_, PyAny>) -> PyResult<Format> {
    if let Ok(nested) = format.cast::<FixedFields>() {
        let nested = &nested.get().description;
        if nested.string {
            return Err(PyValueError::new_err(
                "A nested table's fields can't end with a string, whose length isn't fixed",
            ));
        }
        return Ok(Format::Table(Arc::clone(nested)));
    }

    let code = format.cast::<PyString>().map_err(PyErr::from)?;
    Kind::parse(code.to_str()?)
        .map(Format::Value)
        .ok_or_else(|| {
            PyValueError::new_err(format!(
                "{} isn't the struct format code of a scalar or a struct",
                format
                    .repr()
                    .map_or_else(|_| String::from("The format"), |repr| repr.to_string())
            ))
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{Description, Format, Plan};
    use crate::values::Kind;

    #[test]
    fn plans_a_root_and_a_nested_table_in_place() {
        // A root of 3 fields after an 8-byte header: a float64, a nested
        // table of two float32s, and a string
        let pair = Arc::new(Description {
            fields: 2,
            formats: vec![Format::Value(Kind::F32), Format::Value(Kind::F32)],
            string: false,
        });
        let root = Description {
            fields: 3,
            formats: vec![Format::Value(Kind::F64), Format::Table(pair)],
            string: true,
        };

        let plan = Plan::new(&root, 4, 8).expect("the fields fit");

        // Root slots at 0, 4 and 8 from the first slot, holding offsets from
        // the table's start, which is 8 bytes before them; the float64 at 20
        // from the start, the pair at 28, with its slots and then its values
        // at 36 and 40, and the string's length at 44. The pair's slots
        // hold offsets from its own start.
        assert_eq!(plan.slots, [(0, 20), (4, 28), (20, 8), (24, 12), (8, 44)]);
        assert_eq!(
            plan.values,
            [(12, Kind::F64), (28, Kind::F32), (32, Kind::F32)]
        );
        assert_eq!(plan.string_length, Some(36));
        assert_eq!(plan.len, 40);
        assert!(plan.fits);
        assert!(!Plan::new(&root, 1, 0x100).expect("the fields fit").fits);
    }

    #[test]
    fn plans_no_fields_whose_length_passes_an_isize() {
        let structs = |sizes: &[usize]| Description {
            fields: sizes.len(),
            formats: sizes
                .iter()
                .map(|&size| Format::Value(Kind::Struct(size)))
                .collect(),
            string: false,
        };

        // Lengths that wrap a usize, and that pass an isize without wrapping
        let half = 1 << (usize::BITS - 2);
        assert!(Plan::new(&structs(&[1 << 30, usize::MAX - (1 << 30) + 1]), 4, 0).is_none());
        assert!(Plan::new(&structs(&[half, half]), 4, 0).is_none());
        assert!(Plan::new(&structs(&[usize::MAX]), 1, 0).is_none());
        // isize::MAX bytes after the slot, which is one more
        assert!(Plan::new(&structs(&[isize::MAX.unsigned_abs()]), 1, 0).is_none());
        assert!(Plan::new(&structs(&[isize::MAX.unsigned_abs() - 1]), 1, 8).is_some());

        let many = Description {
            fields: usize::MAX / 4,
            formats: vec![Format::Value(Kind::I32)],
            string: false,
        };
        assert!(Plan::new(&many, 1, 0).is_some());
        assert!(Plan::new(&many, 2, 0).is_none());
    }
}
