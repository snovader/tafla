package tafla_test

// The cost of reading every string of a vector of strings, which decides how
// StringVector.Strings checks them. It checks the strings as one run of
// UTF-8, and each string's first byte; "each" reads every string as Get
// reads one, checking each on its own. Both share the bytes.
//
//	go test -run '^$' -bench StringVectorStrings -count 10 .

import (
	"strconv"
	"testing"

	tafla "github.com/snovader/tafla/go"
)

var sinkStrings []string

// stringVectorTable returns a table whose only field is a vector of count
// strings of length characters, ASCII or of 3 bytes each
func stringVectorTable(count, length int, wide bool) []byte {
	const ascii = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"

	state := uint32(0x2545f491)
	strings := make([]string, count)
	for index := range strings {
		characters := make([]rune, length)
		for at := range characters {
			state ^= state << 13
			state ^= state >> 17
			state ^= state << 5
			if wide {
				characters[at] = 0x4e00 + rune(state%0x5000)
			} else {
				characters[at] = rune(ascii[state%62])
			}
		}
		strings[index] = string(characters)
	}

	builder, err := tafla.NewTableBuilder(1, tafla.TableLayout{})
	if err != nil {
		panic(err)
	}
	if err := builder.SetStringVector(strings); err != nil {
		panic(err)
	}
	bytes, err := builder.Bytes()
	if err != nil {
		panic(err)
	}

	return bytes
}

func BenchmarkStringVectorStrings(b *testing.B) {
	shapes := []struct {
		name   string
		length int
		wide   bool
	}{
		{"16 chars of ASCII", 16, false},
		{"256 chars of ASCII", 256, false},
		{"8 chars of 3 bytes", 8, true},
	}

	for _, count := range []int{10, 100, 10_000} {
		for _, shape := range shapes {
			reader := tafla.NewTableReader(stringVectorTable(count, shape.length, shape.wide), tafla.TableLayout{})
			position, err := reader.RootOffset(0)
			if err != nil {
				b.Fatal(err)
			}
			name := strconv.Itoa(count) + " × " + shape.name

			b.Run(name+"/run", func(b *testing.B) {
				for range b.N {
					vector, err := reader.GetStringVector(position)
					if err != nil {
						b.Fatal(err)
					}
					sinkStrings, err = vector.Strings()
					if err != nil {
						b.Fatal(err)
					}
				}
			})

			b.Run(name+"/each", func(b *testing.B) {
				for range b.N {
					vector, err := reader.GetStringVector(position)
					if err != nil {
						b.Fatal(err)
					}
					strings := make([]string, vector.Len())
					for index := range strings {
						if strings[index], err = vector.Get(index); err != nil {
							b.Fatal(err)
						}
					}
					sinkStrings = strings
				}
			})
		}
	}
}
