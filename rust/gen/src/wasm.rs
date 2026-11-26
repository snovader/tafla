//! The compiler's WebAssembly interface: plain exports with no imports, so
//! any host runs the same module without WASI or bindings.
//!
//! A host allocates a request with `tafla_alloc`, writes it and passes it to
//! `tafla_compile`, which frees it and returns a response. The host reads the
//! response's length from its first four bytes and frees it with `tafla_free`.
//!
//! Every string is a little-endian `u32` byte length and UTF-8 bytes:
//!
//! - request: target, file name, source, runtime, package; an empty runtime
//!   or package takes the target's default, and a package that isn't a Java
//!   package is an error
//! - response: its `u32` length, a `u32` status, then for status 0 a `u32`
//!   file count and each file's path and contents, and for status 1 the
//!   rendered errors
//!
//! `tafla_command` and `tafla_generate` run the `tafla-gen` command for a
//! host that reads the schemas, writes the files and prints, as the `cli`
//! module does for the command's binary. Their requests are freed and their
//! responses start with their length and a status, as `tafla_compile`'s do:
//!
//! - `tafla_command`'s request is the command's arguments, each a string, up
//!   to its end. For status 0, the response is the directory to write under,
//!   a `u32` count of schemas and each schema's path, which the host reads.
//!   For status 1, the command is over: a `u32` exit code, and what to print
//!   to stdout and to stderr. A subcommand names its schemas with status 0
//!   too, and the directory `.`, which it writes nothing under.
//! - `tafla_generate`'s request is a `u32` count of arguments and the same
//!   arguments, and then each of those schemas, in their order: a `u32` of 0
//!   and its bytes, which needn't be UTF-8, or of 1 and why it can't be read.
//!   For status 0, the response is a `u32` count of schemas with files to
//!   write, for each a `u32` count of files and each file's path and
//!   contents, and then a `u32` count of errors and each error. For status 1,
//!   it's as `tafla_command`'s: for a subcommand, which prints rather than
//!   generates, for arguments that don't generate, or a malformed request.

use std::ffi::OsString;

use crate::cli::{Command, Generated, Generation, Inspection, Output, USAGE_ERROR};
use crate::{Diagnostic, GeneratedFile, JavaPackage, Options, Target, compile};

/// Allocates exactly `length` bytes for a request
#[unsafe(no_mangle)]
pub extern "C" fn tafla_alloc(length: usize) -> *mut u8 {
    Box::into_raw(vec![0_u8; length].into_boxed_slice()).cast()
}

/// # Safety
///
/// `pointer` must come from `tafla_alloc` with `length`, or be a response
/// with its `length`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tafla_free(pointer: *mut u8, length: usize) {
    drop(unsafe { take_back(pointer, length) });
}

/// # Safety
///
/// `pointer` must come from `tafla_alloc` with `length`, and hold the request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tafla_compile(pointer: *mut u8, length: usize) -> *mut u8 {
    let request = unsafe { take_back(pointer, length) };

    hand_out(respond(&request))
}

/// # Safety
///
/// `pointer` must come from `tafla_alloc` with `length`, and hold the request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tafla_command(pointer: *mut u8, length: usize) -> *mut u8 {
    let request = unsafe { take_back(pointer, length) };

    hand_out(respond_to_command(&request))
}

/// # Safety
///
/// `pointer` must come from `tafla_alloc` with `length`, and hold the request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tafla_generate(pointer: *mut u8, length: usize) -> *mut u8 {
    let request = unsafe { take_back(pointer, length) };

    hand_out(respond_to_generate(&request))
}

/// Hands a response out to the host, with its length in its first four bytes
fn hand_out(mut response: Vec<u8>) -> *mut u8 {
    let length = u32::try_from(response.len()).unwrap_or(u32::MAX);
    response[..4].copy_from_slice(&length.to_le_bytes());

    Box::into_raw(response.into_boxed_slice()).cast()
}

