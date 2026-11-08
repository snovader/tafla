use std::iter;

use crate::{Alignment, BuildError, Element, OffsetType, TableLayout, element::extend_elements};

/// Counts the bytes of a table, so that a [`TableWriter`] can write it into
/// bytes of exactly that length.
///
/// It's for code that knows a table's fields before it writes them, such as
/// the code that tafla-gen generates, which writes tables without recording
/// their fields as `TableBuilder` does.
///
/// The fields are added in order, each with its type's method, which takes
/// what decides its length: nothing for a scalar, the count of an array, the
/// string itself. A nested table is added with [`TableSizer::add_table()`],
/// and a vector with [`TableSizer::add_vector()`], and the fields of each
/// such table then follow between [`TableSizer::start_table()`] and
/// [`TableSizer::end_table()`]. The sizer pads arrays and nested tables as
/// `TableBuilder` does, so a writer that takes the same fields in the same
/// order writes each where the sizer counted it.
///
/// Adding a field doesn't fail: [`TableSizer::writer()`] returns the error
/// of a table too long for its layout's offset type, or of an array of
/// 0-byte structs, as `TableBuilder`'s setter of the field would have.
///
/// ```
/// use tafla::{ByteBuffer, TableLayout, TableSizer};
///
/// let mut sizer = TableSizer::new(2, TableLayout::DEFAULT)?;
/// sizer.add_f64();
/// sizer.add_str("Europe/Stockholm");
///
/// let mut writer = sizer.writer()?;
/// writer.set_f64(0, 59.3293);
/// writer.set_str(1, "Europe/Stockholm");
/// let bytes = writer.into_bytes()?;
///
/// let buffer = ByteBuffer::new(&bytes, TableLayout::DEFAULT);
/// assert_eq!(buffer.get_str(buffer.root_offset(1)?)?, "Europe/Stockholm");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug)]
pub struct TableSizer {
    /// The number of the root's fields
    fields: usize,
    /// The version, when the layout is versioned
    version: Option<u32>,
    sized: bool,
    offset_type: OffsetType,
    /// The table's length so far, which is where the next field goes. It's
    /// in u64, so that it doesn't saturate at the limit itself in a 32-bit
    /// usize, and saturates far past the longest table.
    position: u64,
    /// The start of the table being sized, which arrays are aligned from: 0
    /// for the root, and past its header for the tables nested in it
    table: u64,
    /// The root's fields added so far, the last of which a table too long
    /// for its offsets names
    root_fields: usize,
    /// The first error: of an array of 0-byte structs, or of the first root
    /// field that made the table too long
    error: Option<BuildError>,
}

macro_rules! scalar_adders {
    ($($name:ident($type:ty)),*) => {$(
        #[doc = concat!("Adds a `", stringify!($type), "` field")]
        #[inline]
        pub fn $name(&mut self) {
            self.add(size_of::<$type>() as u64);
        }
    )*};
}

macro_rules! array_adders {
    ($($name:ident($type:ty)),*) => {$(
        #[doc = concat!("Adds an array field of `count` `", stringify!($type), "` elements")]
        #[inline]
        pub fn $name(&mut self, count: usize) {
            self.add_elements::<$type>(count);
        }
    )*};
}

impl TableSizer {
    /// Starts sizing a table of `fields` fields in `layout`, which its
    /// readers must use too. Nested tables use [`TableLayout::DEFAULT`].
    ///
    /// # Errors
    ///
    /// [`BuildError::HeaderTooLong`] if the header alone is too long for the
    /// layout's offset type, as `TableBuilder::new()` returns.
    pub fn new(fields: usize, layout: TableLayout) -> Result<Self, BuildError> {
        let header = layout.header_length(fields)?;

        Ok(Self::create(fields, header, layout, None))
    }

    /// Starts sizing a table of `fields` fields in `layout`, whose writer
    /// writes `version` in its header
    ///
    /// # Errors
    ///
    /// [`BuildError::VersionTooLarge`] if `version` doesn't fit in the
    /// layout's offset type, or [`BuildError::HeaderTooLong`] if the header
    /// alone is too long for the layout's offset type, as
    /// `TableBuilder::versioned()` returns.
    pub fn versioned(
        fields: usize,
        layout: TableLayout<true>,
        version: u32,
    ) -> Result<Self, BuildError> {
        layout.check_version(version)?;
        let header = layout.header_length(fields)?;

        Ok(Self::create(fields, header, layout, Some(version)))
    }

