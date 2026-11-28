// The setters, and what they call to write a field, are inlined into the
// caller: called out of line, the vector of 100 nested tables took 1.76 µs
// to write rather than 1.33 µs, with the builder that kept long values
#![allow(clippy::inline_always)]

mod bytes;

use std::{borrow::Cow, mem};

use bytes::Bytes;

use crate::{Alignment, BuildError, Element, OffsetType, TableLayout};

/// The most room that a vector written in place makes for its items after
/// its first, each as long as the first
const MOST_ROOM_FOR_ITEMS: usize = 64 * 1024;

/// Builds a table, which a `TableReader` reads back.
///
/// A table is a header with one offset per field, after the table's total
/// length and version when its layout has them, followed by the fields in
/// the order they're set. Arrays, strings and vectors start with their
/// count, and a vector then holds the end of each item. Arrays of scalars
/// and of structs are padded so their elements are aligned from the start of
/// the table. Fields are read back by index with `TableReader::root_offset()`,
/// or `TableReader::field_offset()` for a table nested in another.
///
/// Each field is copied into the builder's buffer when it's set, after the
/// fields set before it, and its offset written with it, so the builder
/// borrows nothing. A vector writes its count and its items' ends, each
/// once its item is written. The buffer then holds the table:
/// [`TableBuilder::finish()`] returns it as a slice that the builder lends,
/// [`TableBuilder::into_bytes()`] as the buffer itself, and
/// [`TableBuilder::to_bytes()`] as a copy of its exact length.
///
/// ```
/// use tafla::{BuildError, TableBuilder, TableLayout};
///
/// let mut builder = TableBuilder::new(2, TableLayout::DEFAULT)?;
/// builder.set_u32(7)?;
/// builder.set_str("seven")?;
/// let table: &[u8] = builder.finish()?;
/// # assert_eq!(table.len(), 8 + 4 + 4 + 5);
/// # Ok::<(), BuildError>(())
/// ```
///
/// The buffer can be the caller's, given to
/// [`TableBuilder::with_buffer()`], and a builder can start the next table
/// in the same buffer with [`TableBuilder::reset()`], so that a loop that
/// writes many tables allocates once.
///
/// A nested table, or each item of a vector of them, can be written in
/// place, among this builder's own fields, with
/// [`TableBuilder::set_table_with()`] and
/// [`TableBuilder::set_vector_with()`], without a builder of its own. A
/// table from another builder, or from bytes, is copied in.
///
/// A setter returns a [`BuildError`], and leaves the builder as it was, when
/// the table already holds all its fields or the field would make it too
/// long for the layout's offset type: longer than 255 bytes, 64 KiB or
/// 4 GiB. Errors are values rather than panics, since a WebAssembly build
/// that aborts on panic couldn't report them. The buffer grows as a `Vec`
/// does, so a table longer than the memory that it can allocate panics or
/// aborts as a `Vec` does, which a 32-bit target's tables of 2 to 4 GiB do.
#[derive(Debug)]
pub struct TableBuilder {
    bytes: Bytes,
    /// The table whose fields are being set: the root, a table nested in
    /// place or a vector's item
    current: Counters,
    /// The tables open around the current one, outermost first
    open: Stack<Counters>,
    /// The paddings and alignments that early moves changed, oldest first,
    /// which a failure restores
    undo: Stack<Undo>,
    sized: bool,
    offset_type: OffsetType,
}

/// A stack whose first few entries are held within it
#[derive(Debug)]
struct Stack<T> {
    first: [T; 4],
    more: Vec<T>,
    len: usize,
}

impl<T: Copy> Stack<T> {
    const fn new(filler: T) -> Self {
        Self {
            first: [filler; 4],
            more: Vec::new(),
            len: 0,
        }
    }

    const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            None
        } else if index < 4 {
            Some(&self.first[index])
        } else {
            self.more.get(index - 4)
        }
    }

    #[inline]
    fn get_mut(&mut self, index: usize) -> &mut T {
        if index < 4 { &mut self.first[index] } else { &mut self.more[index - 4] }
    }

    #[inline]
    fn push(&mut self, value: T) {
        if self.len < 4 {
            self.first[self.len] = value;
        } else {
            self.more.push(value);
        }
        self.len += 1;
    }

    #[inline]
    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        if self.len < 4 { Some(self.first[self.len]) } else { self.more.pop() }
    }

    fn truncate(&mut self, len: usize) {
        if len < self.len {
            self.more.truncate(len.saturating_sub(4));
            self.len = len;
        }
    }

    fn clear(&mut self) {
        self.truncate(0);
    }
}

const NO_COUNTERS: Counters = Counters {
    table: 0,
    slots: 0,
    fields: 0,
    set: 0,
    alignment: Alignment::One,
    padded: false,
    rel: 0,
    padding: 0,
};

const NO_UNDO: Undo = Undo {
    level: 0,
    padding: 0,
    alignment: Alignment::One,
};

/// A level's padding and alignment before an early move changed them
#[derive(Clone, Copy, Debug)]
struct Undo {
    level: usize,
    padding: usize,
    alignment: Alignment,
}

/// What a builder counts of the table whose fields it's setting
#[derive(Clone, Copy, Debug)]
struct Counters {
    /// Where the table starts in the builder's bytes
    table: usize,
    /// Where its first offset lies in the builder's bytes
    slots: usize,
    /// The number of fields that the table holds, which a header of `uint32`
    /// offsets limits to fewer than 2^30
    fields: u32,
    /// The number of fields set so far
    set: u32,
    /// The largest alignment that a field needs from the table's start,
    /// which a parent pads the table to when it nests it, unless the parent
    /// owns it and was given another
    alignment: Alignment,
    /// Whether the table is a field nested in place, which is padded, rather
    /// than the root or a vector's item
    padded: bool,
    /// Where it started in its parent, before its padding
    rel: usize,
    /// The padding before it so far, which its alignment so far needs
    padding: usize,
}

/// Where a builder was before a table or vector nested in place, which it
/// returns to if the table's `fill` fails
#[derive(Clone, Copy, Debug)]
struct Saved {
    counters: Counters,
    bytes: usize,
    open: usize,
    undo: usize,
}

macro_rules! scalar_setters {
    ($($name:ident($type:ty)),*) => {$(
        #[doc = concat!("Sets a `", stringify!($type), "` field")]
        ///
        /// # Errors
        ///
        /// [`BuildError::TooManyFields`] if the table holds all its fields
        /// already, or [`BuildError::TooLong`] if the field would make it
        /// too long for its offset type.
        #[inline(always)]
        pub fn $name(&mut self, value: $type) -> Result<(), BuildError> {
            self.set_scalar(value.to_le_bytes())
        }
    )*};
}

