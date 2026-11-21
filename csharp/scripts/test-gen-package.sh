#!/usr/bin/env bash
#
# Tests the package Tafla.Gen that `dotnet pack` wrote, in artifacts/package/release,
# as a project that uses it would: tests/gen-package restores it, and the runtime's
# package, generates its schemas' code with this host's tafla-gen as it builds,
# with warnings as errors, and runs. A second build generates nothing again, a
# schema with an error fails the build with the error, and so does a directory
# to generate into that holds another file.
#
#   scripts/build-gen.sh
#   dotnet pack src/Tafla
#   dotnet pack src/Tafla.Gen -p:TaflaGenHosts=<the host that build-gen.sh printed>
#   scripts/test-gen-package.sh
#
# A package of every host's commands, as the release packs it, is tested the same
# way on each host.

set -euo pipefail

cd "$(dirname "$0")/.."

runtime_version=$(dotnet msbuild src/Tafla -getProperty:PackageVersion)
version=$(dotnet msbuild src/Tafla.Gen -getProperty:PackageVersion)
for package in "Tafla.$runtime_version" "Tafla.Gen.$version"; do
    if [[ ! -f "artifacts/package/release/$package.nupkg" ]]; then
        echo "artifacts/package/release/$package.nupkg doesn't exist: run dotnet pack first" >&2
        exit 1
    fi
done

# NuGet doesn't restore a version again once it has extracted it, so the
# versions that an earlier run restored are removed, and the packages are the
# ones that were packed last. The project's own outputs are removed too, so
# that the first build generates.
rm -rf artifacts/gen-package-test

versions=(-p:TaflaVersion="$runtime_version" -p:TaflaGenVersion="$version")
generated=artifacts/gen-package-test/obj/GenPackageTest/debug/tafla

dotnet run --project tests/gen-package "${versions[@]}"

# The generated code of a build that changes nothing is the code of the build
# before it, which the build compiled
before=$(ls -l --time-style=full-iso "$generated" 2> /dev/null || ls -lT "$generated")
dotnet build tests/gen-package "${versions[@]}" > /dev/null
after=$(ls -l --time-style=full-iso "$generated" 2> /dev/null || ls -lT "$generated")
if [[ "$before" != "$after" ]]; then
    echo "A build that changed nothing generated the code again" >&2
    exit 1
fi

# A schema's error fails the build, and names the schema. The schema's path is
# relative to the project.
echo "message Point { x: flaot64; }" > artifacts/gen-package-test/invalid.tafla
invalid=../../artifacts/gen-package-test/invalid.tafla
if output=$(dotnet build tests/gen-package "${versions[@]}" -p:InvalidSchema="$invalid" 2>&1); then
    echo "A schema with an error built" >&2
    exit 1
fi
# shellcheck disable=SC2016 # The backticks are the error's own
if ! grep -q 'unknown type `flaot64`' <<< "$output"; then
    echo "The build of a schema with an error doesn't report it:" >&2
    echo "$output" >&2
    exit 1
fi
echo "A schema with an error failed the build with its error"

# The build empties the directory that it generates into, so it fails rather
# than generate into one that holds another file, which it keeps
foreign=artifacts/gen-package-test/foreign
mkdir -p "$foreign"
echo "A file that the build didn't generate" > "$foreign/notes.txt"
if output=$(dotnet build tests/gen-package "${versions[@]}" -p:TaflaGenOutputPath="../../$foreign" 2>&1); then
    echo "A build generated into a directory that held another file" >&2
    exit 1
fi
if [[ ! -f "$foreign/notes.txt" ]] || ! grep -q "holds files that it didn't generate" <<< "$output"; then
    echo "The build into a directory that held another file doesn't keep it and say so:" >&2
    echo "$output" >&2
    exit 1
fi
echo "A directory that held another file was kept, and failed the build"
