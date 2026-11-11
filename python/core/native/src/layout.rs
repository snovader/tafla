use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple, PyType};
use tafla::OffsetType;

/// What the classes use of a layout, copied from it
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub sized: bool,
    pub versioned: bool,
    pub offset_type: OffsetType,
}

impl Shape {
    pub const DEFAULT: Self = Self {
        sized: false,
        versioned: false,
        offset_type: OffsetType::Uint32,
    };

    /// The offset size, the width of offsets, lengths and header slots
    pub const fn size(self) -> usize {
        self.offset_type.size()
    }

    /// The byte length of the header slots before the field offsets
    pub const fn header_size(self) -> usize {
        (self.sized as usize + self.versioned as usize) * self.size()
    }

    /// The longest table that the offset type addresses, and its largest
    /// value
    pub const fn max_length(self) -> usize {
        self.offset_type.max_value() as usize
    }

    /// The Rust runtime's layout, unversioned, which reads the same bytes
    pub const fn core(self) -> tafla::TableLayout {
        let layout = tafla::TableLayout::DEFAULT.with_offset_type(self.offset_type);
        if self.sized { layout.sized() } else { layout }
    }
}

/// The header and offset type of a table. A format creates its layout once
/// and passes it to both the ``TableBuilder`` that writes the table and the
/// ``ByteBuffer`` that reads it, so that the two can't disagree. Nested tables
/// use ``TableLayout.DEFAULT``.
///
/// - ``sized``: whether the header starts with the table's total byte length.
/// - ``versioned``: whether the header has a slot for a version, after the
///   length when ``sized``. Readers can check it before reading any field.
/// - ``offset_type``: the offset type, the unsigned integer type of the header
///   slots, field offsets, counts, lengths and item ends, which limits the
///   table's length: ``"uint8"``, of offset size 1, for up to 255 bytes,
///   ``"uint16"``, of 2, for 65,535 and ``"uint32"``, of 4, the default, for
///   4 GiB.
///
/// Raises ``ValueError`` if ``offset_type`` isn't ``"uint8"``, ``"uint16"`` or
/// ``"uint32"``.
#[pyclass(frozen, module = "tafla._native", name = "TableLayout")]
pub struct TableLayout {
    shape: Shape,
}

impl TableLayout {
    pub const fn shape(&self) -> Shape {
        self.shape
    }
}

/// The shape of a layout argument, or of `TableLayout.DEFAULT` without one
pub fn shape_of(layout: Option<&Bound<'_, TableLayout>>) -> Shape {
    layout.map_or(Shape::DEFAULT, |layout| layout.get().shape)
}

/// An `offset_type` argument: the name of an offset type, or `ValueError`
/// for anything else, `None` included
struct OffsetTypeName(OffsetType);

impl<'a, 'py> FromPyObject<'a, 'py> for OffsetTypeName {
    type Error = PyErr;

    fn extract(name: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        let string = name.cast::<PyString>().ok();
        let offset_type = match string.as_ref().map(|name| name.to_str()).transpose()? {
            Some("uint8") => OffsetType::Uint8,
            Some("uint16") => OffsetType::Uint16,
            Some("uint32") => OffsetType::Uint32,
            _ => {
                return Err(PyValueError::new_err(format!(
                    "{} isn't an offset type",
                    name.repr()?
                )));
            }
        };
        Ok(Self(offset_type))
    }
}

#[pymethods]
impl TableLayout {
    #[new]
    #[pyo3(signature = (*, sized = None, versioned = None, offset_type = OffsetTypeName(OffsetType::Uint32)))]
    fn new(
        sized: Option<&Bound<'_, PyAny>>,
        versioned: Option<&Bound<'_, PyAny>>,
        offset_type: OffsetTypeName,
    ) -> PyResult<Self> {
        // Truthy, as the pure-Python class's flags are, which it keeps as
        // they're given
        let truthy =
            |flag: Option<&Bound<'_, PyAny>>| flag.map_or(Ok(false), PyAnyMethods::is_truthy);

        Ok(Self {
            shape: Shape {
                sized: truthy(sized)?,
                versioned: truthy(versioned)?,
                offset_type: offset_type.0,
            },
        })
    }

    /// No header slots, and offsets and lengths of the offset type ``uint32``
    #[classattr]
    #[allow(non_snake_case)]
    const fn DEFAULT() -> Self {
        Self {
            shape: Shape::DEFAULT,
        }
    }

    /// Returns the class subscripted with its type arguments, as the
    /// pure-Python class, which type checkers know as generic, returns it
    #[classmethod]
    fn __class_getitem__<'py>(
        cls: &Bound<'py, PyType>,
        item: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = cls.py();
        py.import("types")?
            .getattr("GenericAlias")?
            .call1(PyTuple::new(py, [cls.as_any(), item])?)
    }

    fn __repr__(&self) -> String {
        let python_bool = |value: bool| if value { "True" } else { "False" };
        format!(
            "TableLayout(sized={}, versioned={}, offset_type='{}')",
            python_bool(self.shape.sized),
            python_bool(self.shape.versioned),
            self.shape.offset_type.name()
        )
    }

    const fn sized(&self) -> bool {
        self.shape.sized
    }

    const fn versioned(&self) -> bool {
        self.shape.versioned
    }

    const fn offset_type(&self) -> &'static str {
        self.shape.offset_type.name()
    }

    /// The offset size, the width in bytes of the offset type
    const fn offset_size(&self) -> usize {
        self.shape.size()
    }

    /// The byte length of the header slots before the field offsets
    const fn header_size(&self) -> usize {
        self.shape.header_size()
    }
}
