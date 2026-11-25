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
/// Returns the schema's errors, as [`parse`] does, or an error at the
/// schema's first optional field or field added `since` a version, if the
/// target doesn't generate such fields.
pub fn compile(
    file_name: &str,
    source: &str,
    target: Target,
    options: &Options,
) -> Result<Vec<GeneratedFile>, Vec<Diagnostic>> {
    let schema = parse(source)?;

    // A target that doesn't know such fields would generate readers that
    // read them as required, and writers that can't leave them out
    if let Some(span) = schema.presence()
        && !target.generates_missing_fields()
    {
        let message = format!(
            "the `{}` target doesn't generate optional fields or fields added `since` a version yet",
            target.name()
        );
        let error = Diagnostic::new(message, span)
            .with_help("only the `typescript` and `javascript` targets generate them");
        return Err(vec![error]);
    }

    Ok(generate::generate(&schema, file_name, target, options))
}
