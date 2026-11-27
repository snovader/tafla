//! The `tafla-gen` command, as `tafla-gen --help` and README.md's "Command"
//! describe it.
//!
//! The library's `cli` module decides what the arguments ask for and what
//! the schemas generate, or the tables decode and encode as, and this binary
//! reads the schemas and tables, writes the files and prints. It exits with
//! 0 when every schema generated its files or the table converted, 1 when a
//! schema has errors, a file couldn't be read or written, or the table or
//! its JSON couldn't be converted, and 2 when the arguments are wrong.
//! Errors go to stderr, and `--help`, `--version`, `decode` and `encode`
//! print to stdout.

use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

use tafla_gen::GeneratedFile;
use tafla_gen::cli::{Command, FAILURE, Generated, Generation, Output, SUCCESS};

fn main() -> ExitCode {
    let output = match Command::parse(std::env::args_os().skip(1)) {
        Command::Print(output) => output,
        Command::Generate(generation) => run(&generation),
        #[cfg(feature = "json")]
        Command::Convert(conversion) => conversion.convert(read),
    };

    // A pipe that the reader closed, as `head` does, isn't an error of the
    // command, which has nothing else to do
    let _ = std::io::stdout().write_all(&output.stdout);
    eprint!("{}", output.stderr);
    ExitCode::from(output.code)
}

/// Generates every schema's files and writes those of the schemas without
/// errors, and prints the errors of the others
fn run(generation: &Generation) -> Output {
    let Generated {
        file_sets,
        mut errors,
    } = generation.generate(read);

    let out = Path::new(generation.out());
    for files in &file_sets {
        if let Err(error) = write_all(out, files) {
            errors.push(error);
        }
    }

    Output {
        stdout: Vec::new(),
        stderr: errors.join("\n"),
        code: if errors.is_empty() { SUCCESS } else { FAILURE },
    }
}

/// Reads a file, or stdin for `-`, which the arguments reject but as the
/// table to convert
fn read(path: &str) -> Result<Vec<u8>, String> {
    if path == "-" {
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        return Ok(bytes);
    }

    std::fs::read(path).map_err(|error| error.to_string())
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
