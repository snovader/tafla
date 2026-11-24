//! Reads tables with `TableReader` as their descriptions set them out, and
//! returns the descriptions with the values read in place of theirs, so
//! that they equal the descriptions when the bytes hold the described
//! values.
//!
//! A nested table, from `set_table()` or in a vector, is read in place when
//! it has no header and its parent's offset type: a field from its
//! position in its parent's bytes, and a vector item from the bytes that
//! `Vector::iter_in_place()` returns, which end where its parent's do. Any
//! other is read as a table of its own: a field from its position to the end
//! of its parent, since its length isn't written, and a vector item from the
//! bytes that `Vector` iterates over. The scalars of structs are read at their
//! offsets in the bytes that `get_struct()` and `get_struct_array()` return.

use tafla::{DecodeError, TableLayout, TableReader};

use super::description::{Field, Layout, Nested, ScalarType, Table, struct_size, struct_types};

/// Why bytes didn't read as a description
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// A read returned the runtime's error
    Decode(DecodeError),
    /// A sized table's header holds `size`, rather than its byte `length`
    Size { size: usize, length: usize },
    /// A vector holds `count` items, and iterates over `iterated`, rather
    /// than the `described` number
    Items {
        count: usize,
        iterated: usize,
        described: usize,
    },
    /// `Vector::get()` or `Vector::get_in_place()` found item `index` other
    /// than iterating did
    Get { index: usize },
}

impl From<DecodeError> for ReadError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

/// Evaluates `$expression` with `$reader` reading `$bytes` in `$layout`,
/// whose `TableReader` type depends on whether the layout is versioned
macro_rules! with_reader {
    ($bytes:expr, $layout:expr, $reader:ident => $expression:expr) => {{
        let layout = $layout.unversioned();
        if $layout.versioned {
            let $reader = Reader(TableReader::new($bytes, layout.with_version(0)));
            $expression
        } else {
            let $reader = Reader(TableReader::new($bytes, layout));
            $expression
        }
    }};
}

/// Reads a table from its own bytes, as the root or a vector item is read:
/// its header, whose size must be the table's byte length when it's sized,
/// and then its fields
pub fn read_table(bytes: &[u8], table: &Table) -> Result<Table, ReadError> {
    let layout = table.layout.unversioned();

    let (version, fields) = if table.layout.versioned {
        let reader = Reader(TableReader::new(bytes, layout.with_version(0)));
        reader.check_size(table.layout)?;
        (
            Some(reader.0.version()?),
            reader.read_root_fields(&table.fields)?,
        )
    } else {
        let reader = Reader(TableReader::new(bytes, layout));
        reader.check_size(table.layout)?;
        (None, reader.read_root_fields(&table.fields)?)
    };

    Ok(Table {
        layout: table.layout,
        version,
        fields,
    })
}

/// Reads root field `index` of a table of `layout` as `field`, without
/// reading the header
pub fn read_root_field(
    bytes: &[u8],
    layout: Layout,
    index: usize,
    field: &Field,
) -> Result<Field, ReadError> {
    with_reader!(bytes, layout, reader => reader.read_root_field(index, field))
}

/// Reads `field` at `position` of a table of `layout`
pub fn read_field(
    bytes: &[u8],
    layout: Layout,
    position: usize,
    field: &Field,
) -> Result<Field, ReadError> {
    with_reader!(bytes, layout, reader => reader.read_field(position, field))
}

struct Reader<'a, const V: bool>(TableReader<'a, V>);

