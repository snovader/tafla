use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use tafla::OffsetType;

use crate::python::Int;

/// What the classes use of a layout, copied from it
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub sized: bool,
    /// The version that writers write to the header, when it has a slot
    pub version: Option<u32>,
    pub offset_type: OffsetType,
}

impl Shape {
    pub const DEFAULT: Self = Self {
        sized: false,
        version: None,
        offset_type: OffsetType::Uint32,
    };

    /// Whether the header has a slot for a version
    pub const fn versioned(self) -> bool {
        self.version.is_some()
    }

    /// The offset size, the width of offsets, lengths and header slots
    pub const fn size(self) -> usize {
        self.offset_type.size()
    }

    /// The byte length of the header slots before the field offsets
    pub const fn header_size(self) -> usize {
        (self.sized as usize + self.versioned() as usize) * self.size()
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
/// ``TableReader`` that reads it, so that the two can't disagree. Nested tables
/// use ``TableLayout.DEFAULT``.
///
/// - ``sized``: whether the header starts with the table's total byte length.
/// - ``version``: the version that the header holds, after the length when
///   ``sized``, which gives it a slot for one: an integer that fits in the
///   offset type, or ``None``, the default, for a header without one. Writers
///   write it, and readers can check the one that a table holds, with
///   ``TableReader.get_version()``, before reading any field.
/// - ``offset_type``: the offset type, the unsigned integer type of the header
///   slots, field offsets, counts, lengths and item ends, which limits the
///   table's length: ``"uint8"``, of offset size 1, for up to 255 bytes,
///   ``"uint16"``, of 2, for 65,535 and ``"uint32"``, of 4, the default, for
///   4 GiB.
///
/// A layout with a version is the layout of that version of the format, and
/// its readers read tables of any version, so that an application can read a
/// table's version before it chooses how to read the fields. The layouts of a
/// format's versions should differ in the version only, as ``with_version()``
/// makes them.
///
/// Raises ``ValueError`` if ``offset_type`` isn't ``"uint8"``, ``"uint16"`` or
/// ``"uint32"``, or ``version`` doesn't fit in it.
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
            version: layout.call_method0("version")?.extract()?,
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

/// A layout's version, as the pure-Python class checks it: an int that fits
/// in a header slot of `offset_type`, or `ValueError`
fn checked_version(version: &Bound<'_, PyAny>, offset_type: OffsetType) -> PyResult<u32> {
    let version = Int::new(version)?;
    let fits = version
        .index()
        .and_then(|version| u32::try_from(version).ok())
        .filter(|&version| version <= offset_type.max_value());

    match fits {
        Some(version) => Ok(version),
        // display() raises as the pure-Python class's message does, for an
        // int too long to format
        None => Err(PyValueError::new_err(format!(
            "Version {} doesn't fit in a {} slot",
            version.display()?,
            offset_type.name()
        ))),
    }
}

#[pymethods]
impl TableLayout {
    #[new]
    #[pyo3(signature = (*, sized = None, version = None, offset_type = OffsetTypeName(OffsetType::Uint32)))]
    fn new(
        sized: Option<&Bound<'_, PyAny>>,
        version: Option<&Bound<'_, PyAny>>,
        offset_type: OffsetTypeName,
    ) -> PyResult<Self> {
        // Truthy, as the pure-Python class's flag is, which it keeps as
        // it's given
        let sized = sized.map_or(Ok(false), PyAnyMethods::is_truthy)?;
        let version = match version {
            Some(version) if !version.is_none() => Some(checked_version(version, offset_type.0)?),
            _ => None,
        };

        Ok(Self {
            shape: Shape {
                sized,
                version,
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

    /// Pickles and copies the layout as a call of its class with its
    /// arguments
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<(Bound<'py, PyAny>, ())> {
        let py = slf.py();
        let shape = slf.get().shape;
        let arguments = PyDict::new(py);
        arguments.set_item("sized", shape.sized)?;
        arguments.set_item("version", shape.version)?;
        arguments.set_item("offset_type", shape.offset_type.name())?;

        let partial = py.import("functools")?.getattr("partial")?;
        Ok((partial.call((slf.get_type(),), Some(&arguments))?, ()))
    }

    fn __repr__(&self) -> String {
        let python_bool = |value: bool| if value { "True" } else { "False" };
        let version = self
            .shape
            .version
            .map_or_else(|| "None".to_owned(), |version| version.to_string());
        format!(
            "TableLayout(sized={}, version={version}, offset_type='{}')",
            python_bool(self.shape.sized),
            self.shape.offset_type.name()
        )
    }

    const fn sized(&self) -> bool {
        self.shape.sized
    }

    /// Whether the header has a slot for a version
    const fn versioned(&self) -> bool {
        self.shape.versioned()
    }

    /// The version that writers write to the header, when it has a slot
    const fn version(&self) -> Option<u32> {
        self.shape.version
    }

    /// Returns the layout with ``version`` in place of its own, for another
    /// version of the same format.
    ///
    /// Raises ``ValueError`` if ``version`` doesn't fit in the offset type.
    fn with_version(&self, version: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            shape: Shape {
                version: Some(checked_version(version, self.shape.offset_type)?),
                ..self.shape
            },
        })
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
