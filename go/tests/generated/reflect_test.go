package generated_test

// The tests read and write the code generated from every example schema in
// the same ways, through reflection over its readers and the structs of
// their fields:
//
//   - readFields reads every field of a reader through its getters, into
//     the struct of fields that writes it, and the items of the views that
//     the getters of its vectors and arrays of structs return by index and
//     by iterating,
//   - checkWhole checks that a reader's Fields() reads what its getters read,
//   - describe describes a struct of fields as the conformance suite's
//     description of the table that it writes, which TableBuilder builds,
//   - drawFields draws a struct of fields of any values, and
//   - sameValues compares structs of fields, with floats by their bits.

import (
	"errors"
	"fmt"
	"math"
	"reflect"
	"strings"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/tests/internal/tables"
	"pgregory.net/rapid"
)

// isStruct reports whether a type is a struct that the generated code reads
// and writes whole, rather than the fields of a table: one that unmarshals
// itself
func isStruct(t reflect.Type) bool {
	_, ok := reflect.PointerTo(t).MethodByName("UnmarshalBinary")
	return t.Kind() == reflect.Struct && ok
}

// isFields reports whether a type is the struct of a table's fields
func isFields(t reflect.Type) bool {
	return t.Kind() == reflect.Struct && !isStruct(t) && strings.HasSuffix(t.Name(), "Fields")
}

// readFields reads every field of reader, a message's or table's reader, into
// a struct of fieldsType, through the getter of each field. The getter of a
// vector or an array of structs returns a view, whose items it reads by index
// and by iterating, and fails unless they're the same.
func readFields(reader reflect.Value, fieldsType reflect.Type) (reflect.Value, error) {
	fields := reflect.New(fieldsType).Elem()

	for index := range fieldsType.NumField() {
		field := fieldsType.Field(index)
		getter := reader.MethodByName(field.Name)
		if !getter.IsValid() {
			return fields, fmt.Errorf("%v has no getter %s", reader.Type(), field.Name)
		}

		results := getter.Call(nil)
		if err, _ := results[1].Interface().(error); err != nil {
			return fields, err
		}

		value, err := readValue(results[0], field.Type)
		if err != nil {
			return fields, err
		}
		fields.Field(index).Set(value)
	}

	return fields, nil
}

// readValue converts what a getter read into the type of the field that
// writes it: the elements of an ArrayView, the fields of a table's reader,
// and the items of a view
func readValue(read reflect.Value, fieldType reflect.Type) (reflect.Value, error) {
	switch {
	case read.Type() == fieldType:
		return read, nil
	case read.Kind() == reflect.Struct && strings.HasPrefix(read.Type().Name(), "ArrayView["):
		return read.MethodByName("Slice").Call(nil)[0], nil
	case isFields(fieldType):
		return readFields(read, fieldType)
	case isView(read.Type()) && fieldType.Kind() == reflect.Slice:
		return readView(read, fieldType)
	default:
		return read, fmt.Errorf("can't read a %v into a %v", read.Type(), fieldType)
	}
}

// isView reports whether a type is a view of a vector or an array of
// structs, which reads each item when it's asked for
func isView(t reflect.Type) bool {
	for _, method := range []string{"Len", "Get", "All"} {
		if _, ok := t.MethodByName(method); !ok {
			return false
		}
	}

	return true
}

// readView reads the items of a view of a vector or an array of structs, by
// index and by iterating, into a slice of fieldType, and fails unless both
// read the same items
func readView(view reflect.Value, fieldType reflect.Type) (reflect.Value, error) {
	length := int(view.MethodByName("Len").Call(nil)[0].Int())

	byIndex := reflect.MakeSlice(fieldType, length, length)
	for index := range length {
		results := view.MethodByName("Get").Call([]reflect.Value{reflect.ValueOf(index)})
		if len(results) == 2 {
			if err, _ := results[1].Interface().(error); err != nil {
				return byIndex, err
			}
		}

		value, err := readValue(results[0], fieldType.Elem())
		if err != nil {
			return byIndex, err
		}
		byIndex.Index(index).Set(value)
	}

	iterated := reflect.MakeSlice(fieldType, 0, length)
	all := view.MethodByName("All").Call(nil)[0]
	for key, item := range all.Seq2() {
		// An array of structs iterates over indices and structs, and a vector
		// over its items and errors
		if key.Kind() == reflect.Int {
			key, item = item, key
		} else if err, _ := item.Interface().(error); err != nil {
			return byIndex, err
		}

		value, err := readValue(key, fieldType.Elem())
		if err != nil {
			return byIndex, err
		}
		iterated = reflect.Append(iterated, value)
	}

	if !sameValues(iterated, byIndex) {
		return byIndex, errors.New("a view iterates over other items than it reads by index")
	}

	return byIndex, nil
}

