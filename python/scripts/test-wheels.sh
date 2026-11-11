#!/bin/sh
#
# Tests each wheel in a directory on its own: installs it alone in each
# Python that its tags name, checks that tafla chose the implementation that
# the wheel holds, and runs the core tests against it.
#
#   scripts/test-wheels.sh <directory>
#
# Needs uv, which downloads the Pythons. It's POSIX sh, for Alpine's, and
# fails for a wheel whose tags it doesn't know.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
wheels=$(cd "$1" && pwd)

# uv run takes its interpreter from a virtual environment in the working
# directory or above it, such as python/.venv, whose tafla is the sources'. It
# runs from elsewhere, so that tafla is the installed wheel's.
cd "$(mktemp -d)"

for wheel in "$wheels"/*.whl; do
    case "$wheel" in
        *-py3-none-any.whl) versions="3.14+gil" implementation=python ;;
        # The stable ABI, which 3.14 and every later version can load
        *-cp314-abi3-*) versions="3.14+gil 3.15+gil" implementation=native ;;
        # The free-threaded stable ABI, for 3.15 and every later version
        *-cp315-abi3.abi3t-*) versions="3.15t" implementation=native ;;
        *-cp314-cp314t-*) versions="3.14t" implementation=native ;;
        *-cp314-cp314-*) versions="3.14+gil" implementation=native ;;
        *)
            echo "Unknown wheel tags: $wheel" >&2
            exit 1
            ;;
    esac

    for version in $versions; do
        echo "::group::$(basename "$wheel") in Python $version"
        # A request for 3.14+gil doesn't download 3.14, the build with the
        # GIL, but finds it. --no-bin leaves the executables on the path as
        # they were.
        uv python install --no-bin "${version%+gil}"
        uv run --isolated --no-project --python "$version" \
            --with "$wheel" --with "hypothesis>=6.170.15" \
            python -I -c "
import sys, tafla
assert tafla.IMPLEMENTATION == '$implementation', tafla.IMPLEMENTATION
assert 'site-packages' in tafla.__file__, tafla.__file__
print(sys.version, tafla.IMPLEMENTATION, tafla.__file__)
"
        uv run --isolated --no-project --python "$version" \
            --with "$wheel" --with "hypothesis>=6.170.15" \
            python -I -m unittest discover -s "$root/core/tests" -t "$root/core"
        echo "::endgroup::"
    done
done
