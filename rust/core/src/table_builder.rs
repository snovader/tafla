use std::borrow::Cow;

use crate::{Alignment, BuildError, Element, OffsetType, TableLayout, TableWriter};

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
/// Setting a field records it, borrowing or owning its value, and
/// [`TableBuilder::to_bytes()`] then writes every field through a
/// `TableWriter`, into bytes of the table's exact length. Tables nested from
/// their builders are written by the same writer, straight into this one,
/// when they have its offset type and no header.
///
/// A setter returns a [`BuildError`], and leaves the builder as it was, when
/// the table already holds all its fields or the field would make it too
/// long for the layout's offset type: longer than 255 bytes, 64 KiB or
/// 4 GiB. Errors are values rather than panics, since a WebAssembly build
/// that aborts on panic couldn't report them.
#[derive(Debug)]
pub struct TableBuilder<'a> {
    // Narrow fields, since the builder is inline in a field that nests it,
    // and every field is as large as the largest kind
    values: Vec<Field<'a>>,
    /// The number of fields that the table holds, which a header of `uint32`
    /// offsets limits to fewer than 2^30
    fields: u32,
    /// The table's length so far, which is where the next field goes
    length: u32,
    /// The version, when `versioned`
    version: u32,
    versioned: bool,
    sized: bool,
    offset_type: OffsetType,
    /// The largest alignment that a field needs from the table's start,
    /// which a parent pads this table to when it nests it
    alignment: Alignment,
}