macro_rules! array_setters {
    ($($name:ident($type:ty)),*) => {$(
        #[doc = concat!(
            "Sets an array field of `", stringify!($type), "`, copied from a ",
            "slice or a `Vec`, borrowed or owned"
        )]
        ///
        /// # Errors
        ///
        /// [`BuildError::TooManyFields`] if the table holds all its fields
        /// already, or [`BuildError::TooLong`] if the field would make it
        /// too long for its offset type.
        #[inline(always)]
        pub fn $name<'v>(
            &mut self,
            values: impl Into<Cow<'v, [$type]>>,
        ) -> Result<(), BuildError> {
            self.set_array(&values.into())
        }
    )*};
}

impl TableBuilder {
    /// Starts a table of `fields` fields in `layout`, which its readers must
    /// use too, with the layout's version in its header when it has one.
    /// Nested tables use [`TableLayout::DEFAULT`]. The builder allocates its
    /// buffer, with room for 64 bytes a field beyond the header, up to
    /// 64 KiB more, and at least 256 bytes.
    ///
    /// # Errors
    ///
    /// [`BuildError::VersionTooLarge`] if the layout's version doesn't fit
    /// in its offset type, or [`BuildError::HeaderTooLong`] if the header
    /// alone is too long for it.
    #[inline]
    pub fn new<const VERSIONED: bool>(
        fields: usize,
        layout: TableLayout<VERSIONED>,
    ) -> Result<Self, BuildError> {
        Self::with_buffer(Vec::new(), fields, layout)
    }

    /// Starts a table as [`TableBuilder::new()`] does, in `buffer`, whose
    /// bytes it clears, and whose capacity it keeps, adding room only if it
    /// has less than `new()` would allocate. So a buffer that holds a table
    /// already written is reused, and one made with `Vec::with_capacity()`
    /// gives the room that a large table needs. The table's bytes are then
    /// the buffer's, which [`TableBuilder::into_bytes()`] returns.
    ///
    /// ```
    /// use tafla::{BuildError, TableBuilder, TableLayout};
    ///
    /// let column: Vec<f64> = (0..100_000).map(f64::from).collect();
    ///
    /// let buffer = Vec::with_capacity(800_008);
    /// let mut builder = TableBuilder::with_buffer(buffer, 1, TableLayout::DEFAULT)?;
    /// builder.set_f64_array(&column)?;
    /// let table: Vec<u8> = builder.into_bytes()?;
    /// # assert_eq!(table.len(), 800_008);
    /// # Ok::<(), BuildError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// The errors of [`TableBuilder::new()`], which drop the buffer.
    #[inline]
    pub fn with_buffer<const VERSIONED: bool>(
        buffer: Vec<u8>,
        fields: usize,
        layout: TableLayout<VERSIONED>,
    ) -> Result<Self, BuildError> {
        layout.check_version()?;
        let header = layout.header_length(fields)?;

        let mut builder = Self {
            bytes: Bytes::new(buffer),
            current: Counters {
                table: 0,
                slots: 0,
                fields: 0,
                set: 0,
                alignment: Alignment::One,
                padded: false,
                rel: 0,
                padding: 0,
            },
            open: Stack::new(NO_COUNTERS),
            undo: Stack::new(NO_UNDO),
            sized: false,
            offset_type: OffsetType::Uint32,
        };
        builder.start(fields, header, layout);

        Ok(builder)
    }

    /// Starts the next table, of `fields` fields in `layout`, in the
    /// builder's buffer, whose bytes it clears and whose capacity it keeps,
    /// as [`TableBuilder::with_buffer()`] does. Whatever the builder held is
    /// dropped, finished or not, so a loop can write each table with one
    /// builder:
    ///
    /// ```
    /// use tafla::{BuildError, TableBuilder, TableLayout};
    ///
    /// let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
    /// let mut written = Vec::new();
    /// for name in ["first", "second"] {
    ///     builder.reset(1, TableLayout::DEFAULT)?;
    ///     builder.set_str(name)?;
    ///     written.extend_from_slice(builder.finish()?);
    /// }
    /// # assert_eq!(written.len(), 2 * 8 + 5 + 6);
    /// # Ok::<(), BuildError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// The errors of [`TableBuilder::new()`], which leave the builder as it
    /// was.
    #[inline]
    pub fn reset<const VERSIONED: bool>(
        &mut self,
        fields: usize,
        layout: TableLayout<VERSIONED>,
    ) -> Result<(), BuildError> {
        layout.check_version()?;
        let header = layout.header_length(fields)?;

        self.start(fields, header, layout);

        Ok(())
    }

    /// Starts a table of `fields` fields, whose header of `header` bytes its
    /// layout checked, in the cleared buffer, with the header written: zeros
    /// for its length, which `finish()` writes, its version, and zeros for
    /// the offsets
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    fn start<const VERSIONED: bool>(
        &mut self,
        fields: usize,
        header: u32,
        layout: TableLayout<VERSIONED>,
    ) {
        let offset_type = layout.offset_type();
        let sized = layout.is_sized();

        self.bytes.start_table(header as usize, fields);
        if sized {
            self.bytes.push_zeros(offset_type.size());
        }
        if let Some(version) = layout.header_version() {
            push_slot(&mut self.bytes, version as usize, offset_type);
        }
        let slots = self.bytes.len();
        self.bytes.push_zeros(header as usize - slots);

        self.current = Counters {
            table: 0,
            slots,
            // The header holds an offset of at least a byte per field
            fields: fields as u32,
            set: 0,
            alignment: Alignment::One,
            padded: false,
            rel: 0,
            padding: 0,
        };
        self.open.clear();
        self.undo.clear();
        self.sized = sized;
        self.offset_type = offset_type;
    }

    /// The table's bytes, which the builder lends until it's changed, with
    /// its length in a sized header. A table nested in place, with
    /// [`TableBuilder::set_table_with()`], is already among them. They're
    /// the start of the builder's buffer, which [`TableBuilder::reset()`]
    /// reuses and [`TableBuilder::into_bytes()`] returns.
    ///
    /// # Errors
    ///
    /// [`BuildError::Incomplete`] if fewer fields were set than the table
    /// holds.
    #[inline]
    pub fn finish(&mut self) -> Result<&[u8], BuildError> {
        self.check_complete()?;

        self.write_length();

        Ok(self.bytes.as_slice())
    }

