# tafla for Rust

`core/` holds `tafla`, the runtime, and `gen/` holds `tafla-gen`, the
code generator, which compiles schemas into code for every language's
runtime and also runs as WebAssembly. `benchmarks/` holds `tafla-benchmarks`,
which compares the runtime with other Rust crates, such as FlatBuffers and
rkyv, as its README describes.

## Development

This is a Cargo workspace on stable Rust, as `rust-toolchain.toml` pins it,
with a `rust-version` of 1.88. Run the commands below from this directory.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

The fuzz targets in `fuzz/`, which [Fuzzing](#fuzzing) describes, are a
workspace of their own, which CI formats and lints on stable too:

```sh
cargo fmt --all --manifest-path fuzz/Cargo.toml -- --check
cargo clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings
```

The generator's tests compare its output with golden files, which
`TAFLA_GEN_UPDATE=1 cargo test -p tafla-gen` rewrites. They lie in the test
tree of each target's language, as `gen/README.md` lists them, so a change to
them is followed by that language's checks and tests. Its WebAssembly module
builds with the `wasm` profile, and a Node script checks it:

```sh
rustup target add wasm32-unknown-unknown
cargo build -p tafla-gen --lib --target wasm32-unknown-unknown --profile wasm
node gen/tests/wasm/check.mjs target/wasm32-unknown-unknown/wasm/tafla_gen.wasm
```

`cargo test` runs the benchmarks' check that every codec reads back what it
wrote. The benchmarks themselves run with
`cargo bench -p tafla-benchmarks`, and take several minutes.

Miri runs the runtime's tests to check its `unsafe` code for undefined
behavior, on the host and on a big-endian target, which reads every array
through its byte-swapping path. Each takes several minutes:

```sh
rustup +nightly component add miri rust-src
cargo +nightly miri test -p tafla
cargo +nightly miri test -p tafla --target powerpc-unknown-linux-gnu
```

Miri keeps tests from reading files, so the conformance suite needs its own
run, with the same `--target` as above or none:

```sh
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test -p tafla \
    --target powerpc-unknown-linux-gnu --test conformance -- --include-ignored
```

Miri interprets a test process on one core. CI runs the tests with
[cargo-nextest](https://nexte.st), which runs a process for each test, on
every core, as `cargo +nightly miri nextest run` in place of
`cargo +nightly miri test`, with `--run-ignored all` in place of
`-- --include-ignored`. nextest doesn't run the documentation's examples,
which `cargo +nightly miri test -p tafla --doc` does.

CI also runs the runtime's tests on a 32-bit target, where a `usize` is as
wide as a `u32` length, as it is in the generator's WebAssembly module. On
Linux with `gcc-multilib` installed, that's
`cargo test -p tafla --target i686-unknown-linux-gnu`; elsewhere, Miri runs
them with `--target i686-unknown-linux-gnu`.

## Fuzzing

`fuzz/` holds the fuzz targets, a workspace of its own that
[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) builds with libFuzzer
on nightly Rust. A panic, a hang or a run out of memory is a finding:

- `byte_buffer` reads arbitrary bytes with every getter of `ByteBuffer`, at
  every position and past the end, in every offset type and header, and
  reads every array, struct array and vector there. Each read must return
  the bytes that lie where the format puts them, exactly when they lie
  within the input, and the decode error otherwise.
- `conformance` mutates the conformance suite's tables and reads them as
  every valid case's description, as the conformance test reads them.
- `gen` parses and checks arbitrary text as a schema, and generates every
  target's code from each schema that it accepts.

Run each from this directory, with the suite's tables or the schemas as
seeds after the corpus that libFuzzer grows, and stop it with Ctrl-C or
`-max_total_time`:

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run byte_buffer fuzz/corpus/byte_buffer \
    ../conformance/valid ../conformance/equivalent ../conformance/invalid \
    -- -max_total_time=300
cargo +nightly fuzz run conformance fuzz/corpus/conformance \
    ../conformance/valid ../conformance/equivalent ../conformance/invalid \
    -- -max_total_time=300
cargo +nightly fuzz run gen fuzz/corpus/gen ../schemas gen/tests/diagnostics \
    -- -dict=fuzz/gen.dict -max_total_time=300
```

libFuzzer writes an input that fails to `fuzz/artifacts/<target>/`, which
`cargo +nightly fuzz run <target> <file>` runs again and
`cargo +nightly fuzz tmin <target> <file>` minimizes. A finding becomes a
regression test in the crate it's in. The fuzz workflow runs each target
for five minutes every week, and uploads what fails.
