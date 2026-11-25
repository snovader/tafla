//! Reads a message's or a struct's values from JSON, as README.md's "Tables
//! as JSON" describes. The schema leads the parse: each value is read as the
//! type of its field, so the JSON isn't held as a tree first, and each error
//! names the path of the field that it's in.
//!
//! A value of the wrong type, an unknown or repeated field and a missing one
//! are recorded, and the parse goes on past them, so that one run reports
//! them all, up to [`MAX_ERRORS`]. An error of JSON's syntax ends it.

use super::scalar::{ScalarValue, article};
use super::syntax::{Token, Tokens, unexpected};
use super::value::{Elements, Value};
use super::{MAX_DEPTH, Segment, path_of};
use crate::model::{Definition, Field, FieldType, Scalar, Schema};
use crate::{Diagnostic, Span};

/// The most errors that a parse reports
const MAX_ERRORS: usize = 50;

/// The values that the JSON holds
pub struct Parsed {
    pub value: Value,
    /// The span of each root field's name, which an error of a table too
    /// long for its offset type points at, as the field that made it so
    pub fields: Vec<Span>,
}

/// The result of reading a value: `Err` for an error of JSON's syntax, which
/// ends the parse, and `None` for a value with errors, which are recorded
type Step<T> = Result<Option<T>, Diagnostic>;

/// Reads the value of the message or struct at `root` from JSON
///
/// # Errors
///
/// Every error found, up to [`MAX_ERRORS`], or after the first error of
/// JSON's syntax, which ends the parse.
pub fn parse(schema: &Schema, root: usize, json: &str) -> Result<Parsed, Vec<Diagnostic>> {
    let mut parser = Parser {
        schema,
        tokens: Tokens::new(json),
        path: Vec::new(),
        depth: 0,
        errors: Vec::new(),
        root_fields: Vec::new(),
    };

    match parser.root(root) {
        Ok(Some(value)) if parser.errors.is_empty() => Ok(Parsed {
            value,
            fields: parser.root_fields,
        }),
        Ok(_) => Err(parser.errors),
        Err(error) => {
            parser.errors.push(error);
            Err(parser.errors)
        }
    }
}

struct Parser<'a> {
    schema: &'a Schema,
    tokens: Tokens<'a>,
    /// The path of the value being read, from the root
    path: Vec<Segment<'a>>,
    /// How many tables the one being read is nested in
    depth: usize,
    errors: Vec<Diagnostic>,
    root_fields: Vec<Span>,
}

impl<'a> Parser<'a> {
    fn root(&mut self, root: usize) -> Step<Value> {
        let definition = self.schema.definition(root);
        let (token, span) = self.tokens.next()?;

        let value = if definition.is_struct() {
            self.structure(root, &token, span)?.map(Value::Struct)
        } else {
            self.root_fields = vec![span; definition.fields.len()];
            self.table(root, &token, span)?.map(Value::Table)
        };

        let (end, end_span) = self.tokens.next()?;
        if end != Token::End {
            return Err(unexpected(&end, end_span, "the end of the text"));
        }
        Ok(value)
    }

    /// Reads the next value as a field of `field_type`
    fn value(&mut self, field_type: FieldType) -> Step<Value> {
        let (token, span) = self.tokens.next()?;

        Ok(match field_type {
            FieldType::Scalar(scalar) => self.scalar(scalar, &token, span)?.map(Value::Scalar),
            FieldType::Array(scalar) => self.array(scalar, &token, span)?.map(Value::Array),
            FieldType::String => self.string(token, span)?.map(Value::String),
            FieldType::Struct(index) => self.structure(index, &token, span)?.map(Value::Struct),
            FieldType::StructArray(index) => self
                .struct_array(index, &token, span)?
                .map(Value::StructArray),
            FieldType::Table(index) => self.table(index, &token, span)?.map(Value::Table),
            FieldType::Vector(index) => self.vector(index, &token, span)?.map(Value::Vector),
        })
    }

    fn scalar(&mut self, scalar: Scalar, token: &Token, span: Span) -> Step<ScalarValue> {
        let text = self.tokens.text(span);
        if let Some(value) = ScalarValue::from_json(scalar, token, text) {
            return Ok(Some(value));
        }

        let name = scalar.name();
        let what = format!("{} {name}", article(name));
        self.mismatch(token, span, &what, ScalarValue::expectation(scalar))
    }

