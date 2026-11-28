//! Builds tables as their descriptions set them out, with `TableBuilder`,
//! from nested builders or with the tables nested in them written in place

use tafla::{BuildError, InPlaceTable, NestedTable, OffsetType, TableBuilder};

use super::description::{
    Field, Nested, Source, Table, struct_alignment, struct_bytes, struct_size,
};

impl Table {
    /// Starts a builder of the table's layout, version and number of
    /// fields, with no field set
    pub fn builder(&self) -> Result<TableBuilder, BuildError> {
        let layout = self.layout.unversioned();
        let fields = self.fields.len();

        if self.layout.versioned {
            let version = self.version.expect("A versioned table has a version");
            TableBuilder::new(fields, layout.with_version(version))
        } else {
            TableBuilder::new(fields, layout)
        }
    }

    /// Starts the table in `buffer`, as [`Table::builder()`] does in a
    /// buffer of its own
    pub fn builder_in(&self, buffer: Vec<u8>) -> Result<TableBuilder, BuildError> {
        let layout = self.layout.unversioned();
        let fields = self.fields.len();

        if self.layout.versioned {
            let version = self.version.expect("A versioned table has a version");
            TableBuilder::with_buffer(buffer, fields, layout.with_version(version))
        } else {
            TableBuilder::with_buffer(buffer, fields, layout)
        }
    }

    /// Starts the table as the next one of `builder`, in its buffer, with no
    /// field set
    pub fn reset(&self, builder: &mut TableBuilder) -> Result<(), BuildError> {
        let layout = self.layout.unversioned();
        let fields = self.fields.len();

        if self.layout.versioned {
            let version = self.version.expect("A versioned table has a version");
            builder.reset(fields, layout.with_version(version))
        } else {
            builder.reset(fields, layout)
        }
    }

    /// Sets the table's fields on `builder`, started for it, and lends its
    /// bytes
    pub fn finish_on<'b>(&self, builder: &'b mut TableBuilder) -> Result<&'b [u8], BuildError> {
        for field in &self.fields {
            field.set(builder)?;
        }

        builder.finish()
    }

    /// Builds the table, copying its values
    pub fn build(&self) -> Result<TableBuilder, BuildError> {
        let mut builder = self.builder()?;
        for field in &self.fields {
            field.set(&mut builder)?;
        }

        Ok(builder)
    }

    /// Writes the table
    pub fn to_bytes(&self) -> Result<Vec<u8>, BuildError> {
        self.build()?.to_bytes()
    }

    /// Writes the table with `TableBuilder`, writing each table nested
    /// from a builder that it writes in place with `set_table_with()`, and
    /// each vector of them, when its items hold as many fields, with
    /// `set_vector_with()`, rather than from their builders. Its bytes and
    /// errors are `to_bytes()`'s, but for a vector whose count and items'
    /// ends alone make its table too long, which fails before its items are
    /// recorded.
    pub fn to_bytes_in_place(&self) -> Result<Vec<u8>, BuildError> {
        let in_place = Some(self.layout.offset_type);

        let mut builder = self.builder()?;
        for field in &self.fields {
            field.set_on(&mut builder, in_place)?;
        }

        builder.to_bytes()
    }
}

