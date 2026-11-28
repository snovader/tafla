use std::{iter::FusedIterator, mem};

use crate::{DecodeError, OffsetType, table_reader::read_slot};

/// A vector read from a table: its items, which are tables.
///
/// The vector holds its count, the end of each item, counted from the
/// vector's position, and then the items. `TableReader::get_vector()` checks
/// that the ends lie within the bytes, and each item is checked when it's
/// read, so reading one can fail. [`Vector::get()`] and
/// [`Vector::get_in_place()`] find any item from its end and the one before
/// it, without reading the others.
///
/// An item that has no header and its parent's offset type is read in
/// place: from the bytes that [`Vector::get_in_place()`] returns, which start
/// where it does and end where its parent's do, with a `TableReader` of its
/// parent's offset type, at position 0. Its reads may reach past its end, as
/// those of any table read in place may. Any other item is read as a table
/// of its own, from the bytes that [`Vector::get()`] returns, with a
/// `TableReader` of its own layout.
#[derive(Clone, Copy, Debug)]
pub struct Vector<'a> {
    /// The bytes from the vector's position, which is its count's, to the
    /// end of the buffer
    bytes: &'a [u8],
    /// The vector's position in the buffer, which errors report positions
    /// from
    position: usize,
    len: usize,
    offset_type: OffsetType,
}

impl<'a> Vector<'a> {
    /// Views `len` items in `bytes`, from the vector's `position` to the end
    /// of the buffer, which must have been checked to hold the count and
    /// the ends
    pub(crate) const fn new(
        bytes: &'a [u8],
        position: usize,
        len: usize,
        offset_type: OffsetType,
    ) -> Self {
        Self {
            bytes,
            position,
            len,
            offset_type,
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The bytes from the vector's position to the end of the buffer
    pub(crate) const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub(crate) const fn offset_type(&self) -> OffsetType {
        self.offset_type
    }

    /// Returns the bytes of item `index`, from its start to its end, to read
    /// it as a table of its own, or `None` past the end. Only the ends of the
    /// item and the one before it are read.
    ///
    /// # Errors
    ///
    /// `Some(Err(_))` with [`DecodeError::ItemEndsBeforeStart`] if the item
    /// ends before the item before it, or [`DecodeError::OutOfRange`] if it
    /// ends past the bytes.
    pub fn get(&self, index: usize) -> Option<Result<&'a [u8], DecodeError>> {
        let bounds = self.bounds(index)?;

        Some(bounds.and_then(|(start, end)| self.item(start, end)))
    }

    /// Returns the bytes of item `index` to read it in place, from its start
    /// to the end of the buffer, or `None` past the end. Only the ends of the
    /// item and the one before it are read.
    ///
    /// # Errors
    ///
    /// `Some(Err(_))` as [`Vector::get()`] returns.
    // Inlined into its callers, as `bounds()` is: with two of them, LLVM
    // called it out of line, which `#[inline]` didn't change, and a
    // generated view read the last item in 6.5 ns rather than 3.8 ns
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub fn get_in_place(&self, index: usize) -> Option<Result<&'a [u8], DecodeError>> {
        let bounds = self.bounds(index)?;

        Some(bounds.and_then(|(start, end)| self.item_in_place(start, end)))
    }

