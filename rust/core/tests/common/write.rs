//! Sizes and writes tables as their descriptions set them out, with
//! `TableSizer` and `TableWriter`, which write tables nested in them in place

use tafla::{Alignment, BuildError, TableSizer, TableWriter};

use super::description::{
    Field, Layout, Nested, Source, Table, struct_alignment, struct_bytes, struct_size, struct_types,
};

impl Table {
    /// Starts a sizer of the table's layout, version and number of fields,
    /// with no field added
    pub fn sizer(&self) -> Result<TableSizer, BuildError> {
        let layout = self.layout.unversioned();
        let fields = self.fields.len();

        if self.layout.versioned {
            let version = self.version.expect("A versioned table has a version");
            TableSizer::new(fields, layout.with_version(version))
        } else {
            TableSizer::new(fields, layout)
        }
    }

    /// Sizes and writes the table. The tables nested in it are written in
    /// place, so they must have no header and its offset type, as
    /// [`Table::nested_in_place()`] makes them.
    pub fn write(&self) -> Result<Vec<u8>, BuildError> {
        self.write_as(self)
    }

    /// Sizes the table, and writes the fields of `written` into the bytes
    /// that it sized, which fails unless they're as long as its own
    pub fn write_as(&self, written: &Self) -> Result<Vec<u8>, BuildError> {
        let mut sizer = self.sizer()?;
        for field in &self.fields {
            field.add(&mut sizer);
        }

        let mut writer = sizer.writer()?;
        for (index, field) in written.fields.iter().enumerate() {
            field.write(&mut writer, index);
        }

        writer.into_bytes()
    }

    /// The table with every table nested in it made one that a `TableWriter`
    /// writes in place: from its builder, without a header, and with the
    /// root's offset type
    pub fn nested_in_place(&self) -> Self {
        let layout = Layout::headerless(self.layout.offset_type);
        let fields = self
            .fields
            .iter()
            .map(|field| field.nested_in_place(layout))
            .collect();

        Self {
            fields,
            ..self.clone()
        }
    }

    /// Adds the fields of a nested table or vector item to the sizer
    fn add_nested(&self, sizer: &mut TableSizer) {
        let parent = sizer.start_table(self.fields.len());
        for field in &self.fields {
            field.add(sizer);
        }
        sizer.end_table(parent);
    }

    fn write_nested(&self, writer: &mut TableWriter) {
        let parent = writer.start_table(self.fields.len());
        for (index, field) in self.fields.iter().enumerate() {
            field.write(writer, index);
        }
        writer.end_table(parent);
    }

    /// The alignment that `set_table()` pads the table to when given its
    /// builder: the largest alignment of its arrays, of scalars and of
    /// structs, and of its nested tables, or 1 if it has none
    pub fn alignment(&self) -> Alignment {
        let alignments = self.fields.iter().map(Field::alignment);
        alignments.max().unwrap_or(Alignment::One)
    }
}

impl Field {
    /// Adds the field to the sizer, with what decides its length
    pub fn add(&self, sizer: &mut TableSizer) {
        match self {
            // An absent field takes no bytes but its offset
            Self::Absent => {}
            Self::I8(_) => sizer.add_i8(),
            Self::U8(_) | Self::Bool(_) => sizer.add_u8(),
            Self::I16(_) => sizer.add_i16(),
            Self::U16(_) => sizer.add_u16(),
            Self::I32(_) => sizer.add_i32(),
            Self::U32(_) => sizer.add_u32(),
            Self::I64(_) => sizer.add_i64(),
            Self::U64(_) => sizer.add_u64(),
            Self::F32(_) => sizer.add_f32(),
            Self::F64(_) => sizer.add_f64(),
            Self::I8Array(values) => sizer.add_i8_array(values.len()),
            Self::U8Array(values) => sizer.add_u8_array(values.len()),
            Self::I16Array(values) => sizer.add_i16_array(values.len()),
            Self::U16Array(values) => sizer.add_u16_array(values.len()),
            Self::I32Array(values) => sizer.add_i32_array(values.len()),
            Self::U32Array(values) => sizer.add_u32_array(values.len()),
            Self::I64Array(values) => sizer.add_i64_array(values.len()),
            Self::U64Array(values) => sizer.add_u64_array(values.len()),
            Self::F32Array(values) => sizer.add_f32_array(values.len()),
            Self::F64Array(values) => sizer.add_f64_array(values.len()),
            Self::String(string) => sizer.add_str(string),
            Self::Struct(scalars) => sizer.add_struct(struct_size(struct_types(scalars))),
            Self::StructArray { types, items } => sizer.add_struct_array(
                items.len(),
                struct_size(types.iter().copied()),
                struct_alignment(types.iter().copied()),
            ),
            Self::Table { nested, .. } => {
                sizer.add_table(self.alignment());
                nested.table.add_nested(sizer);
            }
            Self::Vector(items) => {
                sizer.add_vector(items.len());
                for item in items {
                    item.table.add_nested(sizer);
                }
            }
        }
    }

