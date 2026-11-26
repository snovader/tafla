//! The `tafla-gen` command without its files: what its arguments ask for,
//! and what it generates from the schemas that its caller reads, or prints
//! about them for its subcommands, `describe` and `check`.
//!
//! The command's binary and the hosts of the WebAssembly module both run
//! it, so that each of them only reads schemas, writes files and prints,
//! and they agree on the arguments, the errors and the exit codes, which
//! README.md's "Command" describes.
//!
//! ```
//! use tafla_gen::cli::Command;
//!
//! let arguments = ["python", "point.tafla", "--out", "generated"];
//! let Command::Generate(generation) = Command::parse(arguments.map(Into::into)) else {
//!     panic!("the arguments ask for code");
//! };
//! assert_eq!(generation.schemas(), ["point.tafla"]);
//! assert_eq!(generation.out(), "generated");
//!
//! let generated = generation.generate(|_schema| Ok(b"message Point { x: float64; }".to_vec()));
//! assert!(generated.errors.is_empty());
//! assert_eq!(generated.file_sets[0][0].path, "point.py");
//! ```

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fmt::Write;

use crate::{
    Diagnostic, GeneratedFile, JavaPackage, Options, Schema, Span, Target, compatibility, compile,
    describe,
};

/// The exit code of a command whose schemas all generated their files, were
/// all described, or are compatible
pub const SUCCESS: u8 = 0;
/// The exit code of a schema with errors, or a file that can't be read or
/// written, or a schema that's incompatible with the one before it
pub const FAILURE: u8 = 1;
/// The exit code of wrong arguments, as of most commands
pub const USAGE_ERROR: u8 = 2;

/// What the command's arguments ask for
pub enum Command {
    /// To print and exit, without reading a schema: the help, the version,
    /// or what's wrong with the arguments
    Print(Output),
    Generate(Generation),
    /// To print about schemas rather than generate from them, as the
    /// subcommands `describe` and `check` do
    Inspect(Inspection),
}

/// What a command prints before it exits
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub code: u8,
}

/// The schemas to generate code from, and where and how
pub struct Generation {
    target: Target,
    schemas: Vec<String>,
    out: String,
    options: Options,
}

/// The schemas that a subcommand reads, and what it prints about them
pub struct Inspection {
    subcommand: Subcommand,
    /// For `check`, the new schema and then the old one
    schemas: Vec<String>,
}

/// A subcommand, which takes the place of a target
#[derive(Clone, Copy, PartialEq, Eq)]
enum Subcommand {
    Describe,
    Check,
}

impl Subcommand {
    const ALL: [Self; 2] = [Self::Describe, Self::Check];

    const fn name(self) -> &'static str {
        match self {
            Self::Describe => "describe",
            Self::Check => "check",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|subcommand| subcommand.name() == name)
    }
}

/// What a [`Generation`] generated
pub struct Generated {
    /// The files of each schema to write under [`Generation::out()`], in the
    /// order the schemas were given. The command writes a schema's files up
    /// to the first that fails, and the other schemas' still.
    pub file_sets: Vec<Vec<GeneratedFile>>,
    /// Each error, which ends its last line. The command adds those of the
    /// files that it can't write, prints them all with an empty line between
    /// them, and fails with [`FAILURE`] if there are any.
    pub errors: Vec<String>,
}

impl Command {
    /// Parses the command's arguments, without the command's own name.
    /// `--help` or `--version` stops at once, so that it works whatever else
    /// the arguments hold, and wrong arguments print what's wrong with them
    /// and the usage.
    pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Self {
        match parse(arguments) {
            Ok(Parsed::Help) => Self::Print(Output {
                stdout: help(),
                stderr: String::new(),
                code: SUCCESS,
            }),
            Ok(Parsed::Version) => Self::Print(Output {
                stdout: format!("tafla-gen {}\n", env!("CARGO_PKG_VERSION")),
                stderr: String::new(),
                code: SUCCESS,
            }),
            Ok(Parsed::Generate(generation)) => Self::Generate(generation),
            Ok(Parsed::Inspect(inspection)) => Self::Inspect(inspection),
            Err(message) => Self::Print(Output {
                stdout: String::new(),
                stderr: format!(
                    "error: {message}\n{}\nRun `tafla-gen --help` for more.\n",
                    usage()
                ),
                code: USAGE_ERROR,
            }),
        }
    }
}

