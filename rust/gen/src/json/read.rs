//! Reads a message's or a struct's values from a table's bytes, through the
//! runtime's `TableReader`, as the generated readers do, and records where
//! each part of the table lies when a layout is asked for.
//!
//! The bytes are untrusted. The runtime checks every read against them, and
//! this walk, which reads every field and item at once where a generated
//! reader reads one when it's asked for, bounds what corrupted bytes can
//! make it do: tables nest at most [`MAX_DEPTH`] deep, and the values read
//! take at most [`MAX_READ_FACTOR`] times the bytes.

use std::fmt::Write;

use tafla::{TableLayout, TableReader};

use super::layout::Part;
use super::scalar::ScalarValue;
use super::value::{Elements, Value};
use super::{DecodeError, DecodeErrorKind, MAX_DEPTH, Segment, offset_type_of, path_of};
use crate::model::{Definition, FieldType, Kind, Layout, Scalar, Schema};

/// How many times the bytes of a table its values may take, read in all.
///
/// A table that a writer wrote holds each value once, so its values take
/// less than its bytes. Offsets that corruption, or a writer of its own
/// kind, points at the same bytes read them once each, and a vector's items
/// that share a vector of such items read it once per path to it: 32 levels
/// of two items each would read 4 billion tables from a few hundred bytes.
/// The limit lets values share bytes a few times over, and keeps the time
/// and memory of a read in proportion to the bytes.
pub const MAX_READ_FACTOR: u64 = 8;

/// Reads the message or struct at `root` in the schema from `bytes`. A
/// versioned message's version must be its schema's unless `any_version` is
/// set, as its generated reader checks it. `with_parts` records the parts of
/// the table for its layout, which are returned whether the read succeeds or
/// fails, up to the field that failed.
///
/// # Errors
///
/// The decode error of bytes that aren't the table, with the path of the
/// field that failed.
pub fn read(
    schema: &Schema,
    root: usize,
    bytes: &[u8],
    any_version: bool,
    with_parts: bool,
) -> (Result<Value, DecodeError>, Vec<Part>) {
    let definition = schema.definition(root);
    let mut walk = Walk {
        schema,
        bytes,
        reader: TableReader::new(bytes, TableLayout::DEFAULT),
        remaining: (bytes.len() as u64).saturating_mul(MAX_READ_FACTOR),
        depth: 0,
        path: Vec::new(),
        parts: with_parts.then(Vec::new),
    };

    let value = match definition.kind {
        Kind::Message(layout) => walk.message(definition, layout, any_version),
        Kind::Struct(_) => walk.field(0, FieldType::Struct(root)),
        Kind::Table => unreachable!("a table is read within its message"),
    };
    (value, walk.parts.unwrap_or_default())
}

/// A message's reader of its own layout, which finds its fields. Every
/// other read is of a position, which no header moves.
enum Root<'a> {
    Plain(TableReader<'a>),
    Versioned(TableReader<'a, true>),
}

impl Root<'_> {
    fn offset(&self, index: usize) -> Result<usize, tafla::DecodeError> {
        match self {
            Self::Plain(reader) => reader.root_offset(index),
            Self::Versioned(reader) => reader.root_offset(index),
        }
    }
}

struct Walk<'a> {
    schema: &'a Schema,
    bytes: &'a [u8],
    /// Reads at positions in the message's bytes, in its offset type, which
    /// is the nested tables' too, since only `uint32` messages nest them
    reader: TableReader<'a>,
    /// The bytes that values may still take, of [`MAX_READ_FACTOR`] times
    /// the table's
    remaining: u64,
    /// How many tables the one being read is nested in
    depth: usize,
    /// The path of the field being read, from the root
    path: Vec<Segment<'a>>,
    /// The parts read so far, when the layout is asked for
    parts: Option<Vec<Part>>,
}

impl<'a> Walk<'a> {
    fn message(
        &mut self,
        definition: &'a Definition,
        layout: Layout,
        any_version: bool,
    ) -> Result<Value, DecodeError> {
        let offset_type = offset_type_of(layout.offset_type);
        let width = offset_type.size();
        let unversioned = TableLayout::DEFAULT.with_offset_type(offset_type);
        let unversioned = if layout.sized {
            unversioned.sized()
        } else {
            unversioned
        };
        self.reader = TableReader::new(self.bytes, unversioned);

        if layout.sized {
            let size = self.reader.size().ok();
            self.part(0, width, || format!("length {}", described(size)));
        }

        let root = if let Some(supported) = layout.version {
            let reader = TableReader::new(self.bytes, unversioned.with_version(supported));
            let version = reader.version();
            let position = unversioned.header_size();
            self.part(position, width, || {
                format!("version {}", described(version.as_ref().ok().copied()))
            });

            // A generated reader checks the version before it reads a field
            if !any_version {
                let version = self.checked(version)?;
                if version != supported {
                    return Err(self.error(DecodeErrorKind::Table(
                        tafla::DecodeError::UnsupportedVersion { version, supported },
                    )));
                }
            }
            Root::Versioned(reader)
        } else {
            Root::Plain(self.reader)
        };

        // The message is the first of the tables that nest
        self.depth = 1;
        let header_size = unversioned.header_size() + layout.version.map_or(0, |_| width);
        let mut values = Vec::with_capacity(definition.fields.len());
        for (index, field) in definition.fields.iter().enumerate() {
            self.path.push(Segment::Field(&field.name));

            let slot = header_size + index * width;
            let offset = root.offset(index);
            self.part(slot, width, || {
                format!("offset {}", described(offset.as_ref().ok().copied()))
            });
            let position = self.checked(offset)?;
            self.charge(width as u64)?;
            values.push(self.field(position, field.field_type)?);

            self.path.pop();
        }
        Ok(Value::Table(values))
    }