/// Takes back the bytes that `tafla_alloc` or a response handed out.
/// They're boxed slices, which are deallocated at exactly their length.
///
/// # Safety
///
/// As for `tafla_free`.
unsafe fn take_back(pointer: *mut u8, length: usize) -> Box<[u8]> {
    unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(pointer, length)) }
}

fn respond(request: &[u8]) -> Vec<u8> {
    let mut reader = Reader { bytes: request };

    match handle(&mut reader) {
        Ok(files) => {
            let mut response = started(0);
            push_files(&mut response, &files);
            response
        }
        Err(errors) => {
            let mut response = started(1);
            push_string(&mut response, &errors);
            response
        }
    }
}

fn handle(reader: &mut Reader) -> Result<Vec<GeneratedFile>, String> {
    let malformed = || "malformed request\n".to_owned();
    let target_name = reader.string().ok_or_else(malformed)?;
    let file_name = reader.string().ok_or_else(malformed)?;
    let source = reader.string().ok_or_else(malformed)?;
    let runtime = reader.string().ok_or_else(malformed)?;
    let package = reader.string().ok_or_else(malformed)?;

    let target = Target::from_name(target_name).ok_or_else(|| {
        let targets: Vec<_> = Target::names().collect();
        format!(
            "unknown target `{target_name}`: this build generates {}\n",
            targets.join(", ")
        )
    })?;
    let package = (!package.is_empty())
        .then(|| package.parse::<JavaPackage>())
        .transpose()
        .map_err(|error| format!("{error}\n"))?;
    let options = Options {
        runtime: (!runtime.is_empty()).then(|| runtime.to_owned()),
        package,
    };

    compile(file_name, source, target, &options)
        .map_err(|diagnostics| Diagnostic::render_all(&diagnostics, file_name, source))
}

fn respond_to_command(request: &[u8]) -> Vec<u8> {
    let mut reader = Reader { bytes: request };
    let mut arguments = Vec::new();
    while !reader.is_at_end() {
        match reader.string() {
            Some(argument) => arguments.push(OsString::from(argument)),
            None => return printed(&malformed_request()),
        }
    }

    match Command::parse(arguments) {
        Command::Print(output) => printed(&output),
        Command::Generate(generation) => to_read(generation.out(), generation.schemas()),
        Command::Inspect(inspection) => to_read(".", inspection.schemas()),
    }
}

/// The response that names the schemas for the host to read, and the
/// directory to write under
fn to_read(out: &str, schemas: &[String]) -> Vec<u8> {
    let mut response = started(0);
    push_string(&mut response, out);
    push_length(&mut response, schemas.len());
    for schema in schemas {
        push_string(&mut response, schema);
    }
    response
}

fn respond_to_generate(request: &[u8]) -> Vec<u8> {
    let mut reader = Reader { bytes: request };
    let Some(arguments) = reader.strings() else {
        return printed(&malformed_request());
    };

    let generation = match Command::parse(arguments.into_iter().map(OsString::from)) {
        Command::Print(output) => return printed(&output),
        Command::Generate(generation) => generation,
        Command::Inspect(inspection) => {
            return printed(&inspect(&inspection, &mut reader).unwrap_or_else(malformed_request));
        }
    };
    let Some(generated) = generate(&generation, &mut reader) else {
        return printed(&malformed_request());
    };

    let mut response = started(0);
    push_length(&mut response, generated.file_sets.len());
    for files in &generated.file_sets {
        push_files(&mut response, files);
    }
    push_length(&mut response, generated.errors.len());
    for error in &generated.errors {
        push_string(&mut response, error);
    }
    response
}

/// Generates from the schemas that the host read, which follow the
/// arguments in the request, or `None` if the request doesn't hold each of
/// the generation's schemas and nothing else
fn generate(generation: &Generation, reader: &mut Reader) -> Option<Generated> {
    let mut sources = sources(generation.schemas(), reader)?;
    Some(generation.generate(|_schema| sources()))
}