enum Parsed {
    Help,
    Version,
    Generate(Generation),
    Inspect(Inspection),
}

/// The value of each option that takes one, where it's given
#[derive(Default)]
struct OptionValues {
    out: Option<String>,
    runtime: Option<String>,
    package: Option<String>,
    against: Option<String>,
}

impl OptionValues {
    /// The first option given among those that generate code, which a
    /// subcommand doesn't take
    fn first_of_generation(&self) -> Option<&'static str> {
        [
            ("--out", &self.out),
            ("--runtime", &self.runtime),
            ("--package", &self.package),
        ]
        .into_iter()
        .find_map(|(name, value)| value.is_some().then_some(name))
    }
}

/// Parses the arguments, or fails with a message of what's wrong with them
fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Parsed, String> {
    let mut arguments = arguments.into_iter();
    let mut positional = Vec::new();
    let mut options = OptionValues::default();
    let mut options_ended = false;

    while let Some(argument) = arguments.next() {
        let argument = utf8(argument)?;
        if options_ended || argument == "-" || !argument.starts_with('-') {
            positional.push(argument);
            continue;
        }

        let slot = match argument.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "-V" | "--version" => return Ok(Parsed::Version),
            "--" => {
                options_ended = true;
                continue;
            }
            "--out" => &mut options.out,
            "--runtime" => &mut options.runtime,
            "--package" => &mut options.package,
            "--against" => &mut options.against,
            _ => return Err(format!("unknown option `{argument}`")),
        };
        if slot.is_some() {
            return Err(format!("`{argument}` is given twice"));
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("`{argument}` needs a value"))?;
        *slot = Some(utf8(value)?);
    }

    let mut positional = positional.into_iter();
    let target_name = positional.next().ok_or("no target is given")?;
    if let Some(subcommand) = Subcommand::from_name(&target_name) {
        return inspection(subcommand, positional, options).map(Parsed::Inspect);
    }
    let target = Target::from_name(&target_name).ok_or_else(|| {
        let targets: Vec<_> = Target::names().collect();
        format!(
            "unknown target `{target_name}`: this build generates {}",
            targets.join(", ")
        )
    })?;
    if options.against.is_some() {
        return Err("`--against` is an option of `check`".to_owned());
    }
    let OptionValues {
        out,
        runtime,
        package,
        ..
    } = options;

    let schemas = distinct(positional);
    if schemas.is_empty() {
        return Err("no schema is given".to_owned());
    }
    // Each schema's Go is a package of its own, and the files of two
    // packages can't share a directory
    #[cfg(feature = "go")]
    if target == Target::Go && schemas.len() > 1 {
        return Err(
            "go generates a package of each schema, which needs a directory of its own: \
             give one schema, and another `--out` for each other"
                .to_owned(),
        );
    }

    let package = package
        .map(|name| name.parse::<JavaPackage>())
        .transpose()
        .map_err(|error| error.to_string())?;
    Ok(Parsed::Generate(Generation {
        target,
        schemas,
        out: out.unwrap_or_else(|| ".".to_owned()),
        options: Options { runtime, package },
    }))
}

/// The schemas in the order they were given, each once: a schema given
/// twice, such as by overlapping globs, is generated or described once
fn distinct(given: impl Iterator<Item = String>) -> Vec<String> {
    let mut schemas: Vec<String> = Vec::new();
    for schema in given {
        if !schemas.contains(&schema) {
            schemas.push(schema);
        }
    }
    schemas
}

