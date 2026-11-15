//! Compares tafla's runtime with other Rust crates that encode small tables
//! and numeric columns, on the workloads of the TypeScript comparison, as
//! README.md describes. `benches/comparison.rs` times them, and the `report`
//! and `encoding` binaries print the results and the encoded sizes.

pub mod check;
pub mod codec;
pub mod codecs;
pub mod item_access;
pub mod workloads;
