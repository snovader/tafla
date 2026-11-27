//! Tables as the conformance suite's `cases.json` describes them, which
//! `conformance.rs` parses and `strategy` generates. See
//! `conformance/README.md` at the root of the repository.

use tafla::{Alignment, OffsetType, TableLayout};

/// A table: its layout, its version and its fields, in the order that
/// they're set
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub layout: Layout,
    /// The version in the header, present only when the layout is
    /// versioned
    pub version: Option<u32>,
    pub fields: Vec<Field>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub sized: bool,
    pub versioned: bool,
    pub offset_type: OffsetType,
}

/// A field of each type that a table holds, with its value. Equal fields
/// hold floats of the same bits, so that NaN equals itself and -0 doesn't
/// equal 0.
#[derive(Clone, Debug)]
pub enum Field {
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    /// A bool, which the runtimes have no API for: it's written as the
    /// uint8 1 or 0, and read as true when its byte isn't 0. It's a field
    /// only, rather than a scalar of structs.
    Bool(bool),
    I8Array(Vec<i8>),
    U8Array(Vec<u8>),
    I16Array(Vec<i16>),
    U16Array(Vec<u16>),
    I32Array(Vec<i32>),
    U32Array(Vec<u32>),
    I64Array(Vec<i64>),
    U64Array(Vec<u64>),
    F32Array(Vec<f32>),
    F64Array(Vec<f64>),
    String(String),
    /// A struct of scalar fields, one after another
    Struct(Vec<Self>),
    /// An array of structs of scalars of `types`, as the scalar fields of
    /// each struct
    StructArray {
        types: Vec<ScalarType>,
        items: Vec<Vec<Self>>,
    },
    /// A table nested with `set_table()`, or with `set_table_aligned()`
    /// when the description gives an alignment
    Table {
        nested: Nested,
        alignment: Option<Alignment>,
    },
    Vector(Vec<Nested>),
    /// A vector of strings, each found from its end and the end of the one
    /// before it
    StringVector(Vec<String>),
}

/// A table nested from its builder or from the bytes that its builder
/// writes
#[derive(Clone, Debug, PartialEq)]
pub struct Nested {
    pub from: Source,
    pub table: Table,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Builder,
    Bytes,
}

/// The type of a scalar, of which structs are made
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarType {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
}

impl ScalarType {
    pub const ALL: [Self; 10] = [
        Self::I8,
        Self::U8,
        Self::I16,
        Self::U16,
        Self::I32,
        Self::U32,
        Self::I64,
        Self::U64,
        Self::F32,
        Self::F64,
    ];

    /// The scalar's size in bytes, which is also its alignment in an array
    pub const fn alignment(self) -> Alignment {
        match self {
            Self::I8 | Self::U8 => Alignment::One,
            Self::I16 | Self::U16 => Alignment::Two,
            Self::I32 | Self::U32 | Self::F32 => Alignment::Four,
            Self::I64 | Self::U64 | Self::F64 => Alignment::Eight,
        }
    }

    pub const fn size(self) -> usize {
        self.alignment().bytes()
    }
}

/// The size of a struct of scalars of `types`: the sum of theirs
pub fn struct_size(types: impl IntoIterator<Item = ScalarType>) -> usize {
    types.into_iter().map(ScalarType::size).sum()
}

/// The alignment of an array of structs of scalars of `types`: the largest
/// of their sizes
pub fn struct_alignment(types: impl IntoIterator<Item = ScalarType>) -> Alignment {
    let alignments = types.into_iter().map(ScalarType::alignment);
    alignments.max().unwrap_or(Alignment::One)
}

/// The types of a struct's scalar fields
pub fn struct_types(scalars: &[Field]) -> impl Iterator<Item = ScalarType> + '_ {
    let scalar_type = |field: &Field| field.scalar_type().expect("A struct holds scalars");
    scalars.iter().map(scalar_type)
}

/// The bytes of a struct of scalar fields: theirs, one after another
pub fn struct_bytes(scalars: &[Field]) -> Vec<u8> {
    let scalar_bytes = |field: &Field| field.scalar_bytes().expect("A struct holds scalars");
    scalars.iter().flat_map(scalar_bytes).collect()
}

impl Layout {
    /// A layout without header slots, which tables nested with
    /// `set_table()` have
    pub const fn headerless(offset_type: OffsetType) -> Self {
        Self {
            sized: false,
            versioned: false,
            offset_type,
        }
    }