/// What a subcommand's arguments ask for, or what's wrong with them
fn inspection(
    subcommand: Subcommand,
    schemas: impl Iterator<Item = String>,
    options: OptionValues,
) -> Result<Inspection, String> {
    let name = subcommand.name();
    if let Some(option) = options.first_of_generation() {
        return Err(format!("`{option}` isn't an option of `{name}`"));
    }

    let mut schemas = match subcommand {
        Subcommand::Describe => distinct(schemas),
        Subcommand::Check => schemas.collect(),
    };
    if schemas.is_empty() {
        return Err("no schema is given".to_owned());
    }

    match (subcommand, options.against) {
        (Subcommand::Describe, None) => {}
        (Subcommand::Describe, Some(_)) => {
            return Err("`--against` is an option of `check`".to_owned());
        }
        (Subcommand::Check, None) => {
            return Err("`check` needs `--against` and the schema to compare with".to_owned());
        }
        (Subcommand::Check, Some(old)) => {
            if schemas.len() > 1 {
                return Err("`check` compares one schema at a time".to_owned());
            }
            schemas.push(old);
        }
    }

    Ok(Inspection {
        subcommand,
        schemas,
    })
}

/// An argument as UTF-8, which schemas' paths are, since they name the
/// generated files and are printed in them
fn utf8(argument: OsString) -> Result<String, String> {
    argument
        .into_string()
        .map_err(|argument| format!("`{}` isn't UTF-8", argument.to_string_lossy()))
}

fn usage() -> String {
    let targets: Vec<_> = Target::names().collect();
    format!(
        "usage: tafla-gen <{}> <schema>... [--out <dir>] [--runtime <module>] [--package <package>]\n       \
         tafla-gen describe <schema>...\n       \
         tafla-gen check <schema> --against <old schema>",
        targets.join("|")
    )
}

/// The help's text after its list of targets, before its options
const SUBCOMMANDS: &str = "
Subcommands:
  describe             Prints a description of the schemas as JSON to stdout:
                       their declarations and fields, with the layout, the
                       sizes and the offsets that the format gives them
  check                Compares a schema with the version of it before, and
                       prints where a reader of one would misread the tables
                       of the other, under the format's rules of evolution
";

/// The help's text after its list of targets
const OPTIONS: &str = "
Options:
  --out <dir>          The directory to write the files under, by default the
                       current directory
  --runtime <module>   The module, package or crate that the code imports tafla's
                       runtime from: `tafla` by default,
                       `io.github.snovader.tafla` in Java,
                       `github.com/snovader/tafla/go` in Go, the header
                       `tafla/tafla.hpp` in C++, `Tafla` in Swift and the
                       namespace `Tafla` in C#
  --package <package>  The package of the Java classes, by default `generated`,
                       and the namespace of the C++, `a::b` for `a.b`, and of
                       the C#, by default the schema's name. Go's package is
                       named after the schema, and Go takes one schema at a time
  --against <schema>   The old schema that `check` compares its schema with
  -h, --help           Prints this help
  -V, --version        Prints the version
  --                   Ends the options, so that every argument after it is a schema

The command prints errors to stderr, and writes no file of a schema that has
errors. The subcommands take none of the options that generate code.

Exit status:
  0  Every schema generated its files or was described, or `check` found no
     errors
  1  A schema has errors, or a file couldn't be read or written, or `check`
     found errors
  2  The arguments are wrong
";

fn help() -> String {
    let mut help = format!(
        "tafla-gen {}\n\
         Generates the code that writes and reads the tables of tafla schemas.\n\n\
         {}\n\n\
         Targets:\n",
        env!("CARGO_PKG_VERSION"),
        usage()
    );
    for names in Target::names_and_abbreviations() {
        let _ = writeln!(help, "  {}", names.join(", "));
    }
    help.push_str(SUBCOMMANDS);
    help.push_str(OPTIONS);
    help
}

