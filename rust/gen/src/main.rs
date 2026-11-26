//! The `tafla-gen` command, as `tafla-gen --help` and README.md's "Command"
//! describe it.
//!
//! The library's `cli` module decides what the arguments ask for and what
//! the schemas generate, and this binary reads the schemas, writes the files
//! and prints. It exits with 0 when every schema generated its files, 1 when
//! a schema has errors or a file couldn't be read or written, and 2 when the
//! arguments are wrong. Errors go to stderr, and only `--help`, `--version`
//! and `describe` print to stdout.

use std::path::Path;
use std::process::ExitCode;

use tafla_gen::GeneratedFile;
use tafla_gen::cli::{Command, FAILURE, Generated, Generation, Output, SUCCESS};

fn main() -> ExitCode {
    let output = match Command::parse(std::env::args_os().skip(1)) {
        Command::Print(output) => output,
        Command::Generate(generation) => run(&generation),
        Command::Inspect(inspection) => inspection.inspect(read),
    };

    print!("{}", output.stdout);
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
        stdout: String::new(),
        stderr: errors.join("\n"),
        code: if errors.is_empty() { SUCCESS } else { FAILURE },
    }
}

fn read(schema: &str) -> Result<Vec<u8>, String> {
    std::fs::read(schema).map_err(|error| error.to_string())
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
