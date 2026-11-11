package generated_test

// The code generated from schemas/shapes.tafla writes the tables of
// conformance/generated, which the generated TypeScript wrote, byte for
// byte, and reads back their values. conformance/generated/README.md
// describes the cases in shapes.json.

import (
	"bytes"
	"encoding"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/snovader/tafla/go/internal/generated/shapes"
	"github.com/snovader/tafla/go/internal/tables"
)

// shapeCase is a case of shapes.json
type shapeCase struct {
	Name   string          `json:"name"`
	Type   string          `json:"type"`
	File   string          `json:"file"`
	Fields json.RawMessage `json:"fields"`
}

func loadShapeCases(t *testing.T) []shapeCase {
	t.Helper()

	directory := filepath.Join(tables.Conformance(), "generated")
	contents, err := os.ReadFile(filepath.Join(directory, "shapes.json"))
	if err != nil {
		t.Fatal(err)
	}

	var suite struct {
		Schema string      `json:"schema"`
		Cases  []shapeCase `json:"cases"`
	}
	if err := json.Unmarshal(contents, &suite); err != nil {
		t.Fatal(err)
	}
	if suite.Schema != "schemas/shapes.tafla" {
		t.Fatalf("shapes.json describes %s", suite.Schema)
	}

	return suite.Cases
}

// shapeTypes are the messages and the struct of shapes.tafla, by their
// names in the schema: the struct of a message's fields, or the struct, and
// how to read bytes as it
var shapeTypes = map[string]struct {
	fields reflect.Type
	read   func([]byte) (reflect.Value, error)
}{
	"Point":    {reflect.TypeFor[shapes.PointFields](), readMessage(reflect.TypeFor[shapes.PointFields]())},
	"Document": {reflect.TypeFor[shapes.DocumentFields](), readMessage(reflect.TypeFor[shapes.DocumentFields]())},
	"ItemList": {reflect.TypeFor[shapes.ItemListFields](), readMessage(reflect.TypeFor[shapes.ItemListFields]())},
	"Sample": {reflect.TypeFor[shapes.Sample](), func(bytes []byte) (reflect.Value, error) {
		var sample shapes.Sample
		err := sample.UnmarshalBinary(bytes)
		return reflect.ValueOf(sample), err
	}},
}

// readMessage returns the reader of the message whose fields are of
// fieldsType, from the list of every message
func readMessage(fieldsType reflect.Type) func([]byte) (reflect.Value, error) {
	return func(bytes []byte) (reflect.Value, error) {
		for _, message := range messages {
			if message.fields == fieldsType {
				return message.readAll(bytes)
			}
		}
		return reflect.Value{}, fmt.Errorf("no message is written from %v", fieldsType)
	}
}

func TestTheGeneratedCodeWritesEveryShapeCaseByteForByte(t *testing.T) {
	for _, shapeCase := range loadShapeCases(t) {
		expected, fields := shapeCase.load(t)

		written, err := fields.Interface().(encoding.BinaryMarshaler).MarshalBinary()
		if err != nil || !bytes.Equal(written, expected) {
			t.Errorf("%s: wrote % x, %v\nrather than % x", shapeCase.Name, written, err, expected)
		}
	}
}

func TestTheGeneratedCodeReadsTheValuesOfEveryShapeCase(t *testing.T) {
	for _, shapeCase := range loadShapeCases(t) {
		expected, fields := shapeCase.load(t)

		read, err := shapeTypes[shapeCase.Type].read(expected)
		if err != nil || !sameValues(read, fields) {
			t.Errorf("%s: read %+v, %v\nrather than %+v", shapeCase.Name, read, err, fields)
		}
	}
}

// load returns the case's bytes and the fields that it describes
func (c shapeCase) load(t *testing.T) ([]byte, reflect.Value) {
	t.Helper()

	shapeType, ok := shapeTypes[c.Type]
	if !ok {
		t.Fatalf("%s: shapes.tafla has no %s", c.Name, c.Type)
	}

	expected, err := os.ReadFile(filepath.Join(tables.Conformance(), "generated", c.File))
	if err != nil {
		t.Fatal(err)
	}

	fields := reflect.New(shapeType.fields).Elem()
	if err := setFromJSON(fields, c.Fields); err != nil {
		t.Fatalf("%s: %v", c.Name, err)
	}

	return expected, fields
}

// setFromJSON sets value from the JSON of a case's fields: an object of each
// field by its name in the schema, 64-bit integers as decimal strings, and
// floats by name when JSON has no number for them
func setFromJSON(value reflect.Value, raw json.RawMessage) error {
	switch value.Kind() {
	case reflect.Struct:
		var object map[string]json.RawMessage
		if err := json.Unmarshal(raw, &object); err != nil {
			return err
		}
		if len(object) != value.NumField() {
			return fmt.Errorf("%d fields of a %v", len(object), value.Type())
		}
		for name, field := range object {
			target := value.FieldByName(pascalCase(name))
			if !target.IsValid() {
				return fmt.Errorf("%v has no field %s", value.Type(), name)
			}
			if err := setFromJSON(target, field); err != nil {
				return err
			}
		}
	case reflect.Slice:
		var elements []json.RawMessage
		if err := json.Unmarshal(raw, &elements); err != nil {
			return err
		}
		value.Set(reflect.MakeSlice(value.Type(), len(elements), len(elements)))
		for index, element := range elements {
			if err := setFromJSON(value.Index(index), element); err != nil {
				return err
			}
		}
	case reflect.Int64:
		parsed, err := tables.ParseInt64(raw)
		value.SetInt(parsed)
		return err
	case reflect.Uint64:
		parsed, err := tables.ParseUint64(raw)
		value.SetUint(parsed)
		return err
	case reflect.Float32:
		parsed, err := tables.ParseFloat32(raw)
		// As a float32, since converting it to a float64 may quiet a NaN
		value.Set(reflect.ValueOf(parsed))
		return err
	case reflect.Float64:
		parsed, err := tables.ParseFloat64(raw)
		value.SetFloat(parsed)
		return err
	default:
		return json.Unmarshal(raw, value.Addr().Interface())
	}

	return nil
}

// pascalCase returns a schema's snake_case name as Go exports it
func pascalCase(name string) string {
	words := strings.Split(name, "_")
	for index, word := range words {
		if word != "" {
			words[index] = strings.ToUpper(word[:1]) + word[1:]
		}
	}

	return strings.Join(words, "")
}
