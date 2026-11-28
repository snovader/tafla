//! Lists of strings in tafla's tables, which the lists suite times.
//!
//! It times a vector of strings, `[string]`, and a vector of tables of one
//! string each, `[Name]`, which is how a schema held a list of strings
//! before it had vectors of strings. tafla-gen's code, generated from
//! `lists.tafla`, writes and reads both, `benches/lists.rs` times it, and
//! README.md describes the suite.

// The codecs use a part of the generated code. tafla-gen's tests check its
// code with clippy's pedantic lints, and this crate's nursery lints suggest
// otherwise, such as `map_or()` for a `match` of an `Option`.
#[allow(dead_code, clippy::nursery)]
#[path = "lists/generated/lists.rs"]
mod generated;
mod shapes;

pub use self::shapes::{COUNTS, ListShape, list_shapes};

use self::generated::{NameFields, NameList, NameListFields, StringList, StringListFields};
use crate::codec::Error;

/// A way to write and read a list of strings
pub trait ListCodec: Copy {
    /// The way's name, which names its benchmarks
    const NAME: &'static str;

    /// Writes the strings, in order
    ///
    /// # Errors
    ///
    /// tafla's error, if it can't write them.
    fn serialize(self, strings: &[&str]) -> Result<Vec<u8>, Error>;

    /// Reads every string, in order, borrowed from the bytes
    ///
    /// # Errors
    ///
    /// A decode error, if the bytes aren't a list of strings.
    fn deserialize(self, bytes: &[u8]) -> Result<Vec<&str>, Error>;

    /// Reads the last string, without reading the others
    ///
    /// # Errors
    ///
    /// A decode error, or an error if the list is empty.
    fn read_last(self, bytes: &[u8]) -> Result<&str, Error>;
}

/// The vector of strings, `StringList`, in tafla-gen's code.
///
/// The generated code writes it with a `TableSizer` and a `TableWriter`. It
/// reads every string with its getter's `StringVector::to_vec()`, which
/// checks the strings' run of bytes as UTF-8 at once, and the last from its
/// ends with `StringVector::get()`.
#[derive(Clone, Copy, Debug)]
pub struct TaflaGenStrings;

impl ListCodec for TaflaGenStrings {
    const NAME: &'static str = "tafla-gen [string]";

    fn serialize(self, strings: &[&str]) -> Result<Vec<u8>, Error> {
        Ok(StringListFields { items: strings }.serialize()?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Vec<&str>, Error> {
        Ok(StringList::new(bytes).items()?.to_vec()?)
    }

    fn read_last(self, bytes: &[u8]) -> Result<&str, Error> {
        let items = StringList::new(bytes).items()?;
        let last = items.len().checked_sub(1).ok_or_else(no_strings)?;

        Ok(items.get(last).ok_or_else(no_strings)??)
    }
}

/// The vector of tables of one string each, `NameList`, in tafla-gen's code.
///
/// The generated code converts each string into the fields of its table as
/// it writes it, and reads each table with the generated getters.
#[derive(Clone, Copy, Debug)]
pub struct TaflaGenNames;

/// A string, as the fields of the table that holds it, which the generated
/// code converts each string of a `NameList` into, as an application's own
/// type
impl<'a> From<&'a &str> for NameFields<'a> {
    fn from(value: &'a &str) -> Self {
        Self { value }
    }
}

impl ListCodec for TaflaGenNames {
    const NAME: &'static str = "tafla-gen [Name]";

    fn serialize(self, strings: &[&str]) -> Result<Vec<u8>, Error> {
        Ok(NameListFields { items: strings }.serialize()?)
    }

    fn deserialize(self, bytes: &[u8]) -> Result<Vec<&str>, Error> {
        let items = NameList::new(bytes).items()?;

        let mut strings = Vec::with_capacity(items.len());
        for item in items.iter() {
            strings.push(item?.value()?);
        }

        Ok(strings)
    }

    fn read_last(self, bytes: &[u8]) -> Result<&str, Error> {
        let items = NameList::new(bytes).items()?;
        let last = items.len().checked_sub(1).ok_or_else(no_strings)?;

        Ok(items.get(last).ok_or_else(no_strings)??.value()?)
    }
}

/// The error of reading the last string of an empty list
fn no_strings() -> Error {
    "The list has no strings".into()
}

/// Checks that `codec` reads back every string of the shape, and its last,
/// from the bytes it wrote
///
/// # Errors
///
/// The codec's error, or an error that names the codec and the shape when
/// it reads back other strings.
pub fn check<C: ListCodec>(codec: C, shape: &ListShape) -> Result<(), Error> {
    let strings = shape.strs();
    let bytes = codec.serialize(&strings)?;

    let every = codec.deserialize(&bytes)?;
    let last = codec.read_last(&bytes)?;
    if every == strings && strings.last() == Some(&last) {
        Ok(())
    } else {
        Err(format!("{} didn't read back {}", C::NAME, shape.name).into())
    }
}

/// Expands `$body` once for each way of writing and reading the lists.
///
/// The ways come in the order that results list them, with `$name` bound to
/// the name it's timed by: the vector of strings, the vector of tables, and
/// the vector of strings again, an A/A pair that shows the harness's noise.
#[macro_export]
macro_rules! for_each_list_codec {
    (|$codec:ident, $name:ident| $body:expr) => {{
        {
            let $codec = $crate::lists::TaflaGenStrings;
            let $name = <$crate::lists::TaflaGenStrings as $crate::lists::ListCodec>::NAME;
            $body;
        }
        {
            let $codec = $crate::lists::TaflaGenNames;
            let $name = <$crate::lists::TaflaGenNames as $crate::lists::ListCodec>::NAME;
            $body;
        }
        {
            let $codec = $crate::lists::TaflaGenStrings;
            let $name = "tafla-gen [string] A/A";
            $body;
        }
    }};
}