// checkWhole reads reader whole, through its Fields(), when it has one, and
// fails unless Fields() reads the fields that its getters read, which are
// fields, or fails with a decode error when they fail with err. It reads a
// table that can hold itself through its getters only, and has no Fields().
func checkWhole(reader reflect.Value, fields reflect.Value, err error) error {
	method := reader.MethodByName("Fields")
	if !method.IsValid() {
		return nil
	}

	results := method.Call(nil)
	whole := results[0]
	wholeErr, _ := results[1].Interface().(error)

	var decodeError *tafla.DecodeError
	switch {
	case err == nil && wholeErr == nil:
		if !sameValues(whole, fields) {
			return fmt.Errorf("Fields() read %+v rather than %+v", whole, fields)
		}
	case err == nil:
		return fmt.Errorf("Fields() failed with %v where the getters read %+v", wholeErr, fields)
	case !errors.As(wholeErr, &decodeError):
		return fmt.Errorf("Fields() read %+v, %v where the getters failed with %v", whole, wholeErr, err)
	}

	return nil
}

// sameValues reports whether two values are the same, with floats compared
// by their bits, so that NaN equals itself and -0 doesn't equal 0, and nil
// and empty slices alike
func sameValues(a, b reflect.Value) bool {
	switch a.Kind() {
	case reflect.Float32:
		// Converting a float32 to a float64 may quiet a signaling NaN
		return math.Float32bits(a.Interface().(float32)) == math.Float32bits(b.Interface().(float32))
	case reflect.Float64:
		return math.Float64bits(a.Float()) == math.Float64bits(b.Float())
	case reflect.Struct:
		for index := range a.NumField() {
			if !sameValues(a.Field(index), b.Field(index)) {
				return false
			}
		}
		return true
	case reflect.Slice:
		if a.Len() != b.Len() {
			return false
		}
		for index := range a.Len() {
			if !sameValues(a.Index(index), b.Index(index)) {
				return false
			}
		}
		return true
	default:
		return a.Equal(b)
	}
}

// describe describes a struct of fields as the conformance suite's
// description of the table that the generated code writes from it, in
// layout and with version
func describe(fields reflect.Value, layout tables.Layout, version uint32) tables.Table {
	table := tables.Table{Layout: layout, Version: version}
	for index := range fields.NumField() {
		table.Fields = append(table.Fields, describeField(fields.Field(index)))
	}

	return table
}

// describeField describes a field by its Go type, as the generated code
// writes it: nested tables from their builders, with no header and the
// default offset type, at their default alignment
func describeField(value reflect.Value) tables.Field {
	nested := func(fields reflect.Value) tables.Nested {
		return tables.Nested{From: tables.FromBuilder, Table: describe(fields, tables.Headerless(tafla.Uint32), 0)}
	}

	switch value.Kind() {
	case reflect.Bool:
		return tables.Bool(value.Bool())
	case reflect.String:
		return tables.String(value.String())
	case reflect.Slice:
		element := value.Type().Elem()
		switch {
		case isFields(element):
			vector := tables.Vector{}
			for index := range value.Len() {
				vector = append(vector, nested(value.Index(index)))
			}
			return vector
		case isStruct(element):
			array := tables.StructArray{Types: structTypes(element), Items: [][]tables.Field{}}
			for index := range value.Len() {
				array.Items = append(array.Items, structScalars(value.Index(index)))
			}
			return array
		default:
			return describeArray(value)
		}
	case reflect.Struct:
		if isFields(value.Type()) {
			return tables.TableField{Nested: nested(value)}
		}
		return tables.Struct(structScalars(value))
	default:
		return describeScalar(value)
	}
}