    fn create<const VERSIONED: bool>(
        fields: usize,
        header: u32,
        layout: TableLayout<VERSIONED>,
        version: Option<u32>,
    ) -> Self {
        Self {
            fields,
            version,
            sized: layout.is_sized(),
            offset_type: layout.offset_type(),
            position: u64::from(header),
            table: 0,
            root_fields: 0,
            error: None,
        }
    }

    /// The byte length of the table, with every field added
    ///
    /// # Errors
    ///
    /// [`BuildError::TooLong`] if the table is too long for its layout's
    /// offset type: longer than 255 bytes, 64 KiB or 4 GiB. It names the root
    /// field that made it too long, and the length that the root's fields up
    /// to it make the table, as `TableBuilder`'s setter of the field returns.
    /// [`BuildError::NotWholeStructs`] if an array of 0-byte structs was
    /// added, unless a field before it made the table too long.
    pub fn byte_length(&self) -> Result<usize, BuildError> {
        if let Some(error) = self.error {
            return Err(error);
        }

        u32::try_from(self.position)
            .ok()
            .filter(|&length| length <= self.offset_type.max_value())
            .and_then(|length| usize::try_from(length).ok())
            .ok_or_else(|| BuildError::TooLong {
                index: self.root_fields.saturating_sub(1),
                length: self.position,
                offset_type: self.offset_type,
            })
    }

    /// Returns a writer of the table, into new bytes of its length, which
    /// takes its fields in the order that they were added
    ///
    /// # Errors
    ///
    /// The [`BuildError::TooLong`] or [`BuildError::NotWholeStructs`] of
    /// [`TableSizer::byte_length()`], before allocating the bytes.
    pub fn writer(&self) -> Result<TableWriter, BuildError> {
        let length = self.byte_length()?;

        Ok(TableWriter::new(
            length,
            self.fields,
            self.sized,
            self.version,
            self.offset_type,
        ))
    }

    /// Starts the fields of a nested table or vector item of `fields`
    /// fields, after its parent's [`TableSizer::add_table()`] or
    /// [`TableSizer::add_vector()`]
    #[inline]
    pub const fn start_table(&mut self, fields: usize) -> SizerParent {
        let parent = SizerParent { table: self.table };
        self.table = self.position;
        self.position = self
            .position
            .saturating_add((fields as u64).saturating_mul(self.width()));

        parent
    }

    /// Ends the fields of the table that [`TableSizer::start_table()`]
    /// started
    // The parent is taken back, so that the table can't end twice
    #[allow(clippy::needless_pass_by_value)]
    #[inline]
    pub const fn end_table(&mut self, parent: SizerParent) {
        self.table = parent.table;
    }

    scalar_adders!(
        add_i8(i8),
        add_u8(u8),
        add_i16(i16),
        add_u16(u16),
        add_i32(i32),
        add_u32(u32),
        add_i64(i64),
        add_u64(u64),
        add_f32(f32),
        add_f64(f64)
    );

    array_adders!(
        add_i8_array(i8),
        add_u8_array(u8),
        add_i16_array(i16),
        add_u16_array(u16),
        add_i32_array(i32),
        add_u32_array(u32),
        add_i64_array(i64),
        add_u64_array(u64),
        add_f32_array(f32),
        add_f64_array(f64)
    );

    /// Adds a string field, of its UTF-8 bytes
    #[inline]
    pub fn add_str(&mut self, string: &str) {
        self.add(self.width().saturating_add(string.len() as u64));
    }

    /// Adds a struct field of `size` bytes
    #[inline]
    pub fn add_struct(&mut self, size: usize) {
        self.add(size as u64);
    }

    /// Adds an array of `count` structs of `size` bytes each, padded so that
    /// they start at a multiple of `alignment` from the start of the table.
    ///
    /// [`TableSizer::writer()`] returns [`BuildError::NotWholeStructs`] if
    /// `size` is 0, as `TableBuilder::set_struct_array()` does.
    #[inline]
    pub fn add_struct_array(&mut self, count: usize, size: usize, alignment: Alignment) {
        if size == 0 {
            self.error
                .get_or_insert(BuildError::NotWholeStructs { length: 0, size });
        }

        let byte_length = (count as u64).saturating_mul(size as u64);
        self.add_array(byte_length, alignment);
    }

