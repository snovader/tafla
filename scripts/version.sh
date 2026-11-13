#!/usr/bin/env bash
#
# Prints or sets the version of one of tafla's packages, each of which has a
# version of its own, as RELEASING.md describes.
#
#   scripts/version.sh <package>             prints the package's version, and
#                                            fails if the places that record
#                                            it disagree
#   scripts/version.sh <package> <version>   writes <version> into every place
#                                            that records the package's version
#
# The packages are typescript, python, java, rust and tafla-gen. The Go module
# records no version, since its tag, go/v<version>, is its version.
#
# A version is MAJOR.MINOR.PATCH. Pre-releases are left out, since npm, PyPI
# and crates.io spell them differently, and uv.lock records PyPI's spelling.

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)

usage() {
    echo "usage: $0 <package> [<version>]" >&2
    exit 2
}

# Sets locations to each place that records the package's version: a file, and
# a Perl pattern that the version follows in it, up to its closing quote. Each
# pattern matches once. The first place is the package's manifest.
locate() {
    case "$1" in
        typescript)
            locations=(
                'typescript/core/package.json|^  "version": "'
            )
            ;;
        python)
            locations=(
                'python/core/pyproject.toml|^version = "'
                'python/uv.lock|^name = "tafla"\nversion = "'
                # The native module, a crate that maturin builds into the
                # distribution, and whose lock file the wheels build with
                # --locked
                'python/core/native/Cargo.toml|^version = "'
                'python/core/native/Cargo.lock|^name = "tafla-python"\nversion = "'
            )
            ;;
        java)
            locations=(
                'java/core/build.gradle.kts|^version = "'
            )
            ;;
        rust)
            locations=(
                'rust/core/Cargo.toml|^version = "'
                'rust/Cargo.lock|^name = "tafla"\nversion = "'
                # The runtime that tafla-gen's tests run its generated Rust
                # against, which crates.io resolves when tafla-gen is
                # published
                'rust/gen/Cargo.toml|^tafla = \{ path = "\.\./core", version = "'
                # The Python runtime's native module, which builds on the Rust
                # runtime
                'python/core/native/Cargo.lock|^name = "tafla"\nversion = "'
            )
            ;;
        tafla-gen)
            locations=(
                'rust/gen/Cargo.toml|^version = "'
                'rust/Cargo.lock|^name = "tafla-gen"\nversion = "'
            )
            ;;
        go)
            echo "The Go module records no version: its tag, go/v<version>, is its version" >&2
            exit 2
            ;;
        *)
            echo "$1 isn't a package: typescript, python, java, rust or tafla-gen" >&2
            exit 2
            ;;
    esac
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

# Prints the version in the package's manifest, once every other place has
# been checked to record it too
print_version() {
    local package="$1" manifest="${locations[0]%%|*}" version failed=0
    version=$(read_version "$root/$manifest" "${locations[0]#*|}")
    if [[ -z "$version" ]]; then
        echo "$manifest: no version matches ${locations[0]#*|}" >&2
        exit 1
    fi

    for location in "${locations[@]:1}"; do
        local file="${location%%|*}" pattern="${location#*|}" actual
        actual=$(read_version "$root/$file" "$pattern")

        if [[ -z "$actual" ]]; then
            echo "$file: no version matches ${pattern//\\n/ }" >&2
            failed=1
        elif [[ "$actual" != "$version" ]]; then
            echo "$file: $actual, but $manifest has $version" >&2
            failed=1
        fi
    done

    if ((failed)); then
        echo "Run scripts/version.sh $package $version to write it into every place" >&2
        exit 1
    fi
    echo "$version"
}

set_version() {
    local package="$1" version="$2"
    if [[ ! "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
        echo "$version isn't a version of the form MAJOR.MINOR.PATCH" >&2
        exit 2
    fi

    for location in "${locations[@]}"; do
        local file="${location%%|*}" pattern="${location#*|}"
        if ! write_version "$root/$file" "$pattern" "$version"; then
            echo "$file: no version matches ${pattern//\\n/ }" >&2
            exit 1
        fi
    done

    print_version "$package" > /dev/null
    echo "$package is at version $version"
}

case "$#:${1:-}" in
    [12]:-*) usage ;;
    1:*) locate "$1"; print_version "$1" ;;
    2:*) locate "$1"; set_version "$1" "$2" ;;
    *) usage ;;
esac