    /// Reads the table at `position`, in place: its offsets count from
    /// there, and it reads within the message's bytes
    fn table(
        &mut self,
        position: usize,
        definition: &'a Definition,
    ) -> Result<Vec<Value>, DecodeError> {
        if self.depth >= MAX_DEPTH {
            return Err(self.error(DecodeErrorKind::TooDeep));
        }
        self.depth += 1;

        let width = self.reader.offset_type().size();
        let mut values = Vec::with_capacity(definition.fields.len());
        for (index, field) in definition.fields.iter().enumerate() {
            self.path.push(Segment::Field(&field.name));

            let offset = self.reader.field_offset(position, index);
            self.part(position.saturating_add(index * width), width, || {
                let stored = offset.as_ref().ok().map(|offset| offset - position);
                format!("offset {}", described(stored))
            });
            let field_position = self.checked(offset)?;
            self.charge(width as u64)?;
            values.push(self.field(field_position, field.field_type)?);

            self.path.pop();
        }

        self.depth -= 1;
        Ok(values)
    }

    /// Reads the value of a field of `field_type` at `position`
    fn field(&mut self, position: usize, field_type: FieldType) -> Result<Value, DecodeError> {
        match field_type {
            FieldType::Scalar(scalar) => self.scalar(position, scalar).map(Value::Scalar),
            FieldType::Array(scalar) => self.array(position, scalar).map(Value::Array),
            FieldType::String => self.string(position).map(Value::String),
            FieldType::Struct(index) => self.structure(position, index).map(Value::Struct),
            FieldType::StructArray(index) => {
                self.struct_array(position, index).map(Value::StructArray)
            }
            FieldType::Table(index) => {
                let definition = self.schema.definition(index);
                self.table(position, definition).map(Value::Table)
            }
            FieldType::Vector(index) => self.vector(position, index).map(Value::Vector),
        }
    }

    fn scalar(&mut self, position: usize, scalar: Scalar) -> Result<ScalarValue, DecodeError> {
        let size = usize::from(scalar.size());
        let bytes = self.reader.get_struct(position, size);
        let bytes = self.checked(bytes)?;
        self.charge(size as u64)?;

        let value = ScalarValue::from_le_bytes(scalar, bytes);
        self.part(position, size, || {
            let mut description = format!("{} ", scalar.name());
            value.push_json(&mut description);
            // A byte that reads as true, but that no writer writes
            if scalar == Scalar::Bool && bytes[0] > 1 {
                let _ = write!(description, ", of the byte {}", bytes[0]);
            }
            description.replace('"', "")
        });
        Ok(value)
    }

    fn array(&mut self, position: usize, scalar: Scalar) -> Result<Elements, DecodeError> {
        let reader = self.reader;
        macro_rules! elements {
            ($getter:ident, $variant:ident) => {{
                let view = self.checked(reader.$getter(position))?;
                self.array_parts(
                    position,
                    view.len(),
                    usize::from(scalar.size()),
                    scalar.name(),
                )?;
                Elements::$variant(view.to_vec())
            }};
        }

        Ok(match scalar {
            Scalar::Bool => unreachable!("bools have no arrays"),
            Scalar::Uint8 => {
                let bytes = self.checked(reader.get_u8_array(position))?;
                self.array_parts(position, bytes.len(), 1, scalar.name())?;
                Elements::Uint8(bytes.to_vec())
            }
            Scalar::Int8 => elements!(get_i8_array, Int8),
            Scalar::Int16 => elements!(get_i16_array, Int16),
            Scalar::Uint16 => elements!(get_u16_array, Uint16),
            Scalar::Int32 => elements!(get_i32_array, Int32),
            Scalar::Uint32 => elements!(get_u32_array, Uint32),
            Scalar::Int64 => elements!(get_i64_array, Int64),
            Scalar::Uint64 => elements!(get_u64_array, Uint64),
            Scalar::Float32 => elements!(get_f32_array, Float32),
            Scalar::Float64 => elements!(get_f64_array, Float64),
        })
    }

    fn string(&mut self, position: usize) -> Result<String, DecodeError> {
        let string = self.reader.get_str(position);
        let string = self.checked(string)?;
        self.array_parts(position, string.len(), 1, "byte of UTF-8")?;

        Ok(string.to_owned())
    }