    /// Adds a nested table, padded to start at a multiple of `alignment`,
    /// whose fields then follow between [`TableSizer::start_table()`] and
    /// [`TableSizer::end_table()`]
    #[inline]
    pub fn add_table(&mut self, alignment: Alignment) {
        self.start_field();
        self.position = self
            .position
            .saturating_add(padding(self.position.wrapping_sub(self.table), alignment));
    }

    /// Adds a vector of `count` tables, whose fields then follow, each
    /// between [`TableSizer::start_table()`] and [`TableSizer::end_table()`]
    #[inline]
    pub fn add_vector(&mut self, count: usize) {
        // The count, and then the end of each item
        let slots = (count as u64).saturating_add(1);
        self.add(slots.saturating_mul(self.width()));
    }

    #[inline]
    fn add_elements<T: Element>(&mut self, count: usize) {
        let byte_length = (count as u64).saturating_mul(size_of::<T>() as u64);
        self.add_array(byte_length, T::ALIGNMENT);
    }

    /// Adds a field of `byte_length` bytes, without padding
    #[inline]
    fn add(&mut self, byte_length: u64) {
        self.start_field();
        self.position = self.position.saturating_add(byte_length);
    }

    /// Adds an array of `byte_length` bytes after its count, padded so that
    /// its elements are aligned
    #[inline]
    fn add_array(&mut self, byte_length: u64, alignment: Alignment) {
        self.start_field();

        let start = self.position.saturating_add(self.width());
        let padding = padding(start.wrapping_sub(self.table), alignment);
        self.position = start.saturating_add(padding).saturating_add(byte_length);
    }

    /// Counts a field of the root, after checking the length that the root's
    /// fields before it make the table. The length is checked only between
    /// the root's fields, so that it's the length after the whole field that
    /// made the table too long, as `TableBuilder` reports it.
    #[inline]
    fn start_field(&mut self) {
        // Only the root starts at 0, since a nested table follows the offset
        // of the field that holds it
        if self.table != 0 {
            return;
        }

        if self.error.is_none() && self.position > u64::from(self.offset_type.max_value()) {
            self.error = Some(BuildError::TooLong {
                index: self.root_fields.saturating_sub(1),
                length: self.position,
                offset_type: self.offset_type,
            });
        }
        self.root_fields = self.root_fields.saturating_add(1);
    }

    /// The offset size, the width of offsets, lengths and header slots
    const fn width(&self) -> u64 {
        self.offset_type.size() as u64
    }
}

/// The table that a nested table or vector item was started in, which
/// [`TableSizer::end_table()`] takes back
#[derive(Debug)]
#[must_use = "end_table() takes it back"]
pub struct SizerParent {
    table: u64,
}

/// Writes a table forward into bytes of exactly its length, which a
/// [`TableSizer`] counted.
///
/// It doesn't record the fields first, as `TableBuilder` does. It comes from
/// [`TableSizer::writer()`], and takes the fields that the sizer added, in
/// the same order, each with its index in its table.
///
/// Each field's offset is written as the writer reaches the field. A nested
/// table follows its parent's [`TableWriter::set_table()`], between
/// [`TableWriter::start_table()`] and [`TableWriter::end_table()`]; the items
/// of a vector follow its [`TableWriter::set_vector()`] the same way, each
/// followed by [`TableWriter::set_item_end()`], which writes where it ends.
/// Arrays and nested tables are padded as `TableBuilder` pads them, so the
/// two write the same bytes for the same fields.
///
/// It's the target of the code that tafla-gen generates, whose calls follow
/// from the schema: fields are set in index order, each once, as the sizer
/// added them, and each vector item is followed by its
/// [`TableWriter::set_item_end()`]. Hand-written tables use `TableBuilder`,
/// which checks every field.
///
/// Writing a field doesn't fail: [`TableWriter::into_bytes()`] returns the
/// error of fields that differ from those that the sizer added. The writer
/// never writes past the table's length, nor allocates more than it. Unlike
/// the other runtimes' writers, it also reports an index past its table's
/// fields, or an item past its vector's count, since [`WriterParent`] and
/// [`VectorPosition`] carry the counts; the contract that the runtimes share
/// leaves the table's bytes undefined for them.
#[derive(Debug)]
pub struct TableWriter {
    /// The bytes written so far, with the table's length reserved.
    ///
    /// Appending to reserved capacity, as `TableBuilder` does, rather than
    /// writing into zeroed bytes at positions, saves zeroing them first: the
    /// generated code wrote a document of 100,000 rows, 1.2 MB, in 16 µs
    /// rather than 23 µs. Small tables took longer to append than to write in
    /// place, a vector of 100 tables of a float32 and a nested table of two
    /// 0.92 µs rather than 0.69 µs, but still a tenth of `TableBuilder`'s
    /// 8.6 µs.
    bytes: Vec<u8>,
    /// The table's length, which the sizer counted
    length: usize,
    offset_type: OffsetType,
    /// The byte length of the root's header slots, which its offsets follow
    header_size: usize,
    /// The start of the table being written, which its offsets count from:
    /// 0 for the root
    table: usize,
    /// The position of that table's first offset
    slots: usize,
    /// The number of that table's fields
    fields: usize,
    /// The first error, which `into_bytes()` returns
    error: Option<BuildError>,
}