impl Generation {
    /// The schemas' paths, each once, in the order they were given
    #[must_use]
    pub fn schemas(&self) -> &[String] {
        &self.schemas
    }

    /// The directory to write the files under, which is `.` by default
    #[must_use]
    pub fn out(&self) -> &str {
        &self.out
    }

    /// Generates the files of every schema. `read` reads the bytes of each
    /// of [`schemas()`](Self::schemas), once and in their order, or fails
    /// with why it can't, such as an I/O error's description.
    ///
    /// A schema that can't be read, isn't UTF-8 or has errors has no files
    /// but an error, and neither have two schemas that write the same
    /// file, since either would hide the other's.
    pub fn generate(&self, mut read: impl FnMut(&str) -> Result<Vec<u8>, String>) -> Generated {
        // Every schema is compiled before any is written, so that schemas
        // whose files would overwrite each other's are found first
        let mut compiled = Vec::new();
        let mut errors = Vec::new();
        for schema in &self.schemas {
            match self.compile(schema, &mut read) {
                Ok(files) => compiled.push((schema.as_str(), files)),
                Err(error) => errors.push(error),
            }
        }

        let collisions = collisions(&compiled);
        errors.extend(
            collisions
                .iter()
                .map(|collision| collision.describe(&compiled)),
        );
        let colliding: HashSet<usize> = collisions
            .iter()
            .flat_map(|collision| collision.schemas)
            .collect();

        let file_sets = compiled
            .into_iter()
            .enumerate()
            .filter(|(index, _)| !colliding.contains(index))
            .map(|(_, (_, files))| files)
            .collect();
        Generated { file_sets, errors }
    }

    /// A schema's generated files, or its errors as the library renders them
    fn compile(
        &self,
        schema: &str,
        read: &mut impl FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<GeneratedFile>, String> {
        let bytes = read(schema).map_err(|error| format!("{schema}: {error}\n"))?;
        let source = decode(schema, bytes)?;

        compile(schema, &source, self.target, &self.options)
            .map_err(|diagnostics| Diagnostic::render_all(&diagnostics, schema, &source))
    }
}

impl Inspection {
    /// The schemas' paths, in the order to read them: each once in the
    /// order they were given for `describe`, and for `check` the schema and
    /// then the old one that it's compared with
    #[must_use]
    pub fn schemas(&self) -> &[String] {
        &self.schemas
    }

    /// Runs the subcommand, and returns what it prints and its exit code.
    /// `read` reads the bytes of each of [`schemas()`](Self::schemas), once
    /// and in their order, as [`Generation::generate()`]'s does.
    ///
    /// `describe` prints the description to stdout, or the errors of the
    /// schemas that have any to stderr and no description. `check` prints
    /// its errors and notes to stderr, and fails if there's an error.
    pub fn inspect(&self, mut read: impl FnMut(&str) -> Result<Vec<u8>, String>) -> Output {
        let sources: Vec<Result<String, String>> = self
            .schemas
            .iter()
            .map(|schema| {
                let bytes = read(schema).map_err(|error| format!("{schema}: {error}\n"))?;
                decode(schema, bytes)
            })
            .collect();

        match self.subcommand {
            Subcommand::Describe => self.describe(sources),
            Subcommand::Check => self.check(sources),
        }
    }

    fn describe(&self, sources: Vec<Result<String, String>>) -> Output {
        let mut parsed: Vec<(&str, Schema)> = Vec::new();
        let mut errors = Vec::new();
        for (path, source) in self.schemas.iter().zip(sources) {
            let schema = source.and_then(|source| {
                crate::parse(&source)
                    .map_err(|diagnostics| Diagnostic::render_all(&diagnostics, path, &source))
            });
            match schema {
                Ok(schema) => parsed.push((path, schema)),
                Err(error) => errors.push(error),
            }
        }
        if !errors.is_empty() {
            return failed(&errors);
        }

        let schemas: Vec<(&str, &Schema)> = parsed
            .iter()
            .map(|(path, schema)| (*path, schema))
            .collect();
        Output {
            stdout: describe::describe(&schemas),
            stderr: String::new(),
            code: SUCCESS,
        }
    }

