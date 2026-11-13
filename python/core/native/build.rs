// Sets the cfgs of the Python that the module is built for, such as
// Py_LIMITED_API for a build against the stable ABI
fn main() {
    pyo3_build_config::use_pyo3_cfgs();
}