/// Runs a subcommand on the schemas that the host read, as [`generate()`]
/// generates from them
fn inspect(inspection: &Inspection, reader: &mut Reader) -> Option<Output> {
    let mut sources = sources(inspection.schemas(), reader)?;
    Some(inspection.inspect(|_schema| sources()))
}

/// What the host read of each of `schemas`, which follow the arguments in
/// the request, as a function that returns the next schema's each time
/// it's called, since the command reads each schema once, in their order.
/// It's `None` if the request doesn't hold each schema and nothing else.
fn sources<'a>(
    schemas: &[String],
    reader: &mut Reader<'a>,
) -> Option<impl FnMut() -> Result<Vec<u8>, String> + use<'a>> {
    let mut sources = Vec::with_capacity(schemas.len());
    for _ in schemas {
        sources.push(reader.source()?);
    }
    if !reader.is_at_end() {
        return None;
    }

    let mut sources = sources.into_iter();
    Some(move || {
        sources
            .next()
            .expect("a source of each schema")
            .map(<[u8]>::to_vec)
            .map_err(str::to_owned)
    })
}

fn malformed_request() -> Output {
    Output {
        stdout: String::new(),
        stderr: "malformed request\n".to_owned(),
        code: USAGE_ERROR,
    }
}

/// The response of a command that's over
fn printed(output: &Output) -> Vec<u8> {
    let mut response = started(1);
    response.extend_from_slice(&u32::from(output.code).to_le_bytes());
    push_string(&mut response, &output.stdout);
    push_string(&mut response, &output.stderr);
    response
}

/// A response's first bytes: room for its length, and its status
fn started(status: u32) -> Vec<u8> {
    let mut response = vec![0; 4];
    response.extend_from_slice(&status.to_le_bytes());
    response
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn is_at_end(&self) -> bool {
        self.bytes.is_empty()
    }

    fn length(&mut self) -> Option<usize> {
        let (length, rest) = self.bytes.split_first_chunk::<4>()?;
        self.bytes = rest;
        Some(u32::from_le_bytes(*length) as usize)
    }

    fn byte_string(&mut self) -> Option<&'a [u8]> {
        let length = self.length()?;
        let (bytes, rest) = self.bytes.split_at_checked(length)?;
        self.bytes = rest;
        Some(bytes)
    }

    fn string(&mut self) -> Option<&'a str> {
        std::str::from_utf8(self.byte_string()?).ok()
    }

    /// A count and that many strings
    fn strings(&mut self) -> Option<Vec<&'a str>> {
        let count = self.length()?;
        // Each string takes four bytes at least, so a count beyond the
        // request's fails before it allocates
        let mut strings = Vec::with_capacity(count.min(self.bytes.len() / 4));
        for _ in 0..count {
            strings.push(self.string()?);
        }
        Some(strings)
    }

    /// A schema's bytes, or why the host couldn't read it
    fn source(&mut self) -> Option<Result<&'a [u8], &'a str>> {
        match self.length()? {
            0 => self.byte_string().map(Ok),
            1 => self.string().map(Err),
            _ => None,
        }
    }
}

fn push_length(bytes: &mut Vec<u8>, length: usize) {
    let length = u32::try_from(length).unwrap_or(u32::MAX);
    bytes.extend_from_slice(&length.to_le_bytes());
}

/// A count of files and each file's path and contents
fn push_files(bytes: &mut Vec<u8>, files: &[GeneratedFile]) {
    push_length(bytes, files.len());
    for file in files {
        push_string(bytes, &file.path);
        push_string(bytes, &file.contents);
    }
}

fn push_string(bytes: &mut Vec<u8>, string: &str) {
    push_length(bytes, string.len());
    bytes.extend_from_slice(string.as_bytes());
}
