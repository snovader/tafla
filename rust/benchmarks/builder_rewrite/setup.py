"""Sets up a scratch worktree that builds main's TableBuilder beside the
rewrite, and the rewrite's variants, for the comparison in README.md.

    python3 rust/benchmarks/builder_rewrite/setup.py <scratch directory> [main revision]

It adds a detached worktree of the current commit at the scratch directory,
and in it:

- main's `rust/core/src/table_builder.rs`, from the revision given (7238a1b,
  the main that the rewrite was measured against), as `tafla::main_builder`;
- copies of the rewrite with one change each, as
  `tafla::rewrite_no_reservation`, without the room that a vector in place
  makes for its items after the first, `tafla::rewrite_segments_128` and
  `tafla::rewrite_segments_64`, which keep runs of 128 or 64 bytes or more
  as segments rather than 256, and `tafla::rewrite_boxed`, with an owned
  builder boxed in a `NestedTable`;
- the `main-builder` feature of the runtime and the benchmarks that builds
  them, and this directory's binaries and differential test.

Nothing in the repository's own checkout changes.
"""

import pathlib
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent

#: The edits that make each variant of the rewrite, as (old, new) pairs of
#: `table_builder.rs`'s text, each of which must be found once
VARIANTS = {
    "rewrite_no_reservation": [
        (
            """            } else if index == 0 {
                builder.reserve_items_like(builder.bytes.len() - item.bytes, count - 1);
            }""",
            """            }""",
        ),
        ("    fn reserve_items_like(", "    #[allow(dead_code)]\n    fn reserve_items_like("),
    ],
    "rewrite_segments_128": [
        (
            "const SEGMENT_BYTES: usize = if cfg!(tafla_segment_stress) { 1 } else { 256 };",
            "const SEGMENT_BYTES: usize = if cfg!(tafla_segment_stress) { 1 } else { 128 };",
        ),
    ],
    "rewrite_segments_64": [
        (
            "const SEGMENT_BYTES: usize = if cfg!(tafla_segment_stress) { 1 } else { 256 };",
            "const SEGMENT_BYTES: usize = if cfg!(tafla_segment_stress) { 1 } else { 64 };",
        ),
    ],
    "rewrite_boxed": [
        ("    OwnedBuilder(TableBuilder<'a>),", "    OwnedBuilder(Box<TableBuilder<'a>>),"),
        (
            "Nested::OwnedBuilder(builder) => self.append_owned_builder(builder),",
            "Nested::OwnedBuilder(builder) => self.append_owned_builder(*builder),",
        ),
        (
            "        Self(Nested::OwnedBuilder(builder))",
            "        Self(Nested::OwnedBuilder(Box::new(builder)))",
        ),
        ("    const fn alignment(&self) -> Alignment {", "    fn alignment(&self) -> Alignment {"),
        ("    const fn aligned(mut self,", "    fn aligned(mut self,"),
        (
            "    const fn check_complete(&self) -> Result<(), BuildError> {\n        match",
            "    fn check_complete(&self) -> Result<(), BuildError> {\n        match",
        ),
    ],
}


def edit(path, old, new):
    text = path.read_text()
    if text.count(old) != 1:
        sys.exit(f"{path}: expected one {old!r}")
    path.write_text(text.replace(old, new))


def git(*arguments, cwd):
    return subprocess.run(
        ["git", *arguments], cwd=cwd, check=True, capture_output=True, text=True
    ).stdout


def main():
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    scratch = pathlib.Path(sys.argv[1]).resolve()
    main_revision = sys.argv[2] if len(sys.argv) == 3 else "7238a1b"
    repository = pathlib.Path(git("rev-parse", "--show-toplevel", cwd=HERE).strip())

    git("worktree", "add", "--detach", str(scratch), "HEAD", cwd=repository)
    rust = scratch / "rust"
    source = rust / "core" / "src"

    main_builder = git("show", f"{main_revision}:rust/core/src/table_builder.rs", cwd=repository)
    (source / "main_builder.rs").write_text(main_builder)

    for variant, edits in VARIANTS.items():
        shutil.copy(source / "table_builder.rs", source / f"{variant}.rs")
        shutil.copytree(source / "table_builder", source / variant)
        for old, new in edits:
            edit(source / f"{variant}.rs", old, new)

    modules = "".join(
        f'#[cfg(feature = "main-builder")]\n#[doc(hidden)]\n'
        f"#[allow(clippy::all, clippy::pedantic, clippy::nursery, dead_code)]\n"
        f"pub mod {module};\n"
        for module in ["main_builder", *VARIANTS]
    )
    edit(source / "lib.rs", "mod table_builder;\n", modules + "mod table_builder;\n")
    edit(rust / "core" / "Cargo.toml", "[dev-dependencies]", "[features]\nmain-builder = []\n\n[dev-dependencies]")

    benchmarks = rust / "benchmarks"
    bins = "".join(
        f'[[bin]]\nname = "{name}"\nbench = false\nrequired-features = ["main-builder"]\n\n'
        for name in ["builder_comparison", "builder_allocation"]
    )
    edit(benchmarks / "Cargo.toml", "[[bench]]", bins + "[[bench]]")
    edit(
        benchmarks / "Cargo.toml",
        "[dependencies]",
        '[features]\nmain-builder = ["tafla/main-builder"]\n\n[dependencies]',
    )
    edit(
        benchmarks / "src" / "lib.rs",
        "pub mod codecs;",
        '#[cfg(feature = "main-builder")]\npub mod builders;\npub mod codecs;',
    )
    shutil.copy(HERE / "builders.rs", benchmarks / "src" / "builders.rs")
    for name in ["builder_comparison", "builder_allocation"]:
        shutil.copy(HERE / f"{name}.rs", benchmarks / "src" / "bin" / f"{name}.rs")
    shutil.copy(HERE / "differential.rs", rust / "core" / "tests" / "differential.rs")

    print(f"Set up {scratch}; see {HERE / 'README.md'} for the commands")


if __name__ == "__main__":
    main()
