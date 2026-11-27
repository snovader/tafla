use std::{iter::FusedIterator, str};

use crate::{
    DecodeError, OffsetType,
    table_reader::decode_str,
    vector::{Ends, Vector},
};

/// A vector of strings read from a table, which reads each string when it's
/// asked for, borrowing it from the bytes.
///
/// It's laid out as a [`Vector`] of tables is: its count, the end of each
/// string, counted from the vector's position, and then the strings' UTF-8,
/// one after another, without counts of their own.
/// `TableReader::get_str_vector()` checks that the count and the ends lie
/// within the bytes, and each string is checked when it's read, so reading
/// one can fail. [`StringVector::get()`] finds any string from its end and
/// the end of the string before it, without reading the others.
///
/// Each string must be valid UTF-8 on its own, as a string field must be: an
/// end that splits a character fails the strings on both sides of it, even
/// though their bytes together are valid UTF-8. Reading every string, with
/// [`StringVector::iter()`] or [`StringVector::to_vec()`], reads what
/// reading each string reads.
///
/// ```
/// use tafla::{TableBuilder, TableLayout, TableReader};
///
/// let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
/// builder.set_str_vector(["Oslo", "Zürich", "東京"].as_slice())?;
/// let bytes = builder.to_bytes()?;
///
/// let reader = TableReader::new(&bytes, TableLayout::DEFAULT);
/// let cities = reader.get_str_vector(reader.root_offset(0)?)?;
/// assert_eq!(cities.len(), 3);
/// assert_eq!(cities.get(2), Some(Ok("東京")));
/// assert_eq!(cities.to_vec()?, ["Oslo", "Zürich", "東京"]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug)]
pub struct StringVector<'a> {
    vector: Vector<'a>,
}

impl<'a> StringVector<'a> {
    /// Views the strings of `vector`, whose count and ends were checked to
    /// lie within the bytes
    pub(crate) const fn new(vector: Vector<'a>) -> Self {
        Self { vector }
    }

    /// The number of strings
    pub const fn len(&self) -> usize {
        self.vector.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.vector.is_empty()
    }

    /// Reads string `index`, or returns `None` past the end. Only the ends
    /// of the string and the one before it are read, and only its own bytes
    /// are checked as UTF-8.
    ///
    /// # Errors
    ///
    /// `Some(Err(_))` with [`DecodeError::ItemEndsBeforeStart`] if the
    /// string ends before the string before it, [`DecodeError::OutOfRange`]
    /// if it ends past the bytes, or [`DecodeError::InvalidUtf8`], at the
    /// position of its first byte, if it isn't valid UTF-8 on its own.
    #[inline]
    pub fn get(&self, index: usize) -> Option<Result<&'a str, DecodeError>> {
        let bounds = self.vector.bounds(index)?;

        Some(bounds.and_then(|(start, end)| string(self.vector, start, end)))
    }

    /// Iterates over the strings, as [`StringVector::get()`] returns them,
    /// reading each string's end once: a string that fails doesn't keep the
    /// others from being read
    pub const fn iter(&self) -> Strings<'a> {
        Strings {
            ends: self.vector.ends(),
        }
    }

    /// Reads every string, in order, or returns the error of the first that
    /// fails, as [`StringVector::get()`] returns it.
    ///
    /// The strings lie one after another, so their run of bytes is checked
    /// as UTF-8 at once, and then each end is checked to fall on a
    /// character's boundary within it, which finds the strings that checking
    /// each would. When a check fails, each string is read on its own, to
    /// find the first that fails.
    ///
    /// Checking the run once makes one call of `is_ascii()` or
    /// `str::from_utf8()` rather than one for each string, checks ASCII a
    /// word at a time across the strings' ends, and leaves no end of a run of
    /// ASCII to check: lists of 10 to 10,000 strings of 16 and 256 characters
    /// of ASCII were read in 0.34–0.92 times the time that reading each
    /// string with [`StringVector::iter()`] took, and of 8 characters of 3
    /// bytes in 0.86–0.90 times, in the lists suite of
    /// `benchmarks/README.md`.
    ///
    /// # Errors
    ///
    /// The error of the first string that fails, as [`StringVector::get()`]
    /// returns it.
    pub fn to_vec(&self) -> Result<Vec<&'a str>, DecodeError> {
        if let Some(strings) = self.decode_run() {
            return Ok(strings);
        }

        let mut strings = Vec::with_capacity(self.len());
        for string in self {
            strings.push(string?);
        }

        Ok(strings)
    }

    /// Every string, sliced from their run of bytes, which is decoded as
    /// UTF-8 at once, or `None` if an end lies before the end before it or
    /// past the bytes, or a string isn't valid UTF-8 on its own
    fn decode_run(&self) -> Option<Vec<&'a str>> {
        let bytes = self.vector.bytes();
        let start = self.vector.items_start();
        // The count and the ends lie within the bytes, as the reader checked
        let ends = &bytes[self.vector.offset_type().size()..start];

        match self.vector.offset_type() {
            OffsetType::Uint8 => slice_run(bytes, start, ends.iter().map(|&end| end.into())),
            OffsetType::Uint16 => {
                let (ends, _) = ends.as_chunks();
                let end = |&end: &[u8; 2]| u16::from_le_bytes(end).into();
                slice_run(bytes, start, ends.iter().map(end))
            }
            OffsetType::Uint32 => {
                let (ends, _) = ends.as_chunks();
                let end = |&end: &[u8; 4]| u32::from_le_bytes(end) as usize;
                slice_run(bytes, start, ends.iter().map(end))
            }
        }
    }
}