    /// The table, as the builder's buffer, without copying it: the bytes
    /// that [`TableBuilder::finish()`] returns, in a `Vec` with the room
    /// that the buffer had beyond them.
    ///
    /// # Errors
    ///
    /// [`BuildError::Incomplete`] if fewer fields were set than the table
    /// holds, which drops the buffer.
    #[inline]
    pub fn into_bytes(mut self) -> Result<Vec<u8>, BuildError> {
        self.check_complete()?;

        self.write_length();

        Ok(self.bytes.into_vec())
    }

    /// A copy of the table, in a `Vec` of its exact length, which leaves the
    /// builder as it was
    ///
    /// # Errors
    ///
    /// [`BuildError::Incomplete`] if fewer fields were set than the table
    /// holds.
    pub fn to_bytes(&self) -> Result<Vec<u8>, BuildError> {
        self.check_complete()?;

        let mut table = self.bytes.as_slice().to_vec();
        self.write_length_into(&mut table);

        Ok(table)
    }

    scalar_setters!(
        set_i8(i8),
        set_u8(u8),
        set_i16(i16),
        set_u16(u16),
        set_i32(i32),
        set_u32(u32),
        set_i64(i64),
        set_u64(u64),
        set_f32(f32),
        set_f64(f64)
    );

    array_setters!(
        set_i8_array(i8),
        set_u8_array(u8),
        set_i16_array(i16),
        set_u16_array(u16),
        set_i32_array(i32),
        set_u32_array(u32),
        set_i64_array(i64),
        set_u64_array(u64),
        set_f32_array(f32),
        set_f64_array(f64)
    );

