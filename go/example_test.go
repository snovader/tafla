package tafla_test

import (
	"errors"
	"fmt"

	tafla "github.com/snovader/tafla/go"
)

// A format's header, shared by its writer and its readers
const formatVersion = 1

var layout = tafla.TableLayout{}.Sized().WithVersion(formatVersion)

func Example() {
	timestamps := []uint32{0, 86_400}
	values := []float32{21.75, 18.5}

	series, err := tafla.NewTableBuilder(2, tafla.TableLayout{})
	if err != nil {
		panic(err)
	}
	_ = series.SetUint32Array(timestamps) // copied in
	_ = series.SetFloat32Array(values)

	builder, err := tafla.NewTableBuilder(3, layout)
	if err != nil {
		panic(err)
	}
	_ = builder.SetFloat64(59.3293)
	_ = builder.SetString("Europe/Stockholm")
	_ = builder.SetTable(series) // its bytes copied in
	bytes, err := builder.Bytes()
	if err != nil {
		panic(err)
	}

	read, err := readSeries(bytes)
	fmt.Println(read, err)
	// Output: 59.3293 Europe/Stockholm [21.75 18.5] <nil>
}

func readSeries(bytes []byte) (string, error) {
	reader := tafla.NewTableReader(bytes, layout)
	version, err := reader.Version()
	if err != nil {
		return "", err
	}
	if version != formatVersion {
		return "", tafla.NewUnsupportedVersionError(version, formatVersion)
	}

	position, err := reader.RootOffset(0)
	if err != nil {
		return "", err
	}
	latitude, err := reader.GetFloat64(position)
	if err != nil {
		return "", err
	}

	position, err = reader.RootOffset(1)
	if err != nil {
		return "", err
	}
	timeZone, err := reader.GetString(position)
	if err != nil {
		return "", err
	}

	table, err := reader.RootOffset(2)
	if err != nil {
		return "", err
	}
	position, err = reader.FieldOffset(table, 1)
	if err != nil {
		return "", err
	}
	values, err := reader.GetFloat32Array(position)
	if err != nil {
		return "", err
	}

	return fmt.Sprint(latitude, " ", timeZone, " ", values.Slice()), nil
}

func ExampleDecodeError() {
	reader := tafla.NewTableReader([]byte{4, 0, 0, 0}, tafla.TableLayout{})
	_, err := reader.GetFloat64(0)

	var decodeError *tafla.DecodeError
	if errors.As(err, &decodeError) {
		fmt.Println(decodeError.Kind, err)
	}
	// Output: OutOfRange tafla: read exceeds the supplied bytes: 8 bytes at 0 of 4
}

func ExampleTableReader_GetBoolArray() {
	builder, err := tafla.NewTableBuilder(1, tafla.TableLayout{})
	if err != nil {
		panic(err)
	}
	_ = builder.SetBoolArray([]bool{true, false, true})
	bytes, err := builder.Bytes()
	if err != nil {
		panic(err)
	}

	reader := tafla.NewTableReader(bytes, tafla.TableLayout{})
	position, err := reader.RootOffset(0)
	if err != nil {
		panic(err)
	}
	fmt.Println(reader.GetUint8Array(position)) // the bytes in place
	fmt.Println(reader.GetBoolArray(position))  // new bools, true where a byte isn't 0
	// Output:
	// [1 0 1] <nil>
	// [true false true] <nil>
}

// A loop that hands each table's bytes back to Reset writes every table
// into the same bytes, which it allocates once.
func ExampleTableBuilder_Reset() {
	readings := []struct {
		sensor string
		value  float64
	}{{"kitchen", 21.75}, {"cellar", 12.5}}

	var builder tafla.TableBuilder
	var table []byte
	for _, reading := range readings {
		err := builder.Reset(table[:0], 2, tafla.TableLayout{})
		if err == nil {
			err = errors.Join(builder.SetString(reading.sensor), builder.SetFloat64(reading.value))
		}
		if err == nil {
			table, err = builder.Bytes()
		}
		if err != nil {
			panic(err)
		}

		// Read, or written out, before the next Reset writes over it
		reader := tafla.NewTableReader(table, tafla.TableLayout{})
		sensor, _ := reader.GetString(must(reader.RootOffset(0)))
		value, _ := reader.GetFloat64(must(reader.RootOffset(1)))
		fmt.Println(sensor, value, len(table))
	}
	// Output:
	// kitchen 21.75 27
	// cellar 12.5 26
}

func must[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}
	return value
}

func ExampleTableBuilder_SetVectorFunc() {
	type point struct{ x, y float32 }
	points := []point{{1.5, 2}, {3, 4.25}}

	builder, err := tafla.NewTableBuilder(1, tafla.TableLayout{})
	if err != nil {
		panic(err)
	}
	// Each point is a table of two float32s, recorded among the builder's
	// own fields rather than from a builder of its own
	err = builder.SetVectorFunc(len(points), 2, func(index int, item tafla.Item) error {
		if err := item.SetFloat32(points[index].x); err != nil {
			return err
		}
		return item.SetFloat32(points[index].y)
	})
	if err != nil {
		panic(err)
	}
	bytes, err := builder.Bytes()
	if err != nil {
		panic(err)
	}

	y, err := readY(bytes, 1)
	fmt.Println(y, err)
	// Output: 4.25 <nil>
}

// readY reads the y of point index of the vector
func readY(bytes []byte, index int) (float32, error) {
	reader := tafla.NewTableReader(bytes, tafla.TableLayout{})
	position, err := reader.RootOffset(0)
	if err != nil {
		return 0, err
	}
	vector, err := reader.GetVector(position)
	if err != nil {
		return 0, err
	}
	item, err := vector.GetInPlace(index)
	if err != nil {
		return 0, err
	}

	// In place, at its start, with a reader of its parent's offset type
	point := tafla.NewTableReader(item, tafla.TableLayout{})
	position, err = point.FieldOffset(0, 1)
	if err != nil {
		return 0, err
	}
	return point.GetFloat32(position)
}
