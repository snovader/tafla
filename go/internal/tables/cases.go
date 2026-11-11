package tables

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"runtime"
	"strconv"

	tafla "github.com/snovader/tafla/go"
)

// Conformance returns the path of the conformance suite, at the root of the
// repository.
func Conformance() string {
	_, file, _, _ := runtime.Caller(0)
	return filepath.Join(filepath.Dir(file), "..", "..", "..", "conformance")
}

// Suite is the conformance suite's cases.json, with the bytes of each case.
type Suite struct {
	Valid      []ValidCase
	Equivalent []ChangedCase
	Invalid    []ChangedCase
}

// ValidCase is a valid table, with its description.
type ValidCase struct {
	Name  string
	Bytes []byte
	Table Table
}

// ChangedCase is a table whose bytes differ from those of a valid case, Case,
// which describes them. Field is the root field that fails to read, for an
// invalid case.
type ChangedCase struct {
	Name  string
	Bytes []byte
	Case  string
	Field int
}

// LoadSuite reads cases.json and the bytes of its cases.
func LoadSuite() (*Suite, error) {
	directory := Conformance()
	contents, err := os.ReadFile(filepath.Join(directory, "cases.json"))
	if err != nil {
		return nil, err
	}

	var suite struct {
		Valid []struct {
			Name  string    `json:"name"`
			File  string    `json:"file"`
			Table jsonTable `json:"table"`
		} `json:"valid"`
		Equivalent []jsonChangedCase `json:"equivalent"`
		Invalid    []jsonChangedCase `json:"invalid"`
	}
	if err := json.Unmarshal(contents, &suite); err != nil {
		return nil, err
	}

	var loaded Suite
	for _, valid := range suite.Valid {
		bytes, err := os.ReadFile(filepath.Join(directory, valid.File))
		if err != nil {
			return nil, err
		}
		table, err := valid.Table.table()
		if err != nil {
			return nil, fmt.Errorf("%s: %w", valid.Name, err)
		}

		loaded.Valid = append(loaded.Valid, ValidCase{Name: valid.Name, Bytes: bytes, Table: table})
	}

	if loaded.Equivalent, err = changedCases(directory, suite.Equivalent); err != nil {
		return nil, err
	}
	if loaded.Invalid, err = changedCases(directory, suite.Invalid); err != nil {
		return nil, err
	}

	return &loaded, nil
}

// TableOf returns the description of the valid case that an equivalent or
// invalid case names.
func (s *Suite) TableOf(name string) Table {
	for _, valid := range s.Valid {
		if valid.Name == name {
			return valid.Table
		}
	}

	panic("there's no valid case " + name)
}

type jsonChangedCase struct {
	Name  string `json:"name"`
	File  string `json:"file"`
	Case  string `json:"case"`
	Field int    `json:"field"`
}

func changedCases(directory string, cases []jsonChangedCase) ([]ChangedCase, error) {
	loaded := make([]ChangedCase, 0, len(cases))
	for _, changed := range cases {
		bytes, err := os.ReadFile(filepath.Join(directory, changed.File))
		if err != nil {
			return nil, err
		}

		loaded = append(loaded, ChangedCase{
			Name:  changed.Name,
			Bytes: bytes,
			Case:  changed.Case,
			Field: changed.Field,
		})
	}

	return loaded, nil
}

type jsonTable struct {
	Layout struct {
		Sized      bool   `json:"sized"`
		Versioned  bool   `json:"versioned"`
		OffsetType string `json:"offsetType"`
	} `json:"layout"`
	Version *uint32     `json:"version"`
	Fields  []jsonField `json:"fields"`
}

type jsonField struct {
	Type  string          `json:"type"`
	Value json.RawMessage `json:"value"`
	// A struct's scalar fields, or the scalar types of an array of structs
	Fields json.RawMessage `json:"fields"`
	// The values of an array of structs, or a vector's items
	Items     json.RawMessage `json:"items"`
	From      string          `json:"from"`
	Table     *jsonTable      `json:"table"`
	Alignment *int            `json:"alignment"`
}

type jsonNested struct {
	From  string    `json:"from"`
	Table jsonTable `json:"table"`
}

func (t *jsonTable) table() (Table, error) {
	offsetTypes := map[string]tafla.OffsetType{
		"uint8": tafla.Uint8, "uint16": tafla.Uint16, "uint32": tafla.Uint32,
	}
	offsetType, ok := offsetTypes[t.Layout.OffsetType]
	if !ok {
		return Table{}, fmt.Errorf("%q isn't an offset type", t.Layout.OffsetType)
	}

	table := Table{Layout: Layout{
		Sized:      t.Layout.Sized,
		Versioned:  t.Layout.Versioned,
		OffsetType: offsetType,
	}}
	if t.Version != nil {
		table.Version = *t.Version
	}

	for _, described := range t.Fields {
		field, err := described.field()
		if err != nil {
			return Table{}, err
		}
		table.Fields = append(table.Fields, field)
	}

	return table, nil
}