    fn check(&self, sources: Vec<Result<String, String>>) -> Output {
        let [new_path, old_path] = self.schemas.as_slice() else {
            unreachable!("`check` reads a schema and the one before it");
        };
        let mut sources = sources.into_iter();
        let (new, old) = match (sources.next(), sources.next()) {
            (Some(Ok(new)), Some(Ok(old))) => (new, old),
            (new, old) => {
                let errors: Vec<String> = [new, old]
                    .into_iter()
                    .filter_map(|source| source?.err())
                    .collect();
                return failed(&errors);
            }
        };

        match compatibility(&old, &new) {
            Ok(comparison) => Output {
                stdout: String::new(),
                stderr: comparison.render(new_path, &new),
                code: if comparison.has_errors() {
                    FAILURE
                } else {
                    SUCCESS
                },
            },
            Err(invalid) => {
                let errors: Vec<String> = [
                    (&invalid.new, new_path, &new),
                    (&invalid.old, old_path, &old),
                ]
                .into_iter()
                .filter(|(diagnostics, ..)| !diagnostics.is_empty())
                .map(|(diagnostics, path, source)| {
                    Diagnostic::render_all(diagnostics, path, source)
                })
                .collect();
                failed(&errors)
            }
        }
    }
}

/// What a subcommand prints when schemas have errors or can't be read: the
/// errors, with an empty line between them
fn failed(errors: &[String]) -> Output {
    Output {
        stdout: String::new(),
        stderr: errors.join("\n"),
        code: FAILURE,
    }
}

/// A schema's source from its bytes, or an error that points at the first
/// bytes that aren't UTF-8, as the library points at a schema's errors
fn decode(schema: &str, bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|error| {
        // The invalid bytes are a replacement character in the line printed
        let valid = error.utf8_error().valid_up_to();
        let source = String::from_utf8_lossy(error.as_bytes());
        let span = Span {
            start: valid,
            end: valid + char::REPLACEMENT_CHARACTER.len_utf8(),
        };

        Diagnostic::new("the schema isn't UTF-8", span)
            .with_help("this isn't a character in UTF-8")
            .render(schema, &source)
    })
}

/// A schema's path and its generated files
type Compiled<'a> = (&'a str, Vec<GeneratedFile>);

/// Two generated files that would be one on disk: their paths are the same,
/// or differ only in case, which macOS and Windows ignore by default
struct Collision<'a> {
    /// The indices of the files' schemas, in the order they were given,
    /// which are the same for classes of one Java schema
    schemas: [usize; 2],
    paths: [&'a str; 2],
}

impl Collision<'_> {
    fn describe(&self, compiled: &[Compiled]) -> String {
        let [first, second] = self.schemas.map(|index| compiled[index].0);
        match self.paths {
            [path, other] if path == other => format!("{first} and {second} both write {path}\n"),
            [path, other] => format!(
                "{first} writes {path} and {second} writes {other}, \
                 which are one file where file names ignore case\n"
            ),
        }
    }
}

/// The generated files that would overwrite a file generated before them
fn collisions<'a>(compiled: &'a [Compiled]) -> Vec<Collision<'a>> {
    // Each path in lowercase, with the schema and path that take it first
    let mut taken: HashMap<String, (usize, &str)> = HashMap::new();
    let mut collisions = Vec::new();

    for (index, (_, files)) in compiled.iter().enumerate() {
        for file in files {
            match taken.entry(file.path.to_lowercase()) {
                Entry::Vacant(entry) => {
                    entry.insert((index, &file.path));
                }
                Entry::Occupied(entry) => {
                    let &(first, path) = entry.get();
                    collisions.push(Collision {
                        schemas: [first, index],
                        paths: [path, &file.path],
                    });
                }
            }
        }
    }

    collisions
}
