//! Tables as JSON: a message's or a struct's table read from its bytes and
//! written as JSON, and written from JSON as its bytes, as README.md's
//! "Tables as JSON" describes the mapping.
//!
//! The schema's checked model leads every step, and the runtime's
//! `TableReader`, `TableSizer` and `TableWriter` read and write the bytes, so
//! that the format's rules lie in one place: the bytes that [`Root::encode()`]
//! writes are those that the code generated from the schema writes for the
//! same values, and [`Root::decode()`] reads the values that its readers read.

mod layout;
mod parse;
mod print;
mod read;
mod scalar;
mod syntax;
mod value;
mod write;

use std::{error::Error, fmt};

use crate::model::{Kind, OffsetType, Schema};
use crate::{Diagnostic, Span};

pub use read::MAX_READ_FACTOR;

/// The most tables that decoding and encoding nest one within another, the
/// message included, through fields and vectors.
///
/// A schema's tables nest in place at most 64 deep, but a vector of a table
/// that holds itself, such as a tree's, nests without end, and corrupted
/// bytes can make a vector's item hold itself, as `SPEC.md`'s "Validation"
/// describes. Generated readers read each item when they're asked for, and
/// never loop; decoding reads every item at once, so it bounds the depth, to
/// keep its stack within the WebAssembly module's.
pub const MAX_DEPTH: usize = 128;

/// A message or a struct of a schema, which is read from bytes on its own,
/// and so decoded and encoded
#[derive(Clone, Copy)]
pub struct Root<'a> {
    schema: &'a Schema,
    index: usize,
}

/// How [`Root::decode()`] and [`Root::layout()`] read a table
#[derive(Clone, Copy, Debug, Default)]
pub struct DecodeOptions {
    /// Reads a versioned message whatever version its header holds, where
    /// its generated reader fails with the decode error
    pub any_version: bool,
    /// Writes every character of a string outside ASCII as an escape, as
    /// `conformance/cases.json` does
    pub ascii: bool,
}

/// The annotated layout of a table's bytes, and the error of the read that
/// failed, if one did, after which the layout ends
pub struct ByteLayout {
    pub text: String,
    pub error: Option<DecodeError>,
}

impl<'a> Root<'a> {
    /// The message or struct named `name`
    ///
    /// # Errors
    ///
    /// [`NotARoot`] if the schema declares no message or struct of the name.
    pub fn find(schema: &'a Schema, name: &str) -> Result<Self, NotARoot> {
        let index = schema
            .definitions
            .iter()
            .position(|definition| definition.name == name);

        match index.map(|index| (index, schema.definition(index).kind)) {
            Some((index, Kind::Message(_) | Kind::Struct(_))) => Ok(Self { schema, index }),
            found => Err(NotARoot {
                name: name.to_owned(),
                is_table: found.is_some(),
                roots: schema
                    .definitions
                    .iter()
                    .filter(|definition| !matches!(definition.kind, Kind::Table))
                    .map(|definition| definition.name.clone())
                    .collect(),
            }),
        }
    }

    /// The JSON of the table that `bytes` hold, which ends with a line feed
    ///
    /// # Errors
    ///
    /// The [`DecodeError`] of bytes that aren't the table, with the path of
    /// the field that failed.
    pub fn decode(&self, bytes: &[u8], options: &DecodeOptions) -> Result<String, DecodeError> {
        let (value, _) = read::read(self.schema, self.index, bytes, options.any_version, false);

        Ok(print::print(
            self.schema,
            self.index,
            &value?,
            options.ascii,
        ))
    }

    /// The annotated layout of `bytes` as the table: each header slot,
    /// offset, count, value and run of padding, with its position, its
    /// length and the path of its field, up to the read that failed, if one
    /// did
    pub fn layout(&self, bytes: &[u8], options: &DecodeOptions) -> ByteLayout {
        let (value, parts) = read::read(self.schema, self.index, bytes, options.any_version, true);

        ByteLayout {
            text: layout::render(parts, bytes),
            error: value.err(),
        }
    }

    /// The bytes of the table that `json` describes
    ///
    /// # Errors
    ///
    /// Every error in the JSON, at its span, or that of a table too long for
    /// its offset type, at the name of the field that made it so.
    pub fn encode(&self, json: &str) -> Result<Vec<u8>, Vec<Diagnostic>> {
        let parsed = parse::parse(self.schema, self.index, json)?;

        write::write(self.schema, self.index, &parsed.value).map_err(|error| {
            let definition = self.schema.definition(self.index);
            let index = match error {
                tafla::BuildError::TooLong { index, .. } => Some(index),
                _ => None,
            };
            let span = index
                .and_then(|index| parsed.fields.get(index))
                .copied()
                .unwrap_or(Span { start: 0, end: 1 });

            let message = match (error, index) {
                (tafla::BuildError::TooLong { length, offset_type, .. }, Some(index)) => format!(
                    "the field `{}` makes `{}` {length} bytes, longer than its offset type, `{offset_type}`, addresses",
                    definition.fields[index].name, definition.name
                ),
                (error, _) => format!("`{}` can't be written: {error}", definition.name),
            };
            let help = format!(
                "a table of the offset type `{offset_type}` takes at most {max} bytes",
                offset_type = offset_type_name(self.schema, self.index),
                max = offset_type_max(self.schema, self.index),
            );
            vec![Diagnostic::new(message, span).with_help(help)]
        })
    }
}