impl<const V: bool> Reader<'_, V> {
    fn check_size(&self, layout: Layout) -> Result<(), ReadError> {
        let length = self.0.bytes().len();
        if layout.sized {
            let size = self.0.size()?;
            if size != length {
                return Err(ReadError::Size { size, length });
            }
        }

        Ok(())
    }

    fn read_root_fields(&self, fields: &[Field]) -> Result<Vec<Field>, ReadError> {
        let fields = fields.iter().enumerate();
        fields
            .map(|(index, field)| self.read_root_field(index, field))
            .collect()
    }

    fn read_root_field(&self, index: usize, field: &Field) -> Result<Field, ReadError> {
        self.read_field(self.0.root_offset(index)?, field)
    }

    fn read_field(&self, position: usize, field: &Field) -> Result<Field, ReadError> {
        let reader = &self.0;

        Ok(match field {
            Field::I8(_) => Field::I8(reader.get_i8(position)?),
            Field::U8(_) => Field::U8(reader.get_u8(position)?),
            Field::I16(_) => Field::I16(reader.get_i16(position)?),
            Field::U16(_) => Field::U16(reader.get_u16(position)?),
            Field::I32(_) => Field::I32(reader.get_i32(position)?),
            Field::U32(_) => Field::U32(reader.get_u32(position)?),
            Field::I64(_) => Field::I64(reader.get_i64(position)?),
            Field::U64(_) => Field::U64(reader.get_u64(position)?),
            Field::F32(_) => Field::F32(reader.get_f32(position)?),
            Field::F64(_) => Field::F64(reader.get_f64(position)?),
            Field::Bool(_) => Field::Bool(reader.get_u8(position)? != 0),
            Field::I8Array(_) => Field::I8Array(reader.get_i8_array(position)?.to_vec()),
            Field::U8Array(_) => Field::U8Array(reader.get_u8_array(position)?.to_vec()),
            Field::I16Array(_) => Field::I16Array(reader.get_i16_array(position)?.to_vec()),
            Field::U16Array(_) => Field::U16Array(reader.get_u16_array(position)?.to_vec()),
            Field::I32Array(_) => Field::I32Array(reader.get_i32_array(position)?.to_vec()),
            Field::U32Array(_) => Field::U32Array(reader.get_u32_array(position)?.to_vec()),
            Field::I64Array(_) => Field::I64Array(reader.get_i64_array(position)?.to_vec()),
            Field::U64Array(_) => Field::U64Array(reader.get_u64_array(position)?.to_vec()),
            Field::F32Array(_) => Field::F32Array(reader.get_f32_array(position)?.to_vec()),
            Field::F64Array(_) => Field::F64Array(reader.get_f64_array(position)?.to_vec()),
            Field::String(_) => Field::String(reader.get_str(position)?.to_owned()),
            Field::Struct(scalars) => {
                let size = struct_size(struct_types(scalars));
                let bytes = reader.get_struct(position, size)?;

                Field::Struct(read_struct(bytes, struct_types(scalars))?)
            }
            Field::StructArray { types, .. } => {
                let size = struct_size(types.iter().copied());
                let bytes = reader.get_struct_array(position, size)?;
                let items = bytes
                    .chunks_exact(size)
                    .map(|bytes| read_struct(bytes, types.iter().copied()))
                    .collect::<Result<_, _>>()?;

                Field::StructArray {
                    types: types.clone(),
                    items,
                }
            }
            Field::Table { nested, alignment } => Field::Table {
                nested: Nested {
                    from: nested.from,
                    table: self.read_nested(position, &nested.table)?,
                },
                alignment: *alignment,
            },
            Field::Vector(items) => Field::Vector(self.read_vector(position, items)?),
        })
    }

    /// Reads a table nested with `set_table()`
    fn read_nested(&self, position: usize, table: &Table) -> Result<Table, ReadError> {
        if self.reads_in_place(table) {
            return self.read_in_place(position, table);
        }

        // A position past the end leaves no bytes, which the first read fails
        // for
        let bytes = self.0.bytes();
        read_table(&bytes[position.min(bytes.len())..], table)
    }

    /// Reads a vector's items, after checking that finding each from its
    /// ends alone, and none past the last, finds what iterating finds
    fn read_vector(&self, position: usize, items: &[Nested]) -> Result<Vec<Nested>, ReadError> {
        let vector = self.0.get_vector(position)?;
        let iterated: Vec<_> = vector.iter().collect();
        let in_place: Vec<_> = vector.iter_in_place().collect();

        for index in 0..=iterated.len() {
            if vector.get(index) != iterated.get(index).cloned()
                || vector.get_in_place(index) != in_place.get(index).cloned()
            {
                return Err(ReadError::Get { index });
            }
        }

        let bytes = iterated.into_iter().collect::<Result<Vec<_>, _>>()?;
        let in_place = in_place.into_iter().collect::<Result<Vec<_>, _>>()?;
        if vector.len() != items.len() || bytes.len() != items.len() {
            return Err(ReadError::Items {
                count: vector.len(),
                iterated: bytes.len(),
                described: items.len(),
            });
        }

        let read_item = |((bytes, in_place), item): ((&[u8], &[u8]), &Nested)| {
            let table = if self.reads_in_place(&item.table) {
                // At its start, with a reader of its offset type, which is its
                // parent's
                let reader = TableReader::new(in_place, item.table.layout.unversioned());
                Reader(reader).read_in_place(0, &item.table)?
            } else {
                read_table(bytes, &item.table)?
            };

            Ok(Nested {
                from: item.from,
                table,
            })
        };

        bytes
            .into_iter()
            .zip(in_place)
            .zip(items)
            .map(read_item)
            .collect()
    }

    /// Whether a nested table is read in place: it has no header, and its
    /// parent's offset type
    fn reads_in_place(&self, table: &Table) -> bool {
        let layout = &table.layout;

        !layout.sized && !layout.versioned && layout.offset_type == self.0.offset_type()
    }

    /// Reads the fields of a table nested at `position`, from offsets counted
    /// from its start, within its parent's bytes
    fn read_in_place(&self, position: usize, table: &Table) -> Result<Table, ReadError> {
        let fields = table.fields.iter().enumerate();
        let fields = fields
            .map(|(index, field)| self.read_field(self.0.field_offset(position, index)?, field))
            .collect::<Result<_, _>>()?;

        Ok(Table {
            layout: table.layout,
            version: None,
            fields,
        })
    }
}

/// Reads the scalars of a struct of `types` at their offsets in its bytes,
/// which a `TableReader` reads in any layout
fn read_struct(
    bytes: &[u8],
    types: impl IntoIterator<Item = ScalarType>,
) -> Result<Vec<Field>, DecodeError> {
    let reader = TableReader::new(bytes, TableLayout::DEFAULT);
    let mut scalars = Vec::new();

    let mut offset = 0;
    for scalar in types {
        scalars.push(read_scalar(&reader, scalar, offset)?);
        offset += scalar.size();
    }

    Ok(scalars)
}

fn read_scalar(
    reader: &TableReader<'_>,
    scalar: ScalarType,
    position: usize,
) -> Result<Field, DecodeError> {
    Ok(match scalar {
        ScalarType::I8 => Field::I8(reader.get_i8(position)?),
        ScalarType::U8 => Field::U8(reader.get_u8(position)?),
        ScalarType::I16 => Field::I16(reader.get_i16(position)?),
        ScalarType::U16 => Field::U16(reader.get_u16(position)?),
        ScalarType::I32 => Field::I32(reader.get_i32(position)?),
        ScalarType::U32 => Field::U32(reader.get_u32(position)?),
        ScalarType::I64 => Field::I64(reader.get_i64(position)?),
        ScalarType::U64 => Field::U64(reader.get_u64(position)?),
        ScalarType::F32 => Field::F32(reader.get_f32(position)?),
        ScalarType::F64 => Field::F64(reader.get_f64(position)?),
    })
}