    fn array(&mut self, scalar: Scalar, token: &Token, span: Span) -> Step<Elements> {
        let name = scalar.name();
        if *token != Token::BeginArray {
            let help = format!("an array of {name} is a JSON array of its elements");
            return self.mismatch(token, span, &format!("an array of {name}"), help);
        }

        let mut elements = Elements::new(scalar);
        let mut valid = true;
        let mut index = 0;
        while self.tokens.element(index == 0)? {
            self.path.push(Segment::Index(index));
            let (token, span) = self.tokens.next()?;
            match self.scalar(scalar, &token, span)? {
                Some(value) => elements.push(value),
                None => valid = false,
            }
            self.path.pop();
            index += 1;
        }
        Ok(valid.then_some(elements))
    }

    fn string(&mut self, token: Token, span: Span) -> Step<String> {
        match token {
            Token::String(string) => Ok(Some(string)),
            token => {
                let help = "a string is a JSON string".to_owned();
                self.mismatch(&token, span, "a string", help)
            }
        }
    }

    /// Reads a struct into bytes of its size, its scalars little-endian
    fn structure(&mut self, index: usize, token: &Token, span: Span) -> Step<Vec<u8>> {
        let definition = self.schema.definition(index);
        let mut bytes = vec![0; definition.struct_layout().size];

        let valid = self.struct_into(definition, token, span, &mut bytes)?;
        Ok(valid.then_some(bytes))
    }

    fn struct_array(&mut self, index: usize, token: &Token, span: Span) -> Step<Vec<u8>> {
        let definition = self.schema.definition(index);
        if *token != Token::BeginArray {
            let what = format!("an array of `{}`", definition.name);
            let help = "an array of structs is a JSON array of their objects".to_owned();
            return self.mismatch(token, span, &what, help);
        }

        let size = definition.struct_layout().size;
        let mut bytes = Vec::new();
        let mut valid = true;
        let mut item = 0;
        while self.tokens.element(item == 0)? {
            self.path.push(Segment::Index(item));
            let (token, span) = self.tokens.next()?;
            let start = bytes.len();
            bytes.resize(start + size, 0);
            valid &= self.struct_into(definition, &token, span, &mut bytes[start..])?;
            self.path.pop();
            item += 1;
        }
        Ok(valid.then_some(bytes))
    }

    /// Reads the object of a struct's fields into `bytes`, which start where
    /// the struct does, and returns whether every field was read
    fn struct_into(
        &mut self,
        definition: &'a Definition,
        token: &Token,
        span: Span,
        bytes: &mut [u8],
    ) -> Result<bool, Diagnostic> {
        if *token != Token::BeginObject {
            let what = format!("a `{}`", definition.name);
            let help = "a struct is an object of its fields, by their names".to_owned();
            return Ok(self.mismatch::<()>(token, span, &what, help)?.is_some());
        }

        let offsets = struct_offsets(self.schema, definition);
        self.members(definition, span, &mut |parser, index, field| {
            let bytes = &mut bytes[offsets[index]..];
            let (token, span) = parser.tokens.next()?;

            match field.field_type {
                FieldType::Scalar(scalar) => {
                    let value = parser.scalar(scalar, &token, span)?;
                    Ok(value.map(|value| value.write_le(bytes)).is_some())
                }
                FieldType::Struct(inner) => {
                    let inner = parser.schema.definition(inner);
                    parser.struct_into(inner, &token, span, bytes)
                }
                _ => unreachable!("structs hold scalars and structs only"),
            }
        })
    }

    /// Reads the object of a table's fields
    fn table(&mut self, index: usize, token: &Token, span: Span) -> Step<Vec<Value>> {
        let definition = self.schema.definition(index);
        if *token != Token::BeginObject {
            let what = format!("a `{}`", definition.name);
            let help = "a table is an object of its fields, by their names".to_owned();
            return self.mismatch(token, span, &what, help);
        }
        if self.depth >= MAX_DEPTH {
            let message = format!("tables nest more than {MAX_DEPTH} deep here");
            let help = "encode writes tables nested up to that depth, as decode reads them";
            self.record(Diagnostic::new(message, span).with_help(help))?;
            self.tokens.skip(token)?;
            return Ok(None);
        }

        self.depth += 1;
        let mut values: Vec<Option<Value>> = definition.fields.iter().map(|_| None).collect();
        let valid = self.members(definition, span, &mut |parser, index, field| {
            values[index] = parser.value(field.field_type)?;
            Ok(values[index].is_some())
        })?;
        self.depth -= 1;

        // Every field is read once the object is valid, missing ones included
        Ok(valid.then(|| values.into_iter().flatten().collect()))
    }

