// Package stringlists holds the lists of strings that cmd/lists writes and
// reads, and the ways that it writes and reads them: tafla-gen's vector of
// strings, and its vector of tables of one string each.
package stringlists

import (
	"fmt"

	"github.com/snovader/tafla/go/benchmarks/harness"
)

// Shape is a list of distinct strings of the same length and mix of UTF-8
// widths.
type Shape struct {
	Name    string
	Strings []string
}

// Counts are the lists' numbers of strings.
var Counts = []int{10, 100, 10_000}

// ascii are the characters of the strings of ASCII
const ascii = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"

// threeBytes is the first of the CJK ideographs, of three bytes in UTF-8
const threeBytes = 0x4e00

// kind is a kind of string, with its index among the TypeScript suite's
// kinds, which seeds its shapes
type kind struct {
	name   string
	length int
	index  int
	pick   func(random *xorshift) rune
}

// kinds are the TypeScript suite's kinds of strings that every language's
// suite times; that suite times five more
var kinds = []kind{
	{"16 chars of ASCII", 16, 1, pickASCII},
	{"256 chars of ASCII", 256, 2, pickASCII},
	{"8 chars of 3 bytes", 8, 6, pickThreeBytes},
}

// kindsInTypeScript is the number of the TypeScript suite's kinds, which
// spaces the seeds of each count's shapes
const kindsInTypeScript = 8

// Shapes are every list, for each count and then each kind of string, with
// the strings of typescript/benchmarks/strings/lists/shapes.ts.
var Shapes = shapes()

func shapes() []Shape {
	var all []Shape
	for countIndex, count := range Counts {
		for _, kind := range kinds {
			name := fmt.Sprintf("%s × %s", harness.Thousands(count), kind.name)
			seed := uint32(1 + kind.index + kindsInTypeScript*countIndex)
			all = append(all, shape(name, count, kind, seed))
		}
	}

	return all
}

// shape returns count distinct strings of a kind, from a generator of its
// own, and panics if they aren't distinct
func shape(name string, count int, kind kind, seed uint32) Shape {
	random := newXorshift(seed)
	strings := make([]string, count)
	seen := make(map[string]bool, count)
	for index := range strings {
		characters := make([]rune, kind.length)
		for at := range characters {
			characters[at] = kind.pick(random)
		}

		strings[index] = string(characters)
		if seen[strings[index]] {
			panic("the strings of " + name + " aren't distinct")
		}
		seen[strings[index]] = true
	}

	return Shape{Name: name, Strings: strings}
}

func pickASCII(random *xorshift) rune {
	return rune(ascii[random.below(len(ascii))])
}

func pickThreeBytes(random *xorshift) rune {
	return threeBytes + rune(random.below(0x5000))
}

// xorshift is the TypeScript suite's deterministic generator, so that every
// language's suite writes the same strings
type xorshift struct {
	state uint32
}

func newXorshift(seed uint32) *xorshift {
	return &xorshift{state: 0x2545f491 ^ seed}
}

// below returns floor(next() * n), where next() is the generator's next
// state over 2^32, in [0, 1), as the TypeScript suite picks a character. The
// product is exact in a float64 there, so it's computed in integers here.
func (x *xorshift) below(n int) int {
	x.state ^= x.state << 13
	x.state ^= x.state >> 17
	x.state ^= x.state << 5

	return int(uint64(x.state) * uint64(n) >> 32)
}
