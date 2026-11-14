// The setters, and what they call to write a field, are inlined into the
// caller, as C++'s prototype found that they must be
#![allow(clippy::inline_always)]

use std::{borrow::Cow, mem};

use super::{
    buffer::{Buffer, VecBuffer},
    segments::{Run, Segment, Segments},
};
use crate::{Alignment, BuildError, Element, OffsetType, TableLayout};

/// The bytes of the proposed design
pub type DefaultBuffer = VecBuffer;

/// The segments that the proposed design keeps within the builder
const FIRST_SEGMENTS: usize = 4;

/// Builds a table as [`crate::TableBuilder`] does, with its setters, bytes
/// and errors, but writes each field's bytes when it's set, rather than
/// recording the field and writing every field in `to_bytes()`.
///
/// Each field's offset is written as the field is set, from the bytes
/// written so far. A vector writes its count and room for its items' ends,
/// and the end of each item once the item is written. A table nested in
/// place with [`DirectTableBuilder::set_table_with()`] is written where its
/// parent is, unpadded, and once its fields are set, and so its alignment
/// known, it's moved forward by its padding; its offsets count from its own
/// start, so none of them changes. A nested builder is copied in, as its
/// bytes and segments, whatever its layout.
///
/// Runs of `B::SEGMENT_BYTES` bytes or more, an array's elements, a string,
/// a struct, an array of structs or a table nested as bytes, borrowed or
/// owned, are kept as segments rather than copied, and `to_bytes()` copies
/// them between the builder's own bytes, so a large borrowed column is
/// copied once, into the table. The first `N` segments are kept within the
/// builder.
///
/// A setter checks the field before it writes anything, and returns the
/// error that `TableBuilder`'s does. A table or vector nested in place whose
/// `fill` fails or panics is removed by truncating the bytes and the
/// segments to where they were before it.
#[derive(Debug)]
pub struct DirectTableBuilder<'a, B: Buffer = DefaultBuffer, const N: usize = FIRST_SEGMENTS> {
    bytes: B,
    segments: Segments<'a, N>,
    /// The bytes of the segments so far, which the table holds before the
    /// builder's own bytes after them: the table's length so far is
    /// `bytes.len() + shift`
    shift: usize,
    /// The table whose fields are being set: the root, or a table nested in
    /// place, or a vector's item
    current: Counters,
    sized: bool,
    offset_type: OffsetType,
}

/// What a builder counts of the table whose fields it's setting
#[derive(Clone, Copy, Debug)]
struct Counters {
    /// Where the table starts in the root
    table: usize,
    /// Where its first offset lies in the builder's bytes
    slots: usize,
    /// The number of fields that the table holds
    fields: u32,
    /// The number of fields set so far
    set: u32,
    /// The largest alignment that a field needs from the table's start
    alignment: Alignment,
}

/// Where a builder was before a table or vector nested in place, which it
/// returns to if the table's `fill` fails
#[derive(Clone, Copy, Debug)]
struct Saved {
    counters: Counters,
    bytes: usize,
    segments: usize,
    shift: usize,
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
            "the builder borrows or a `Vec` that it owns"
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

impl DirectTableBuilder<'_> {
    /// Starts a table of `fields` fields in `layout`, as
    /// `TableBuilder::new()` does
    ///
    /// # Errors
    ///
    /// [`BuildError::HeaderTooLong`] if the header alone is too long for the
    /// layout's offset type.
    #[inline]
    pub fn new(fields: usize, layout: TableLayout) -> Result<Self, BuildError> {
        Self::with_buffer(fields, layout)
    }

    /// Starts a table of `fields` fields in `layout`, with `version` in its
    /// header, as `TableBuilder::versioned()` does
    ///
    /// # Errors
    ///
    /// [`BuildError::VersionTooLarge`] if `version` doesn't fit in the
    /// layout's offset type, or [`BuildError::HeaderTooLong`] if the header
    /// alone is too long for it.
    #[inline]
    pub fn versioned(
        fields: usize,
        layout: TableLayout<true>,
        version: u32,
    ) -> Result<Self, BuildError> {
        Self::versioned_with_buffer(fields, layout, version)
    }
}