    fn vector(&mut self, index: usize, token: &Token, span: Span) -> Step<Vec<Vec<Value>>> {
        if *token != Token::BeginArray {
            let what = format!("a vector of `{}`", self.schema.definition(index).name);
            let help = "a vector is a JSON array of its tables' objects".to_owned();
            return self.mismatch(token, span, &what, help);
        }

        let mut items = Vec::new();
        let mut valid = true;
        let mut item = 0;
        while self.tokens.element(item == 0)? {
            self.path.push(Segment::Index(item));
            let (token, span) = self.tokens.next()?;
            match self.table(index, &token, span)? {
                Some(values) => items.push(values),
                None => valid = false,
            }
            self.path.pop();
            item += 1;
        }
        Ok(valid.then_some(items))
    }

    /// Reads the members of an object of a definition's fields, whose `{`
    /// at `open` was read, each field's value with `read`, and returns
    /// whether every field was given once, and read
    fn members(
        &mut self,
        definition: &'a Definition,
        open: Span,
        read: &mut dyn FnMut(&mut Self, usize, &'a Field) -> Result<bool, Diagnostic>,
    ) -> Result<bool, Diagnostic> {
        let mut given = vec![false; definition.fields.len()];
        let mut valid = true;
        let mut first = true;

        loop {
            let (key, key_span) = self.tokens.member(first)?;
            let Some(key) = key else {
                break;
            };
            first = false;

            let index = definition.fields.iter().position(|field| field.name == key);
            let Some(index) = index.filter(|&index| !given[index]) else {
                let error = if index.is_some() {
                    Diagnostic::new(format!("the field `{key}` is given twice"), key_span)
                        .with_help("each field is given once")
                } else {
                    unknown_field(definition, &key, key_span)
                };
                self.record(error)?;
                let (token, _) = self.tokens.next()?;
                self.tokens.skip(&token)?;
                valid = false;
                continue;
            };
            given[index] = true;
            if self.path.is_empty() {
                if let Some(span) = self.root_fields.get_mut(index) {
                    *span = key_span;
                }
            }

            let field = &definition.fields[index];
            self.path.push(Segment::Field(&field.name));
            valid &= read(self, index, field)?;
            self.path.pop();
        }

        let missing: Vec<String> = definition
            .fields
            .iter()
            .zip(&given)
            .filter(|(_, given)| !**given)
            .map(|(field, _)| {
                self.path.push(Segment::Field(&field.name));
                let path = path_of(&self.path);
                self.path.pop();
                path
            })
            .collect();
        if !missing.is_empty() {
            let fields = if missing.len() == 1 {
                "field"
            } else {
                "fields"
            };
            let message = format!("missing the {fields} {}", listed(&missing));
            self.record(Diagnostic::new(message, open).with_help("every field is required"))?;
            valid = false;
        }

        Ok(valid)
    }

    /// Records that the value at `span`, which starts with `token`, isn't
    /// `what` the field expects, and reads past it
    fn mismatch<T>(&mut self, token: &Token, span: Span, what: &str, help: String) -> Step<T> {
        let subject = if self.path.is_empty() {
            "the JSON".to_owned()
        } else {
            format!("`{}`", path_of(&self.path))
        };
        let error = Diagnostic::new(format!("{subject} isn't {what}"), span).with_help(help);

        self.record(error)?;
        self.tokens.skip(token)?;
        Ok(None)
    }

    /// Records an error, or ends the parse with it once there are
    /// [`MAX_ERRORS`]
    fn record(&mut self, error: Diagnostic) -> Result<(), Diagnostic> {
        if self.errors.len() + 1 >= MAX_ERRORS {
            return Err(error);
        }
        self.errors.push(error);
        Ok(())
    }
}

/// The error of a key that names no field of the definition
fn unknown_field(definition: &Definition, key: &str, span: Span) -> Diagnostic {
    let names: Vec<String> = definition
        .fields
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let fields = if names.len() == 1 {
        "field is"
    } else {
        "fields are"
    };

    Diagnostic::new(format!("`{}` has no field `{key}`", definition.name), span)
        .with_help(format!("its {fields} {}", listed(&names)))
}

/// Names in backticks, as a list: `a`, `b` and `c`
fn listed(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// The offset of each of a struct's fields from its start
fn struct_offsets(schema: &Schema, definition: &Definition) -> Vec<usize> {
    let mut offset = 0;
    definition
        .fields
        .iter()
        .map(|field| {
            let start = offset;
            offset += match field.field_type {
                FieldType::Scalar(scalar) => usize::from(scalar.size()),
                FieldType::Struct(index) => schema.definition(index).struct_layout().size,
                _ => unreachable!("structs hold scalars and structs only"),
            };
            start
        })
        .collect()
}
