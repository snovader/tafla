use std::{borrow::Cow, fmt, iter::FusedIterator, marker::PhantomData, slice};

use crate::{Element, element::elements_in_place};

/// An array read from a table: its elements' little-endian bytes, which it
/// reads in place at any alignment and in any host byte order. Reading it
/// never allocates, except for [`ArrayView::to_vec()`].
///
/// [`ArrayView::as_slice()`] returns the elements as a slice when the host is
/// little-endian and the bytes are aligned to the element type, as
/// `TableBuilder` aligns them within an aligned table.
#[derive(Clone, Copy)]
pub struct ArrayView<'a, T: Element> {
    /// A whole number of elements
    bytes: &'a [u8],
    element: PhantomData<T>,
}

impl<'a, T: Element> ArrayView<'a, T> {
    /// Views `bytes`, which must hold a whole number of elements
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        debug_assert!(bytes.len().is_multiple_of(size_of::<T>()));

        Self {
            bytes,
            element: PhantomData,
        }
    }

    pub const fn len(&self) -> usize {
        self.bytes.len() / size_of::<T>()
    }

    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Returns the element at `index`, or `None` past the end
    pub fn get(&self, index: usize) -> Option<T> {
        T::chunks(self.bytes)
            .get(index)
            .map(|&bytes| T::from_le(bytes))
    }

    pub fn iter(&self) -> Iter<'a, T> {
        Iter {
            chunks: T::chunks(self.bytes).iter(),
        }
    }

    /// Returns the elements in place, or `None` when they aren't aligned to
    /// their type in memory or the host is big-endian. Single bytes are
    /// always in place.
    pub fn as_slice(&self) -> Option<&'a [T]> {
        elements_in_place(self.bytes)
    }

    /// Copies the elements, in the host's byte order
    pub fn to_vec(&self) -> Vec<T> {
        // Copying the slice is a memcpy, which took 1.9 µs for 27,028 floats
        // against 8.3 µs to collect them
        self.as_slice()
            .map_or_else(|| self.iter().collect(), <[T]>::to_vec)
    }

    /// Returns the elements in place when [`ArrayView::as_slice()`] can, and
    /// otherwise a copy
    // The copy is a cold function of its own, so that LLVM inlines the
    // rest. With the copy in it, LLVM called it out of line, and tafla-gen
    // read a document of 100 rows in 15.3 ns rather than 12.5 ns. It's a
    // match because `map_or_else()` put the calls of the copy among the
    // reads, and the document took 12.9 ns rather than 12.3 ns.
    #[allow(clippy::option_if_let_else)]
    pub fn to_cow(&self) -> Cow<'a, [T]> {
        match self.as_slice() {
            Some(elements) => Cow::Borrowed(elements),
            None => self.copy(),
        }
    }

    /// Copies elements that aren't in place, as [`ArrayView::to_vec()`] does
    #[cold]
    fn copy(&self) -> Cow<'a, [T]> {
        Cow::Owned(self.iter().collect())
    }

    /// Returns the elements' little-endian bytes, without the length prefix
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

impl<T: Element + fmt::Debug> fmt::Debug for ArrayView<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<'a, T: Element> IntoIterator for ArrayView<'a, T> {
    type Item = T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: Element> IntoIterator for &ArrayView<'a, T> {
    type Item = T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An iterator over the elements of an [`ArrayView`]
#[derive(Clone)]
pub struct Iter<'a, T: Element> {
    chunks: slice::Iter<'a, T::Bytes>,
}

impl<T: Element> Iterator for Iter<'_, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<T> {
        self.chunks.next().map(|&bytes| T::from_le(bytes))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chunks.size_hint()
    }
}

impl<T: Element> DoubleEndedIterator for Iter<'_, T> {
    #[inline]
    fn next_back(&mut self) -> Option<T> {
        self.chunks.next_back().map(|&bytes| T::from_le(bytes))
    }
}

impl<T: Element> ExactSizeIterator for Iter<'_, T> {}

impl<T: Element> FusedIterator for Iter<'_, T> {}