/// Defines a method of `Field` that sets the field on a target, a builder or
/// a table that one writes in place, and with `in_place`, the offset type
/// of the builder's table, writes the tables nested in it in place as
/// [`Table::to_bytes_in_place()`] does
macro_rules! set_field_on {
    ($(#[$doc:meta])* $name:ident, $target:ty) => {
        impl Field {
            $(#[$doc])*
            pub fn $name(
                &self,
                target: &mut $target,
                in_place: Option<OffsetType>,
            ) -> Result<(), BuildError> {
                match self {
                    Self::I8(value) => target.set_i8(*value),
                    Self::U8(value) => target.set_u8(*value),
                    Self::I16(value) => target.set_i16(*value),
                    Self::U16(value) => target.set_u16(*value),
                    Self::I32(value) => target.set_i32(*value),
                    Self::U32(value) => target.set_u32(*value),
                    Self::I64(value) => target.set_i64(*value),
                    Self::U64(value) => target.set_u64(*value),
                    Self::F32(value) => target.set_f32(*value),
                    Self::F64(value) => target.set_f64(*value),
                    Self::Bool(value) => target.set_u8(u8::from(*value)),
                    Self::I8Array(values) => target.set_i8_array(values),
                    Self::U8Array(values) => target.set_u8_array(values),
                    Self::I16Array(values) => target.set_i16_array(values),
                    Self::U16Array(values) => target.set_u16_array(values),
                    Self::I32Array(values) => target.set_i32_array(values),
                    Self::U32Array(values) => target.set_u32_array(values),
                    Self::I64Array(values) => target.set_i64_array(values),
                    Self::U64Array(values) => target.set_u64_array(values),
                    Self::F32Array(values) => target.set_f32_array(values),
                    Self::F64Array(values) => target.set_f64_array(values),
                    Self::BoolArray(values) => target.set_bool_array(values),
                    Self::String(string) => target.set_str(string),
                    Self::Struct(scalars) => target.set_struct(struct_bytes(scalars)),
                    Self::StructArray { types, items } => {
                        let bytes = items.iter().flat_map(|scalars| struct_bytes(scalars));
                        let size = struct_size(types.iter().copied());
                        let alignment = struct_alignment(types.iter().copied());

                        target.set_struct_array(bytes.collect::<Vec<_>>(), size, alignment)
                    }
                    Self::Table {
                        nested,
                        alignment: None,
                    } if nested.is_written_in_place(in_place) => {
                        let fields = &nested.table.fields;
                        target.set_table_with(fields.len(), |table| {
                            set_fields(fields, table, in_place)
                        })
                    }
                    Self::Table { nested, alignment } => {
                        let table = nested.build()?;
                        match alignment {
                            Some(alignment) => target.set_table_aligned(table, *alignment),
                            None => target.set_table(table),
                        }
                    }
                    Self::Vector(items) if Nested::all_written_in_place(items, in_place) => {
                        let fields = items.first().map_or(0, |item| item.table.fields.len());
                        target.set_vector_with(items.len(), fields, |index, item| {
                            set_fields(&items[index].table.fields, item, in_place)
                        })
                    }
                    Self::Vector(items) => {
                        let items = items
                            .iter()
                            .map(Nested::build)
                            .collect::<Result<Vec<_>, _>>()?;
                        target.set_vector(items)
                    }
                    Self::StringVector(strings) => {
                        let strings: Vec<&str> = strings.iter().map(String::as_str).collect();
                        target.set_str_vector(strings)
                    }
                }
            }
        }
    };
}

set_field_on!(
    /// Sets the field on `builder`, which copies its value, and with
    /// `in_place`, writes the tables nested in it in place
    set_on,
    TableBuilder
);

set_field_on!(
    /// Sets the field on a table that a builder writes in place, as
    /// [`Field::set_on()`] sets it on a builder
    set_in_place,
    InPlaceTable<'_>
);

impl Field {
    /// Sets the field on `builder`, which copies its value
    pub fn set(&self, builder: &mut TableBuilder) -> Result<(), BuildError> {
        self.set_on(builder, None)
    }
}

/// Sets `fields` on a table that a builder writes in place, writing the
/// tables nested in them in place in a table of `in_place` offsets
fn set_fields(
    fields: &[Field],
    table: &mut InPlaceTable<'_>,
    in_place: Option<OffsetType>,
) -> Result<(), BuildError> {
    fields
        .iter()
        .try_for_each(|field| field.set_in_place(table, in_place))
}

impl Nested {
    /// Whether the table is written in place in a table of `in_place`
    /// offsets: from a builder without a header and of those offsets, which
    /// is a builder that `set_table()` writes in place
    fn is_written_in_place(&self, in_place: Option<OffsetType>) -> bool {
        let layout = self.table.layout;
        self.from == Source::Builder
            && !layout.sized
            && !layout.versioned
            && Some(layout.offset_type) == in_place
    }

    /// Whether a vector of `items` is written in place: each of them is,
    /// and they hold as many fields
    fn all_written_in_place(items: &[Self], in_place: Option<OffsetType>) -> bool {
        let fields = items.first().map(|item| item.table.fields.len());
        items.iter().all(|item| {
            item.is_written_in_place(in_place) && Some(item.table.fields.len()) == fields
        })
    }

    /// Builds the table to nest, as its builder or its bytes
    pub fn build(&self) -> Result<NestedTable<'_>, BuildError> {
        let builder = self.table.build()?;

        Ok(match self.from {
            Source::Builder => builder.into(),
            Source::Bytes => builder.to_bytes()?.into(),
        })
    }
}
