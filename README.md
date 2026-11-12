# tafla

Binary tables of scalars, strings and typed arrays, in one format with
implementations in TypeScript, Python, Java, Rust, Go and C++. A builder
writes a table at its exact size, copying typed arrays in bulk, and a reader
reads fields back by offset, with arrays as views of the bytes where it can.
Every read checks that it stays within the bytes it was given, so tables can
come from untrusted sources.

## Status

tafla hasn't been released yet.

[SPEC.md](SPEC.md) describes the format.

[CHANGELOG.md](CHANGELOG.md) lists the changes that each release brings, and
[RELEASING.md](RELEASING.md) describes how to release the packages.

## Packages

| Language   | Runtime                                           | Code generator                                        |
| ---------- | ------------------------------------------------- | ----------------------------------------------------- |
| TypeScript | `tafla` on npm                                    | `tafla-gen` on npm                                    |
| Python     | `tafla` on PyPI                                   | `tafla-gen` on PyPI                                   |
| Java       | `io.github.snovader.tafla:tafla` on Maven Central | `io.github.snovader.tafla:tafla-gen` on Maven Central |
| Rust       | `tafla` on crates.io                              | `tafla-gen` on crates.io                              |
| Go         | `github.com/snovader/tafla/go`                    | `tafla-gen`'s `go` target                             |
| C++        | The headers of `cpp/include`, not yet published   | `tafla-gen`, any of the above                         |

A runtime writes and reads tables. The code generator compiles a schema of
messages and tables into code that writes and reads them with a language's
runtime. It's written in Rust, and the packages for the other languages run it
as WebAssembly.

Every package has tafla's version. A Go module is published by a git tag, so
each release tags the Go runtime `go/v<version>`, and
`go get github.com/snovader/tafla/go@v0.1.0` adds the release 0.1.0.

## Repository

| Directory      | Contents                                                                 |
| -------------- | ------------------------------------------------------------------------ |
| `conformance/` | Golden tables that every implementation reads and rebuilds byte for byte |
| `typescript/`  | The reference implementation, which writes the conformance tables        |
| `python/`      | The Python runtime                                                       |
| `java/`        | The Java runtime                                                         |
| `rust/`        | The Rust runtime and the generator                                       |
| `go/`          | The Go runtime                                                           |
| `cpp/`         | The C++ runtime, a header-only library of C++20                          |
| `schemas/`     | Example schemas, which the generator's tests compile                     |

Each language's directory is the root of its own toolchain, and its README
lists the commands that check it.

## License

[MIT](LICENSE)
