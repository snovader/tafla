// The setters, and what they call to write a field, are inlined into the
// caller: called out of line, the vector of 100 nested tables took 1.76 µs
// to write rather than 1.33 µs
#![allow(clippy::inline_always)]

mod bytes;
mod segments;

use std::{borrow::Cow, mem};

use bytes::Bytes;
use segments::{Run, Segment, Segments};

use crate::{Alignment, BuildError, Element, OffsetType, TableLayout};

/// The shortest run of a table that a builder keeps as a segment, rather
/// than copying it into its bytes when it's set: an array's elements, a
/// string, a struct, an array of structs or a table nested as bytes.
/// Copying every run when it's set, rather than these once into the table,
/// took 2.2 times as long for the benchmarks' document of 100 rows, and 2.3
/// times as long for 100,000.
///
/// Built with `--cfg tafla_segment_stress`, every run of a byte or more is
/// a segment, so that the tests write every value as one.
const SEGMENT_BYTES: usize = if cfg!(tafla_segment_stress) { 1 } else { 256 };

/// The most room that a vector written in place makes for its items after
/// its first, each as long as the first
const MOST_ROOM_FOR_ITEMS: usize = 64 * 1024;

/// The padding before a table nested in place, which is less than its
/// alignment, when it's kept as a segment
static PADDING: [u8; 7] = [0; 7];

/// Builds a table, which a `ByteBuffer` reads back.
///
/// A table is a header with one offset per field, after the table's total
/// length and version when its layout has them, followed by the fields in
/// the order they're set. Arrays, strings and vectors start with their
/// count, and a vector then holds the end of each item. Arrays of scalars
/// and of structs are padded so their elements are aligned from the start of
/// the table. Fields are read back by index with `ByteBuffer::root_offset()`,
/// or `ByteBuffer::field_offset()` for a table nested in another.
///
/// Each field is written when it's set, after the fields set before it, and
/// its offset with it. A vector writes its count and its items' ends, each
/// once its item is written. [`TableBuilder::to_bytes()`] copies the table
/// into bytes of its exact length. Runs of 256 bytes or more, of an array's
/// elements, a string, a struct, an array of structs or a table nested as
/// bytes, aren't copied when they're set: the builder borrows or owns them,
/// as it's given them, and `to_bytes()` copies them into the table, so that
/// a large column is copied once.
///
/// A nested table, or each item of a vector of them, can be written in
/// place, among this builder's own fields, with
/// [`TableBuilder::set_table_with()`] and
/// [`TableBuilder::set_vector_with()`], without a builder of its own.
///
/// A setter returns a [`BuildError`], and leaves the builder as it was, when
/// the table already holds all its fields or the field would make it too
/// long for the layout's offset type: longer than 255 bytes, 64 KiB or
/// 4 GiB. Errors are values rather than panics, since a WebAssembly build
/// that aborts on panic couldn't report them.
#[derive(Debug)]
pub struct TableBuilder<'a> {
    /// The table's bytes but for its segments
    bytes: Bytes,
    segments: Segments<'a>,
    /// Whether a table nested in place was padded with a segment before
    /// segments that it holds, which `to_bytes()` then sorts
    unordered: bool,
    /// The bytes that the table holds besides the builder's own: its
    /// segments', and those of the items that a vector in place dropped once
    /// it made the table too long. The table's length so far is
    /// `bytes.len() + shift`, in u64, which, unlike a 32-bit `usize`, holds
    /// the lengths of borrowed runs past 4 GiB, so that they're rejected.
    shift: u64,
    /// The table whose fields are being set: the root, a table nested in
    /// place or a vector's item
    current: Counters,
    sized: bool,
    offset_type: OffsetType,
}

/// What a builder counts of the table whose fields it's setting
#[derive(Clone, Copy, Debug)]
struct Counters {
    /// Where the table starts in the root, in u64 as the root's length is
    table: u64,
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
}

