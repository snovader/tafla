package stringlists

import (
	"errors"
	"fmt"
	"slices"

	gen "github.com/snovader/tafla/go/benchmarks/generated/lists"
)

// Codec is a way to write a list of strings, and to read it back.
type Codec interface {
	// Name returns the way's name, which names its variants.
	Name() string
	// Serialize writes the strings into new bytes.
	Serialize(strings []string) ([]byte, error)
	// Deserialize reads every string, in order, into a new slice.
	Deserialize(bytes []byte) ([]string, error)
	// ReadLast reads the last string, without reading the others.
	ReadLast(bytes []byte) (string, error)
}

// Codecs are every way, in the order that the report lists them, the vector
// of strings twice, an A/A pair that shows the harness's noise.
func Codecs() []Codec {
	return []Codec{
		StringList{name: "tafla-gen [string]"},
		NameList{},
		StringList{name: "tafla-gen [string] A/A"},
	}
}

var errEmpty = errors.New("there are no strings")

// Check checks that a way reads back exactly the strings of a list that it
// wrote, in order, every string and the last.
func Check(codec Codec, shape Shape) error {
	bytes, err := codec.Serialize(shape.Strings)
	if err != nil {
		return fmt.Errorf("%s can't write %s: %w", codec.Name(), shape.Name, err)
	}

	read, err := codec.Deserialize(bytes)
	if err != nil || !slices.Equal(read, shape.Strings) {
		return fmt.Errorf("%s doesn't read back %s: %v", codec.Name(), shape.Name, err)
	}

	last, err := codec.ReadLast(bytes)
	if err != nil || last != shape.Strings[len(shape.Strings)-1] {
		return fmt.Errorf("%s doesn't read back the last string of %s: %v", codec.Name(), shape.Name, err)
	}

	return nil
}

// StringList writes and reads the list as a vector of strings, with the code
// that tafla-gen generates from lists.tafla, in generated/lists, whose
// StringListFields hold the strings as they are. Its getter returns a
// tafla.StringVector, whose Strings() reads every string and whose Get()
// reads one from its ends, sharing the bytes.
type StringList struct {
	name string
}

func (s StringList) Name() string { return s.name }

func (StringList) Serialize(strings []string) ([]byte, error) {
	return gen.StringListFields{Items: strings}.MarshalBinary()
}

func (StringList) Deserialize(bytes []byte) ([]string, error) {
	items, err := gen.NewStringList(bytes).Items()
	if err != nil {
		return nil, err
	}

	return items.Strings()
}

func (StringList) ReadLast(bytes []byte) (string, error) {
	items, err := gen.NewStringList(bytes).Items()
	if err != nil {
		return "", err
	}
	if items.Len() == 0 {
		return "", errEmpty
	}

	return items.Get(items.Len() - 1)
}

// NameList writes and reads the list as a vector of tables of one string
// each, which is how a schema holds a list of strings without a vector of
// strings, with the code that tafla-gen generates. It creates the generated
// fields of each table in each call, as the TypeScript suite does, and reads
// every string through the view of the vector's All(), which reads each
// item's end once, and each item's getter, sharing the bytes.
type NameList struct{}

func (NameList) Name() string { return "tafla-gen [Name]" }

func (NameList) Serialize(strings []string) ([]byte, error) {
	items := make([]gen.NameFields, len(strings))
	for index, value := range strings {
		items[index] = gen.NameFields{Value: value}
	}

	return gen.NameListFields{Items: items}.MarshalBinary()
}

func (NameList) Deserialize(bytes []byte) ([]string, error) {
	items, err := gen.NewNameList(bytes).Items()
	if err != nil {
		return nil, err
	}

	strings := make([]string, 0, items.Len())
	for item, err := range items.All() {
		if err != nil {
			return nil, err
		}
		value, err := item.Value()
		if err != nil {
			return nil, err
		}
		strings = append(strings, value)
	}

	return strings, nil
}

func (NameList) ReadLast(bytes []byte) (string, error) {
	items, err := gen.NewNameList(bytes).Items()
	if err != nil {
		return "", err
	}
	if items.Len() == 0 {
		return "", errEmpty
	}

	item, err := items.Get(items.Len() - 1)
	if err != nil {
		return "", err
	}

	return item.Value()
}