    fn structure(&mut self, position: usize, index: usize) -> Result<Vec<u8>, DecodeError> {
        let definition = self.schema.definition(index);
        let size = definition.struct_layout().size;
        let bytes = self.reader.get_struct(position, size);
        let bytes = self.checked(bytes)?;
        self.charge(size as u64)?;

        if self.parts.is_some() {
            self.struct_parts(position, definition, bytes);
        }
        Ok(bytes.to_vec())
    }

    fn struct_array(&mut self, position: usize, index: usize) -> Result<Vec<u8>, DecodeError> {
        let definition = self.schema.definition(index);
        let size = definition.struct_layout().size;
        let bytes = self.reader.get_struct_array(position, size);
        let bytes = self.checked(bytes)?;
        self.array_parts(position, bytes.len() / size, size, &definition.name)?;

        Ok(bytes.to_vec())
    }

    fn vector(&mut self, position: usize, index: usize) -> Result<Vec<Vec<Value>>, DecodeError> {
        let definition = self.schema.definition(index);
        let width = self.reader.offset_type().size();
        let vector = self.reader.get_vector(position);
        let vector = self.checked(vector)?;
        self.charge((vector.len() as u64 + 1) * width as u64)?;
        self.part(position, width, || format!("count {}", vector.len()));

        // The items are read in their order, each from the end of the one
        // before it, which fails at an item that ends before it starts, and
        // so before an item that corrupted ends start within the vector's
        // own count and ends, which could hold itself
        let mut items = Vec::new();
        for (item, bytes) in vector.iter_in_place().enumerate() {
            self.path.push(Segment::Index(item));

            let slot = position + (item + 1) * width;
            let end = self.reader.get_struct(slot, width).ok().map(|end| {
                let mut value = [0; 4];
                value[..width].copy_from_slice(end);
                u32::from_le_bytes(value)
            });
            self.part(slot, width, || format!("end {}", described(end)));

            // An item's bytes end where the message's do
            let bytes = self.checked(bytes)?;
            let start = self.bytes.len() - bytes.len();
            items.push(self.table(start, definition)?);

            self.path.pop();
        }
        Ok(items)
    }

    /// Counts the bytes of a value towards what the values may take
    fn charge(&mut self, byte_length: u64) -> Result<(), DecodeError> {
        match self.remaining.checked_sub(byte_length) {
            Some(remaining) => {
                self.remaining = remaining;
                Ok(())
            }
            None => Err(self.error(DecodeErrorKind::ReadTooOften)),
        }
    }

    /// Counts an array's count and its `count` elements of `size` bytes, at
    /// `position`, and records them as parts
    fn array_parts(
        &mut self,
        position: usize,
        count: usize,
        size: usize,
        element: &str,
    ) -> Result<(), DecodeError> {
        let width = self.reader.offset_type().size();
        self.charge((width + count * size) as u64)?;

        self.part(position, width, || format!("count {count}"));
        if count > 0 {
            self.part(position + width, count * size, || match count {
                1 => format!("1 {element}"),
                _ => format!("{count} {element}s").replace("byte of UTF-8s", "bytes of UTF-8"),
            });
        }
        Ok(())
    }

    /// Records each scalar of a struct at `position` as a part, those of
    /// the structs in it under their fields
    fn struct_parts(&mut self, position: usize, definition: &'a Definition, bytes: &[u8]) {
        let mut offset = 0;
        for field in &definition.fields {
            self.path.push(Segment::Field(&field.name));
            match field.field_type {
                FieldType::Scalar(scalar) => {
                    let value = ScalarValue::from_le_bytes(scalar, &bytes[offset..]);
                    self.part(position + offset, usize::from(scalar.size()), || {
                        let mut description = format!("{} ", scalar.name());
                        value.push_json(&mut description);
                        description.replace('"', "")
                    });
                    offset += usize::from(scalar.size());
                }
                FieldType::Struct(index) => {
                    let inner = self.schema.definition(index);
                    self.struct_parts(position + offset, inner, &bytes[offset..]);
                    offset += inner.struct_layout().size;
                }
                _ => unreachable!("structs hold scalars and structs only"),
            }
            self.path.pop();
        }
    }

    /// Records the part of `length` bytes at `start`, of the field being
    /// read, when the layout is asked for
    fn part(&mut self, start: usize, length: usize, describe: impl FnOnce() -> String) {
        if let Some(parts) = &mut self.parts {
            parts.push(Part {
                start,
                length,
                path: path_of(&self.path),
                description: describe(),
            });
        }
    }

    /// The value of a read, or the runtime's decode error at the field
    /// being read
    fn checked<T>(&self, read: Result<T, tafla::DecodeError>) -> Result<T, DecodeError> {
        read.map_err(|error| self.error(DecodeErrorKind::Table(error)))
    }

    fn error(&self, kind: DecodeErrorKind) -> DecodeError {
        DecodeError {
            path: path_of(&self.path),
            kind,
        }
    }
}

/// A header slot's or an offset's value in a layout, or that it lies past
/// the bytes
fn described(value: Option<impl std::fmt::Display>) -> String {
    value.map_or_else(|| "past the bytes".to_owned(), |value| value.to_string())
}