/// Slices the strings that end at `ends`, from `bytes`, which start at the
/// vector's position, from `start`, where the first starts, to the last
/// end, or returns `None` if they don't read as each string on its own would
fn slice_run<I>(bytes: &[u8], start: usize, ends: I) -> Option<Vec<&str>>
where
    I: DoubleEndedIterator<Item = usize> + ExactSizeIterator + Clone,
{
    let end = ends.clone().next_back().unwrap_or(start);
    let run = bytes.get(start..end)?;

    // ASCII first, which `is_ascii()` checks a word at a time, as
    // `decode_str()` checks a string, and whose every byte is a character's
    // boundary, so that no end needs checking: lists of 16 and 256
    // characters of ASCII were read in 0.64–0.97 times the time that slicing
    // the run as a `str` took
    if run.is_ascii() {
        return slice_strings(start, ends, |from, to| {
            // SAFETY: ASCII is valid UTF-8, from and to any of its bytes
            let string = run.get(from..to)?;
            Some(unsafe { str::from_utf8_unchecked(string) })
        });
    }

    // `get()` checks that each string falls on characters' boundaries, so
    // that it's valid UTF-8 on its own
    let run = str::from_utf8(run).ok()?;
    slice_strings(start, ends, |from, to| run.get(from..to))
}

/// Slices each string from the end of the string before it to its end, of
/// `ends`, which count from `start` in the run of strings, with `slice()`,
/// which returns `None` for a string that starts after it ends or past the
/// run, or isn't valid UTF-8
#[inline]
fn slice_strings<'a>(
    start: usize,
    ends: impl ExactSizeIterator<Item = usize>,
    slice: impl Fn(usize, usize) -> Option<&'a str>,
) -> Option<Vec<&'a str>> {
    let mut strings = Vec::with_capacity(ends.len());
    let mut from = 0;
    for end in ends {
        // An end before the run's start wraps past its end
        let to = end.wrapping_sub(start);
        strings.push(slice(from, to)?);
        from = to;
    }

    Some(strings)
}

/// Reads the string from `start` to `end`, from the vector's position
#[inline]
fn string(vector: Vector<'_>, start: usize, end: usize) -> Result<&str, DecodeError> {
    let bytes = vector.item(start, end)?;

    decode_str(bytes, vector.position_of(start))
}

impl<'a> IntoIterator for StringVector<'a> {
    type Item = Result<&'a str, DecodeError>;
    type IntoIter = Strings<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &StringVector<'a> {
    type Item = Result<&'a str, DecodeError>;
    type IntoIter = Strings<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An iterator over the strings of a [`StringVector`], which returns each
/// string as [`StringVector::get()`] does: a string that fails doesn't keep
/// the others from being read
#[derive(Clone, Debug)]
pub struct Strings<'a> {
    ends: Ends<'a>,
}

impl<'a> Iterator for Strings<'a> {
    type Item = Result<&'a str, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        let vector = self.ends.vector();
        let bounds = self.ends.next()?;

        Some(bounds.and_then(|(start, end)| string(vector, start, end)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ends.size_hint()
    }
}

impl ExactSizeIterator for Strings<'_> {}

impl FusedIterator for Strings<'_> {}
