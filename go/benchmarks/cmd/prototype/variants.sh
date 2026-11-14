#!/bin/sh
# Generates variants/<name>/, the variants of the prototype that main.go
# times: each a copy of tafla/go/prototype with its own constants in
# variant.go, and the comparison's serializers from serialize.go.tmpl.
#
#   sh variants.sh && go run -tags prototype_variants .
set -eu

here=$(cd "$(dirname "$0")" && pwd)
prototype=$here/../../../prototype
rm -rf "$here/variants"

# variant <name> <inlineBytes> <firstHeapBytes> <growByAppend> <segmentMin> <bytesWithoutCopy>
variant() {
	dir=$here/variants/$1
	mkdir -p "$dir"
	for file in "$prototype"/*.go; do
		case $(basename "$file") in
		*_test.go | variant.go | variant_stress.go) continue ;;
		esac
		sed "s/^package prototype$/package $1/" "$file" >"$dir/$(basename "$file")"
	done

	cat >"$dir/variant.go" <<GO
package $1

const (
	inlineBytes      = $2
	firstHeapBytes   = $3
	growByAppend     = $4
	segmentMin       = $5
	bytesWithoutCopy = $6
)
GO
	sed -e "s/^package template$/package $1/" \
		-e '/direct "github.com\/snovader\/tafla\/go\/prototype"/d' \
		-e 's/direct\.//g' "$here/serialize.go.tmpl" >"$dir/serialize.go"
	gofmt -w "$dir"
}

#       name               inline first append segments  no copy
variant direct             128    512   false  256       false
variant appendgrowth       128    512   true   256       false
variant noinline128        0      128   false  256       false
variant noinline512        0      512   false  256       false
variant nocopy             128    512   false  256       true
variant noinline128nocopy  0      128   false  256       true
variant copying            128    512   false  '1 << 62' false
variant copyingappend      128    512   true   '1 << 62' false