fn offset_type_name(schema: &Schema, index: usize) -> &'static str {
    schema
        .definition(index)
        .layout()
        .map_or("uint32", |layout| layout.offset_type.name())
}

fn offset_type_max(schema: &Schema, index: usize) -> u64 {
    schema
        .definition(index)
        .layout()
        .map_or(OffsetType::Uint32, |layout| layout.offset_type)
        .max_value()
}

/// The error of a name that isn't a message or a struct of the schema
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotARoot {
    name: String,
    /// Whether the name is a table's, which is read within a message
    is_table: bool,
    /// The messages and structs of the schema
    roots: Vec<String>,
}

impl NotARoot {
    /// What the schema declares that can be decoded and encoded
    pub fn help(&self) -> String {
        let names: Vec<String> = self.roots.iter().map(|name| format!("`{name}`")).collect();
        match names.as_slice() {
            [] => "the schema declares no message or struct".to_owned(),
            [name] => format!("the schema's message or struct is {name}"),
            [rest @ .., last] => format!(
                "the schema's messages and structs are {} and {last}",
                rest.join(", ")
            ),
        }
    }
}

impl fmt::Display for NotARoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_table {
            write!(
                f,
                "`{}` is a table, which is read within a message, not on its own",
                self.name
            )
        } else {
            write!(
                f,
                "the schema declares no message or struct `{}`",
                self.name
            )
        }
    }
}

impl Error for NotARoot {}

/// The error of bytes that can't be read as a table: the runtime's decode
/// error, or a table that decoding doesn't read, and the path of the field
/// whose read failed
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    path: String,
    kind: DecodeErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DecodeErrorKind {
    /// The runtime's decode error, as a generated reader returns it
    Table(tafla::DecodeError),
    /// Tables that nest deeper than [`MAX_DEPTH`]
    TooDeep,
    /// Values that take more than [`MAX_READ_FACTOR`] times the bytes
    ReadTooOften,
}

impl DecodeError {
    /// The path of the field whose read failed, such as `columns.keys` or
    /// `items[2].pair`, or empty for the root's header
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Whether the bytes hold another version of the message, which
    /// [`DecodeOptions::any_version`] reads
    pub const fn is_unsupported_version(&self) -> bool {
        matches!(
            self.kind,
            DecodeErrorKind::Table(tafla::DecodeError::UnsupportedVersion { .. })
        )
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.path.is_empty() {
            write!(f, "`{}`: ", self.path)?;
        }

        match &self.kind {
            DecodeErrorKind::Table(error) => write!(f, "{error}"),
            DecodeErrorKind::TooDeep => write!(
                f,
                "tables nest more than {MAX_DEPTH} deep, as only corrupted bytes or a tree \
                 deeper than decoding reads hold them"
            ),
            DecodeErrorKind::ReadTooOften => write!(
                f,
                "offsets lead to the same bytes so often that their values would take more \
                 than {MAX_READ_FACTOR} times the bytes, as only corrupted bytes do"
            ),
        }
    }
}

impl Error for DecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            DecodeErrorKind::Table(error) => Some(error),
            DecodeErrorKind::TooDeep | DecodeErrorKind::ReadTooOften => None,
        }
    }
}

/// A step of the path from the root to a field: a field's name, or the
/// index of an array's element or a vector's item
#[derive(Clone, Copy)]
enum Segment<'a> {
    Field(&'a str),
    Index(usize),
}

/// A path as errors name it: `columns.keys`, `items[2].pair`
fn path_of(segments: &[Segment]) -> String {
    let mut path = String::new();
    for segment in segments {
        match segment {
            Segment::Field(name) => {
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(name);
            }
            Segment::Index(index) => {
                let _ = fmt::Write::write_fmt(&mut path, format_args!("[{index}]"));
            }
        }
    }
    path
}

/// The runtime's offset type of the schema's
const fn offset_type_of(offset_type: OffsetType) -> tafla::OffsetType {
    match offset_type {
        OffsetType::Uint8 => tafla::OffsetType::Uint8,
        OffsetType::Uint16 => tafla::OffsetType::Uint16,
        OffsetType::Uint32 => tafla::OffsetType::Uint32,
    }
}