/// Where a builder was before a table or vector nested in place, which it
/// returns to if the table's `fill` fails
#[derive(Clone, Copy, Debug)]
struct Saved {
    counters: Counters,
    bytes: usize,
    segments: usize,
    shift: u64,
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
    ($($name:ident($type:ty) => $variant:ident),*) => {$(
        #[doc = concat!(
            "Sets an array field of `", stringify!($type), "`, from a slice that ",
            "the builder borrows or a `Vec` that it owns until the table is written"
        )]
        ///
        /// # Errors
        ///
        /// [`BuildError::TooManyFields`] if the table holds all its fields
        /// already, or [`BuildError::TooLong`] if the field would make it
        /// too long for its offset type.
        #[inline]
        pub fn $name(&mut self, values: impl Into<Cow<'a, [$type]>>) -> Result<(), BuildError> {
            self.set_array(values.into(), Run::$variant)
        }
    )*};
}

impl<'a> TableBuilder<'a> {
    /// Starts a table of `fields` fields in `layout`, which its readers must
    /// use too. Nested tables use [`TableLayout::DEFAULT`].
    ///
    /// # Errors
    ///
    /// [`BuildError::HeaderTooLong`] if the header alone is too long for the
    /// layout's offset type.
    #[inline]
    pub fn new(fields: usize, layout: TableLayout) -> Result<Self, BuildError> {
        let header = layout.header_length(fields)?;

        Ok(Self::create(fields, header, layout, None))
    }

    /// Starts a table of `fields` fields in `layout`, with `version` in its
    /// header. Readers can check it with `ByteBuffer::version()` before
    /// reading any field.
    ///
    /// # Errors
    ///
    /// [`BuildError::VersionTooLarge`] if `version` doesn't fit in the
    /// layout's offset type, or [`BuildError::HeaderTooLong`] if the header
    /// alone is too long for the layout's offset type.
    #[inline]
    pub fn versioned(
        fields: usize,
        layout: TableLayout<true>,
        version: u32,
    ) -> Result<Self, BuildError> {
        layout.check_version(version)?;
        let header = layout.header_length(fields)?;

        Ok(Self::create(fields, header, layout, Some(version)))
    }

    /// A builder of a table of `fields` fields, whose header of `header`
    /// bytes its layout checked, with the header written: zeros for its
    /// length, which `to_bytes()` writes, its version, and zeros for the
    /// offsets
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    fn create<const VERSIONED: bool>(
        fields: usize,
        header: u32,
        layout: TableLayout<VERSIONED>,
        version: Option<u32>,
    ) -> Self {
        let offset_type = layout.offset_type();
        let sized = layout.is_sized();

        let mut bytes = Bytes::for_table(header as usize, fields);
        if sized {
            bytes.push_zeros(offset_type.size());
        }
        if let Some(version) = version {
            push_slot(&mut bytes, version as usize, offset_type);
        }
        let slots = bytes.len();
        bytes.push_zeros(header as usize - slots);

        Self {
            bytes,
            segments: Segments::new(),
            unordered: false,
            shift: 0,
            current: Counters {
                table: 0,
                slots,
                // The header holds an offset of at least a byte per field
                fields: fields as u32,
                set: 0,
                alignment: Alignment::One,
            },
            sized,
            offset_type,
        }
    }

    /// Writes the table, at its exact size: its bytes, with its segments in
    /// their places and its length in a sized header.
    ///
    /// # Errors
    ///
    /// [`BuildError::Incomplete`] if fewer fields were set than the table
    /// holds.
    #[allow(clippy::cast_possible_truncation)]
    pub fn to_bytes(&self) -> Result<Vec<u8>, BuildError> {
        self.check_complete()?;

        // The setters checked that the length fits in the table's offsets,
        // and so in a `usize`
        let mut table = Vec::with_capacity(self.position() as usize);
        self.copy_into(&mut table);
        self.write_length(&mut table);

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
        set_i8_array(i8) => I8, set_u8_array(u8) => U8,
        set_i16_array(i16) => I16, set_u16_array(u16) => U16,
        set_i32_array(i32) => I32, set_u32_array(u32) => U32,
        set_i64_array(i64) => I64, set_u64_array(u64) => U64,
        set_f32_array(f32) => F32, set_f64_array(f64) => F64
    );