func describeScalar(value reflect.Value) tables.Field {
	switch value.Kind() {
	case reflect.Bool:
		// A bool in a struct is its uint8 to the runtimes
		if value.Bool() {
			return tables.Uint8(1)
		}
		return tables.Uint8(0)
	case reflect.Int8:
		return tables.Int8(value.Int())
	case reflect.Uint8:
		return tables.Uint8(value.Uint())
	case reflect.Int16:
		return tables.Int16(value.Int())
	case reflect.Uint16:
		return tables.Uint16(value.Uint())
	case reflect.Int32:
		return tables.Int32(value.Int())
	case reflect.Uint32:
		return tables.Uint32(value.Uint())
	case reflect.Int64:
		return tables.Int64(value.Int())
	case reflect.Uint64:
		return tables.Uint64(value.Uint())
	case reflect.Float32:
		// As a float32, since converting it to a float64 may quiet a
		// signaling NaN
		return tables.Float32(value.Interface().(float32))
	default:
		return tables.Float64(value.Float())
	}
}

func describeArray(value reflect.Value) tables.Field {
	switch values := value.Interface().(type) {
	case []int8:
		return tables.Int8Array(values)
	case []uint8:
		return tables.Uint8Array(values)
	case []int16:
		return tables.Int16Array(values)
	case []uint16:
		return tables.Uint16Array(values)
	case []int32:
		return tables.Int32Array(values)
	case []uint32:
		return tables.Uint32Array(values)
	case []int64:
		return tables.Int64Array(values)
	case []uint64:
		return tables.Uint64Array(values)
	case []float32:
		return tables.Float32Array(values)
	default:
		return tables.Float64Array(value.Interface().([]float64))
	}
}

// structScalars returns the scalars of a struct, those of the structs in it
// in their place
func structScalars(value reflect.Value) []tables.Field {
	var scalars []tables.Field
	for index := range value.NumField() {
		field := value.Field(index)
		if field.Kind() == reflect.Struct {
			scalars = append(scalars, structScalars(field)...)
		} else {
			scalars = append(scalars, describeScalar(field))
		}
	}

	return scalars
}

// structTypes returns the types of the scalars of a struct type
func structTypes(t reflect.Type) []tables.ScalarType {
	return tables.StructTypes(structScalars(reflect.New(t).Elem()))
}

// drawFields draws a value of a struct of fields, or of anything in it, with
// tables nested at most depth deep, which ends a table that holds a vector
// of itself
func drawFields(t *rapid.T, fieldsType reflect.Type, depth int) reflect.Value {
	value := reflect.New(fieldsType).Elem()

	switch fieldsType.Kind() {
	case reflect.Struct:
		for index := range fieldsType.NumField() {
			value.Field(index).Set(drawFields(t, fieldsType.Field(index).Type, depth-1))
		}
	case reflect.Slice:
		most := 6
		if isFields(fieldsType.Elem()) {
			most = min(max(depth, 0), 3)
		}
		count := rapid.IntRange(0, most).Draw(t, "count")
		value.Set(reflect.MakeSlice(fieldsType, count, count))
		for index := range count {
			value.Index(index).Set(drawFields(t, fieldsType.Elem(), depth-1))
		}
	case reflect.String:
		value.SetString(tables.DrawString(t, 8))
	case reflect.Bool:
		value.SetBool(rapid.Bool().Draw(t, "bool"))
	default:
		scalar := map[reflect.Kind]tables.ScalarType{
			reflect.Int8: tables.Int8Type, reflect.Uint8: tables.Uint8Type,
			reflect.Int16: tables.Int16Type, reflect.Uint16: tables.Uint16Type,
			reflect.Int32: tables.Int32Type, reflect.Uint32: tables.Uint32Type,
			reflect.Int64: tables.Int64Type, reflect.Uint64: tables.Uint64Type,
			reflect.Float32: tables.Float32Type, reflect.Float64: tables.Float64Type,
		}[fieldsType.Kind()]
		drawn := tables.DrawScalar(t, scalar)
		if float, ok := drawn.(tables.Float32); ok {
			// Set as a float32, since converting it to a float64 may quiet a
			// signaling NaN
			value.Set(reflect.ValueOf(float32(float)))
		} else {
			value.Set(reflect.ValueOf(drawn).Convert(fieldsType))
		}
	}

	return value
}
