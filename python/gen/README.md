# tafla-gen

tafla's schema compiler, as a command that pip, pipx and uv install. It
compiles a schema of messages, tables and structs into code that writes and
reads them with [tafla](https://pypi.org/project/tafla/)'s runtime for
Python, or for TypeScript, JavaScript, Java, Rust, Go, C++, Swift or C#.

```sh
uvx tafla-gen python shapes.tafla --out generated
pipx run tafla-gen python shapes.tafla --out generated
```

Or install it into a project's environment, beside the runtime that the
generated code imports:

```sh
uv add tafla
uv add --dev tafla-gen
uv run tafla-gen python shapes.tafla --out generated
```

```text
tafla-gen <typescript|javascript|python|java|rust|go|cpp|swift|csharp> <schema>... [--out <dir>] [--runtime <module>] [--package <package>]
tafla-gen decode <schema> <name> [<file>] [--layout] [--any-version] [--ascii]
tafla-gen encode <schema> <name> [<file>]
```

The generator's
[README](https://github.com/snovader/tafla/tree/main/rust/gen#readme)
describes the schema language, the generated code, and the command's options
and exit codes, and its
[changelog](https://github.com/snovader/tafla/blob/main/rust/gen/CHANGELOG.md)
the changes in each release.

## The distribution

This is the same generator as the crate
[`tafla-gen`](https://crates.io/crates/tafla-gen) on crates.io, at the same
version: a program written in Rust, without any Python. Each wheel holds the
command built for a platform, and installs it as a script:

| Platform | Architectures   |
| -------- | --------------- |
| Linux    | x86_64, aarch64 |
| macOS    | arm64, x86_64   |
| Windows  | x64             |

Linux has wheels for glibc, manylinux, and for musl, musllinux 1.2.

On any other platform, pip builds the command from the source distribution,
which holds the crate's sources and needs Rust 1.88 or later with cargo.

## Development

The package isn't a member of the uv workspace in `python/`, since its build
compiles the generator, which the workspace's environment has no use for.
From this directory, `uv build` builds the source distribution, and a wheel
from it for this platform, into `dist/`:

```sh
uv build
../scripts/test-gen-wheels.sh dist
```

`scripts/test-gen-wheels.sh` installs each wheel in a directory on its own,
checks that the command prints the wheel's version, and generates the Python
of the example schemas, which must match the golden files in
`core/tests/generated`.

Its version is the generator's, which `scripts/version.sh tafla-gen` writes
into `pyproject.toml` and the crate's manifest, as
[RELEASING.md](https://github.com/snovader/tafla/blob/main/RELEASING.md)
describes.
