//! Compares tafla's runtime with other Rust crates that encode small tables
//! and numeric columns, on the workloads of the TypeScript comparison, as
//! README.md describes. `benches/comparison.rs` times them,
//! `benches/strings.rs` times how the crates write and read strings, and the
//! `report` and `encoding` binaries print the results and the encoded sizes.
//! `benches/lists.rs` times how tafla-gen's code writes and reads lists of
//! strings, which `lists` holds.

pub mod check;
pub mod codec;
pub mod codecs;
pub mod item_access;
pub mod lists;
pub mod sized_items;
pub mod string_workloads;
pub mod workloads;
