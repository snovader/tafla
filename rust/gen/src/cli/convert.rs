//! `tafla-gen decode` and `tafla-gen encode`: the command's arguments of a
//! table to convert, and the conversion of what its caller reads, as
//! README.md's "Tables as JSON" describes them.

use super::{FAILURE, Flags, Output, SUCCESS, Values, check_schema_path, source_of};
use crate::{DecodeOptions, Diagnostic, Root, parse};

/// The usage of the subcommands, after that of the targets
pub const USAGE: &str =
    "       tafla-gen decode <schema> <name> [<file>] [--layout] [--any-version] [--ascii]
       tafla-gen encode <schema> <name> [<file>]";

/// The help's text of the subcommands, after the targets' options
pub const OPTIONS: &str = "
Decoding and encoding:
  decode <schema> <name> [<file>]
                       Reads the table of the message or struct `name` from
                       the file, or from stdin without one or for `-`, and
                       prints it as JSON
  encode <schema> <name> [<file>]
                       Reads the JSON of a table of the message or struct
                       `name`, as `decode` prints it, and prints the table's
                       bytes
  --layout             Prints where each header slot, offset, value and run of
                       padding of the table lies, rather than its JSON
  --any-version        Reads a versioned message whatever version it holds
  --ascii              Writes the characters of strings outside ASCII as
                       escapes
";

/// The input that stands for stdin, as the file's argument and when it's
/// left out
pub const STDIN: &str = "-";

/// A table to decode or encode
pub struct Conversion {
    operation: Operation,
    schema: String,
    name: String,
    input: String,
    options: DecodeOptions,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Decode,
    Layout,
    Encode,
}

impl Conversion {
    /// The conversion that the arguments after the options ask for, when
    /// the first is `decode` or `encode`, or `None` for any other command
    pub(super) fn parse(
        command: &str,
        arguments: &mut impl Iterator<Item = String>,
        flags: &Flags,
        values: &Values,
    ) -> Result<Option<Self>, String> {
        let operation = match command {
            "decode" if flags.layout => Operation::Layout,
            "decode" => Operation::Decode,
            "encode" => Operation::Encode,
            _ => return Ok(None),
        };

        let value_option = [
            (values.out.is_some(), "--out"),
            (values.runtime.is_some(), "--runtime"),
            (values.package.is_some(), "--package"),
        ]
        .into_iter()
        .find_map(|(given, name)| given.then_some(name));
        if let Some(option) = value_option {
            return Err(format!("`{command}` doesn't take `{option}`"));
        }
        if let Some(flag) = flags.first().filter(|_| operation == Operation::Encode) {
            return Err(format!("`encode` doesn't take `{flag}`"));
        }

        let schema = arguments.next().ok_or("no schema is given")?;
        check_schema_path(&schema)?;
        let name = arguments
            .next()
            .ok_or("no message or struct is given, by its name in the schema")?;
        let input = arguments.next().unwrap_or_else(|| STDIN.to_owned());
        if let Some(extra) = arguments.next() {
            return Err(format!(
                "`{command}` reads one file, `{input}`, and `{extra}` is another"
            ));
        }

        Ok(Some(Self {
            operation,
            schema,
            name,
            input,
            options: DecodeOptions {
                any_version: flags.any_version,
                ascii: flags.ascii,
            },
        }))
    }

    /// The path of the schema
    pub fn schema(&self) -> &str {
        &self.schema
    }

    /// The path of the file to convert, or [`STDIN`], `-`, for stdin
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Decodes or encodes the table. `read` reads the bytes of the
    /// [`schema()`](Self::schema) and then of the [`input()`](Self::input),
    /// with `-` as stdin, or fails with why it can't, such as an I/O error's
    /// description.
    ///
    /// The output's stdout is the JSON, the layout or the table's bytes, and
    /// its stderr the errors, which end their last line.
    pub fn convert(&self, mut read: impl FnMut(&str) -> Result<Vec<u8>, String>) -> Output {
        match self.try_convert(&mut read) {
            Ok(output) => output,
            Err(stderr) => Output {
                stdout: Vec::new(),
                stderr,
                code: FAILURE,
            },
        }
    }

    fn try_convert(
        &self,
        read: &mut impl FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<Output, String> {
        let bytes = read(&self.schema).map_err(|error| format!("{}: {error}\n", self.schema))?;
        let source = source_of(&self.schema, bytes, "schema")?;
        let schema = parse(&source)
            .map_err(|errors| Diagnostic::render_all(&errors, &self.schema, &source))?;
        let root = Root::find(&schema, &self.name).map_err(|error| {
            format!(
                "error: {error}\n --> {}\n  = help: {}\n",
                self.schema,
                error.help()
            )
        })?;

        let input = self.input_name();
        let bytes = read(&self.input).map_err(|error| format!("{input}: {error}\n"))?;

        match self.operation {
            Operation::Decode => match root.decode(&bytes, &self.options) {
                Ok(json) => Ok(printed(json.into_bytes(), String::new())),
                Err(error) => Err(self.decode_error(&error)),
            },
            Operation::Layout => {
                let layout = root.layout(&bytes, &self.options);
                let stderr = layout
                    .error
                    .map(|error| self.decode_error(&error))
                    .unwrap_or_default();
                Ok(printed(layout.text.into_bytes(), stderr))
            }
            Operation::Encode => {
                let json = source_of(input, bytes, "JSON")?;
                let table = root
                    .encode(&json)
                    .map_err(|errors| Diagnostic::render_all(&errors, input, &json))?;
                Ok(printed(table, String::new()))
            }
        }
    }

    /// The input as errors name it: its path, or `<stdin>`
    fn input_name(&self) -> &str {
        if self.input == STDIN {
            "<stdin>"
        } else {
            &self.input
        }
    }

    fn decode_error(&self, error: &crate::DecodeError) -> String {
        let mut message = format!(
            "error: {} isn't a `{}`: {error}\n",
            self.input_name(),
            self.name
        );
        if error.is_unsupported_version() {
            message.push_str("  = help: `--any-version` reads it whatever its version\n");
        }
        message
    }
}

/// The output of a conversion that printed `stdout`, and that failed when
/// it printed errors too, as a layout does up to the read that failed
fn printed(stdout: Vec<u8>, stderr: String) -> Output {
    let code = if stderr.is_empty() { SUCCESS } else { FAILURE };
    Output {
        stdout,
        stderr,
        code,
    }
}