macro_rules! scalar_setters {
    ($($name:ident($type:ty)),*) => {$(
        #[doc = concat!("Writes `", stringify!($type), "` field `index`")]
        #[inline]
        pub fn $name(&mut self, index: usize, value: $type) {
            self.set_offset(index);
            self.push(&value.to_le_bytes());
        }
    )*};
}

macro_rules! array_setters {
    ($($name:ident($type:ty)),*) => {$(
        #[doc = concat!("Writes array field `index` of `", stringify!($type), "` elements")]
        #[inline]
        pub fn $name(&mut self, index: usize, values: &[$type]) {
            self.set_elements(index, values);
        }
    )*};
}

impl TableWriter {
    /// Writes the header of the root table of `fields` fields, into new
    /// bytes with `length` reserved, which the sizer checked are at least
    /// the header and at most what the offset type addresses
    fn new(
        length: usize,
        fields: usize,
        sized: bool,
        version: Option<u32>,
        offset_type: OffsetType,
    ) -> Self {
        let header_size =
            (usize::from(sized) + usize::from(version.is_some())) * offset_type.size();
        let mut writer = Self {
            bytes: Vec::with_capacity(length),
            length,
            offset_type,
            header_size,
            table: 0,
            slots: header_size,
            fields,
            error: None,
        };
        // `with_capacity()` requests exactly `length` bytes, which `room()`
        // counts from the capacity
        debug_assert_eq!(writer.bytes.capacity(), length);

        if sized {
            writer.push_slot(length);
        }

        if let Some(version) = version {
            writer.push_slot(version as usize);
        }

        // Each field's offset is written as the writer reaches the field
        writer.push_zeros(fields * offset_type.size());

        writer
    }

    /// Returns the table's bytes.
    ///
    /// # Errors
    ///
    /// [`BuildError::NotAsSized`] if the fields didn't fill the bytes, as
    /// when they differ from those that the sizer added,
    /// [`BuildError::TooManyFields`] if one was written at an index past its
    /// table's fields, or [`BuildError::NotWholeStructs`] if an array of
    /// structs was given bytes that aren't whole structs.
    pub fn into_bytes(self) -> Result<Vec<u8>, BuildError> {
        if let Some(error) = self.error {
            return Err(error);
        }

        if self.bytes.len() != self.length {
            return Err(BuildError::NotAsSized {
                written: self.bytes.len(),
                length: self.length,
            });
        }

        Ok(self.bytes)
    }

    /// Starts the fields of a nested table or vector item of `fields`
    /// fields, after its parent's [`TableWriter::set_table()`] or
    /// [`TableWriter::set_vector()`]
    #[inline]
    pub fn start_table(&mut self, fields: usize) -> WriterParent {
        let parent = WriterParent {
            table: self.table,
            fields: self.fields,
        };

        let start = self.bytes.len();
        self.table = start;
        self.slots = start;
        // Without room for its slots, the table has no fields to write, so
        // that no slot is looked for past the bytes
        let slots = fields.checked_mul(self.offset_type.size());
        if let Some(slots) = slots.filter(|&slots| slots <= self.room()) {
            self.push_zeros(slots);
            self.fields = fields;
        } else {
            self.fail();
            self.fields = 0;
        }

        parent
    }

