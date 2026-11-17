//! The runs of a table that a `TableBuilder` keeps as segments, borrowed or
//! owned, rather than copying them into its bytes, and which `to_bytes()`
//! copies between its bytes

use std::borrow::Cow;

use crate::element::extend_elements;

/// The segments that a builder keeps within itself, before it needs a `Vec`
/// for more
const FIRST_SEGMENTS: usize = 4;

/// A run of a table that a builder keeps rather than copies: an array's
/// elements, which are written little-endian when they're copied, or bytes,
/// of a string, a struct, an array of structs or a nested table
#[derive(Debug)]
pub enum Run<'a> {
    I8(Cow<'a, [i8]>),
    U8(Cow<'a, [u8]>),
    I16(Cow<'a, [i16]>),
    U16(Cow<'a, [u16]>),
    I32(Cow<'a, [i32]>),
    U32(Cow<'a, [u32]>),
    I64(Cow<'a, [i64]>),
    U64(Cow<'a, [u64]>),
    F32(Cow<'a, [f32]>),
    F64(Cow<'a, [f64]>),
}

/// Evaluates `$expression` with `$values` bound to the elements of `$run`,
/// whatever their type, and `$variant` to the run's constructor
macro_rules! with_run {
    ($run:expr, $variant:ident, $values:ident => $expression:expr) => {
        match $run {
            Run::I8($values) => {
                let $variant = Run::I8;
                $expression
            }
            Run::U8($values) => {
                let $variant = Run::U8;
                $expression
            }
            Run::I16($values) => {
                let $variant = Run::I16;
                $expression
            }
            Run::U16($values) => {
                let $variant = Run::U16;
                $expression
            }
            Run::I32($values) => {
                let $variant = Run::I32;
                $expression
            }
            Run::U32($values) => {
                let $variant = Run::U32;
                $expression
            }
            Run::I64($values) => {
                let $variant = Run::I64;
                $expression
            }
            Run::U64($values) => {
                let $variant = Run::U64;
                $expression
            }
            Run::F32($values) => {
                let $variant = Run::F32;
                $expression
            }
            Run::F64($values) => {
                let $variant = Run::F64;
                $expression
            }
        }
    };
}

impl Run<'_> {
    /// Appends the run's bytes, little-endian, to `to`
    pub fn copy_into(&self, to: &mut Vec<u8>) {
        match self {
            Self::U8(bytes) => to.extend_from_slice(bytes),
            run => with_run!(run, _variant, values => extend_elements(values, to)),
        }
    }

    /// The run, borrowing what it owns
    pub fn reborrow(&self) -> Run<'_> {
        with_run!(self, variant, values => variant(Cow::Borrowed(values.as_ref())))
    }
}

/// A run that goes in the table before the builder's own byte at `at`
#[derive(Debug)]
pub struct Segment<'a> {
    pub at: usize,
    pub run: Run<'a>,
}

/// A builder's segments, in the order of the table: the first few within the
/// builder, and the others in a `Vec`
#[derive(Debug)]
pub struct Segments<'a> {
    first: [Option<Segment<'a>>; FIRST_SEGMENTS],
    others: Vec<Segment<'a>>,
    len: usize,
}

impl<'a> Segments<'a> {
    pub const fn new() -> Self {
        Self {
            first: [const { None }; FIRST_SEGMENTS],
            others: Vec::new(),
            len: 0,
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn push(&mut self, segment: Segment<'a>) {
        if let Some(slot) = self.first.get_mut(self.len) {
            *slot = Some(segment);
        } else {
            self.others.push(segment);
        }
        self.len += 1;
    }

    /// Removes the segments past the first `len`
    pub fn truncate(&mut self, len: usize) {
        if len >= self.len {
            return;
        }

        for slot in self.first.iter_mut().take(self.len).skip(len) {
            *slot = None;
        }
        self.others.truncate(len.saturating_sub(FIRST_SEGMENTS));
        self.len = len;
    }

    pub fn iter(&self) -> impl Iterator<Item = &Segment<'a>> {
        self.first.iter().flatten().chain(&self.others)
    }

    /// Moves the segments from the `first`th on by `by` bytes, with the
    /// builder's bytes that they go before
    pub fn shift_from(&mut self, first: usize, by: usize) {
        let segments = self.first.iter_mut().flatten().chain(&mut self.others);
        for segment in segments.skip(first) {
            segment.at += by;
        }
    }
}

impl<'a> IntoIterator for Segments<'a> {
    type Item = Segment<'a>;
    type IntoIter = std::iter::Chain<
        std::iter::Flatten<std::array::IntoIter<Option<Segment<'a>>, FIRST_SEGMENTS>>,
        std::vec::IntoIter<Segment<'a>>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.first.into_iter().flatten().chain(self.others)
    }
}
