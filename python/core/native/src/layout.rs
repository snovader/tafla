use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyTuple, PyType};
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

/// A layout argument: a native `TableLayout`, or a pure-Python one, whose
/// methods give its shape, since a format may create its layout from
/// `tafla.layout`, whose class the pure-Python classes take as theirs.
/// Raises `TypeError` for anything else.
pub struct Layout(Shape);

impl<'a, 'py> FromPyObject<'a, 'py> for Layout {
    type Error = PyErr;

    #[inline]
    fn extract(layout: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        if let Ok(native) = layout.cast::<TableLayout>() {
            return Ok(Self(native.get().shape));
        }
        Self::from_pure(&layout)
    }
}

impl Layout {
    #[cold]
    fn from_pure(layout: &Bound<'_, PyAny>) -> PyResult<Self> {
        let py = layout.py();
        let pure = py.import("tafla.layout")?.getattr("TableLayout")?;
        if !layout.is_instance(&pure)? {
            return Err(PyTypeError::new_err(format!(
                "A layout is a TableLayout, not {}",
                layout.get_type().name()?
            )));
        }

        let flag = |name: &str| layout.call_method0(name)?.is_truthy();
        let offset_type: OffsetTypeName = layout.call_method0("offset_type")?.extract()?;
        Ok(Self(Shape {
            sized: flag("sized")?,
            versioned: flag("versioned")?,
            offset_type: offset_type.0,
        }))
    }

    pub const fn shape(&self) -> Shape {
        self.0
    }
}

/// The shape of a layout argument, or of `TableLayout.DEFAULT` without one
pub fn shape_of(layout: Option<&Layout>) -> Shape {
    layout.map_or(Shape::DEFAULT, Layout::shape)
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

    /// Pickles and copies the layout as a call of its class with its
    /// arguments
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<(Bound<'py, PyAny>, ())> {
        let py = slf.py();
        let shape = slf.get().shape;
        let arguments = PyDict::new(py);
        arguments.set_item("sized", shape.sized)?;
        arguments.set_item("versioned", shape.versioned)?;
        arguments.set_item("offset_type", shape.offset_type.name())?;

        let partial = py.import("functools")?.getattr("partial")?;
        Ok((partial.call((slf.get_type(),), Some(&arguments))?, ()))
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