    /// Ends the fields of the table that [`TableWriter::start_table()`]
    /// started
    // The parent is taken back, so that the table can't end twice
    #[allow(clippy::needless_pass_by_value)]
    #[inline]
    pub const fn end_table(&mut self, parent: WriterParent) {
        self.table = parent.table;
        self.fields = parent.fields;
        // Only the root starts at 0, and only its offsets follow a header
        self.slots = if parent.table == 0 {
            self.header_size
        } else {
            parent.table
        };
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

    /// Writes string field `index`, as its UTF-8 bytes
    #[inline]
    pub fn set_str(&mut self, index: usize, string: &str) {
        self.set_offset(index);
        self.push_slot(string.len());
        self.push(string.as_bytes());
    }

    /// Writes struct field `index` from its bytes: its scalars,
    /// little-endian, one after another, which are written as they are,
    /// without padding
    #[inline]
    pub fn set_struct(&mut self, index: usize, bytes: &[u8]) {
        self.set_offset(index);
        self.push(bytes);
    }

    /// Writes field `index`, an array of structs of `size` bytes each, from
    /// their bytes one after another, which it counts. It's padded so that
    /// the structs start at a multiple of `alignment` from the start of the
    /// table, which is the size of the structs' largest scalar.
    ///
    /// [`TableWriter::into_bytes()`] returns [`BuildError::NotWholeStructs`]
    /// if `size` is 0 or the bytes aren't a whole number of structs.
    #[inline]
    pub fn set_struct_array(
        &mut self,
        index: usize,
        bytes: &[u8],
        size: usize,
        alignment: Alignment,
    ) {
        let length = bytes.len();
        if size == 0 || !length.is_multiple_of(size) {
            self.error
                .get_or_insert(BuildError::NotWholeStructs { length, size });
            return;
        }

        self.start_array(index, length / size, alignment);
        self.push(bytes);
    }

    /// Writes the offset of nested table field `index`, padded to start at a
    /// multiple of `alignment`, whose fields then follow between
    /// [`TableWriter::start_table()`] and [`TableWriter::end_table()`]
    #[inline]
    pub fn set_table(&mut self, index: usize, alignment: Alignment) {
        let offset = self.bytes.len().wrapping_sub(self.table);
        self.push_zeros(alignment.padding(offset));
        self.set_offset(index);
    }

    /// Writes the offset and count of vector field `index`, of `count`
    /// tables, whose items then follow, each between
    /// [`TableWriter::start_table()`] and [`TableWriter::end_table()`] and
    /// then [`TableWriter::set_item_end()`]
    #[inline]
    pub fn set_vector(&mut self, index: usize, count: usize) -> VectorPosition {
        self.set_offset(index);

        let position = self.bytes.len();
        // The count, and then the end of each item, which is written after
        // the item. Without room for them, the vector has no items to end.
        let width = self.offset_type.size();
        let slots = count
            .checked_add(1)
            .and_then(|slots| slots.checked_mul(width));
        if slots.is_none_or(|slots| slots > self.room()) {
            self.fail();
            return VectorPosition { position, count: 0 };
        }

        self.push_slot(count);
        self.push_zeros(count * width);

        VectorPosition { position, count }
    }

    /// Writes the end of item `item` of the vector, which was just written
    #[inline]
    pub fn set_item_end(&mut self, vector: VectorPosition, item: usize) {
        if item >= vector.count {
            self.fail();
            return;
        }

        // Each end counts from the vector's position, which is its count's
        let slot = vector.position + (item + 1) * self.offset_type.size();
        let end = self.bytes.len().saturating_sub(vector.position);
        self.set_slot(slot, end);
    }

    #[inline]
    fn set_elements<T: Element>(&mut self, index: usize, values: &[T]) {
        self.start_array(index, values.len(), T::ALIGNMENT);

        if size_of_val(values) <= self.room() {
            extend_elements(values, &mut self.bytes);
        } else {
            self.fail();
        }
    }

    /// Pads an array so that its elements, after its count, are aligned, and
    /// writes its offset and count
    #[inline]
    fn start_array(&mut self, index: usize, count: usize, alignment: Alignment) {
        // Wrapping, since only the remainder matters
        let offset = (self.bytes.len() + self.offset_type.size()).wrapping_sub(self.table);
        self.push_zeros(alignment.padding(offset));
        self.set_offset(index);
        self.push_slot(count);
    }

    /// Writes the offset of field `index` of the table being written, which
    /// is where the field starts
    #[inline]
    fn set_offset(&mut self, index: usize) {
        if index >= self.fields {
            self.error.get_or_insert(BuildError::TooManyFields {
                fields: self.fields,
            });
            return;
        }

        // The table's slots were written when it started, so the slot lies
        // within the bytes, and the offset within the table's length
        let slot = self.slots + index * self.offset_type.size();
        let offset = self.bytes.len().saturating_sub(self.table);
        self.set_slot(slot, offset);
    }

    /// The bytes left of the table's length, which is the bytes' capacity.
    ///
    /// Counting them from the capacity, rather than from `length`, lets the
    /// compiler see that `Vec`'s own check before each append passes, and
    /// drop its path to reallocate. That path took the bytes' address, which
    /// kept the writer's fields in memory rather than in registers: the
    /// generated code wrote a vector of 100 nested tables in 0.84 µs rather
    /// than 0.96 µs, and a document of 100 rows in 64 ns rather than 73 ns.
    #[inline]
    const fn room(&self) -> usize {
        self.bytes.capacity() - self.bytes.len()
    }

    /// Appends `bytes` if the table has room for them
    #[inline]
    fn push(&mut self, bytes: &[u8]) {
        if bytes.len() <= self.room() {
            self.bytes.extend_from_slice(bytes);
        } else {
            self.fail();
        }
    }

    /// Appends `count` zeros, of padding or of slots written later, if the
    /// table has room for them
    #[inline]
    fn push_zeros(&mut self, count: usize) {
        if count <= self.room() {
            // From an iterator, which is inlined, rather than with
            // `resize()`: writing 100 tables that each nest another took
            // 0.92 µs rather than 1.38 µs
            self.bytes.extend(iter::repeat_n(0, count));
        } else {
            self.fail();
        }
    }

    // The slots, like the scalars, are copied at constant lengths, which are
    // single stores rather than calls to copy a slice. A value is at most
    // the table's length, which fits in the offsets, or it's truncated after
    // a write past the length failed.

    /// Appends a count, length or header slot
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    fn push_slot(&mut self, value: usize) {
        match self.offset_type {
            OffsetType::Uint8 => self.push(&[value as u8]),
            OffsetType::Uint16 => self.push(&(value as u16).to_le_bytes()),
            OffsetType::Uint32 => self.push(&(value as u32).to_le_bytes()),
        }
    }

    /// Sets the offset or item end at `position`, if the bytes hold it
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    fn set_slot(&mut self, position: usize, value: usize) {
        let Some(slot) = self.bytes.get_mut(position..) else {
            self.fail();
            return;
        };

        let set = match self.offset_type {
            OffsetType::Uint8 => set_le(slot, [value as u8]),
            OffsetType::Uint16 => set_le(slot, (value as u16).to_le_bytes()),
            OffsetType::Uint32 => set_le(slot, (value as u32).to_le_bytes()),
        };
        if !set {
            self.fail();
        }
    }

    /// Records that the fields differ from those sized, where the first that
    /// did was written
    #[cold]
    fn fail(&mut self) {
        self.error.get_or_insert(BuildError::NotAsSized {
            written: self.bytes.len(),
            length: self.length,
        });
    }
}

/// The table that a nested table or vector item was started in, which
/// [`TableWriter::end_table()`] takes back
#[derive(Debug)]
#[must_use = "end_table() takes it back"]
pub struct WriterParent {
    table: usize,
    fields: usize,
}

/// The position of a vector that [`TableWriter::set_vector()`] wrote, and
/// its count, which [`TableWriter::set_item_end()`] takes
#[derive(Clone, Copy, Debug)]
pub struct VectorPosition {
    position: usize,
    count: usize,
}

/// Copies `value` to the start of `bytes`, if they're as long
#[inline]
fn set_le<const N: usize>(bytes: &mut [u8], value: [u8; N]) -> bool {
    bytes
        .first_chunk_mut::<N>()
        .map(|slot| *slot = value)
        .is_some()
}

/// The bytes that bring `offset` to a multiple of `alignment`
const fn padding(offset: u64, alignment: Alignment) -> u64 {
    offset.wrapping_neg() % alignment.bytes() as u64
}
