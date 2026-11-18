//! Code generators, one per target language. Each writes code that its
//! language's formatter leaves as it is, so generated files can be checked in
//! and linted with the rest of a project.
//!
//! Each generator has a Cargo feature, which the default features enable. A
//! build without a generator's feature doesn't know its target.

#[cfg(feature = "cpp")]
mod cpp;
#[cfg(feature = "csharp")]
mod csharp;
// The fields that Go's, Rust's, C++'s, TypeScript's and C#'s generated code
// lays out itself
#[cfg(any(
    feature = "go",
    feature = "rust",
    feature = "cpp",
    feature = "typescript",
    feature = "csharp"
))]
mod fixed;
#[cfg(feature = "go")]
mod go;
#[cfg(feature = "java")]
mod java;
// Options name a Java package in every build
mod java_package;
#[cfg(any(
    feature = "typescript",
    feature = "python",
    feature = "java",
    feature = "rust",
    feature = "go",
    feature = "cpp",
    feature = "swift",
    feature = "csharp"
))]
mod prose;
#[cfg(feature = "python")]
mod python;
// The checker rejects every target's names in every build
pub mod reserved;
#[cfg(feature = "rust")]
mod rust;
#[cfg(feature = "swift")]
mod swift;
#[cfg(feature = "typescript")]
mod typescript;
#[cfg(any(
    feature = "typescript",
    feature = "python",
    feature = "java",
    feature = "rust",
    feature = "go",
    feature = "cpp",
    feature = "swift",
    feature = "csharp"
))]
// Go's and C++'s generators write their lines without the others'
// expressions, and Swift's without some of their checks of width
#[cfg_attr(
    not(any(
        feature = "typescript",
        feature = "python",
        feature = "java",
        feature = "rust",
        feature = "csharp"
    )),
    allow(dead_code)
)]
mod writer;

pub use java_package::{InvalidJavaPackage, JavaPackage};

use crate::lexer::garbles_text;
use crate::model::Schema;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    #[cfg(feature = "typescript")]
    TypeScript,
    /// ES modules with `JSDoc` types
    #[cfg(feature = "typescript")]
    JavaScript,
    #[cfg(feature = "python")]
    Python,
    #[cfg(feature = "java")]
    Java,
    #[cfg(feature = "rust")]
    Rust,
    #[cfg(feature = "go")]
    Go,
    /// A header of C++20
    #[cfg(feature = "cpp")]
    Cpp,
    #[cfg(feature = "swift")]
    Swift,
    /// C# 14, for .NET 10
    #[cfg(feature = "csharp")]
    CSharp,
}

impl Target {
    /// The targets that this build generates, each after its names
    const TARGETS: &[(&[&str], Self)] = &[
        #[cfg(feature = "typescript")]
        (&["typescript", "ts"], Self::TypeScript),
        #[cfg(feature = "typescript")]
        (&["javascript", "js"], Self::JavaScript),
        #[cfg(feature = "python")]
        (&["python", "py"], Self::Python),
        #[cfg(feature = "java")]
        (&["java"], Self::Java),
        #[cfg(feature = "rust")]
        (&["rust", "rs"], Self::Rust),
        #[cfg(feature = "go")]
        (&["go"], Self::Go),
        #[cfg(feature = "cpp")]
        (&["cpp"], Self::Cpp),
        #[cfg(feature = "swift")]
        (&["swift"], Self::Swift),
        #[cfg(feature = "csharp")]
        (&["csharp", "cs"], Self::CSharp),
    ];

    /// The target of a name or its abbreviation, if this build generates it
    pub fn from_name(name: &str) -> Option<Self> {
        Self::TARGETS
            .iter()
            .find(|(names, _)| names.contains(&name))
            .map(|&(_, target)| target)
    }

    /// The full names of the targets that this build generates
    pub fn names() -> impl Iterator<Item = &'static str> {
        Self::TARGETS.iter().map(|(names, _)| names[0])
    }

    /// The names of each target that this build generates: its full name,
    /// and then its abbreviation if it has one
    pub fn names_and_abbreviations() -> impl Iterator<Item = &'static [&'static str]> {
        Self::TARGETS.iter().map(|&(names, _)| names)
    }
}

/// Where the generated code finds its runtime, and where it goes
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The runtime's module, package, crate or namespace, which is tafla's
    /// runtime for the target by default: `tafla` for TypeScript, JavaScript,
    /// Python and Rust, `io.github.snovader.tafla` for Java, and `Tafla` for
    /// Swift and C#
    pub runtime: Option<String>,
    /// The package of generated Java classes, which is `generated` by
    /// default, and the namespace of C++ and C#, which is named after the
    /// schema by default. Other targets ignore it.
    pub package: Option<JavaPackage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedFile {
    /// Relative to the output directory and within it, with `/` separators
    pub path: String,
    pub contents: String,
}

/// Generates code for a checked schema from the file `file_name`, which the
/// generated files' header names
// A build without generators has no target to call this with, and Go's
// generator writes a header of its own
#[cfg_attr(
    not(any(
        feature = "typescript",
        feature = "python",
        feature = "java",
        feature = "rust",
        feature = "cpp",
        feature = "swift",
        feature = "csharp"
    )),
    allow(unused_variables)
)]
pub fn generate(
    schema: &Schema,
    file_name: &str,
    target: Target,
    options: &Options,
) -> Vec<GeneratedFile> {
    let header = format!(
        "Generated by tafla-gen from {}. Do not edit.",
        comment_text(file_name)
    );

    match target {
        #[cfg(feature = "typescript")]
        Target::TypeScript => typescript::generate(schema, stem(file_name), &header, options, true),
        #[cfg(feature = "typescript")]
        Target::JavaScript => {
            typescript::generate(schema, stem(file_name), &header, options, false)
        }
        #[cfg(feature = "python")]
        Target::Python => python::generate(schema, stem(file_name), &header, options),
        #[cfg(feature = "java")]
        Target::Java => java::generate(schema, &header, options),
        #[cfg(feature = "rust")]
        Target::Rust => rust::generate(schema, stem(file_name), &header, options),
        #[cfg(feature = "go")]
        Target::Go => go::generate(schema, stem(file_name), &comment_text(file_name), options),
        #[cfg(feature = "cpp")]
        Target::Cpp => cpp::generate(schema, stem(file_name), &header, options),
        #[cfg(feature = "swift")]
        Target::Swift => swift::generate(schema, stem(file_name), &header, options),
        #[cfg(feature = "csharp")]
        Target::CSharp => csharp::generate(schema, stem(file_name), &header, options),
    }
}

/// A file name in a line comment of any target, with a replacement character
/// for each character that would end the line or garble it, such as a
/// newline, which a path can hold on Unix
fn comment_text(file_name: &str) -> String {
    file_name
        .chars()
        .map(|character| {
            if garbles_text(character) {
                char::REPLACEMENT_CHARACTER
            } else {
                character
            }
        })
        .collect()
}

/// The schema's file name without its directory and extensions, which names
/// the file of the generators that write one per schema. Java's writes one
/// per class. On Windows, the directory may end with `\` or a drive's `:`,
/// either of which would take the file out of the output directory.
#[cfg(any(
    feature = "typescript",
    feature = "python",
    feature = "rust",
    feature = "go",
    feature = "cpp",
    feature = "swift",
    feature = "csharp"
))]
fn stem(file_name: &str) -> &str {
    let name = file_name
        .rsplit(['/', '\\', ':'])
        .next()
        .unwrap_or(file_name);
    name.split('.').next().unwrap_or(name)
}