    /// The layout without its version slot, which `versioned()` adds
    pub const fn unversioned(self) -> TableLayout {
        let layout = TableLayout::DEFAULT.with_offset_type(self.offset_type);
        if self.sized { layout.sized() } else { layout }
    }

    /// The byte length of the header slots before the field offsets
    pub const fn header_size(self) -> usize {
        (self.sized as usize + self.versioned as usize) * self.offset_type.size()
    }
}

impl Field {
    /// The type of a scalar field, or `None` for a field of any other type
    pub const fn scalar_type(&self) -> Option<ScalarType> {
        Some(match self {
            Self::I8(_) => ScalarType::I8,
            Self::U8(_) => ScalarType::U8,
            Self::I16(_) => ScalarType::I16,
            Self::U16(_) => ScalarType::U16,
            Self::I32(_) => ScalarType::I32,
            Self::U32(_) => ScalarType::U32,
            Self::I64(_) => ScalarType::I64,
            Self::U64(_) => ScalarType::U64,
            Self::F32(_) => ScalarType::F32,
            Self::F64(_) => ScalarType::F64,
            _ => return None,
        })
    }

    /// The little-endian bytes of a scalar field, or `None` for a field of
    /// any other type
    pub fn scalar_bytes(&self) -> Option<Vec<u8>> {
        Some(match self {
            Self::I8(value) => value.to_le_bytes().to_vec(),
            Self::U8(value) => value.to_le_bytes().to_vec(),
            Self::I16(value) => value.to_le_bytes().to_vec(),
            Self::U16(value) => value.to_le_bytes().to_vec(),
            Self::I32(value) => value.to_le_bytes().to_vec(),
            Self::U32(value) => value.to_le_bytes().to_vec(),
            Self::I64(value) => value.to_le_bytes().to_vec(),
            Self::U64(value) => value.to_le_bytes().to_vec(),
            Self::F32(value) => value.to_le_bytes().to_vec(),
            Self::F64(value) => value.to_le_bytes().to_vec(),
            _ => return None,
        })
    }
}

impl PartialEq for Field {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::F32(a), Self::F32(b)) => a.to_bits() == b.to_bits(),
            (Self::F64(a), Self::F64(b)) => a.to_bits() == b.to_bits(),
            (Self::F32Array(a), Self::F32Array(b)) => a
                .iter()
                .map(|value| value.to_bits())
                .eq(b.iter().map(|value| value.to_bits())),
            (Self::F64Array(a), Self::F64Array(b)) => a
                .iter()
                .map(|value| value.to_bits())
                .eq(b.iter().map(|value| value.to_bits())),
            (Self::I8(a), Self::I8(b)) => a == b,
            (Self::U8(a), Self::U8(b)) => a == b,
            (Self::I16(a), Self::I16(b)) => a == b,
            (Self::U16(a), Self::U16(b)) => a == b,
            (Self::I32(a), Self::I32(b)) => a == b,
            (Self::U32(a), Self::U32(b)) => a == b,
            (Self::I64(a), Self::I64(b)) => a == b,
            (Self::U64(a), Self::U64(b)) => a == b,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::I8Array(a), Self::I8Array(b)) => a == b,
            (Self::U8Array(a), Self::U8Array(b)) => a == b,
            (Self::I16Array(a), Self::I16Array(b)) => a == b,
            (Self::U16Array(a), Self::U16Array(b)) => a == b,
            (Self::I32Array(a), Self::I32Array(b)) => a == b,
            (Self::U32Array(a), Self::U32Array(b)) => a == b,
            (Self::I64Array(a), Self::I64Array(b)) => a == b,
            (Self::U64Array(a), Self::U64Array(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Struct(a), Self::Struct(b)) => a == b,
            (
                Self::StructArray {
                    types: a_types,
                    items: a,
                },
                Self::StructArray {
                    types: b_types,
                    items: b,
                },
            ) => a_types == b_types && a == b,
            (
                Self::Table {
                    nested: a,
                    alignment: a_alignment,
                },
                Self::Table {
                    nested: b,
                    alignment: b_alignment,
                },
            ) => a == b && a_alignment == b_alignment,
            (Self::Vector(a), Self::Vector(b)) => a == b,
            (Self::StringVector(a), Self::StringVector(b)) => a == b,
            _ => false,
        }
    }
}
