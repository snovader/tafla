//! The native implementation of the classes of tafla's Python runtime that
//! the code that tafla-gen generates uses: `TableLayout`, `FixedFields`,
//! `TableWriter` and `ByteBuffer`, with the pure-Python classes' API,
//! conversions and errors. `tafla` imports them from this module, `_native`,
//! unless it's missing or `TAFLA_PUREPYTHON` is set, and the pure-Python
//! classes otherwise.
//!
//! Reads go through the Rust runtime's `ByteBuffer`, for its bounds checks.
//! Writes append to bytes that grow, as the pure-Python `TableWriter`'s
//! `bytearray` does, which the Rust runtime's writer doesn't, since it writes
//! into the length that a sizer counted.

mod elements;
mod fixed_fields;
mod layout;
mod python;
mod reader;
mod values;
mod writer;

use std::sync::atomic::{AtomicBool, Ordering};

use pyo3::prelude::*;

/// Whether the host's byte order is little-endian, which arrays of its
/// elements are in, and which tables are in. The tests set it to take the
/// big-endian branches on any host.
static LITTLE_ENDIAN_HOST: AtomicBool = AtomicBool::new(cfg!(target_endian = "little"));

fn little_endian_host() -> bool {
    LITTLE_ENDIAN_HOST.load(Ordering::Relaxed)
}

/// Treats the host as little-endian or not, for the tests, as patching
/// ``tafla.elements.IS_LITTLE_ENDIAN`` does for the pure-Python runtime, so
/// that arrays are byte-swapped as on a big-endian host. Returns whether it
/// was treated as little-endian.
#[pyfunction]
fn _set_little_endian_host(little_endian: bool) -> bool {
    LITTLE_ENDIAN_HOST.swap(little_endian, Ordering::Relaxed)
}

/// The native implementation of the classes of tafla's runtime that the code
/// that tafla-gen generates uses, which ``tafla`` imports from here
#[pymodule(gil_used = false)]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<layout::TableLayout>()?;
    module.add_class::<fixed_fields::FixedFields>()?;
    module.add_class::<writer::TableWriter>()?;
    module.add_class::<reader::ByteBuffer>()?;
    module.add_function(wrap_pyfunction!(_set_little_endian_host, module)?)?;
    Ok(())
}