    /// Iterates over the items' bytes, as [`Vector::get()`] returns them,
    /// reading each item's end once
    pub const fn iter(&self) -> Items<'a> {
        Items { ends: self.ends() }
    }

    /// Iterates over the items' bytes, as [`Vector::get_in_place()`] returns
    /// them, reading each item's end once
    pub const fn iter_in_place(&self) -> ItemsInPlace<'a> {
        ItemsInPlace { ends: self.ends() }
    }

    /// Where item `index` starts and ends, from the vector's position, or
    /// `None` past the end
    // Inlined, as are the reads of an item's end and its checks: with two
    // callers rather than one, LLVM called it out of line, and a generated
    // view took half as long again to read the last item
    #[inline]
    pub(crate) fn bounds(&self, index: usize) -> Option<Result<(usize, usize), DecodeError>> {
        if index >= self.len {
            return None;
        }

        let start = match index {
            0 => Ok(self.items_start()),
            _ => self.end(index - 1),
        };

        Some(start.and_then(|start| Ok((start, self.end(index)?))))
    }

    pub(crate) const fn ends(&self) -> Ends<'a> {
        Ends {
            vector: *self,
            index: 0,
            start: self.items_start(),
        }
    }

    /// Where item 0 starts, from the vector's position: after the count and
    /// the ends
    pub(crate) const fn items_start(&self) -> usize {
        // The count and the ends lie within the bytes, so this doesn't
        // overflow
        (self.len + 1) * self.offset_type.size()
    }

    /// Reads the end of item `index`, from the vector's position
    #[inline]
    fn end(&self, index: usize) -> Result<usize, DecodeError> {
        let width = self.offset_type.size();
        let slot = (index + 1) * width;

        self.bytes
            .get(slot..)
            .and_then(|bytes| read_slot(bytes, self.offset_type))
            .map(|end| end as usize)
            .ok_or_else(|| {
                DecodeError::out_of_range(self.position_of(slot), width as u64, self.available())
            })
    }

    /// Returns the bytes of the item from `start` to `end`, from the
    /// vector's position
    #[inline]
    pub(crate) fn item(&self, start: usize, end: usize) -> Result<&'a [u8], DecodeError> {
        if start > end {
            return Err(self.ends_before_start(start, end));
        }

        self.bytes
            .get(start..end)
            .ok_or_else(|| self.outside(start, end))
    }

    /// Returns the bytes from the start of the item from `start` to `end`,
    /// from the vector's position, to the end of the buffer
    #[inline]
    fn item_in_place(&self, start: usize, end: usize) -> Result<&'a [u8], DecodeError> {
        if start > end {
            return Err(self.ends_before_start(start, end));
        }

        if end > self.bytes.len() {
            return Err(self.outside(start, end));
        }

        // It starts no later than it ends, within the bytes
        Ok(&self.bytes[start..])
    }

    const fn ends_before_start(&self, start: usize, end: usize) -> DecodeError {
        DecodeError::ItemEndsBeforeStart {
            start: self.position_of(start),
            end: self.position_of(end),
        }
    }

    /// The error for an item from `start` to `end` that ends past the bytes
    const fn outside(&self, start: usize, end: usize) -> DecodeError {
        let byte_length = (end - start) as u64;
        DecodeError::out_of_range(self.position_of(start), byte_length, self.available())
    }

    /// The position in the buffer of `offset` from the vector's position,
    /// which an error reports. Saturating, since only a 32-bit host can
    /// overflow, at a position past any bytes.
    pub(crate) const fn position_of(&self, offset: usize) -> usize {
        self.position.saturating_add(offset)
    }

    /// The length of the buffer that the vector lies in
    const fn available(&self) -> usize {
        self.position + self.bytes.len()
    }
}

impl<'a> IntoIterator for Vector<'a> {
    type Item = Result<&'a [u8], DecodeError>;
    type IntoIter = Items<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &Vector<'a> {
    type Item = Result<&'a [u8], DecodeError>;
    type IntoIter = Items<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An iterator over the bytes of a [`Vector`]'s items, which returns each
/// item as [`Vector::get()`] does: an item that fails doesn't keep the
/// others from being read
#[derive(Clone, Debug)]
pub struct Items<'a> {
    ends: Ends<'a>,
}

impl<'a> Iterator for Items<'a> {
    type Item = Result<&'a [u8], DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        let vector = self.ends.vector;
        let bounds = self.ends.next()?;

        Some(bounds.and_then(|(start, end)| vector.item(start, end)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ends.size_hint()
    }
}

impl ExactSizeIterator for Items<'_> {}

impl FusedIterator for Items<'_> {}

/// An iterator over the bytes of a [`Vector`]'s items to read them in place,
/// which returns each item as [`Vector::get_in_place()`] does: an item that
/// fails doesn't keep the others from being read
#[derive(Clone, Debug)]
pub struct ItemsInPlace<'a> {
    ends: Ends<'a>,
}

impl<'a> Iterator for ItemsInPlace<'a> {
    type Item = Result<&'a [u8], DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        let vector = self.ends.vector;
        let bounds = self.ends.next()?;

        Some(bounds.and_then(|(start, end)| vector.item_in_place(start, end)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ends.size_hint()
    }
}

impl ExactSizeIterator for ItemsInPlace<'_> {}

impl FusedIterator for ItemsInPlace<'_> {}

/// An iterator over where a [`Vector`]'s items start and end, from the
/// vector's position, which reads each item's end once
#[derive(Clone, Debug)]
pub struct Ends<'a> {
    vector: Vector<'a>,
    index: usize,
    /// Where the next item starts, from the vector's position, which is
    /// where the item before it ends
    start: usize,
}

impl<'a> Ends<'a> {
    /// The vector whose items' bounds it iterates over
    pub(crate) const fn vector(&self) -> Vector<'a> {
        self.vector
    }
}

impl Iterator for Ends<'_> {
    type Item = Result<(usize, usize), DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.vector.len {
            return None;
        }

        let bounds = self
            .vector
            .end(self.index)
            .map(|end| (mem::replace(&mut self.start, end), end));
        self.index += 1;

        Some(bounds)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.vector.len - self.index;
        (remaining, Some(remaining))
    }
}