    /// Sets an array field of bools, written as a `u8` array of 1 for each
    /// true value and 0 for each false one, copied from a slice or a `Vec`,
    /// borrowed or owned
    ///
    /// # Errors
    ///
    /// [`BuildError::TooManyFields`] if the table holds all its fields
    /// already, or [`BuildError::TooLong`] if the field would make it too
    /// long for its offset type.
    #[inline]
    pub fn set_bool_array<'v>(
        &mut self,
        values: impl Into<Cow<'v, [bool]>>,
    ) -> Result<(), BuildError> {
        let values = values.into();
        let length = values.len();
        self.check_field(padded_length(0, self.width(), length))?;

        self.write_offset(self.position());
        self.push_slot(length);
        self.bytes.push_bools(&values);
        self.current.set += 1;

        Ok(())
    }

    /// Sets a string field, copied from a `&str` or a `String`
    ///
    /// # Errors
    ///
    /// [`BuildError::TooManyFields`] if the table holds all its fields
    /// already, or [`BuildError::TooLong`] if the field would make it too
    /// long for its offset type.
    #[inline(always)]
    pub fn set_str<'v>(&mut self, string: impl Into<Cow<'v, str>>) -> Result<(), BuildError> {
        self.set_string(string.into().as_bytes())
    }

    /// Sets a vector of strings, copied from a slice or a `Vec` of them,
    /// borrowed or owned. The strings follow the end of each, which lets readers find any string
    /// without reading the others, and take no more bytes each than their
    /// end, where a vector of tables of one string takes each table's offset
    /// and the string's length too.
    ///
    /// # Errors
    ///
    /// [`BuildError::TooManyFields`] if the table holds all its fields
    /// already, or [`BuildError::TooLong`] if the field would make it too
    /// long for its offset type.
    #[allow(clippy::cast_possible_truncation)]
    pub fn set_str_vector<'v>(
        &mut self,
        strings: impl Into<Cow<'v, [&'v str]>>,
    ) -> Result<(), BuildError> {
        let strings = strings.into();
        let strings = strings.as_ref();
        let width = self.width();
        // The count, and an end for each string beside its bytes, saturating
        // far past the longest table
        let byte_length = strings.iter().fold(width as u64, |length, string| {
            length.saturating_add(width as u64 + string.len() as u64)
        });
        self.check_field(byte_length)?;

        // Each end counts from the vector's position, which is its count's,
        // and is known from the strings' lengths, so the ends are written
        // before the strings. The length fits in the offsets, and so in a
        // `usize`.
        self.bytes.reserve(byte_length as usize);
        self.write_offset(self.position());
        self.push_slot(strings.len());
        let mut end = (strings.len() + 1) * width;
        for string in strings {
            end += string.len();
            self.push_slot(end);
        }
        for string in strings {
            self.bytes.push(string.as_bytes());
        }
        self.current.set += 1;

        Ok(())
    }

    /// Sets a struct field from its bytes: its scalars, little-endian, one
    /// after another, which are copied as they are, without padding, from a
    /// slice or a `Vec`, borrowed or owned.
    ///
    /// # Errors
    ///
    /// [`BuildError::TooManyFields`] if the table holds all its fields
    /// already, or [`BuildError::TooLong`] if the field would make it too
    /// long for its offset type.
    #[inline(always)]
    pub fn set_struct<'v>(&mut self, bytes: impl Into<Cow<'v, [u8]>>) -> Result<(), BuildError> {
        let bytes = bytes.into();
        self.check_field(bytes.len() as u64)?;

        self.write_offset(self.position());
        self.bytes.push(&bytes);
        self.current.set += 1;

        Ok(())
    }

    /// Sets an array of structs of `size` bytes each, from their bytes one
    /// after another, which it counts. It's padded so that the structs start
    /// at a multiple of `alignment` from the start of the table, which is
    /// the size of the structs' largest scalar.
    ///
    /// # Errors
    ///
    /// [`BuildError::NotWholeStructs`] if `size` is 0 or the bytes aren't a
    /// whole number of structs, or an error of any setter.
    #[inline(always)]
    pub fn set_struct_array<'v>(
        &mut self,
        bytes: impl Into<Cow<'v, [u8]>>,
        size: usize,
        alignment: Alignment,
    ) -> Result<(), BuildError> {
        let bytes = bytes.into();
        let length = bytes.len();
        if size == 0 || !length.is_multiple_of(size) {
            return Err(BuildError::NotWholeStructs { length, size });
        }

        let width = self.width();
        let padding = alignment.padding(self.table_length() + width);
        self.check_field(padded_length(padding, width, length))?;

        self.align_to(alignment);
        self.bytes.push_zeros(padding);
        self.write_offset(self.position());
        self.push_slot(length / size);
        self.bytes.push(&bytes);
        self.count(alignment);

        Ok(())
    }

    /// Embeds a table, from its builder or its bytes, without a length prefix.
    /// Readers find its fields with `TableReader::field_offset()` from the
    /// field's position, which reads offsets of the parent's offset type, so
    /// the table should have no header and the parent's offset type: that's
    /// [`TableLayout::DEFAULT`] in a parent of `uint32` offsets.
    ///
    /// A builder, borrowed or owned, must have all its fields set. Its bytes
    /// are copied into this table, in its own layout, with its length in a
    /// sized header. [`TableBuilder::set_table_with()`] writes a table in
    /// place, rather than copying it.
    ///
    /// The nested table's arrays are aligned from its own start, so it's
    /// padded to start at a multiple of an alignment, which they need to be
    /// read in place. For a builder, that's the largest alignment of its
    /// arrays, of scalars and of structs, and of its nested tables, or 1 if
    /// it has neither. For bytes, it's 4, which suits tables of 8-, 16- and
    /// 32-bit elements; [`TableBuilder::set_table_aligned()`] takes another.
    ///
    /// # Errors
    ///
    /// [`BuildError::Incomplete`] if the builder has fields left to set, or
    /// an error of any setter.
    #[inline]
    pub fn set_table<'n>(&mut self, table: impl Into<NestedTable<'n>>) -> Result<(), BuildError> {
        self.add_table(&table.into())
    }

    /// Embeds a table like [`TableBuilder::set_table()`], padded to start at
    /// a multiple of `alignment`, such as [`Alignment::Eight`] for a table
    /// of 64-bit arrays given as bytes
    ///
    /// # Errors
    ///
    /// The errors of [`TableBuilder::set_table()`].
    #[inline]
    pub fn set_table_aligned<'n>(
        &mut self,
        table: impl Into<NestedTable<'n>>,
        alignment: Alignment,
    ) -> Result<(), BuildError> {
        self.add_table(&table.into().aligned(alignment))
    }

    /// Writes a vector of tables, from their builders or their bytes, after
    /// the end of each, which lets readers find any item without reading the
    /// others. Builders must have all their fields set, and are copied as
    /// `set_table()` copies them. Items aren't padded, so their arrays are
    /// read in place only when they happen to be aligned.
    ///
    /// # Errors
    ///
    /// [`BuildError::Incomplete`] if a builder has fields left to set, or an
    /// error of any setter.
    #[allow(clippy::cast_possible_truncation)]
    pub fn set_vector<'n, I>(&mut self, items: I) -> Result<(), BuildError>
    where
        I: IntoIterator,
        I::Item: Into<NestedTable<'n>>,
    {
        let items: Vec<NestedTable<'n>> = items.into_iter().map(Into::into).collect();
        for item in &items {
            item.check_complete()?;
        }

        let width = self.width();
        let byte_length = items.iter().fold(width as u64, |length, item| {
            length.saturating_add(width as u64 + item.byte_length() as u64)
        });
        self.check_field(byte_length)?;

        // The length fits in the offsets, and so in a `usize`
        self.bytes.reserve(byte_length as usize);
        let vector = self.position();
        self.write_offset(vector);
        let ends = self.bytes.len() + width;
        self.push_slot(items.len());
        self.bytes.push_zeros(items.len() * width);

        for (index, item) in items.iter().enumerate() {
            self.append(item);
            self.write_item_end(ends, index, vector);
        }
        self.current.set += 1;

        Ok(())
    }

    /// Writes a table of `fields` fields nested in this one, in place, as a
    /// nested builder would be, but without a builder of its own: `fill`
    /// sets its fields, in order, on the [`InPlaceTable`] that it's given,
    /// whose setters write each among this builder's own fields. The table
    /// has no header and this table's offset type, and is padded as
    /// [`TableBuilder::set_table()`] pads a builder: to start at a multiple
    /// of the largest alignment of its fields. It's written where this table
    /// ends, and moved forward by that padding once its fields are set.
    ///
    /// The builder is borrowed while `fill` runs, which sets the table's
    /// fields only through the `InPlaceTable`.
    ///
    /// # Errors
    ///
    /// [`BuildError::TooManyFields`] if this table holds all its fields
    /// already, [`BuildError::HeaderTooLong`] if the nested table's header
    /// alone is too long for the offset type, [`BuildError::Incomplete`] if
    /// `fill` leaves its fields unset, or an error of any setter, each
    /// converted into `E`, or the error that `fill` returns. Each leaves the
    /// builder as it was, as a panic in `fill` does.
    #[allow(clippy::cast_possible_truncation)]
    #[inline(always)]
    pub fn set_table_with<E, F>(&mut self, fields: usize, fill: F) -> Result<(), E>
    where
        E: From<BuildError>,
        F: FnOnce(&mut InPlaceTable<'_>) -> Result<(), E>,
    {
        // Room for the field, and its table's header, before `fill` runs
        self.check_field(0)?;
        let header = self.in_place_header(fields)?;

        let checkpoint = Checkpoint::new(self);
        let builder = &mut *checkpoint.builder;
        // The header holds an offset of at least a byte per field
        builder.start_in_place(fields as u32, header, true);
        fill(&mut InPlaceTable {
            builder: &mut *builder,
        })?;
        builder.check_complete()?;

        // Padded from this table's start, as a nested builder is, already
        // as far as its fields raised its alignment
        let table = builder.end_in_place();
        let padding = table.alignment.padding(table.rel) - table.padding;
        builder.check_field(padding as u64)?;
        if padding != 0 {
            builder.move_by(padding, table.table);
        }

        builder.write_offset(table.table + padding);
        builder.count(table.alignment);
        checkpoint.commit();

        Ok(())
    }

    /// Writes a vector of `count` tables of `fields` fields each, in place,
    /// as [`TableBuilder::set_table_with()`] writes one: `fill(index, item)`
    /// sets the fields of item `index`, for each in order. It writes the
    /// bytes that [`TableBuilder::set_vector()`] writes from a builder per
    /// item, without their allocations. Items aren't padded, as
    /// `set_vector()` doesn't pad them. After the first item, it makes room
    /// for the others, each as long as the first, up to 64 KiB, if it can.
    /// `fill` runs for every item, even once the items make the table too
    /// long, which the vector then fails with, but the builder holds no more
    /// than one item past that length.
    ///
    /// ```
    /// use tafla::{BuildError, TableBuilder, TableLayout};
    ///
    /// let points = [(1.0_f32, 2.0_f32), (3.0, 4.0)];
    ///
    /// let mut builder = TableBuilder::new(1, TableLayout::DEFAULT)?;
    /// builder.set_vector_with(points.len(), 2, |index, point| {
    ///     let (x, y) = points[index];
    ///     point.set_f32(x)?;
    ///     point.set_f32(y)
    /// })?;
    /// let table = builder.finish()?;
    /// # Ok::<(), BuildError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// [`BuildError::TooManyFields`] if this table holds all its fields
    /// already, or [`BuildError::TooLong`] if the vector's count and its
    /// items' ends make it too long, before any item is written, with the
    /// length that they make it, and otherwise the errors of
    /// [`TableBuilder::set_table_with()`] for each item, or
    /// [`BuildError::TooLong`] if the items make the table too long. Each
    /// leaves the builder as it was, as a panic in `fill` does.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    pub fn set_vector_with<E, F>(
        &mut self,
        count: usize,
        fields: usize,
        mut fill: F,
    ) -> Result<(), E>
    where
        E: From<BuildError>,
        F: FnMut(usize, &mut InPlaceTable<'_>) -> Result<(), E>,
    {
        // The count and the items' ends, which fail before any item is
        // written, and which leave room for fewer than 2^32 items
        let width = self.width();
        let slots = (count as u64)
            .saturating_add(1)
            .saturating_mul(width as u64);
        self.check_field(slots)?;
        let header = self.in_place_header(fields)?;

        let checkpoint = Checkpoint::new(self);
        let builder = &mut *checkpoint.builder;
        let vector = builder.position();
        builder.write_offset(vector);
        let ends = builder.bytes.len() + width;
        builder.push_slot(count);
        builder.bytes.push_zeros(count * width);

        // The bytes of the items dropped once the table is too long
        let mut dropped = 0;
        let max_length = u64::from(builder.offset_type.max_value());
        for index in 0..count {
            let item = builder.bytes.len();
            // The header holds an offset of at least a byte per field
            builder.start_in_place(fields as u32, header, false);
            fill(
                index,
                &mut InPlaceTable {
                    builder: &mut *builder,
                },
            )?;
            builder.check_complete()?;

            builder.end_in_place();
            // Once the table is too long, the items that follow, which fail
            // with it, are kept only as lengths. After the first item, there's
            // room for the others, up to 64 KiB, each as long.
            if builder.table_length() as u64 + dropped > max_length {
                dropped += (builder.bytes.len() - item) as u64;
                builder.bytes.truncate(item);
            } else {
                builder.write_item_end(ends, index, vector);
                if index == 0 {
                    builder.reserve_items_like(builder.bytes.len() - item, count - 1);
                }
            }
        }

        // The items can make the table too long
        builder.check_field(dropped)?;
        builder.current.set += 1;
        checkpoint.commit();

        Ok(())
    }

    /// Sets a scalar field from its little-endian bytes
    #[inline(always)]
    fn set_scalar<const SIZE: usize>(&mut self, bytes: [u8; SIZE]) -> Result<(), BuildError> {
        self.check_field(SIZE as u64)?;

        self.write_offset(self.position());
        self.bytes.push(&bytes);
        self.current.set += 1;

        Ok(())
    }

    /// Sets an array field, padded so that its elements are aligned
    #[inline(always)]
    fn set_array<T: Element>(&mut self, values: &[T]) -> Result<(), BuildError> {
        let width = self.width();
        let padding = T::ALIGNMENT.padding(self.table_length() + width);
        self.check_field(padded_length(padding, width, size_of_val(values)))?;

        self.align_to(T::ALIGNMENT);
        self.bytes.push_zeros(padding);
        self.write_offset(self.position());
        self.push_slot(values.len());
        self.bytes.push_elements(values);
        self.count(T::ALIGNMENT);

        Ok(())
    }

    /// Sets a string field from its UTF-8 bytes
    #[inline(always)]
    fn set_string(&mut self, bytes: &[u8]) -> Result<(), BuildError> {
        let length = bytes.len();
        self.check_field(padded_length(0, self.width(), length))?;

        self.write_offset(self.position());
        self.push_slot(length);
        self.bytes.push(bytes);
        self.current.set += 1;

        Ok(())
    }

    fn add_table(&mut self, table: &NestedTable<'_>) -> Result<(), BuildError> {
        table.check_complete()?;
        let alignment = table.alignment();
        let padding = alignment.padding(self.table_length());
        self.check_field(padding as u64 + table.byte_length() as u64)?;

        self.align_to(alignment);
        self.bytes.push_zeros(padding);
        self.write_offset(self.position());
        self.append(table);
        self.count(alignment);

        Ok(())
    }

    /// Appends a nested table's bytes, after its offset or before its end,
    /// with its length in a sized header
    fn append(&mut self, table: &NestedTable<'_>) {
        match &table.0 {
            Nested::Builder(builder, _) => self.append_builder(builder),
            Nested::OwnedBuilder(builder) => self.append_builder(builder),
            Nested::Bytes(bytes, _) => self.bytes.push(bytes),
        }
    }

    /// Appends another builder's bytes, with its length in a sized header
    fn append_builder(&mut self, builder: &Self) {
        let start = self.bytes.len();
        self.bytes.push(builder.bytes.as_slice());
        builder.write_length_into(&mut self.bytes.as_mut_slice()[start..]);
    }

    /// Appends a count or length slot
    #[inline(always)]
    fn push_slot(&mut self, value: usize) {
        push_slot(&mut self.bytes, value, self.offset_type);
    }

    /// Starts a table of `fields` fields in place, whose header of `header`
    /// bytes holds only their offsets, at the end of the bytes so far, and
    /// counts its fields rather than its parent's
    #[inline(always)]
    fn start_in_place(&mut self, fields: u32, header: u32, padded: bool) {
        let table = self.position();
        let rel = self.table_length();
        self.bytes.push_zeros(header as usize);

        let parent = mem::replace(
            &mut self.current,
            Counters {
                table,
                slots: table,
                fields,
                set: 0,
                alignment: Alignment::One,
                padded,
                rel,
                padding: 0,
            },
        );
        self.open.push(parent);
    }

    /// Returns to the parent of a table written in place, and returns the
    /// table's counters
    #[inline(always)]
    fn end_in_place(&mut self) -> Counters {
        let parent = self.open.pop().expect("a table is open in place");
        mem::replace(&mut self.current, parent)
    }

    /// Raises the alignment of the table whose fields are being set to
    /// `alignment`, before a field that needs it is written. A table nested
    /// in place is then padded for it, and the tables in place that hold it,
    /// which their alignment will need once it's set, by moving what's
    /// written of them so far.
    #[inline(always)]
    fn align_to(&mut self, alignment: Alignment) {
        if alignment > self.current.alignment && self.current.padded {
            self.raise(alignment);
        }
    }

    #[cold]
    #[inline(never)]
    fn raise(&mut self, alignment: Alignment) {
        let last = self.open.len();
        let mut first = last;
        while first > 0 && self.level(first - 1).padded {
            first -= 1;
        }

        let mut paddings = [0; 64];
        let mut more = Vec::new();
        for index in first..=last {
            let level = *self.level(index);
            let raised = level.alignment.max(alignment);
            if raised != level.alignment {
                self.undo.push(Undo {
                    level: index,
                    padding: level.padding,
                    alignment: level.alignment,
                });
                self.level_mut(index).alignment = raised;
            }
            let padding = raised.padding(level.rel);
            match paddings.get_mut(index - first) {
                Some(slot) => *slot = padding,
                None => more.push(padding),
            }
        }
        let count = last - first + 1;
        if count <= paddings.len() {
            self.shift_chain(first, &paddings[..count]);
        } else {
            let mut all = paddings.to_vec();
            all.extend(more);
            self.shift_chain(first, &all);
        }
    }

    /// Sets the padding of each level from `first` to the current table to
    /// `paddings`, moving what's written of each by its padding and those of
    /// the levels that hold it
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn shift_chain(&mut self, first: usize, paddings: &[usize]) {
        let last = first + paddings.len() - 1;
        let end = self.bytes.len();
        let delta = |builder: &Self, index: usize| {
            paddings[index - first] as isize - builder.level(index).padding as isize
        };
        let total: isize = (first..=last).map(|index| delta(self, index)).sum();
        let segment = |builder: &Self, index: usize| {
            let start = builder.level(index).table;
            let stop = if index == last {
                end
            } else {
                let next = builder.level(index + 1);
                next.table - next.padding
            };
            (start, stop)
        };

        // Each level's content moves by the deltas of its own padding and
        // those of the levels that hold it: from the last, which moves
        // furthest, when they grow, and from the first when they shrink
        if total > 0 {
            self.bytes.push_zeros(total as usize);
            let mut shift = total;
            for index in (first..=last).rev() {
                let (start, stop) = segment(self, index);
                let to = (start as isize + shift) as usize;
                self.bytes.as_mut_slice().copy_within(start..stop, to);
                shift -= delta(self, index);
            }
        } else if total < 0 {
            let mut shift = 0;
            for index in first..=last {
                shift += delta(self, index);
                let (start, stop) = segment(self, index);
                let to = (start as isize + shift) as usize;
                self.bytes.as_mut_slice().copy_within(start..stop, to);
            }
            self.bytes.truncate((end as isize + total) as usize);
        }

        let mut shift = 0;
        for index in first..=last {
            shift += delta(self, index);
            let padding = paddings[index - first];
            let level = self.level_mut(index);
            level.table = (level.table as isize + shift) as usize;
            level.slots = (level.slots as isize + shift) as usize;
            level.padding = padding;
            let table = level.table;
            self.bytes.as_mut_slice()[table - padding..table].fill(0);
        }
    }

    /// The table at `index` among those open, or the current table, at
    /// the index past them
    fn level(&self, index: usize) -> &Counters {
        self.open.get(index).unwrap_or(&self.current)
    }

    fn level_mut(&mut self, index: usize) -> &mut Counters {
        if index < self.open.len() {
            self.open.get_mut(index)
        } else {
            &mut self.current
        }
    }

    /// Restores the paddings and alignments that early moves changed since
    /// `saved`, of the tables open then, moving them back
    #[cold]
    #[inline(never)]
    fn undo_moves(&mut self, saved: Saved) {
        let last = saved.open;
        let changed: Vec<Undo> = (saved.undo..self.undo.len())
            .filter_map(|index| self.undo.get(index).copied())
            .filter(|undo| undo.level <= last)
            .collect();
        self.undo.truncate(saved.undo);
        let Some(first) = changed.iter().map(|undo| undo.level).min() else {
            return;
        };

        let mut paddings: Vec<usize> = (first..=last).map(|index| self.level(index).padding).collect();
        let mut alignments: Vec<Alignment> =
            (first..=last).map(|index| self.level(index).alignment).collect();
        // The oldest change of each level holds what it was
        for undo in changed.iter().rev() {
            paddings[undo.level - first] = undo.padding;
            alignments[undo.level - first] = undo.alignment;
        }
        self.shift_chain(first, &paddings);
        for (index, alignment) in (first..=last).zip(alignments) {
            self.level_mut(index).alignment = alignment;
        }
    }

    /// Moves the table written in place from `start` forward by `padding`,
    /// and zeros the padding before it. A table of the benchmarks never
    /// needs it, so it's out of line. A table nested in place in another,
    /// each padded, is moved again with the table that holds it.
    #[cold]
    #[inline(never)]
    fn move_by(&mut self, padding: usize, start: usize) {
        let end = self.bytes.len();
        self.bytes.push_zeros(padding);

        let bytes = self.bytes.as_mut_slice();
        bytes.copy_within(start..end, start + padding);
        bytes[start..start + padding].fill(0);
    }

    /// Makes room, after a vector's first item wrote `first` bytes, for its
    /// `others`, as many bytes each, up to [`MOST_ROOM_FOR_ITEMS`], if it
    /// can: an uneven vector, whose first item is its longest, would
    /// otherwise make room for many times its length
    fn reserve_items_like(&mut self, first: usize, others: usize) {
        let room = first.saturating_mul(others);
        self.bytes.try_reserve(room.min(MOST_ROOM_FOR_ITEMS));
    }

    /// Writes the offset of the next field, which starts at `position`
    #[inline(always)]
    fn write_offset(&mut self, position: usize) {
        let slot = self.current.slots + self.current.set as usize * self.width();
        let offset = position - self.current.table;
        write_slot(self.bytes.as_mut_slice(), slot, offset, self.offset_type);
    }

    /// Writes the end of item `index` of the vector at `vector`, whose ends
    /// start at `ends` in the builder's bytes, where the bytes so far end
    #[inline(always)]
    fn write_item_end(&mut self, ends: usize, index: usize, vector: usize) {
        let end = self.position() - vector;
        let slot = ends + index * self.width();
        write_slot(self.bytes.as_mut_slice(), slot, end, self.offset_type);
    }

    /// Counts a field that needs `alignment` from the table's start
    #[inline(always)]
    fn count(&mut self, alignment: Alignment) {
        self.current.set += 1;
        self.current.alignment = self.current.alignment.max(alignment);
    }

    /// Checks that the table can hold another field, of `byte_length` bytes
    /// with its padding. Checking each field before it's written, rather
    /// than the table when it's written, reports a field too long for the
    /// layout from its own setter and leaves the builder as it was. Every
    /// offset, count, length and item end is at most the table's length, so
    /// that's the one value to check. It's counted in u64, which a 32-bit
    /// `usize` doesn't hold.
    #[inline(always)]
    fn check_field(&self, byte_length: u64) -> Result<(), BuildError> {
        let Counters { fields, set, .. } = self.current;
        if set == fields {
            return Err(BuildError::TooManyFields {
                fields: fields as usize,
            });
        }

        let length = (self.table_length() as u64).saturating_add(byte_length);
        if length > u64::from(self.offset_type.max_value()) {
            return Err(BuildError::TooLong {
                index: set as usize,
                length,
                offset_type: self.offset_type,
            });
        }

        Ok(())
    }

    #[inline]
    const fn check_complete(&self) -> Result<(), BuildError> {
        let Counters { fields, set, .. } = self.current;
        if set == fields {
            Ok(())
        } else {
            Err(BuildError::Incomplete {
                set: set as usize,
                fields: fields as usize,
            })
        }
    }

    /// The length of the header of a table of `fields` fields nested in
    /// place, which has no header slots and this table's offset type
    #[inline]
    fn in_place_header(&self, fields: usize) -> Result<u32, BuildError> {
        let layout = TableLayout::DEFAULT.with_offset_type(self.offset_type);
        layout.header_length(fields)
    }

    #[inline(always)]
    const fn width(&self) -> usize {
        self.offset_type.size()
    }

    /// The root's length so far, which is where the next byte goes
    #[inline(always)]
    const fn position(&self) -> usize {
        self.bytes.len()
    }

    /// The length so far of the table whose fields are being set
    #[inline(always)]
    const fn table_length(&self) -> usize {
        self.position() - self.current.table
    }

    /// Writes the table's length into a sized header
    #[inline]
    fn write_length(&mut self) {
        if self.sized {
            let length = self.position();
            write_slot(self.bytes.as_mut_slice(), 0, length, self.offset_type);
        }
    }

    /// Writes the table's length into a sized header at the start of
    /// `table`, a copy of its bytes
    fn write_length_into(&self, table: &mut [u8]) {
        if self.sized {
            write_slot(table, 0, self.position(), self.offset_type);
        }
    }

    /// Returns to where the builder was before a table or vector nested in
    /// place
    fn restore(&mut self, saved: Saved) {
        // The tables written in place since, which are dropped
        if self.open.len() > saved.open {
            self.current = *self.open.get_mut(saved.open);
            self.open.truncate(saved.open);
        }
        if self.undo.len() > saved.undo {
            self.undo_moves(saved);
        }

        self.bytes.truncate(saved.bytes);
        self.current = saved.counters;
    }
}

