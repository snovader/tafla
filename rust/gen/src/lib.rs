#![doc = include_str!("../README.md")]

mod check;
pub mod cli;
mod diagnostic;
mod generate;
mod lexer;
mod model;
mod naming;
mod parser;
#[cfg(target_family = "wasm")]
mod wasm;

pub use diagnostic::{Diagnostic, Span};
pub use generate::{GeneratedFile, InvalidJavaPackage, JavaPackage, Options, Target};
pub use model::Schema;

/// Parses and checks a schema.
///
/// # Errors
///
/// Returns the first syntax error, or every error the checks find.
pub fn parse(source: &str) -> Result<Schema, Vec<Diagnostic>> {
    let declarations = parser::parse(source).map_err(|error| vec![error])?;
    check::check(&declarations)
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