// field returns a field from the JSON that holds 64-bit integers as strings
// and the floats that JSON has no number for by name
func (f *jsonField) field() (Field, error) {
	if scalar, ok := scalarTypeNamed(f.Type); ok {
		return scalarOf(scalar, f.Value)
	}

	switch f.Type {
	case "bool":
		var value bool
		err := json.Unmarshal(f.Value, &value)
		return Bool(value), err
	case "int8[]":
		values, err := elements(f.Value, integer[int8])
		return Int8Array(values), err
	case "uint8[]":
		values, err := elements(f.Value, integer[uint8])
		return Uint8Array(values), err
	case "int16[]":
		values, err := elements(f.Value, integer[int16])
		return Int16Array(values), err
	case "uint16[]":
		values, err := elements(f.Value, integer[uint16])
		return Uint16Array(values), err
	case "int32[]":
		values, err := elements(f.Value, integer[int32])
		return Int32Array(values), err
	case "uint32[]":
		values, err := elements(f.Value, integer[uint32])
		return Uint32Array(values), err
	case "int64[]":
		values, err := elements(f.Value, parseInt64)
		return Int64Array(values), err
	case "uint64[]":
		values, err := elements(f.Value, parseUint64)
		return Uint64Array(values), err
	case "float32[]":
		values, err := elements(f.Value, float32Of)
		return Float32Array(values), err
	case "float64[]":
		values, err := elements(f.Value, float64Of)
		return Float64Array(values), err
	case "string":
		var value string
		err := json.Unmarshal(f.Value, &value)
		return String(value), err
	case "struct":
		var fields []jsonField
		if err := json.Unmarshal(f.Fields, &fields); err != nil {
			return nil, err
		}
		scalars := make(Struct, 0, len(fields))
		for _, scalar := range fields {
			field, err := scalar.field()
			if err != nil {
				return nil, err
			}
			scalars = append(scalars, field)
		}
		return scalars, nil
	case "struct[]":
		return f.structArray()
	case "table":
		nested, err := nestedOf(f.From, f.Table)
		if err != nil {
			return nil, err
		}
		field := TableField{Nested: nested}
		if f.Alignment != nil {
			field.Alignment = tafla.Alignment(*f.Alignment)
		}
		return field, nil
	case "vector":
		var items []jsonNested
		if err := json.Unmarshal(f.Items, &items); err != nil {
			return nil, err
		}
		vector := make(Vector, 0, len(items))
		for _, item := range items {
			nested, err := nestedOf(item.From, &item.Table)
			if err != nil {
				return nil, err
			}
			vector = append(vector, nested)
		}
		return vector, nil
	default:
		return nil, fmt.Errorf("unknown type %s", f.Type)
	}
}

func (f *jsonField) structArray() (Field, error) {
	var names []string
	if err := json.Unmarshal(f.Fields, &names); err != nil {
		return nil, err
	}
	types := make([]ScalarType, 0, len(names))
	for _, name := range names {
		scalar, ok := scalarTypeNamed(name)
		if !ok {
			return nil, fmt.Errorf("%s isn't a scalar type", name)
		}
		types = append(types, scalar)
	}

	var items [][]json.RawMessage
	if err := json.Unmarshal(f.Items, &items); err != nil {
		return nil, err
	}
	array := StructArray{Types: types, Items: make([][]Field, 0, len(items))}
	for _, values := range items {
		if len(values) != len(types) {
			return nil, fmt.Errorf("a struct of %d scalars has %d values", len(types), len(values))
		}
		scalars := make([]Field, 0, len(values))
		for index, value := range values {
			scalar, err := scalarOf(types[index], value)
			if err != nil {
				return nil, err
			}
			scalars = append(scalars, scalar)
		}
		array.Items = append(array.Items, scalars)
	}

	return array, nil
}

func nestedOf(from string, table *jsonTable) (Nested, error) {
	sources := map[string]Source{"builder": FromBuilder, "bytes": FromBytes}
	source, ok := sources[from]
	if !ok || table == nil {
		return Nested{}, fmt.Errorf("tables can't be nested from %q", from)
	}

	nested, err := table.table()
	return Nested{From: source, Table: nested}, err
}

// scalarTypeNamed returns the scalar type that a description names, or false
// for any other type
func scalarTypeNamed(name string) (ScalarType, bool) {
	names := map[string]ScalarType{
		"int8": Int8Type, "uint8": Uint8Type, "int16": Int16Type, "uint16": Uint16Type,
		"int32": Int32Type, "uint32": Uint32Type, "int64": Int64Type, "uint64": Uint64Type,
		"float32": Float32Type, "float64": Float64Type,
	}
	scalar, ok := names[name]
	return scalar, ok
}