impl<'a, B: Buffer, const N: usize> DirectTableBuilder<'a, B, N> {
    /// Starts a table of `fields` fields in `layout`, in bytes of type `B`
    ///
    /// # Errors
    ///
    /// [`BuildError::HeaderTooLong`] if the header alone is too long for the
    /// layout's offset type.
    #[inline]
    pub fn with_buffer(fields: usize, layout: TableLayout) -> Result<Self, BuildError> {
        let header = layout.header_length(fields)?;

        Ok(Self::create(fields, header, layout, None))
    }

    /// Starts a table of `fields` fields in `layout`, with `version` in its
    /// header, in bytes of type `B`
    ///
    /// # Errors
    ///
    /// [`BuildError::VersionTooLarge`] if `version` doesn't fit in the
    /// layout's offset type, or [`BuildError::HeaderTooLong`] if the header
    /// alone is too long for it.
    #[inline]
    pub fn versioned_with_buffer(
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

        let mut bytes = B::new();
        bytes.reserve(header as usize);
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

    /// Writes the table into new bytes of its exact length, with its
    /// segments in their places and its length in a sized header
    ///
    /// # Errors
    ///
    /// [`BuildError::Incomplete`] if fewer fields were set than the table
    /// holds.
    // Out of line, as `TableBuilder::to_bytes()` is compiled in the
    // comparison, whichever builder calls it
    #[inline(never)]
    pub fn to_bytes(&self) -> Result<Vec<u8>, BuildError> {
        self.check_complete()?;

        let mut table = Vec::with_capacity(self.position());
        self.copy_into(&mut table);
        self.write_length(&mut table);

        Ok(table)
    }

    /// Returns the table, as [`DirectTableBuilder::to_bytes()`] does, but
    /// hands over the builder's own bytes, rather than copying them, when
    /// they're a `Vec` that holds the whole table, with no segments. Their
    /// capacity can then exceed their length.
    ///
    /// # Errors
    ///
    /// [`BuildError::Incomplete`] if fewer fields were set than the table
    /// holds.
    pub fn into_bytes(mut self) -> Result<Vec<u8>, BuildError> {
        self.check_complete()?;

        if self.segments.is_empty() {
            let length = self.position();
            if let Some(mut table) = self.bytes.take_vec() {
                if self.sized {
                    write_slot(&mut table, 0, length, self.offset_type);
                }
                return Ok(table);
            }
        }

        self.to_bytes()
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
    /// `String` that it owns
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

    /// Sets a struct field from its bytes, as `TableBuilder::set_struct()`
    /// does
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

    /// Sets an array of structs of `size` bytes each, as
    /// `TableBuilder::set_struct_array()` does
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
        let padding = alignment.padding(self.table_length() + width);
        self.check_field(padded_length(padding, width, length))?;

        self.bytes.push_zeros(padding);
        self.write_offset(self.position());
        self.push_slot(length / size);
        self.push_bytes(bytes);
        self.count(alignment);

        Ok(())
    }

    /// Embeds a table, from its builder or its bytes, as
    /// `TableBuilder::set_table()` does. A builder's bytes and segments are
    /// copied in, whatever its layout.
    ///
    /// # Errors
    ///
    /// [`BuildError::Incomplete`] if the builder has fields left to set, or
    /// an error of any setter.
    #[inline]
    pub fn set_table(
        &mut self,
        table: impl Into<DirectNestedTable<'a, B, N>>,
    ) -> Result<(), BuildError> {
        self.add_table(table.into())
    }

    /// Embeds a table like [`DirectTableBuilder::set_table()`], padded to
    /// start at a multiple of `alignment`
    ///
    /// # Errors
    ///
    /// The errors of [`DirectTableBuilder::set_table()`].
    #[inline]
    pub fn set_table_aligned(
        &mut self,
        table: impl Into<DirectNestedTable<'a, B, N>>,
        alignment: Alignment,
    ) -> Result<(), BuildError> {
        self.add_table(table.into().aligned(alignment))
    }

    /// Writes a vector of tables, from their builders or their bytes, as
    /// `TableBuilder::set_vector()` does
    ///
    /// # Errors
    ///
    /// [`BuildError::Incomplete`] if a builder has fields left to set, or an
    /// error of any setter.
    pub fn set_vector<I>(&mut self, items: I) -> Result<(), BuildError>
    where
        I: IntoIterator,
        I::Item: Into<DirectNestedTable<'a, B, N>>,
    {
        let items: Vec<_> = items.into_iter().map(Into::into).collect();
        for item in &items {
            item.check_complete()?;
        }

        let width = self.width();
        let byte_length = items.iter().fold(width as u64, |length, item| {
            length.saturating_add(width as u64 + item.byte_length() as u64)
        });
        self.check_field(byte_length)?;

        let vector = self.position();
        self.write_offset(vector);
        let ends = self.bytes.len() + width;
        self.push_slot(items.len());
        self.bytes.push_zeros(items.len() * width);

        for (index, item) in items.into_iter().enumerate() {
            self.append(item);
            let end = self.position() - vector;
            let slot = ends + index * width;
            write_slot(self.bytes.as_mut_slice(), slot, end, self.offset_type);
        }
        self.current.set += 1;

        Ok(())
    }

    /// Writes a table of `fields` fields nested in this one, in place, as
    /// `TableBuilder::set_table_with()` records one: `fill` sets its fields,
    /// in order, on the [`DirectInPlaceTable`] that it's given. It's written
    /// where this table is, unpadded, and then moved forward by the padding
    /// that its fields' alignment needs.
    ///
    /// # Errors
    ///
    /// The errors of `TableBuilder::set_table_with()`, which each leave the
    /// builder as it was, as a panic in `fill` does.
    #[allow(clippy::cast_possible_truncation)]
    #[inline(always)]
    pub fn set_table_with<E, F>(&mut self, fields: usize, fill: F) -> Result<(), E>
    where
        E: From<BuildError>,
        F: FnOnce(&mut DirectInPlaceTable<'_, 'a, B, N>) -> Result<(), E>,
    {
        // Room for the field, and its table's header, before `fill` runs
        self.check_field(0)?;
        let header = self.in_place_header(fields)?;

        let checkpoint = Checkpoint::new(self);
        let builder = &mut *checkpoint.builder;
        // The header holds an offset of at least a byte per field
        builder.start_in_place(fields as u32, header);
        fill(&mut DirectInPlaceTable {
            builder: &mut *builder,
        })?;
        builder.check_complete()?;

        // Padded from this table's length where the nested table started,
        // as a nested builder is
        let table = builder.current;
        builder.current = checkpoint.saved.counters;
        let padding = table.alignment.padding(table.table - builder.current.table);
        builder.check_field(padding as u64)?;
        if padding != 0 {
            builder.move_by(padding, checkpoint.saved);
        }

        builder.write_offset(table.table + padding);
        builder.count(table.alignment);
        checkpoint.commit();

        Ok(())
    }

    /// Writes a vector of `count` tables of `fields` fields each, in place,
    /// as `TableBuilder::set_vector_with()` records one: `fill(index, item)`
    /// sets the fields of item `index`, for each in order. After the first
    /// item, it makes room for the others, each as long as the first.
    ///
    /// # Errors
    ///
    /// The errors of `TableBuilder::set_vector_with()`, which each leave the
    /// builder as it was, as a panic in `fill` does.
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
        F: FnMut(usize, &mut DirectInPlaceTable<'_, 'a, B, N>) -> Result<(), E>,
    {
        // The count and the items' ends, which fail before any item is
        // written
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

        for index in 0..count {
            let start = builder.bytes.len();
            // The header holds an offset of at least a byte per field
            builder.start_in_place(fields as u32, header);
            fill(
                index,
                &mut DirectInPlaceTable {
                    builder: &mut *builder,
                },
            )?;
            builder.check_complete()?;
            builder.current = parent;

            let end = builder.position() - vector;
            let slot = ends + index * width;
            write_slot(builder.bytes.as_mut_slice(), slot, end, builder.offset_type);
            if index == 0 {
                builder.reserve_items_like(builder.bytes.len() - start, count - 1);
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
        let padding = T::ALIGNMENT.padding(self.table_length() + width);
        let byte_length = size_of_val::<[T]>(&values);
        self.check_field(padded_length(padding, width, byte_length))?;

        self.bytes.push_zeros(padding);
        self.write_offset(self.position());
        self.push_slot(values.len());
        if byte_length >= B::SEGMENT_BYTES {
            self.push_segment(run(values), byte_length);
        } else {
            self.bytes.push_elements(&values);
        }
        self.count(T::ALIGNMENT);

        Ok(())
    }

    fn add_table(&mut self, table: DirectNestedTable<'a, B, N>) -> Result<(), BuildError> {
        table.check_complete()?;
        let alignment = table.alignment();
        let padding = alignment.padding(self.table_length());
        self.check_field(padding as u64 + table.byte_length() as u64)?;

        self.bytes.push_zeros(padding);
        self.write_offset(self.position());
        self.append(table);
        self.count(alignment);

        Ok(())
    }

    /// Appends a nested table's bytes, after its offset or before its end
    fn append(&mut self, table: DirectNestedTable<'a, B, N>) {
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

        for segment in builder.segments.iter() {
            self.segments.push(Segment {
                at: start + segment.at,
                run: segment.run.reborrow(),
            });
        }
        self.shift += builder.shift;
    }

    /// Appends an owned builder's bytes, and takes its segments
    fn append_owned_builder(&mut self, builder: Self) {
        let start = self.bytes.len();
        self.bytes.push(builder.bytes.as_slice());
        builder.write_length(&mut self.bytes.as_mut_slice()[start..]);

        for segment in builder.segments {
            self.segments.push(Segment {
                at: start + segment.at,
                run: segment.run,
            });
        }
        self.shift += builder.shift;
    }

    /// Appends bytes, as a segment if they're long enough
    #[inline(always)]
    fn push_bytes(&mut self, bytes: Cow<'a, [u8]>) {
        let byte_length = bytes.len();
        if byte_length >= B::SEGMENT_BYTES {
            self.push_segment(Run::U8(bytes), byte_length);
        } else {
            self.bytes.push(&bytes);
        }
    }

    /// Keeps a run of `byte_length` bytes as a segment, at the end of the
    /// bytes so far
    #[inline(always)]
    fn push_segment(&mut self, run: Run<'a>, byte_length: usize) {
        self.segments.push(Segment {
            at: self.bytes.len(),
            run,
        });
        self.shift += byte_length;
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

    /// Moves the table written in place since `saved` forward by `padding`,
    /// with its segments, and zeros the padding before it
    #[cold]
    #[inline(never)]
    fn move_by(&mut self, padding: usize, saved: Saved) {
        let (start, end) = (saved.bytes, self.bytes.len());
        self.bytes.push_zeros(padding);

        let bytes = self.bytes.as_mut_slice();
        bytes.copy_within(start..end, start + padding);
        bytes[start..start + padding].fill(0);
        self.segments.shift_from(saved.segments, padding);
    }

    /// Makes room, after a vector's first item wrote `first` bytes, for its
    /// `others`, as many bytes each, but for no more than the table's
    /// offsets address
    fn reserve_items_like(&mut self, first: usize, others: usize) {
        let room = first.saturating_mul(others);
        self.bytes
            .reserve(room.min(self.offset_type.max_value() as usize));
    }

    /// Writes the offset of the next field, which starts at `position`
    #[inline(always)]
    fn write_offset(&mut self, position: usize) {
        let slot = self.current.slots + self.current.set as usize * self.width();
        let offset = position - self.current.table;
        write_slot(self.bytes.as_mut_slice(), slot, offset, self.offset_type);
    }

    /// Counts a field that needs `alignment` from the table's start
    #[inline(always)]
    fn count(&mut self, alignment: Alignment) {
        self.current.set += 1;
        self.current.alignment = self.current.alignment.max(alignment);
    }

    /// Checks that the table can hold another field, of `byte_length` bytes
    /// with its padding, as `TableBuilder` checks it, with its errors
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
    fn position(&self) -> usize {
        self.bytes.len() + self.shift
    }

    /// The length so far of the table whose fields are being set
    #[inline(always)]
    fn table_length(&self) -> usize {
        self.position() - self.current.table
    }

    /// Copies the table's bytes, with its segments in their places
    fn copy_into(&self, to: &mut Vec<u8>) {
        let bytes = self.bytes.as_slice();
        let mut from = 0;
        for segment in self.segments.iter() {
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

/// A table or vector being nested in place. Until it's committed, dropping
/// it, as an error or a panic does, returns the builder to where it was.
struct Checkpoint<'g, 'a, B: Buffer, const N: usize> {
    builder: &'g mut DirectTableBuilder<'a, B, N>,
    saved: Saved,
}

impl<'g, 'a, B: Buffer, const N: usize> Checkpoint<'g, 'a, B, N> {
    #[inline]
    fn new(builder: &'g mut DirectTableBuilder<'a, B, N>) -> Self {
        let saved = Saved {
            counters: builder.current,
            bytes: builder.bytes.len(),
            segments: builder.segments.len(),
            shift: builder.shift,
        };

        Self { builder, saved }
    }

    /// Keeps what was written
    #[inline]
    const fn commit(self) {
        mem::forget(self);
    }
}

impl<B: Buffer, const N: usize> Drop for Checkpoint<'_, '_, B, N> {
    fn drop(&mut self) {
        self.builder.restore(self.saved);
    }
}

/// The fields of a table that a [`DirectTableBuilder`] writes in place, as
/// `TableBuilder`'s `InPlaceTable` records them
#[derive(Debug)]
pub struct DirectInPlaceTable<'b, 'a, B: Buffer = DefaultBuffer, const N: usize = FIRST_SEGMENTS> {
    builder: &'b mut DirectTableBuilder<'a, B, N>,
}

/// Setters of a [`DirectInPlaceTable`], which are the builder's
macro_rules! in_place_setters {
    ($($name:ident($($argument:ident: $type:ty),*)),* $(,)?) => {$(
        #[doc = concat!("Sets a field as [`DirectTableBuilder::", stringify!($name), "()`] does")]
        ///
        /// # Errors
        ///
        #[doc = concat!("The errors of [`DirectTableBuilder::", stringify!($name), "()`].")]
        #[inline(always)]
        pub fn $name(&mut self, $($argument: $type),*) -> Result<(), BuildError> {
            self.builder.$name($($argument),*)
        }
    )*};
}

impl<'a, B: Buffer, const N: usize> DirectInPlaceTable<'_, 'a, B, N> {
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
        set_table(table: impl Into<DirectNestedTable<'a, B, N>>),
        set_table_aligned(table: impl Into<DirectNestedTable<'a, B, N>>, alignment: Alignment),
    );

    /// Sets a vector field as [`DirectTableBuilder::set_vector()`] does
    ///
    /// # Errors
    ///
    /// The errors of [`DirectTableBuilder::set_vector()`].
    pub fn set_vector<I>(&mut self, items: I) -> Result<(), BuildError>
    where
        I: IntoIterator,
        I::Item: Into<DirectNestedTable<'a, B, N>>,
    {
        self.builder.set_vector(items)
    }

    /// Writes a table nested in this one in place, as
    /// [`DirectTableBuilder::set_table_with()`] does
    ///
    /// # Errors
    ///
    /// The errors of [`DirectTableBuilder::set_table_with()`].
    #[inline(always)]
    pub fn set_table_with<E, F>(&mut self, fields: usize, fill: F) -> Result<(), E>
    where
        E: From<BuildError>,
        F: FnOnce(&mut DirectInPlaceTable<'_, 'a, B, N>) -> Result<(), E>,
    {
        self.builder.set_table_with(fields, fill)
    }

    /// Writes a vector of tables in place, as
    /// [`DirectTableBuilder::set_vector_with()`] does
    ///
    /// # Errors
    ///
    /// The errors of [`DirectTableBuilder::set_vector_with()`].
    #[inline]
    pub fn set_vector_with<E, F>(&mut self, count: usize, fields: usize, fill: F) -> Result<(), E>
    where
        E: From<BuildError>,
        F: FnMut(usize, &mut DirectInPlaceTable<'_, 'a, B, N>) -> Result<(), E>,
    {
        self.builder.set_vector_with(count, fields, fill)
    }
}

/// A table to nest with [`DirectTableBuilder::set_table()`] or in
/// [`DirectTableBuilder::set_vector()`]: a builder, borrowed or owned, or
/// bytes, as `TableBuilder`'s `NestedTable`
#[derive(Debug)]
pub struct DirectNestedTable<'a, B: Buffer = DefaultBuffer, const N: usize = FIRST_SEGMENTS>(
    Nested<'a, B, N>,
);

/// A table to nest, and the alignment that it's padded to as a field. An
/// owned builder keeps that as its own alignment.
#[derive(Debug)]
enum Nested<'a, B: Buffer, const N: usize> {
    Builder(&'a DirectTableBuilder<'a, B, N>, Alignment),
    OwnedBuilder(DirectTableBuilder<'a, B, N>),
    Bytes(Cow<'a, [u8]>, Alignment),
}

impl<B: Buffer, const N: usize> DirectNestedTable<'_, B, N> {
    fn byte_length(&self) -> usize {
        match &self.0 {
            Nested::Builder(builder, _) => builder.position(),
            Nested::OwnedBuilder(builder) => builder.position(),
            Nested::Bytes(bytes, _) => bytes.len(),
        }
    }

    const fn alignment(&self) -> Alignment {
        match &self.0 {
            Nested::Builder(_, alignment) | Nested::Bytes(_, alignment) => *alignment,
            Nested::OwnedBuilder(builder) => builder.current.alignment,
        }
    }

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

impl<'a, B: Buffer, const N: usize> From<DirectTableBuilder<'a, B, N>>
    for DirectNestedTable<'a, B, N>
{
    fn from(builder: DirectTableBuilder<'a, B, N>) -> Self {
        Self(Nested::OwnedBuilder(builder))
    }
}

impl<'b, 'a: 'b, B: Buffer, const N: usize> From<&'b DirectTableBuilder<'a, B, N>>
    for DirectNestedTable<'b, B, N>
{
    fn from(builder: &'b DirectTableBuilder<'a, B, N>) -> Self {
        Self(Nested::Builder(builder, builder.current.alignment))
    }
}

impl<'a, B: Buffer, const N: usize> From<Cow<'a, [u8]>> for DirectNestedTable<'a, B, N> {
    fn from(bytes: Cow<'a, [u8]>) -> Self {
        Self(Nested::Bytes(bytes, Alignment::Four))
    }
}

impl<'a, B: Buffer, const N: usize> From<&'a [u8]> for DirectNestedTable<'a, B, N> {
    fn from(bytes: &'a [u8]) -> Self {
        Self::from(Cow::Borrowed(bytes))
    }
}

impl<'a, B: Buffer, const N: usize, const M: usize> From<&'a [u8; M]>
    for DirectNestedTable<'a, B, N>
{
    fn from(bytes: &'a [u8; M]) -> Self {
        Self::from(bytes.as_slice())
    }
}

impl<'a, B: Buffer, const N: usize> From<&'a Vec<u8>> for DirectNestedTable<'a, B, N> {
    fn from(bytes: &'a Vec<u8>) -> Self {
        Self::from(bytes.as_slice())
    }
}

impl<B: Buffer, const N: usize> From<Vec<u8>> for DirectNestedTable<'_, B, N> {
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
fn push_slot<B: Buffer>(bytes: &mut B, value: usize, offset_type: OffsetType) {
    match offset_type {
        OffsetType::Uint8 => bytes.push(&[value as u8]),
        OffsetType::Uint16 => bytes.push(&(value as u16).to_le_bytes()),
        OffsetType::Uint32 => bytes.push(&(value as u32).to_le_bytes()),
    }
}

/// Writes an offset, item end or length into the slot at `at`, which the
/// bytes hold
#[allow(clippy::cast_possible_truncation)]
#[inline(always)]
fn write_slot(bytes: &mut [u8], at: usize, value: usize, offset_type: OffsetType) {
    let Some(slot) = bytes.get_mut(at..) else {
        return;
    };

    match offset_type {
        OffsetType::Uint8 => set_le(slot, [value as u8]),
        OffsetType::Uint16 => set_le(slot, (value as u16).to_le_bytes()),
        OffsetType::Uint32 => set_le(slot, (value as u32).to_le_bytes()),
    }
}

/// Copies `value` to the start of `bytes`, if they're as long
#[inline(always)]
fn set_le<const W: usize>(bytes: &mut [u8], value: [u8; W]) {
    if let Some(slot) = bytes.first_chunk_mut::<W>() {
        *slot = value;
    }
}
