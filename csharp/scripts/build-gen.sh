#!/usr/bin/env bash
#
# Builds the tafla-gen command that the package Tafla.Gen packs for a host, from
# rust/gen, into artifacts/tafla-gen/<host>/, and prints the host's name, such as
# osx-arm64, which is the directory of its command in the package.
#
#   scripts/build-gen.sh              builds for this host
#   scripts/build-gen.sh <target>     builds for a Rust target, which rustup has
#                                     installed, such as x86_64-unknown-linux-musl
#
# The release builds Linux's commands for musl, which links them statically, so
# that one command runs with glibc and with musl. Windows' link the C runtime
# statically too, since a host with .NET needn't have Visual C++'s.

set -euo pipefail

cd "$(dirname "$0")/.."

# rust/'s toolchain, which rust-toolchain.toml names
target=${1:-$(cd ../rust && rustc --version --verbose | sed -n 's/^host: //p')}

case "$target" in
    x86_64-unknown-linux-*) host=linux-x64 ;;
    aarch64-unknown-linux-*) host=linux-arm64 ;;
    x86_64-apple-darwin) host=osx-x64 ;;
    aarch64-apple-darwin) host=osx-arm64 ;;
    x86_64-pc-windows-msvc) host=win-x64 ;;
    aarch64-pc-windows-msvc) host=win-arm64 ;;
    *)
        echo "Tafla.Gen packs no command for $target" >&2
        exit 2
        ;;
esac

command=tafla-gen
if [[ "$host" == win-* ]]; then
    command=tafla-gen.exe
    export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static"
fi

# Without its symbols, which are most of the command's size
(cd ../rust && CARGO_PROFILE_RELEASE_STRIP=symbols \
    cargo build --release --locked --package tafla-gen --bin tafla-gen --target "$target" >&2)

mkdir -p "artifacts/tafla-gen/$host"
cp "../rust/target/$target/release/$command" "artifacts/tafla-gen/$host/$command"

echo "$host"