/// A table or vector being written in place. Until it's committed, dropping
/// it, as an error or a panic does, returns the builder to where it was.
struct Checkpoint<'g> {
    builder: &'g mut TableBuilder,
    saved: Saved,
}

impl<'g> Checkpoint<'g> {
    #[inline]
    fn new(builder: &'g mut TableBuilder) -> Self {
        let saved = Saved {
            counters: builder.current,
            bytes: builder.bytes.len(),
            open: builder.open.len(),
            undo: builder.undo.len(),
        };

        Self { builder, saved }
    }

    /// Keeps what was written
    #[inline]
    const fn commit(self) {
        mem::forget(self);
    }
}

impl Drop for Checkpoint<'_> {
    fn drop(&mut self) {
        self.builder.restore(self.saved);
    }
}

/// The fields of a table that a [`TableBuilder`] writes in place, as one of
/// its own fields or an item of a vector.
///
/// The builder's [`TableBuilder::set_table_with()`] and
/// [`TableBuilder::set_vector_with()`] give it to the function that sets the
/// fields. Its setters are the builder's, taking the same values and
/// returning the same errors, and write each field among the builder's own,
/// so the table needs no builder of its own and its fields are never copied.
#[derive(Debug)]
pub struct InPlaceTable<'b> {
    builder: &'b mut TableBuilder,
}

