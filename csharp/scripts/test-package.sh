#!/usr/bin/env bash
#
# Tests the package Tafla that `dotnet pack` wrote, in artifacts/package/release,
# as a project that uses it would: tests/package restores the package at the
# runtime's version, builds with warnings as errors and runs, and then
# publishes with Native AOT, whose analysis of the trimmed package fails on any
# warning, and runs compiled ahead of time.
#
#   dotnet pack src/Tafla
#   scripts/test-package.sh
#
# Native AOT needs a C toolchain, and its packages come from nuget.org.

set -euo pipefail

cd "$(dirname "$0")/.."

version=$(dotnet msbuild src/Tafla -getProperty:PackageVersion)
package="artifacts/package/release/Tafla.$version.nupkg"
if [[ ! -f "$package" ]]; then
    echo "$package doesn't exist: run dotnet pack src/Tafla first" >&2
    exit 1
fi

# NuGet doesn't restore a version again once it has extracted it, so the
# version that an earlier run restored is removed, and the package is the one
# that was packed last
rm -rf artifacts/package-test/packages/tafla

dotnet run --project tests/package -p:TaflaVersion="$version"

aot=artifacts/package-test/aot
rm -rf "$aot"
dotnet publish tests/package -p:TaflaVersion="$version" --output "$aot"
"$aot/PackageTest"
