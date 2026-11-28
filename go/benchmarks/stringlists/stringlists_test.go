package stringlists_test

// The lists are the TypeScript suite's strings, and every way reads back
// exactly what it wrote, which cmd/lists checks again before timing them

import (
	"bytes"
	"fmt"
	"hash/fnv"
	"slices"
	"testing"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/benchmarks/stringlists"
)

// The first 16 characters of each list's first and last strings, and the
// FNV-1a hash of every string's UTF-8 and a line feed after it, which the
// TypeScript suite's shapes.ts gives
var typescript = []struct {
	name, first, last string
	hash              uint32
}{
	{"10 × 16 chars of ASCII", "2FsyeXxp1ewxJijA", "CcfRsv23GwWuZIqm", 0x081860c6},
	{"10 × 256 chars of ASCII", "2GYBM2rvPSVojjhN", "3VpjDEeuixVZfJJy", 0x0d958bb6},
	{"10 × 8 chars of 3 bytes", "鑡絞眙蜩榩眳矝愞", "琑翘趠谎衬耂竎刓", 0x0807805f},
	{"100 × 16 chars of ASCII", "2N45gJLJhyrBp817", "GuM8hMtpdCgpm337", 0x29c3f485},
	{"100 × 256 chars of ASCII", "2OC9KSUHzW0k790o", "e1d0Fdjo5UszbCUg", 0x57711e4a},
	{"100 × 8 chars of 3 bytes", "鑗蝝姘徃栲鈞甕顽", "鱵髩痧剻熉五憓荜", 0x344f0439},
	{"10,000 × 16 chars of ASCII", "2VGBSOLG6JrrCjwD", "FzBrmc8Y0jcGbWah", 0xf517a545},
	{"10,000 × 256 chars of ASCII", "2WgxnHUMGp0eUivR", "VU0lrgd6V0hWxlMq", 0x1968e965},
	{"10,000 × 8 chars of 3 bytes", "鑶鄷攴臚鷑靆畖鱯", "淟坓惗聳侞忝鞂姩", 0x88e50eac},
}

func TestTheListsAreTheTypeScriptSuitesStrings(t *testing.T) {
	if len(stringlists.Shapes) != len(typescript) {
		t.Fatalf("%d lists rather than %d", len(stringlists.Shapes), len(typescript))
	}

	for index, shape := range stringlists.Shapes {
		expected := typescript[index]
		hash := fnv.New32a()
		for _, value := range shape.Strings {
			fmt.Fprintf(hash, "%s\n", value)
		}

		first, last := prefix(shape.Strings[0]), prefix(shape.Strings[len(shape.Strings)-1])
		if shape.Name != expected.name || first != expected.first || last != expected.last || hash.Sum32() != expected.hash {
			t.Errorf("%s: %q to %q, hashed to %x, rather than %+v", shape.Name, first, last, hash.Sum32(), expected)
		}
	}
}

// prefix returns the first 16 characters of a string, or all of a shorter
// one
func prefix(value string) string {
	characters := []rune(value)
	return string(characters[:min(len(characters), 16)])
}

func TestEveryWayReadsBackWhatItWrote(t *testing.T) {
	for _, codec := range stringlists.Codecs() {
		for _, shape := range stringlists.Shapes {
			if err := stringlists.Check(codec, shape); err != nil {
				t.Error(err)
			}
		}
	}
}

// The vector of strings that tafla-gen's code writes is the one that
// TableBuilder writes
func TestTheVectorOfStringsIsTableBuildersVector(t *testing.T) {
	for _, shape := range stringlists.Shapes {
		written, err := stringlists.StringList{}.Serialize(shape.Strings)
		if err != nil {
			t.Fatal(err)
		}

		builder, err := tafla.NewTableBuilder(1, tafla.TableLayout{})
		if err != nil {
			t.Fatal(err)
		}
		if err := builder.SetStringVector(shape.Strings); err != nil {
			t.Fatal(err)
		}
		built, err := builder.Bytes()
		if err != nil || !bytes.Equal(written, built) {
			t.Errorf("%s: wrote %d bytes rather than TableBuilder's %d, %v", shape.Name, len(written), len(built), err)
		}
	}
}

func TestEveryWayFailsForAnEmptyList(t *testing.T) {
	for _, codec := range stringlists.Codecs() {
		written, err := codec.Serialize(nil)
		if err != nil {
			t.Fatal(err)
		}
		if read, err := codec.Deserialize(written); err != nil || !slices.Equal(read, nil) {
			t.Errorf("%s: read %q, %v", codec.Name(), read, err)
		}
		if _, err := codec.ReadLast(written); err == nil {
			t.Errorf("%s read a last string of none", codec.Name())
		}
	}
}