macro_rules! scalar_setters {
    ($($name:ident($type:ty) => $variant:ident($bits:ty)),*) => {$(
        #[doc = concat!("Sets a `", stringify!($type), "` field")]
        ///
        /// # Errors
        ///
        /// [`BuildError::TooManyFields`] if the table holds all its fields
        /// already, or [`BuildError::TooLong`] if the field would make it
        /// too long for its offset type.
        #[inline]
        pub fn $name(&mut self, value: $type) -> Result<(), BuildError> {
            let bits = <$bits>::from_ne_bytes(value.to_ne_bytes());
            self.add_field(Field::Scalar(Scalar::$variant(bits)))
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
            self.add_field(Field::Array(Array::$variant(values.into())))
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
    /// bytes its layout checked
    #[allow(clippy::cast_possible_truncation)]
    fn create<const VERSIONED: bool>(
        fields: usize,
        header: u32,
        layout: TableLayout<VERSIONED>,
        version: Option<u32>,
    ) -> Self {
        Self {
            values: Vec::with_capacity(fields),
            // The header holds an offset of at least a byte per field
            fields: fields as u32,
            length: header,
            version: version.unwrap_or(0),
            versioned: version.is_some(),
            sized: layout.is_sized(),
            offset_type: layout.offset_type(),
            alignment: Alignment::One,
        }
    }

    /// Writes the table, at its exact size.
    ///
    /// # Errors
    ///
    /// [`BuildError::Incomplete`] if fewer fields were set than the table
    /// holds.
    pub fn to_bytes(&self) -> Result<Vec<u8>, BuildError> {
        self.check_complete()?;

        // The constructor checked the header and the version, and each
        // setter the table's length, so the writer fails only for a field
        // counted otherwise than it's written
        let version = self.versioned.then_some(self.version);
        let mut writer = TableWriter::new(
            self.length as usize,
            self.fields as usize,
            self.sized,
            version,
            self.offset_type,
        );
        self.write_fields(&mut writer);

        writer.into_bytes()
    }

    scalar_setters!(
        set_i8(i8) => U8(u8), set_u8(u8) => U8(u8),
        set_i16(i16) => U16(u16), set_u16(u16) => U16(u16),
        set_i32(i32) => U32(u32), set_u32(u32) => U32(u32),
        set_i64(i64) => U64(u64), set_u64(u64) => U64(u64),
        set_f32(f32) => U32(u32), set_f64(f64) => U64(u64)
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
    pub fn set_str(&mut self, string: impl Into<Cow<'a, str>>) -> Result<(), BuildError> {
        self.add_field(Field::String(string.into()))
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
    pub fn set_struct(&mut self, bytes: impl Into<Cow<'a, [u8]>>) -> Result<(), BuildError> {
        self.add_field(Field::Struct(bytes.into()))
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

        self.add_field(Field::StructArray {
            bytes,
            size,
            alignment,
        })
    }

    /// Embeds a table, from its builder or its bytes, without a length prefix.
    /// Readers find its fields with `ByteBuffer::field_offset()` from the
    /// field's position, which reads offsets of the parent's offset type, so
    /// the table should have no header and the parent's offset type: that's
    /// [`TableLayout::DEFAULT`] in a parent of `uint32` offsets.
    ///
    /// A builder, borrowed or owned, must have all its fields set. Without a
    /// header and with this table's offset type, it's written straight into
    /// this table when this table is written, without bytes of its own.
    /// Otherwise it's written into bytes of its own, in its own layout, and
    /// copied.
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
    pub fn set_table(&mut self, table: impl Into<NestedTable<'a>>) -> Result<(), BuildError> {
        let table = table.into();
        let alignment = table.default_alignment();

        self.add_table(table, alignment)
    }

    /// Embeds a table like [`TableBuilder::set_table()`], padded to start at
    /// a multiple of `alignment`, such as [`Alignment::Eight`] for a table
    /// of 64-bit arrays given as bytes
    ///
    /// # Errors
    ///
    /// The errors of [`TableBuilder::set_table()`].
    pub fn set_table_aligned(
        &mut self,
        table: impl Into<NestedTable<'a>>,
        alignment: Alignment,
    ) -> Result<(), BuildError> {
        self.add_table(table.into(), alignment)
    }

    /// Writes a vector of tables, from their builders or their bytes, after
    /// the end of each, which lets readers find any item without reading the
    /// others. Builders must have all their fields set, and are written as
    /// `set_table()` writes them: straight into this table, or into bytes of
    /// their own, in their own layout, and copied. Items aren't padded, so
    /// their arrays are read in place only when they happen to be aligned.
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

        self.add_field(Field::Vector(items))
    }

    fn add_table(
        &mut self,
        table: NestedTable<'a>,
        alignment: Alignment,
    ) -> Result<(), BuildError> {
        table.check_complete()?;
        self.add_field(Field::Table { table, alignment })
    }

    /// Records a field after the padding that it needs. Checking the table's
    /// length here, rather than when writing it, reports a field too long for
    /// the layout from its own setter and leaves the builder as it was. Every
    /// offset, count, length and item end is at most the table's length, so
    /// that's the one value to check.
    fn add_field(&mut self, field: Field<'a>) -> Result<(), BuildError> {
        let index = self.values.len();
        if index == self.fields as usize {
            return Err(BuildError::TooManyFields {
                fields: self.fields as usize,
            });
        }

        let width = self.offset_type.size();
        let padding = field.padding(self.length as usize, width);
        let length = u64::from(self.length) + padding as u64 + field.byte_length(width);

        self.length = u32::try_from(length)
            .ok()
            .filter(|&length| length <= self.offset_type.max_value())
            .ok_or(BuildError::TooLong {
                index,
                length,
                offset_type: self.offset_type,
            })?;
        self.alignment = self.alignment.max(field.alignment());
        self.values.push(field);

        Ok(())
    }

    const fn check_complete(&self) -> Result<(), BuildError> {
        if self.values.len() == self.fields as usize {
            Ok(())
        } else {
            Err(BuildError::Incomplete {
                set: self.values.len(),
                fields: self.fields as usize,
            })
        }
    }

    /// Writes the fields into the table that `writer` is writing, each at
    /// its index
    fn write_fields(&self, writer: &mut TableWriter) {
        for (index, field) in self.values.iter().enumerate() {
            field.write(writer, index, self.offset_type);
        }
    }

    /// Writes the table as a field or vector item of a table of
    /// `offset_type` offsets. It's written in place when the writer's
    /// `start_table()` writes it as it is, with that offset type and without
    /// a header. Otherwise it's written into bytes of its own and copied.
    fn write_nested(&self, writer: &mut TableWriter, offset_type: OffsetType) {
        if self.offset_type == offset_type && !self.sized && !self.versioned {
            let parent = writer.start_table(self.fields as usize);
            self.write_fields(writer);
            writer.end_table(parent);
        } else if let Ok(table) = self.to_bytes() {
            // Its own writer fails only for a field counted otherwise than
            // it's written, and the parent's then fails too, for the bytes
            // left out
            writer.copy_table(&table);
        }
    }
}

/// A table to nest with [`TableBuilder::set_table()`] or in
/// [`TableBuilder::set_vector()`]: a builder, borrowed or owned, or bytes.
///
/// A builder with its parent's offset type and no header is written straight
/// into its parent. Bytes, and other builders, are copied.
#[derive(Debug)]
pub struct NestedTable<'a>(Nested<'a>);

#[derive(Debug)]
enum Nested<'a> {
    Builder(&'a TableBuilder<'a>),
    OwnedBuilder(TableBuilder<'a>),
    Bytes(Cow<'a, [u8]>),
}

impl<'a> NestedTable<'a> {
    const fn builder(&self) -> Option<&TableBuilder<'a>> {
        match &self.0 {
            Nested::Builder(builder) => Some(builder),
            Nested::OwnedBuilder(builder) => Some(builder),
            Nested::Bytes(_) => None,
        }
    }

    fn byte_length(&self) -> usize {
        match &self.0 {
            Nested::Builder(builder) => builder.length as usize,
            Nested::OwnedBuilder(builder) => builder.length as usize,
            Nested::Bytes(bytes) => bytes.len(),
        }
    }

    fn default_alignment(&self) -> Alignment {
        self.builder()
            .map_or(Alignment::Four, |builder| builder.alignment)
    }

    fn check_complete(&self) -> Result<(), BuildError> {
        self.builder().map_or(Ok(()), TableBuilder::check_complete)
    }

    /// Writes the table as a field or vector item of a table of
    /// `offset_type` offsets, after the field's offset or before the item's
    /// end
    fn write(&self, writer: &mut TableWriter, offset_type: OffsetType) {
        match &self.0 {
            Nested::Builder(builder) => builder.write_nested(writer, offset_type),
            Nested::OwnedBuilder(builder) => builder.write_nested(writer, offset_type),
            Nested::Bytes(table) => writer.copy_table(table),
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
        Self(Nested::Builder(builder))
    }
}

impl<'a> From<Cow<'a, [u8]>> for NestedTable<'a> {
    fn from(bytes: Cow<'a, [u8]>) -> Self {
        Self(Nested::Bytes(bytes))
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

#[derive(Debug)]
enum Field<'a> {
    Scalar(Scalar),
    Array(Array<'a>),
    String(Cow<'a, str>),
    /// A struct's bytes, written as they are
    Struct(Cow<'a, [u8]>),
    /// The bytes of structs of `size` bytes each, which start at a multiple
    /// of `alignment` from the table's start
    StructArray {
        bytes: Cow<'a, [u8]>,
        size: usize,
        alignment: Alignment,
    },
    Table {
        table: NestedTable<'a>,
        alignment: Alignment,
    },
    Vector(Vec<NestedTable<'a>>),
}

impl Field<'_> {
    /// The alignment that the field needs from the table's start
    fn alignment(&self) -> Alignment {
        match self {
            Self::Array(array) => array.alignment(),
            Self::StructArray { alignment, .. } | Self::Table { alignment, .. } => *alignment,
            Self::Scalar(_) | Self::String(_) | Self::Struct(_) | Self::Vector(_) => Alignment::One,
        }
    }

    /// The padding before the field when it would start at `offset`, which
    /// aligns an array's elements after its count and a nested table's start
    fn padding(&self, offset: usize, width: usize) -> usize {
        match self {
            // Wrapping, since only the remainder matters
            Self::Array(array) => array.alignment().padding(offset.wrapping_add(width)),
            Self::StructArray { alignment, .. } => alignment.padding(offset.wrapping_add(width)),
            Self::Table { alignment, .. } => alignment.padding(offset),
            Self::Scalar(_) | Self::String(_) | Self::Struct(_) | Self::Vector(_) => 0,
        }
    }

    /// The field's byte length after its padding, in u64, which only a
    /// vector that nests a builder many times can overflow, saturating far
    /// past the longest table
    fn byte_length(&self, width: usize) -> u64 {
        let width = width as u64;

        match self {
            Self::Scalar(scalar) => scalar.size() as u64,
            Self::Array(array) => width + array.byte_length() as u64,
            Self::String(string) => width + string.len() as u64,
            Self::Struct(bytes) => bytes.len() as u64,
            Self::StructArray { bytes, .. } => width + bytes.len() as u64,
            Self::Table { table, .. } => table.byte_length() as u64,
            Self::Vector(items) => items.iter().fold(width, |length, item| {
                length.saturating_add(width + item.byte_length() as u64)
            }),
        }
    }

    /// Writes the field as field `index` of the table that `writer` is
    /// writing, of `offset_type` offsets
    // Inlined into `TableBuilder::write_fields()`, which makes the replay one
    // recursive function. Called out of line for each field, saving and
    // restoring 12 registers, it wrote a document of 100 rows in 92 ns
    // rather than 73 ns, and a vector of 100 nested tables in 2.2–3.1 µs
    // rather than 1.95 µs.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn write(&self, writer: &mut TableWriter, index: usize, offset_type: OffsetType) {
        match self {
            Self::Scalar(scalar) => scalar.write(writer, index),
            Self::Array(array) => array.write(writer, index),
            Self::String(string) => writer.set_str(index, string),
            Self::Struct(bytes) => writer.set_struct(index, bytes),
            Self::StructArray {
                bytes,
                size,
                alignment,
            } => writer.set_struct_array(index, bytes, *size, *alignment),
            Self::Table { table, alignment } => {
                writer.set_table(index, *alignment);
                table.write(writer, offset_type);
            }
            Self::Vector(items) => {
                let vector = writer.set_vector(index, items.len());
                for (item, table) in items.iter().enumerate() {
                    table.write(writer, offset_type);
                    writer.set_item_end(vector, item);
                }
            }
        }
    }
}

/// A scalar, as the unsigned integer of its bits, which the writer writes
/// as the scalar's little-endian bytes
#[derive(Clone, Copy, Debug)]
enum Scalar {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
}

impl Scalar {
    const fn size(self) -> usize {
        match self {
            Self::U8(_) => 1,
            Self::U16(_) => 2,
            Self::U32(_) => 4,
            Self::U64(_) => 8,
        }
    }

    fn write(self, writer: &mut TableWriter, index: usize) {
        match self {
            Self::U8(bits) => writer.set_u8(index, bits),
            Self::U16(bits) => writer.set_u16(index, bits),
            Self::U32(bits) => writer.set_u32(index, bits),
            Self::U64(bits) => writer.set_u64(index, bits),
        }
    }
}

#[derive(Debug)]
enum Array<'a> {
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

/// Evaluates `$expression` with `$values` bound to the elements of `$array`,
/// whatever their type
macro_rules! with_elements {
    ($array:expr, $values:ident => $expression:expr) => {
        match $array {
            Array::I8($values) => $expression,
            Array::U8($values) => $expression,
            Array::I16($values) => $expression,
            Array::U16($values) => $expression,
            Array::I32($values) => $expression,
            Array::U32($values) => $expression,
            Array::I64($values) => $expression,
            Array::U64($values) => $expression,
            Array::F32($values) => $expression,
            Array::F64($values) => $expression,
        }
    };
}

impl Array<'_> {
    fn alignment(&self) -> Alignment {
        with_elements!(self, values => alignment_of(values))
    }

    fn byte_length(&self) -> usize {
        with_elements!(self, values => size_of_val::<[_]>(values))
    }

    fn write(&self, writer: &mut TableWriter, index: usize) {
        with_elements!(self, values => writer.set_elements(index, values));
    }
}

const fn alignment_of<T: Element>(_: &[T]) -> Alignment {
    T::ALIGNMENT
}
