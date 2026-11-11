# tafla for Python

`core/` holds `tafla`, the runtime, whose package and module share its name.
Its `native/` holds the native implementation of the classes that generated
code uses, a Rust crate that maturin builds into the module `tafla._native`
with PyO3, on the Rust runtime in `rust/core`.

`benchmarks/` compares tafla with FlatBuffers, protobuf, MessagePack and
`struct` in pyperf benchmarks, on the same workloads as the TypeScript
comparison. Its [README](benchmarks/README.md) describes them and has results.
It isn't published, and its libraries are its own dependencies, not
`tafla`'s.

## Development

This is a uv workspace. Run the commands below from this directory; `uv run`
creates the environment from `uv.lock` first, which builds the native module
with maturin, so it needs a Rust toolchain. uv rebuilds it when its sources
or the Rust runtime's change, and puts it in `core/src/tafla`, which Git
ignores.

```sh
uv run ruff format --check
uv run ruff check
uv run ty check
uv run python -m unittest discover -s core/tests -t core
uv run python -m unittest discover -s benchmarks/tests -t benchmarks
```

The tests run against the native classes, and again against the pure-Python
ones with `TAFLA_PUREPYTHON` set, which `tafla.IMPLEMENTATION` confirms. The
tests in `core/tests/test_implementations.py` compare the two directly, call
by call, whichever `tafla` uses:

```sh
TAFLA_PUREPYTHON=1 uv run python -m unittest discover -s core/tests -t core
TAFLA_PUREPYTHON=1 uv run python -m unittest discover -s benchmarks/tests -t benchmarks
```

The native crate has Rust's checks of its own, run from `core/native`, where
`PYO3_PYTHON` names the Python that `cargo test` links its tests with:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings
PYO3_PYTHON=python3.14 cargo test --all-features
```

Its feature `test-hooks` adds the tests' hooks, such as one that treats the
host as big-endian, so that the tests take the byte-swapping branches on any
host. The development environment builds the module with it, as
`config-settings-package` in `pyproject.toml` asks, and the wheels without
it, so the tests that need a hook skip with a wheel. CI runs the crate's
tests with its `rust-version`, 1.88, too.

`uv build --package tafla` builds the runtime's source distribution, which
holds the Rust sources, and a wheel with the native module for this platform,
into `dist/`. Without `--package`, `uv build` fails, since the workspace's
root isn't a package. The pure-Python wheel, `py3-none-any`, is built from
the same sources by uv_build, which `[tool.uv.build-backend]` in
`core/pyproject.toml` configures, from `core/`:

```sh
mkdir -p ../dist
uv run --isolated --no-project --with 'uv-build==0.13.7' python -c "import uv_build; print(uv_build.build_wheel('../dist'))"
```

`scripts/test-wheels.sh` tests each wheel in a directory on its own. It
installs the wheel alone in each Python that its tags name, checks that
`tafla` uses the implementation that the wheel holds, and runs the core tests
against it. The release builds a native wheel for each platform and Python,
which [RELEASING.md](../RELEASING.md) lists, and tests each with it:

```sh
scripts/test-wheels.sh dist
```

The benchmarks' tests check that their workloads have the TypeScript
comparison's values, and that every codec in the comparison reads back what it
wrote. `uv run --python 3.15 ...` runs the commands with another Python. Where
uv would pick a free-threaded build, `--python 3.14+gil` picks one with the
GIL.

`core/tests/generated` holds the Python that tafla-gen generates from the
example schemas in `schemas/`, in a directory of each schema, and the tests
that run it. They're the generator's golden files, which `rust/gen/README.md`
describes: rewrite them with the generator rather than by hand.
