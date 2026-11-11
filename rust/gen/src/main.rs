//! The `tafla-gen` command, as `tafla-gen --help` and README.md's "Command"
//! describe it.
//!
//! It exits with 0 when every schema generated its files, 1 when a schema
//! has errors or a file couldn't be read or written, and 2 when the
//! arguments are wrong. Errors go to stderr, and only `--help` and
//! `--version` print to stdout.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tafla_gen::{Diagnostic, GeneratedFile, JavaPackage, Options, Span, Target, compile};

/// The exit code of wrong arguments, as of most commands
const USAGE_ERROR: u8 = 2;

fn main() -> ExitCode {
    match Command::parse(std::env::args_os().skip(1)) {
        Ok(Command::Help) => {
            print!("{}", help());
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("tafla-gen {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Generate(generation)) => match generation.run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(errors) => {
                eprint!("{errors}");
                ExitCode::FAILURE
            }
        },
        Err(message) => {
            eprintln!("error: {message}");
            eprintln!("{}", usage());
            eprintln!("Run `tafla-gen --help` for more.");
            ExitCode::from(USAGE_ERROR)
        }
    }
}

enum Command {
    Help,
    Version,
    Generate(Generation),
}

/// The schemas to generate code from, and where and how
struct Generation {
    target: Target,
    /// The schemas' paths, each once, in the order they were given
    schemas: Vec<String>,
    out: PathBuf,
    options: Options,
}

impl Command {
    /// Parses the command's arguments, or fails with a message of what's
    /// wrong with them. `--help` or `--version` stops at once, so that it
    /// works whatever else the arguments hold.
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut arguments = arguments.into_iter();
        let mut positional = Vec::new();
        let mut out = None;
        let mut runtime = None;
        let mut package = None;
        let mut options_ended = false;

        while let Some(argument) = arguments.next() {
            let argument = utf8(argument)?;
            if options_ended || argument == "-" || !argument.starts_with('-') {
                positional.push(argument);
                continue;
            }

            let slot = match argument.as_str() {
                "-h" | "--help" => return Ok(Self::Help),
                "-V" | "--version" => return Ok(Self::Version),
                "--" => {
                    options_ended = true;
                    continue;
                }
                "--out" => &mut out,
                "--runtime" => &mut runtime,
                "--package" => &mut package,
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
        let target = Target::from_name(&target_name).ok_or_else(|| {
            let targets: Vec<_> = Target::names().collect();
            format!(
                "unknown target `{target_name}`: this build generates {}",
                targets.join(", ")
            )
        })?;

        // A schema given twice, such as by overlapping globs, is generated
        // once
        let mut schemas: Vec<String> = Vec::new();
        for schema in positional {
            if !schemas.contains(&schema) {
                schemas.push(schema);
            }
        }
        if schemas.is_empty() {
            return Err("no schema is given".to_owned());
        }

        let package = package
            .map(|name| name.parse::<JavaPackage>())
            .transpose()
            .map_err(|error| error.to_string())?;
        Ok(Self::Generate(Generation {
            target,
            schemas,
            out: out.map_or_else(|| PathBuf::from("."), PathBuf::from),
            options: Options { runtime, package },
        }))
    }
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
        "usage: tafla-gen <{}> <schema>... [--out <dir>] [--runtime <module>] [--package <package>]",
        targets.join("|")
    )
}

/// The help's text after its list of targets
const OPTIONS: &str = "
Options:
  --out <dir>          The directory to write the files under, by default the
                       current directory
  --runtime <module>   The module, package or crate that the code imports tafla's
                       runtime from: `tafla` by default,
                       `io.github.snovader.tafla` in Java and
                       `github.com/snovader/tafla/go` in Go
  --package <package>  The package of the Java classes, by default `generated`
  -h, --help           Prints this help
  -V, --version        Prints the version
  --                   Ends the options, so that every argument after it is a schema

The command prints errors to stderr, and writes no file of a schema that has
errors.

Exit status:
  0  Every schema generated its files
  1  A schema has errors, or a file couldn't be read or written
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
    help.push_str(OPTIONS);
    help
}

impl Generation {
    /// Generates every schema's files and writes those of the schemas
    /// without errors, or fails with the errors of the others
    fn run(self) -> Result<(), String> {
        // Every schema is compiled before any is written, so that schemas
        // whose files would overwrite each other's are found first
        let mut compiled = Vec::new();
        let mut errors = Vec::new();
        for schema in &self.schemas {
            match self.compile(schema) {
                Ok(files) => compiled.push((schema.clone(), files)),
                Err(error) => errors.push(error),
            }
        }

        let collisions = collisions(&compiled);
        // Neither schema of a collision is written, since either would hide
        // the other's file
        let colliding: HashSet<usize> = collisions
            .iter()
            .flat_map(|collision| collision.schemas)
            .collect();
        for (index, (_, files)) in compiled.iter().enumerate() {
            if colliding.contains(&index) {
                continue;
            }
            if let Err(error) = write_all(&self.out, files) {
                errors.push(error);
            }
        }
        errors.extend(
            collisions
                .iter()
                .map(|collision| collision.describe(&compiled)),
        );

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }

    /// A schema's generated files, or its errors as the library renders them
    fn compile(&self, schema: &str) -> Result<Vec<GeneratedFile>, String> {
        let bytes = std::fs::read(schema).map_err(|error| format!("{schema}: {error}\n"))?;
        let source = decode(schema, bytes)?;

        compile(schema, &source, self.target, &self.options)
            .map_err(|diagnostics| Diagnostic::render_all(&diagnostics, schema, &source))
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

/// Two generated files that would be one on disk: their paths are the same,
/// or differ only in case, which macOS and Windows ignore by default
struct Collision<'a> {
    /// The indices of the files' schemas, in the order they were given,
    /// which are the same for classes of one Java schema
    schemas: [usize; 2],
    paths: [&'a str; 2],
}

impl Collision<'_> {
    fn describe(&self, compiled: &[(String, Vec<GeneratedFile>)]) -> String {
        let [first, second] = self.schemas.map(|index| compiled[index].0.as_str());
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
fn collisions(compiled: &[(String, Vec<GeneratedFile>)]) -> Vec<Collision<'_>> {
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

/// Writes a schema's files under `out`, up to the first that fails
fn write_all(out: &Path, files: &[GeneratedFile]) -> Result<(), String> {
    for file in files {
        write(&out.join(&file.path), &file.contents)?;
    }
    Ok(())
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("{}: {error}\n", directory.display()))?;
    }
    std::fs::write(path, contents).map_err(|error| format!("{}: {error}\n", path.display()))
}