func scalarOf(scalar ScalarType, value json.RawMessage) (Field, error) {
	switch scalar {
	case Int8Type:
		parsed, err := integer[int8](value)
		return Int8(parsed), err
	case Uint8Type:
		parsed, err := integer[uint8](value)
		return Uint8(parsed), err
	case Int16Type:
		parsed, err := integer[int16](value)
		return Int16(parsed), err
	case Uint16Type:
		parsed, err := integer[uint16](value)
		return Uint16(parsed), err
	case Int32Type:
		parsed, err := integer[int32](value)
		return Int32(parsed), err
	case Uint32Type:
		parsed, err := integer[uint32](value)
		return Uint32(parsed), err
	case Int64Type:
		parsed, err := parseInt64(value)
		return Int64(parsed), err
	case Uint64Type:
		parsed, err := parseUint64(value)
		return Uint64(parsed), err
	case Float32Type:
		parsed, err := float32Of(value)
		return Float32(parsed), err
	default:
		parsed, err := float64Of(value)
		return Float64(parsed), err
	}
}

func elements[T any](value json.RawMessage, element func(json.RawMessage) (T, error)) ([]T, error) {
	var raw []json.RawMessage
	if err := json.Unmarshal(value, &raw); err != nil {
		return nil, err
	}

	values := make([]T, 0, len(raw))
	for _, item := range raw {
		parsed, err := element(item)
		if err != nil {
			return nil, err
		}
		values = append(values, parsed)
	}

	return values, nil
}

// integer parses a JSON number into an integer type, which it must fit
func integer[T int8 | uint8 | int16 | uint16 | int32 | uint32](value json.RawMessage) (T, error) {
	var parsed T
	err := json.Unmarshal(value, &parsed)
	return parsed, err
}

// parseInt64 parses a 64-bit integer, which JSON holds as a string
func parseInt64(value json.RawMessage) (int64, error) {
	var digits string
	if err := json.Unmarshal(value, &digits); err != nil {
		return 0, err
	}

	return strconv.ParseInt(digits, 10, 64)
}

// parseUint64 parses a 64-bit unsigned integer, which JSON holds as a string
func parseUint64(value json.RawMessage) (uint64, error) {
	var digits string
	if err := json.Unmarshal(value, &digits); err != nil {
		return 0, err
	}

	return strconv.ParseUint(digits, 10, 64)
}

// ParseFloat64 parses a float64 as cases.json and shapes.json hold it: a
// number, or the name of a float that JSON has no number for.
func ParseFloat64(value json.RawMessage) (float64, error) {
	return float64Of(value)
}

// ParseFloat32 parses a float32 as the cases hold it: the float64 that
// equals it, or a name.
func ParseFloat32(value json.RawMessage) (float32, error) {
	return float32Of(value)
}

// ParseInt64 and ParseUint64 parse 64-bit integers, which the cases hold as
// decimal strings.
func ParseInt64(value json.RawMessage) (int64, error) {
	return parseInt64(value)
}

func ParseUint64(value json.RawMessage) (uint64, error) {
	return parseUint64(value)
}

// float64Of parses a JSON number, which strconv parses exactly, or the name
// of a float that JSON has no number for
func float64Of(value json.RawMessage) (float64, error) {
	var name string
	if json.Unmarshal(value, &name) != nil {
		var number float64
		err := json.Unmarshal(value, &number)
		return number, err
	}

	switch name {
	case "NaN":
		return math.Float64frombits(QuietNaN64), nil
	case "Infinity":
		return math.Inf(1), nil
	case "-Infinity":
		return math.Inf(-1), nil
	case "-0":
		return math.Copysign(0, -1), nil
	default:
		return 0, fmt.Errorf("not a number: %s", name)
	}
}

// QuietNaN32 and QuietNaN64 are the quiet NaN that the cases' "NaN" stands
// for, which JavaScript writes. Go's math.NaN has another payload, and its
// conversion of a float64 NaN to a float32 is unspecified, so they're set by
// their bits.
const (
	QuietNaN32 = 0x7fc0_0000
	QuietNaN64 = 0x7ff8_0000_0000_0000
)

// float32Of parses a float32, which JSON holds as the float64 that equals it,
// or by name
func float32Of(value json.RawMessage) (float32, error) {
	float, err := float64Of(value)
	if err != nil {
		return 0, err
	}

	if math.IsNaN(float) {
		return math.Float32frombits(QuietNaN32), nil
	}

	narrowed := float32(float)
	if math.Float64bits(float64(narrowed)) != math.Float64bits(float) {
		return 0, fmt.Errorf("%v isn't a float32", float)
	}

	return narrowed, nil
}
