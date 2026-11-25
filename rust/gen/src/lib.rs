#![doc = include_str!("../README.md")]

mod check;
pub mod cli;
mod compatibility;
mod describe;
mod diagnostic;
mod generate;
mod lexer;
mod model;
mod naming;
mod parser;
#[cfg(target_family = "wasm")]
mod wasm;

pub use compatibility::Comparison;
pub use describe::DESCRIPTION_VERSION;
pub use diagnostic::{Diagnostic, Severity, Span};
pub use generate::{GeneratedFile, InvalidJavaPackage, JavaPackage, Options, Target};
pub use model::Schema;

/// Parses and checks a schema.
///
/// # Errors
///
/// Returns the first syntax error, or every error the checks find.
pub fn parse(source: &str) -> Result<Schema, Vec<Diagnostic>> {
    parse_declarations(source).map(|(_, schema)| schema)
}

/// A schema's declarations, which hold where each lies in the source, and
/// the schema checked from them
fn parse_declarations(source: &str) -> Result<(Vec<parser::Declaration>, Schema), Vec<Diagnostic>> {
    let declarations = parser::parse(source).map_err(|error| vec![error])?;
    let schema = check::check(&declarations)?;
    Ok((declarations, schema))
}

/// Describes a schema as JSON, for tools that aren't code generators: its
/// declarations and fields, with the layout, the sizes and the offsets that
/// the format gives them, as README.md's "Description" specifies.
/// `file_name` is the schema's path in the description.
///
/// # Errors
///
/// Returns the schema's errors, as [`parse`] does.
pub fn describe(file_name: &str, source: &str) -> Result<String, Vec<Diagnostic>> {
    let schema = parse(source)?;
    Ok(describe::describe(&[(file_name, &schema)]))
}

/// The errors of the schemas that [`compatibility()`] couldn't compare, each
/// of which points into its own schema's source
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InvalidSchemas {
    /// The old schema's errors, or none
    pub old: Vec<Diagnostic>,
    /// The new schema's errors, or none
    pub new: Vec<Diagnostic>,
}

/// Compares a new version of a schema with an old one under the format's
/// evolution rules, as README.md's "Compatibility" lists them: where a
/// reader generated from one would misread the tables written with the
/// other. The errors and notes point into the new schema's source.
///
/// # Errors
///
/// Returns the errors of each schema that has any, as [`parse`] does.
pub fn compatibility(old_source: &str, new_source: &str) -> Result<Comparison, InvalidSchemas> {
    match (parse(old_source), parse_declarations(new_source)) {
        (Ok(old), Ok((declarations, new))) => Ok(compatibility::compare(&old, &new, &declarations)),
        (old, new) => Err(InvalidSchemas {
            old: old.err().unwrap_or_default(),
            new: new.err().unwrap_or_default(),
        }),
    }
}

/// Generates the code for a schema, whose file name names the output files
///
/// # Errors
///
/// Returns the schema's errors, as [`parse`] does.
pub fn compile(
    file_name: &str,
    source: &str,
    target: Target,
    options: &Options,
) -> Result<Vec<GeneratedFile>, Vec<Diagnostic>> {
    let schema = parse(source)?;
    Ok(generate::generate(&schema, file_name, target, options))
}
