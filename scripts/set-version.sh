#!/usr/bin/env bash
#
# Sets the version of every published package from VERSION, the one file that
# holds it, or checks that they agree with it.
#
#   scripts/set-version.sh <version>   writes <version> into VERSION, every
#                                      package's manifest, and the lock files
#                                      that record it
#   scripts/set-version.sh --check     fails if a manifest or lock file
#                                      disagrees with VERSION
#
# A version is MAJOR.MINOR.PATCH. Pre-releases are left out, since npm, PyPI
# and crates.io spell them differently, and uv.lock records PyPI's spelling.

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
version_file="$root/VERSION"

# Each place that records the version: a file, and a Perl pattern that the
# version follows in it, up to its closing quote. Each pattern matches once.
# The Go module records none, since its tag, go/v<version>, is its version,
# which the release workflow takes from VERSION.
locations=(
    'typescript/core/package.json|^  "version": "'
    'python/core/pyproject.toml|^version = "'
    'python/uv.lock|^name = "tafla"\nversion = "'
    'rust/core/Cargo.toml|^version = "'
    'rust/gen/Cargo.toml|^version = "'
    'rust/gen/Cargo.toml|^tafla = \{ path = "\.\./core", version = "'
    'rust/Cargo.lock|^name = "tafla"\nversion = "'
    'rust/Cargo.lock|^name = "tafla-gen"\nversion = "'
    # The Python runtime's native module, a crate on the Rust runtime, whose
    # lock file records the runtime's version, which the release's wheels
    # build with --locked
    'python/core/native/Cargo.toml|^version = "'
    'python/core/native/Cargo.lock|^name = "tafla"\nversion = "'
    'python/core/native/Cargo.lock|^name = "tafla-python"\nversion = "'
    'java/core/build.gradle.kts|^version = "'
)

usage() {
    echo "usage: $0 <version> | --check" >&2
    exit 2
}

# Prints the version that follows the pattern in the file, or nothing if the
# pattern doesn't match
read_version() {
    perl -0777 -e '
        my ($file, $pattern) = @ARGV;
        open my $in, "<", $file or die "$file: $!\n";
        my $text = <$in>;
        print $1 if $text =~ /${pattern}([^"]*)"/m;
    ' "$1" "$2"
}

# Replaces the version that follows the pattern in the file, and fails without
# writing the file unless the pattern matched
write_version() {
    perl -0777 -e '
        my ($file, $pattern, $version) = @ARGV;
        open my $in, "<", $file or die "$file: $!\n";
        my $text = <$in>;
        close $in;
        $text =~ s/(${pattern})[^"]*"/$1$version"/m or exit 1;
        open my $out, ">", $file or die "$file: $!\n";
        print $out $text;
    ' "$1" "$2" "$3"
}

check() {
    local expected failed=0
    expected=$(tr -d '[:space:]' < "$version_file")

    for location in "${locations[@]}"; do
        local file="${location%%|*}" pattern="${location#*|}" actual
        actual=$(read_version "$root/$file" "$pattern")

        if [[ -z "$actual" ]]; then
            echo "$file: no version matches ${pattern//\\n/ }" >&2
            failed=1
        elif [[ "$actual" != "$expected" ]]; then
            echo "$file: $actual, but VERSION is $expected" >&2
            failed=1
        fi
    done

    if ((failed)); then
        echo "Run scripts/set-version.sh $expected to write VERSION into every package" >&2
        exit 1
    fi
    echo "Every package is at version $expected"
}

set_version() {
    local version="$1"
    if [[ ! "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
        echo "$version isn't a version of the form MAJOR.MINOR.PATCH" >&2
        exit 2
    fi

    echo "$version" > "$version_file"
    for location in "${locations[@]}"; do
        local file="${location%%|*}" pattern="${location#*|}"
        if ! write_version "$root/$file" "$pattern" "$version"; then
            echo "$file: no version matches ${pattern//\\n/ }" >&2
            exit 1
        fi
    done

    check
}

case "${1:-}" in
    --check) [[ $# -eq 1 ]] || usage; check ;;
    -* | "") usage ;;
    *) [[ $# -eq 1 ]] || usage; set_version "$1" ;;
esac