/// Setters of an [`InPlaceTable`], which are the builder's
macro_rules! in_place_setters {
    ($($name:ident$([$($generic:tt)*])?($($argument:ident: $type:ty),*)),* $(,)?) => {$(
        #[doc = concat!("Sets a field as [`TableBuilder::", stringify!($name), "()`] does")]
        ///
        /// # Errors
        ///
        #[doc = concat!("The errors of [`TableBuilder::", stringify!($name), "()`].")]
        #[inline(always)]
        pub fn $name$(<$($generic)*>)?(&mut self, $($argument: $type),*) -> Result<(), BuildError> {
            self.builder.$name($($argument),*)
        }
    )*};
}

impl InPlaceTable<'_> {
    in_place_setters!(
        set_i8(value: i8),
        set_u8(value: u8),
        set_i16(value: i16),
        set_u16(value: u16),
        set_i32(value: i32),
        set_u32(value: u32),
        set_i64(value: i64),
        set_u64(value: u64),
        set_f32(value: f32),
        set_f64(value: f64),
        set_i8_array['v](values: impl Into<Cow<'v, [i8]>>),
        set_u8_array['v](values: impl Into<Cow<'v, [u8]>>),
        set_i16_array['v](values: impl Into<Cow<'v, [i16]>>),
        set_u16_array['v](values: impl Into<Cow<'v, [u16]>>),
        set_i32_array['v](values: impl Into<Cow<'v, [i32]>>),
        set_u32_array['v](values: impl Into<Cow<'v, [u32]>>),
        set_i64_array['v](values: impl Into<Cow<'v, [i64]>>),
        set_u64_array['v](values: impl Into<Cow<'v, [u64]>>),
        set_f32_array['v](values: impl Into<Cow<'v, [f32]>>),
        set_f64_array['v](values: impl Into<Cow<'v, [f64]>>),
        set_bool_array['v](values: impl Into<Cow<'v, [bool]>>),
        set_str['v](string: impl Into<Cow<'v, str>>),
        set_struct['v](bytes: impl Into<Cow<'v, [u8]>>),
        set_struct_array['v](bytes: impl Into<Cow<'v, [u8]>>, size: usize, alignment: Alignment),
        set_table['n](table: impl Into<NestedTable<'n>>),
        set_table_aligned['n](table: impl Into<NestedTable<'n>>, alignment: Alignment),
        set_str_vector['v](strings: impl Into<Cow<'v, [&'v str]>>),
    );

    /// Sets a vector field as [`TableBuilder::set_vector()`] does
    ///
    /// # Errors
    ///
    /// The errors of [`TableBuilder::set_vector()`].
    pub fn set_vector<'n, I>(&mut self, items: I) -> Result<(), BuildError>
    where
        I: IntoIterator,
        I::Item: Into<NestedTable<'n>>,
    {
        self.builder.set_vector(items)
    }

    /// Writes a table nested in this one in place, as
    /// [`TableBuilder::set_table_with()`] does
    ///
    /// # Errors
    ///
    /// The errors of [`TableBuilder::set_table_with()`].
    #[inline(always)]
    pub fn set_table_with<E, F>(&mut self, fields: usize, fill: F) -> Result<(), E>
    where
        E: From<BuildError>,
        F: FnOnce(&mut InPlaceTable<'_>) -> Result<(), E>,
    {
        self.builder.set_table_with(fields, fill)
    }

    /// Writes a vector of tables in place, as
    /// [`TableBuilder::set_vector_with()`] does
    ///
    /// # Errors
    ///
    /// The errors of [`TableBuilder::set_vector_with()`].
    #[inline]
    pub fn set_vector_with<E, F>(&mut self, count: usize, fields: usize, fill: F) -> Result<(), E>
    where
        E: From<BuildError>,
        F: FnMut(usize, &mut InPlaceTable<'_>) -> Result<(), E>,
    {
        self.builder.set_vector_with(count, fields, fill)
    }
}

/// A table to nest with [`TableBuilder::set_table()`] or in
/// [`TableBuilder::set_vector()`]: a builder, borrowed or owned, or bytes,
/// which are copied into the parent.
#[derive(Debug)]
pub struct NestedTable<'a>(Nested<'a>);

/// A table to nest, and the alignment that it's padded to as a field. An
/// owned builder keeps that as its own alignment.
///
/// An owned builder is held as it is, rather than boxed, which would add an
/// allocation for each table nested from one, to the allocation of its
/// bytes.
#[derive(Debug)]
enum Nested<'a> {
    Builder(&'a TableBuilder, Alignment),
    OwnedBuilder(TableBuilder),
    Bytes(Cow<'a, [u8]>, Alignment),
}

impl NestedTable<'_> {
    fn byte_length(&self) -> usize {
        match &self.0 {
            Nested::Builder(builder, _) => builder.position(),
            Nested::OwnedBuilder(builder) => builder.position(),
            Nested::Bytes(bytes, _) => bytes.len(),
        }
    }

    /// The alignment that the table is padded to as a field: by default,
    /// its builder's largest alignment, or 4 for bytes
    const fn alignment(&self) -> Alignment {
        match &self.0 {
            Nested::Builder(_, alignment) | Nested::Bytes(_, alignment) => *alignment,
            Nested::OwnedBuilder(builder) => builder.current.alignment,
        }
    }

    /// The table, padded to start at a multiple of `alignment` as a field
    const fn aligned(mut self, alignment: Alignment) -> Self {
        match &mut self.0 {
            Nested::Builder(_, padded) | Nested::Bytes(_, padded) => *padded = alignment,
            Nested::OwnedBuilder(builder) => builder.current.alignment = alignment,
        }

        self
    }

    const fn check_complete(&self) -> Result<(), BuildError> {
        match &self.0 {
            Nested::Builder(builder, _) => builder.check_complete(),
            Nested::OwnedBuilder(builder) => builder.check_complete(),
            Nested::Bytes(..) => Ok(()),
        }
    }
}

impl From<TableBuilder> for NestedTable<'_> {
    fn from(builder: TableBuilder) -> Self {
        Self(Nested::OwnedBuilder(builder))
    }
}

impl<'a> From<&'a TableBuilder> for NestedTable<'a> {
    fn from(builder: &'a TableBuilder) -> Self {
        Self(Nested::Builder(builder, builder.current.alignment))
    }
}

