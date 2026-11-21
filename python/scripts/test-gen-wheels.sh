#!/bin/sh
#
# Tests each of tafla-gen's wheels in a directory on its own: installs it
# alone, checks that its command has the wheel's version, and generates the
# Python of the example schemas in schemas/, which must match the golden files
# in core/tests/generated.
#
#   scripts/test-gen-wheels.sh <directory>
#
# Needs uv, which downloads a Python to install the wheel into. It's POSIX
# sh, for Alpine's.

set -eu

python=$(cd "$(dirname "$0")/.." && pwd)
wheels=$(cd "$1" && pwd)

# The golden files name each schema by its path from the root of the
# repository, schemas/<schema>.tafla, so the command runs from there. No
# virtual environment lies in it or above it, which uv run would take its
# interpreter from.
cd "$python/.."

for wheel in "$wheels"/tafla_gen-*.whl; do
    echo "::group::$(basename "$wheel")"

    # tafla_gen-<version>-py3-none-<platform>.whl, whose version is
    # pyproject.toml's, and the command prints the crate's
    version=$(basename "$wheel" | cut -d- -f2)
    printed=$(uv run --isolated --no-project --with "$wheel" tafla-gen --version)
    if [ "$printed" != "tafla-gen $version" ]; then
        echo "$wheel: the command prints \"$printed\", not \"tafla-gen $version\"" >&2
        exit 1
    fi
    echo "$printed"

    out=$(mktemp -d)
    uv run --isolated --no-project --with "$wheel" tafla-gen python schemas/*.tafla --out "$out"

    for schema in schemas/*.tafla; do
        name=$(basename "$schema" .tafla)
        diff "$python/core/tests/generated/$name/$name.py" "$out/$name.py"
    done

    rm -r "$out"
    echo "::endgroup::"
done