    /// Sets a string field, from a `&str` that the builder borrows or a
    /// `String` that it owns until the table is written
    ///
    /// # Errors
    ///
    /// [`BuildError::TooManyFields`] if the table holds all its fields
    /// already, or [`BuildError::TooLong`] if the field would make it too
    /// long for its offset type.
    #[inline(always)]
    pub fn set_str(&mut self, string: impl Into<Cow<'a, str>>) -> Result<(), BuildError> {
        let string = string.into();
        let length = string.len();
        self.check_field(self.width() as u64 + length as u64)?;

        self.write_offset(self.position());
        self.push_slot(length);
        self.push_bytes(str_bytes(string));
        self.current.set += 1;

        Ok(())
    }

    /// Sets a struct field from its bytes: its scalars, little-endian, one
    /// after another, which are written as they are, without padding. The
    /// builder borrows a slice or owns a `Vec` until the table is written.
    ///
    /// # Errors
    ///
    /// [`BuildError::TooManyFields`] if the table holds all its fields
    /// already, or [`BuildError::TooLong`] if the field would make it too
    /// long for its offset type.
    #[inline(always)]
    pub fn set_struct(&mut self, bytes: impl Into<Cow<'a, [u8]>>) -> Result<(), BuildError> {
        let bytes = bytes.into();
        self.check_field(bytes.len() as u64)?;

        self.write_offset(self.position());
        self.push_bytes(bytes);
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
    pub fn set_struct_array(
        &mut self,
        bytes: impl Into<Cow<'a, [u8]>>,
        size: usize,
        alignment: Alignment,
    ) -> Result<(), BuildError> {
        let bytes = bytes.into();
        let length = bytes.len();
        if size == 0 || !length.is_multiple_of(size) {
            return Err(BuildError::NotWholeStructs { length, size });
        }

        let width = self.width();
        let padding = padding_at(alignment, self.table_length() + width as u64);
        self.check_field(padded_length(padding, width, length))?;

        self.bytes.push_zeros(padding);
        self.write_offset(self.position());
        self.push_slot(length / size);
        self.push_bytes(bytes);
        self.count(alignment);

        Ok(())
    }

    /// Embeds a table, from its builder or its bytes, without a length prefix.
    /// Readers find its fields with `ByteBuffer::field_offset()` from the
    /// field's position, which reads offsets of the parent's offset type, so
    /// the table should have no header and the parent's offset type: that's
    /// [`TableLayout::DEFAULT`] in a parent of `uint32` offsets.
    ///
    /// A builder, borrowed or owned, must have all its fields set. Its bytes
    /// are copied into this table, in its own layout, with its length in a
    /// sized header, and the runs that it keeps as segments are kept by this
    /// builder: borrowed from a borrowed builder, and taken from an owned
    /// one.
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
    pub fn set_table(&mut self, table: impl Into<NestedTable<'a>>) -> Result<(), BuildError> {
        self.add_table(table.into())
    }

    /// Embeds a table like [`TableBuilder::set_table()`], padded to start at
    /// a multiple of `alignment`, such as [`Alignment::Eight`] for a table
    /// of 64-bit arrays given as bytes
    ///
    /// # Errors
    ///
    /// The errors of [`TableBuilder::set_table()`].
    #[inline]
    pub fn set_table_aligned(
        &mut self,
        table: impl Into<NestedTable<'a>>,
        alignment: Alignment,
    ) -> Result<(), BuildError> {
        self.add_table(table.into().aligned(alignment))
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
    pub fn set_vector<I>(&mut self, items: I) -> Result<(), BuildError>
    where
        I: IntoIterator,
        I::Item: Into<NestedTable<'a>>,
    {
        let items: Vec<_> = items.into_iter().map(Into::into).collect();
        for item in &items {
            item.check_complete()?;
        }

        let width = self.width();
        let byte_length = items.iter().fold(width as u64, |length, item| {
            length.saturating_add(width as u64 + item.byte_length())
        });
        self.check_field(byte_length)?;

        let vector = self.position();
        self.write_offset(vector);
        let ends = self.bytes.len() + width;
        self.push_slot(items.len());
        self.bytes.push_zeros(items.len() * width);

        for (index, item) in items.into_iter().enumerate() {
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
    /// ends, and padded once its fields are set.
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
        F: FnOnce(&mut InPlaceTable<'_, 'a>) -> Result<(), E>,
    {
        // Room for the field, and its table's header, before `fill` runs
        self.check_field(0)?;
        let header = self.in_place_header(fields)?;

        let checkpoint = Checkpoint::new(self);
        let builder = &mut *checkpoint.builder;
        // The header holds an offset of at least a byte per field
        builder.start_in_place(fields as u32, header);
        fill(&mut InPlaceTable {
            builder: &mut *builder,
        })?;
        builder.check_complete()?;

        // Padded from this table's length where the nested table started,
        // as a nested builder is
        let table = builder.current;
        builder.current = checkpoint.saved.counters;
        let padding = padding_at(table.alignment, table.table - builder.current.table);
        builder.check_field(padding as u64)?;
        if padding != 0 {
            builder.pad_in_place(padding, checkpoint.saved);
        }

        builder.write_offset(table.table + padding as u64);
        builder.count(table.alignment);
        checkpoint.commit();

        Ok(())
    }

    /// Writes a vector of `count` tables of `fields` fields each, in place,
    /// as [`TableBuilder::set_table_with()`] writes one: `fill(index, item)`
    /// sets the fields of item `index`, for each in order. It writes the
    /// bytes that [`TableBuilder::set_vector()`] writes from a builder per
    /// item, without their allocations. Items aren't padded, as
    /// `set_vector()` doesn't pad them. `fill` runs for every item, even
    /// once the items make the table too long, which the vector then fails
    /// with.
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
    /// let table = builder.to_bytes()?;
    /// # Ok::<(), BuildError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// [`BuildError::TooManyFields`] if this table holds all its fields
    /// already, or [`BuildError::TooLong`] if the vector's count and its
    /// items' ends make it too long, before any item is written, and
    /// otherwise the errors of [`TableBuilder::set_table_with()`] for each
    /// item, or [`BuildError::TooLong`] if the items make the table too long.
    /// Each leaves the builder as it was, as a panic in `fill` does.
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
        F: FnMut(usize, &mut InPlaceTable<'_, 'a>) -> Result<(), E>,
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
        let parent = builder.current;
        let vector = builder.position();
        builder.write_offset(vector);
        let ends = builder.bytes.len() + width;
        builder.push_slot(count);
        builder.bytes.push_zeros(count * width);

        let max_length = u64::from(builder.offset_type.max_value());
        for index in 0..count {
            let item = builder.saved();
            // The header holds an offset of at least a byte per field
            builder.start_in_place(fields as u32, header);
            fill(
                index,
                &mut InPlaceTable {
                    builder: &mut *builder,
                },
            )?;
            builder.check_complete()?;

            builder.current = parent;
            builder.write_item_end(ends, index, vector);
            // Once the table is too long, the items that follow, which fail
            // with it, are kept only as lengths. After the first item, there's
            // room for the others, up to 64 KiB, each as long.
            if builder.table_length() > max_length {
                builder.keep_length_only(item);
            } else if index == 0 {
                builder.reserve_items_like(builder.bytes.len() - item.bytes, count - 1);
            }
        }

        // The items can make the table too long
        builder.check_field(0)?;
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

    /// Sets an array field, whose elements are kept as `run` when they're a
    /// segment's bytes or more
    #[inline(always)]
    fn set_array<T: Element>(
        &mut self,
        values: Cow<'a, [T]>,
        run: fn(Cow<'a, [T]>) -> Run<'a>,
    ) -> Result<(), BuildError> {
        let width = self.width();
        let padding = padding_at(T::ALIGNMENT, self.table_length() + width as u64);
        let byte_length = size_of_val::<[T]>(&values);
        self.check_field(padded_length(padding, width, byte_length))?;

        self.bytes.push_zeros(padding);
        self.write_offset(self.position());
        self.push_slot(values.len());
        if byte_length >= SEGMENT_BYTES {
            self.push_segment(run(values), byte_length);
        } else {
            self.bytes.push_elements(&values);
        }
        self.count(T::ALIGNMENT);

        Ok(())
    }

    fn add_table(&mut self, table: NestedTable<'a>) -> Result<(), BuildError> {
        table.check_complete()?;
        let alignment = table.alignment();
        let padding = padding_at(alignment, self.table_length());
        self.check_field(padding as u64 + table.byte_length())?;

        self.bytes.push_zeros(padding);
        self.write_offset(self.position());
        self.append(table);
        self.count(alignment);

        Ok(())
    }

    /// Appends a nested table's bytes, after its offset or before its end
    fn append(&mut self, table: NestedTable<'a>) {
        match table.0 {
            Nested::Builder(builder, _) => self.append_builder(builder),
            Nested::OwnedBuilder(builder) => self.append_owned_builder(builder),
            Nested::Bytes(bytes, _) => self.push_bytes(bytes),
        }
    }

    /// Appends a borrowed builder's bytes, and its segments, which borrow
    /// what it owns
    fn append_builder(&mut self, builder: &'a Self) {
        let start = self.bytes.len();
        self.bytes.push(builder.bytes.as_slice());
        builder.write_length(&mut self.bytes.as_mut_slice()[start..]);

        let count = builder.segments.len();
        for (index, segment) in builder.segments.iter().enumerate() {
            let segment = Segment {
                at: start + segment.at,
                run: segment.run.reborrow(),
            };
            self.segments.push(segment, count - index);
        }
        self.unordered |= builder.unordered;
        self.shift += builder.shift;
    }

    /// Appends an owned builder's bytes, and takes its segments
    fn append_owned_builder(&mut self, builder: Self) {
        let start = self.bytes.len();
        self.bytes.push(builder.bytes.as_slice());
        builder.write_length(&mut self.bytes.as_mut_slice()[start..]);

        let count = builder.segments.len();
        for (index, segment) in builder.segments.into_iter().enumerate() {
            let segment = Segment {
                at: start + segment.at,
                run: segment.run,
            };
            self.segments.push(segment, count - index);
        }
        self.unordered |= builder.unordered;
        self.shift += builder.shift;
    }

    /// Appends bytes, as a segment if they're long enough
    #[inline(always)]
    fn push_bytes(&mut self, bytes: Cow<'a, [u8]>) {
        let byte_length = bytes.len();
        if byte_length >= SEGMENT_BYTES {
            self.push_segment(Run::U8(bytes), byte_length);
        } else {
            self.bytes.push(&bytes);
        }
    }

    /// Keeps a run of `byte_length` bytes as a segment, at the end of the
    /// bytes so far, with room past the first few segments for one for each
    /// field left to set
    #[inline(always)]
    fn push_segment(&mut self, run: Run<'a>, byte_length: usize) {
        let segment = Segment {
            at: self.bytes.len(),
            run,
        };
        let fields_left = self.current.fields - self.current.set;
        self.segments.push(segment, fields_left as usize);
        self.shift += byte_length as u64;
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
    fn start_in_place(&mut self, fields: u32, header: u32) {
        let table = self.position();
        let slots = self.bytes.len();
        self.bytes.push_zeros(header as usize);

        self.current = Counters {
            table,
            slots,
            fields,
            set: 0,
            alignment: Alignment::One,
        };
    }

    /// Pads the table written in place since `saved` with `padding` zeros
    /// before it. A table of fewer bytes than a segment is moved forward,
    /// with its segments. A longer one stays, and its padding is kept as a
    /// segment before it, which `to_bytes()` sorts into place: moved, tables
    /// nested in place one in another, each padded, would each be moved
    /// again by every table that holds it. A table of the benchmarks never
    /// needs padding, so it's out of line.
    #[cold]
    #[inline(never)]
    fn pad_in_place(&mut self, padding: usize, saved: Saved) {
        let (start, end) = (saved.bytes, self.bytes.len());
        if end - start < SEGMENT_BYTES {
            self.bytes.push_zeros(padding);
            let bytes = self.bytes.as_mut_slice();
            bytes.copy_within(start..end, start + padding);
            bytes[start..start + padding].fill(0);
            self.segments.shift_from(saved.segments, padding);
        } else {
            let segment = Segment {
                at: start,
                run: Run::U8(Cow::Borrowed(&PADDING[..padding])),
            };
            self.unordered |= self.segments.len() > saved.segments;
            self.segments.push(segment, 1);
            self.shift += padding as u64;
        }
    }

    /// Makes room, after a vector's first item wrote `first` bytes, for its
    /// `others`, as many bytes each, up to [`MOST_ROOM_FOR_ITEMS`], if it
    /// can: an uneven vector, whose first item is its longest, would
    /// otherwise make room for many times its length
    fn reserve_items_like(&mut self, first: usize, others: usize) {
        let room = first.saturating_mul(others);
        self.bytes.try_reserve(room.min(MOST_ROOM_FOR_ITEMS));
    }

    /// Drops the bytes and segments of a vector's item written since
    /// `item`, keeping its length in the shift, once the vector has made its
    /// table too long, which then fails. Its other items still run, as their
    /// fills and errors would, but the builder holds no more of their bytes
    /// than one item's, rather than every item's until the table fails.
    #[cold]
    #[inline(never)]
    fn keep_length_only(&mut self, item: Saved) {
        let length = self.position() - (item.bytes as u64 + item.shift);
        self.bytes.truncate(item.bytes);
        self.segments.truncate(item.segments);
        self.shift = item.shift + length;
    }

    /// Writes the offset of the next field, which starts at `position`
    #[inline(always)]
    fn write_offset(&mut self, position: u64) {
        let slot = self.current.slots + self.current.set as usize * self.width();
        let offset = position - self.current.table;
        write_slot(self.bytes.as_mut_slice(), slot, offset, self.offset_type);
    }

    /// Writes the end of item `index` of the vector at `vector`, whose ends
    /// start at `ends` in the builder's bytes, where the bytes so far end
    #[inline(always)]
    fn write_item_end(&mut self, ends: usize, index: usize, vector: u64) {
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
    /// that's the one value to check.
    #[inline(always)]
    fn check_field(&self, byte_length: u64) -> Result<(), BuildError> {
        let Counters { fields, set, .. } = self.current;
        if set == fields {
            return Err(BuildError::TooManyFields {
                fields: fields as usize,
            });
        }

        let length = self.table_length().saturating_add(byte_length);
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
    const fn position(&self) -> u64 {
        self.bytes.len() as u64 + self.shift
    }

    /// The length so far of the table whose fields are being set
    #[inline(always)]
    const fn table_length(&self) -> u64 {
        self.position() - self.current.table
    }

    /// Where the builder is, which [`Checkpoint`] returns it to, and which a
    /// vector's item is dropped back to
    #[inline]
    const fn saved(&self) -> Saved {
        Saved {
            counters: self.current,
            bytes: self.bytes.len(),
            segments: self.segments.len(),
            shift: self.shift,
        }
    }

    /// Copies the table's bytes, with its segments in their places
    fn copy_into(&self, to: &mut Vec<u8>) {
        if self.unordered {
            let mut segments: Vec<_> = self.segments.iter().collect();
            // A stable sort, which keeps a table's padding after the
            // segments before the table, at the same byte
            segments.sort_by_key(|segment| segment.at);
            self.copy_with(segments, to);
        } else {
            self.copy_with(self.segments.iter(), to);
        }
    }

    /// Copies the table's bytes, with `segments`, in order, in their places
    #[inline]
    fn copy_with<'s>(&self, segments: impl IntoIterator<Item = &'s Segment<'a>>, to: &mut Vec<u8>)
    where
        'a: 's,
    {
        let bytes = self.bytes.as_slice();
        let mut from = 0;
        for segment in segments {
            to.extend_from_slice(&bytes[from..segment.at]);
            segment.run.copy_into(to);
            from = segment.at;
        }
        to.extend_from_slice(&bytes[from..]);
    }

    /// Writes the table's length into a sized header at the start of `table`
    fn write_length(&self, table: &mut [u8]) {
        if self.sized {
            write_slot(table, 0, self.position(), self.offset_type);
        }
    }

    /// Returns to where the builder was before a table or vector nested in
    /// place
    fn restore(&mut self, saved: Saved) {
        self.bytes.truncate(saved.bytes);
        self.segments.truncate(saved.segments);
        self.shift = saved.shift;
        self.current = saved.counters;
    }
}

/// A table or vector being written in place. Until it's committed, dropping
/// it, as an error or a panic does, returns the builder to where it was.
struct Checkpoint<'g, 'a> {
    builder: &'g mut TableBuilder<'a>,
    saved: Saved,
}

impl<'g, 'a> Checkpoint<'g, 'a> {
    #[inline]
    const fn new(builder: &'g mut TableBuilder<'a>) -> Self {
        let saved = builder.saved();

        Self { builder, saved }
    }

    /// Keeps what was written
    #[inline]
    const fn commit(self) {
        mem::forget(self);
    }
}

impl Drop for Checkpoint<'_, '_> {
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
pub struct InPlaceTable<'b, 'a> {
    builder: &'b mut TableBuilder<'a>,
}

/// Setters of an [`InPlaceTable`], which are the builder's
macro_rules! in_place_setters {
    ($($name:ident($($argument:ident: $type:ty),*)),* $(,)?) => {$(
        #[doc = concat!("Sets a field as [`TableBuilder::", stringify!($name), "()`] does")]
        ///
        /// # Errors
        ///
        #[doc = concat!("The errors of [`TableBuilder::", stringify!($name), "()`].")]
        #[inline(always)]
        pub fn $name(&mut self, $($argument: $type),*) -> Result<(), BuildError> {
            self.builder.$name($($argument),*)
        }
    )*};
}

impl<'a> InPlaceTable<'_, 'a> {
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
        set_i8_array(values: impl Into<Cow<'a, [i8]>>),
        set_u8_array(values: impl Into<Cow<'a, [u8]>>),
        set_i16_array(values: impl Into<Cow<'a, [i16]>>),
        set_u16_array(values: impl Into<Cow<'a, [u16]>>),
        set_i32_array(values: impl Into<Cow<'a, [i32]>>),
        set_u32_array(values: impl Into<Cow<'a, [u32]>>),
        set_i64_array(values: impl Into<Cow<'a, [i64]>>),
        set_u64_array(values: impl Into<Cow<'a, [u64]>>),
        set_f32_array(values: impl Into<Cow<'a, [f32]>>),
        set_f64_array(values: impl Into<Cow<'a, [f64]>>),
        set_str(string: impl Into<Cow<'a, str>>),
        set_struct(bytes: impl Into<Cow<'a, [u8]>>),
        set_struct_array(bytes: impl Into<Cow<'a, [u8]>>, size: usize, alignment: Alignment),
        set_table(table: impl Into<NestedTable<'a>>),
        set_table_aligned(table: impl Into<NestedTable<'a>>, alignment: Alignment),
    );

    /// Sets a vector field as [`TableBuilder::set_vector()`] does
    ///
    /// # Errors
    ///
    /// The errors of [`TableBuilder::set_vector()`].
    pub fn set_vector<I>(&mut self, items: I) -> Result<(), BuildError>
    where
        I: IntoIterator,
        I::Item: Into<NestedTable<'a>>,
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
        F: FnOnce(&mut InPlaceTable<'_, 'a>) -> Result<(), E>,
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
        F: FnMut(usize, &mut InPlaceTable<'_, 'a>) -> Result<(), E>,
    {
        self.builder.set_vector_with(count, fields, fill)
    }
}

/// A table to nest with [`TableBuilder::set_table()`] or in
/// [`TableBuilder::set_vector()`]: a builder, borrowed or owned, or bytes.
///
/// A builder's bytes are copied into its parent, and its segments kept by
/// it. Bytes are copied, or kept as a segment when they're long.
#[derive(Debug)]
pub struct NestedTable<'a>(Nested<'a>);

/// A table to nest, and the alignment that it's padded to as a field. An
/// owned builder keeps that as its own alignment.
///
/// An owned builder is held as it is, rather than boxed, which would add an
/// allocation for each table nested from one, to the allocation of its
/// bytes.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Nested<'a> {
    Builder(&'a TableBuilder<'a>, Alignment),
    OwnedBuilder(TableBuilder<'a>),
    Bytes(Cow<'a, [u8]>, Alignment),
}

impl NestedTable<'_> {
    fn byte_length(&self) -> u64 {
        match &self.0 {
            Nested::Builder(builder, _) => builder.position(),
            Nested::OwnedBuilder(builder) => builder.position(),
            Nested::Bytes(bytes, _) => bytes.len() as u64,
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

impl<'a> From<TableBuilder<'a>> for NestedTable<'a> {
    fn from(builder: TableBuilder<'a>) -> Self {
        Self(Nested::OwnedBuilder(builder))
    }
}

impl<'b, 'a: 'b> From<&'b TableBuilder<'a>> for NestedTable<'b> {
    fn from(builder: &'b TableBuilder<'a>) -> Self {
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

/// A string's UTF-8 bytes, borrowed or owned as the string is
fn str_bytes(string: Cow<'_, str>) -> Cow<'_, [u8]> {
    match string {
        Cow::Borrowed(string) => Cow::Borrowed(string.as_bytes()),
        Cow::Owned(string) => Cow::Owned(string.into_bytes()),
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
/// the table's length, which the setters checked fits in the offsets, but
/// for the item ends of a vector that makes its table too long, which then
/// fails.
#[allow(clippy::cast_possible_truncation)]
#[inline(always)]
fn write_slot(bytes: &mut [u8], at: usize, value: u64, offset_type: OffsetType) {
    match offset_type {
        OffsetType::Uint8 => bytes[at] = value as u8,
        OffsetType::Uint16 => bytes[at..at + 2].copy_from_slice(&(value as u16).to_le_bytes()),
        OffsetType::Uint32 => bytes[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes()),
    }
}

/// The padding that brings `offset` to a multiple of `alignment`. Only the
/// offset's remainder matters, which the low bits of a `usize` keep.
#[allow(clippy::cast_possible_truncation)]
#[inline(always)]
const fn padding_at(alignment: Alignment, offset: u64) -> usize {
    alignment.padding(offset as usize)
}