impl<'a> From<Cow<'a, [u8]>> for NestedTable<'a> {
    fn from(bytes: Cow<'a, [u8]>) -> Self {
        Self(Nested::Bytes(bytes, Alignment::Four))
    }
}

impl<'a> From<&'a [u8]> for NestedTable<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Self::from(Cow::Borrowed(bytes))
    }
}

impl<'a, const N: usize> From<&'a [u8; N]> for NestedTable<'a> {
    fn from(bytes: &'a [u8; N]) -> Self {
        Self::from(bytes.as_slice())
    }
}

impl<'a> From<&'a Vec<u8>> for NestedTable<'a> {
    fn from(bytes: &'a Vec<u8>) -> Self {
        Self::from(bytes.as_slice())
    }
}

impl From<Vec<u8>> for NestedTable<'_> {
    fn from(bytes: Vec<u8>) -> Self {
        Self::from(Cow::Owned(bytes))
    }
}

/// A field's byte length with its padding and count, in u64, as the length
/// check takes it
const fn padded_length(padding: usize, width: usize, byte_length: usize) -> u64 {
    padding as u64 + width as u64 + byte_length as u64
}

/// Appends a count, length or header slot. A value is at most the table's
/// length, which the setter checked fits in the offsets.
#[allow(clippy::cast_possible_truncation)]
#[inline(always)]
fn push_slot(bytes: &mut Bytes, value: usize, offset_type: OffsetType) {
    match offset_type {
        OffsetType::Uint8 => bytes.push(&[value as u8]),
        OffsetType::Uint16 => bytes.push(&(value as u16).to_le_bytes()),
        OffsetType::Uint32 => bytes.push(&(value as u32).to_le_bytes()),
    }
}

/// Writes an offset, item end or length into the slot at `at`, which the
/// bytes hold, since the slots are written before their values, and panics
/// if they don't, rather than write a table without it. A value is at most
/// the table's length, which the setters checked fits in the offsets.
#[allow(clippy::cast_possible_truncation)]
#[inline(always)]
fn write_slot(bytes: &mut [u8], at: usize, value: usize, offset_type: OffsetType) {
    match offset_type {
        OffsetType::Uint8 => bytes[at] = value as u8,
        OffsetType::Uint16 => bytes[at..at + 2].copy_from_slice(&(value as u16).to_le_bytes()),
        OffsetType::Uint32 => bytes[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes()),
    }
}
