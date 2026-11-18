#!/bin/sh
# Runs a fuzz target under libFuzzer, on Linux: builds libfuzzer-dotnet, which runs the .NET
# process for libFuzzer and passes it the inputs, from its source at a pinned commit, builds the
# fuzz targets, instruments the runtime with SharpFuzz, and runs the target from the corpus that
# its earlier runs grew and the conformance suite's tables. It needs the .NET 10 SDK, a Clang with
# libFuzzer, such as a Linux distribution's clang and libclang-rt-dev, curl and sha256sum, and the
# network, to fetch libfuzzer-dotnet's source and the targets' packages the first time.
#
#   fuzz/fuzz.sh <table_reader|conformance> [libFuzzer option ...]
#
# TAFLA_PORTABLE=true builds the runtime to copy every array that it reads, as a big-endian host and
# 32-bit Arm do, rather than read it in place.
#
# libFuzzer writes an input that fails to artifacts/fuzz/<target>/, which
# `dotnet artifacts/bin/Tafla.Fuzz/release/Tafla.Fuzz.dll <target> <file>` reads again, on any host.
set -eu

usage="Usage: fuzz/fuzz.sh <table_reader|conformance> [libFuzzer option ...]"
target=${1:?$usage}
shift

# libfuzzer-dotnet's latest release, and the SHA-256 of its driver for Linux there
driver_commit=bd39d4e88d715ab460a929943645be2a186cde52
driver_sha256=90f019e2e9ad3a0b93c7ecc2c5afb2fbfc8b5aab6aac51c7e0d349ec79354f36

cd "$(dirname "$0")/.."
fuzz=artifacts/fuzz
driver=$fuzz/libfuzzer-dotnet-$driver_commit
mkdir -p "$fuzz/corpus/$target" "$fuzz/$target"

if [ ! -x "$driver" ]; then
    curl --fail --silent --show-error --location --retry 5 --retry-all-errors --output "$driver.cc" \
        "https://raw.githubusercontent.com/Metalnem/libfuzzer-dotnet/$driver_commit/libfuzzer-dotnet.cc"
    echo "$driver_sha256  $driver.cc" | sha256sum --check --quiet -

    # The driver reports a crash of the .NET process with __builtin_trap(), which raises SIGTRAP
    # on Arm64, where libFuzzer saves no input for it. abort() raises SIGABRT on every host.
    "${CXX:-clang++}" -fsanitize=fuzzer -O2 '-D__builtin_trap()=abort()' "$driver.cc" -o "$driver"
fi

# The build copies the runtime anew, which is then instrumented
dotnet build fuzz/Tafla.Fuzz --configuration Release "-p:TaflaPortable=${TAFLA_PORTABLE:-false}"
dotnet artifacts/bin/Tafla.Fuzz/release/Tafla.Fuzz.dll instrument

# libFuzzer adds the inputs that it finds to the first directory, the corpus
exec "$driver" --target_path=artifacts/bin/Tafla.Fuzz/release/Tafla.Fuzz --target_arg="$target" \
    -artifact_prefix="$fuzz/$target/" "$@" \
    "$fuzz/corpus/$target" ../conformance/valid ../conformance/equivalent ../conformance/invalid
