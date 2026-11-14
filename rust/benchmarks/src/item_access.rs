//! Ways to read the items of tafla's vector of nested tables and of its array
//! of structs, which tafla and tafla-gen write alike.
//!
//! Each reads the last item alone, which a reader can find without reading
//! the others, and every item. It sums the three float32s of the items it
//! reads as `f64`s, as the TypeScript comparison adds them, so that it
//! returns a number rather than collecting items of its own.

use crate::codec::{Error, Workload};
use crate::codecs::Tafla;
use crate::workloads::{Item, List, ListShape, StructsShape};

/// A way to read tafla's items
pub trait ItemAccess: Copy {
    /// The way's name, which names its benchmarks
    const NAME: &'static str;

    /// Sums the last item of the vector of nested tables
    ///
    /// # Errors
    ///
    /// A decode error, or an error if the vector is empty.
    fn last_table(self, bytes: &[u8]) -> Result<f64, Error>;

    /// Sums every item of the vector of nested tables, in order
    ///
    /// # Errors
    ///
    /// A decode error.
    fn every_table(self, bytes: &[u8]) -> Result<f64, Error>;

    /// Sums the last struct of the array
    ///
    /// # Errors
    ///
    /// A decode error, or an error if the array is empty.
    fn last_struct(self, bytes: &[u8]) -> Result<f64, Error>;

    /// Sums every struct of the array, in order
    ///
    /// # Errors
    ///
    /// A decode error.
    fn every_struct(self, bytes: &[u8]) -> Result<f64, Error>;
}

/// The list as a vector of nested tables and as an array of structs, in
/// tafla's bytes
pub struct ItemBytes {
    pub tables: Vec<u8>,
    pub structs: Vec<u8>,
}

impl ItemBytes {
    /// Writes the list with tafla's codec, which tafla-gen's code writes alike
    ///
    /// # Errors
    ///
    /// tafla's error, if it can't write the list.
    pub fn write(list: &List) -> Result<Self, Error> {
        Ok(Self {
            tables: <Tafla as Workload<ListShape>>::serialize(Tafla, list)?,
            structs: <Tafla as Workload<StructsShape>>::serialize(Tafla, list)?,
        })
    }
}

/// The sum of an item's three float32s, in the order that every way adds
/// them
pub fn sum_item(value: f32, first: f32, second: f32) -> f64 {
    f64::from(value) + f64::from(first) + f64::from(second)
}

/// The error of an access to the last item of an empty vector or array
pub fn no_items() -> Error {
    "The list has no items".into()
}

/// Checks that `access` sums the last item and every item of the list's
/// bytes to the sums of `list`'s items, with the same bits
///
/// # Errors
///
/// The access's error, or an error that names the access and the sum that
/// differs.
pub fn check<A: ItemAccess>(access: A, list: &List, bytes: &ItemBytes) -> Result<(), Error> {
    let sum_of = |item: &Item| sum_item(item.value, item.first, item.second);
    let last_sum = list.items.last().map(sum_of).ok_or_else(no_items)?;
    let every_sum = list
        .items
        .iter()
        .map(sum_of)
        .fold(0.0, |sum, item| sum + item);

    let sums = [
        (
            "the vector's last item",
            access.last_table(&bytes.tables)?,
            last_sum,
        ),
        (
            "every item of the vector",
            access.every_table(&bytes.tables)?,
            every_sum,
        ),
        (
            "the array's last struct",
            access.last_struct(&bytes.structs)?,
            last_sum,
        ),
        (
            "every struct of the array",
            access.every_struct(&bytes.structs)?,
            every_sum,
        ),
    ];
    for (items, sum, expected) in sums {
        if sum.to_bits() != expected.to_bits() {
            return Err(
                format!("{} summed {items} as {sum} rather than {expected}", A::NAME).into(),
            );
        }
    }

    Ok(())
}

/// Expands `$body` once for each way of reading the items, in the order that
/// results list them, with `$name` bound to the name it's timed by:
/// tafla-gen's twice, an A/A pair that shows the harness's noise
#[macro_export]
macro_rules! for_each_item_access {
    (|$access:ident, $name:ident| $body:expr) => {{
        {
            let $access = $crate::codecs::Tafla;
            let $name = <$crate::codecs::Tafla as $crate::item_access::ItemAccess>::NAME;
            $body;
        }
        {
            let $access = $crate::codecs::TaflaGen;
            let $name = <$crate::codecs::TaflaGen as $crate::item_access::ItemAccess>::NAME;
            $body;
        }
        {
            let $access = $crate::codecs::TaflaGen;
            let $name = "tafla-gen A/A";
            $body;
        }
    }};
}