    /// Writes the field as field `index` of the writer's table
    pub fn write(&self, writer: &mut TableWriter, index: usize) {
        match self {
            Self::Absent => writer.set_absent(index),
            Self::I8(value) => writer.set_i8(index, *value),
            Self::U8(value) => writer.set_u8(index, *value),
            Self::I16(value) => writer.set_i16(index, *value),
            Self::U16(value) => writer.set_u16(index, *value),
            Self::I32(value) => writer.set_i32(index, *value),
            Self::U32(value) => writer.set_u32(index, *value),
            Self::I64(value) => writer.set_i64(index, *value),
            Self::U64(value) => writer.set_u64(index, *value),
            Self::F32(value) => writer.set_f32(index, *value),
            Self::F64(value) => writer.set_f64(index, *value),
            Self::Bool(value) => writer.set_u8(index, u8::from(*value)),
            Self::I8Array(values) => writer.set_i8_array(index, values),
            Self::U8Array(values) => writer.set_u8_array(index, values),
            Self::I16Array(values) => writer.set_i16_array(index, values),
            Self::U16Array(values) => writer.set_u16_array(index, values),
            Self::I32Array(values) => writer.set_i32_array(index, values),
            Self::U32Array(values) => writer.set_u32_array(index, values),
            Self::I64Array(values) => writer.set_i64_array(index, values),
            Self::U64Array(values) => writer.set_u64_array(index, values),
            Self::F32Array(values) => writer.set_f32_array(index, values),
            Self::F64Array(values) => writer.set_f64_array(index, values),
            Self::String(string) => writer.set_str(index, string),
            Self::Struct(scalars) => writer.set_struct(index, &struct_bytes(scalars)),
            Self::StructArray { types, items } => {
                let bytes: Vec<u8> = items.iter().flat_map(|item| struct_bytes(item)).collect();
                let size = struct_size(types.iter().copied());
                let alignment = struct_alignment(types.iter().copied());

                writer.set_struct_array(index, &bytes, size, alignment);
            }
            Self::Table { nested, .. } => {
                writer.set_table(index, self.alignment());
                nested.table.write_nested(writer);
            }
            Self::Vector(items) => {
                let vector = writer.set_vector(index, items.len());
                for (item, nested) in items.iter().enumerate() {
                    nested.table.write_nested(writer);
                    writer.set_item_end(vector, item);
                }
            }
        }
    }

    /// The alignment that the field needs from its table's start: an array's
    /// element size, an array of structs' largest scalar's, the alignment
    /// that a nested table is set with, or else 1
    pub fn alignment(&self) -> Alignment {
        match self {
            Self::I16Array(_) | Self::U16Array(_) => Alignment::Two,
            Self::I32Array(_) | Self::U32Array(_) | Self::F32Array(_) => Alignment::Four,
            Self::I64Array(_) | Self::U64Array(_) | Self::F64Array(_) => Alignment::Eight,
            Self::StructArray { types, .. } => struct_alignment(types.iter().copied()),
            Self::Table { nested, alignment } => alignment.unwrap_or(match nested.from {
                Source::Builder => nested.table.alignment(),
                Source::Bytes => Alignment::Four,
            }),
            // Arrays of bytes, and the fields that aren't padded
            _ => Alignment::One,
        }
    }

    /// The field with the tables nested in it made ones that a `TableWriter`
    /// writes in place, in `layout`
    fn nested_in_place(&self, layout: Layout) -> Self {
        let in_place = |nested: &Nested| Nested {
            from: Source::Builder,
            table: Table {
                layout,
                version: None,
                fields: nested
                    .table
                    .fields
                    .iter()
                    .map(|field| field.nested_in_place(layout))
                    .collect(),
                missing: nested.table.missing,
            },
        };

        match self {
            Self::Table { nested, alignment } => Self::Table {
                nested: in_place(nested),
                alignment: *alignment,
            },
            Self::Vector(items) => Self::Vector(items.iter().map(in_place).collect()),
            _ => self.clone(),
        }
    }
}
