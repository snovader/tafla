package tafla_test

import (
	"errors"
	"fmt"

	tafla "github.com/snovader/tafla/go"
)

// A format's header, shared by its writer and its readers
var layout = tafla.TableLayout{}.Sized().Versioned()

const formatVersion = 1

func Example() {
	timestamps := []uint32{0, 86_400}
	values := []float32{21.75, 18.5}

	series, err := tafla.NewTableBuilder(2, tafla.TableLayout{})
	if err != nil {
		panic(err)
	}
	_ = series.SetUint32Array(timestamps) // kept until the table is written
	_ = series.SetFloat32Array(values)

	builder, err := tafla.NewVersionedTableBuilder(3, layout, formatVersion)
	if err != nil {
		panic(err)
	}
	_ = builder.SetFloat64(59.3293)
	_ = builder.SetString("Europe/Stockholm")
	_ = builder.SetTable(series) // written straight into the table
	bytes, err := builder.Bytes()
	if err != nil {
		panic(err)
	}

	read, err := readSeries(bytes)
	fmt.Println(read, err)
	// Output: 59.3293 Europe/Stockholm [21.75 18.5] <nil>
}

func readSeries(bytes []byte) (string, error) {
	buffer := tafla.NewByteBuffer(bytes, layout)
	version, err := buffer.Version()
	if err != nil {
		return "", err
	}
	if version != formatVersion {
		return "", tafla.NewUnsupportedVersionError(version, formatVersion)
	}

	position, err := buffer.RootOffset(0)
	if err != nil {
		return "", err
	}
	latitude, err := buffer.GetFloat64(position)
	if err != nil {
		return "", err
	}

	position, err = buffer.RootOffset(1)
	if err != nil {
		return "", err
	}
	timeZone, err := buffer.GetString(position)
	if err != nil {
		return "", err
	}

	table, err := buffer.RootOffset(2)
	if err != nil {
		return "", err
	}
	position, err = buffer.FieldOffset(table, 1)
	if err != nil {
		return "", err
	}
	values, err := buffer.GetFloat32Array(position)
	if err != nil {
		return "", err
	}

	return fmt.Sprint(latitude, " ", timeZone, " ", values.Slice()), nil
}

func ExampleDecodeError() {
	buffer := tafla.NewByteBuffer([]byte{4, 0, 0, 0}, tafla.TableLayout{})
	_, err := buffer.GetFloat64(0)

	var decodeError *tafla.DecodeError
	if errors.As(err, &decodeError) {
		fmt.Println(decodeError.Kind, err)
	}
	// Output: OutOfRange tafla: read exceeds the supplied bytes: 8 bytes at 0 of 4
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
	buffer := tafla.NewByteBuffer(bytes, tafla.TableLayout{})
	position, err := buffer.RootOffset(0)
	if err != nil {
		return 0, err
	}
	vector, err := buffer.GetVector(position)
	if err != nil {
		return 0, err
	}
	item, err := vector.GetInPlace(index)
	if err != nil {
		return 0, err
	}

	// In place, at its start, in a buffer of its parent's offset type
	point := tafla.NewByteBuffer(item, tafla.TableLayout{})
	position, err = point.FieldOffset(0, 1)
	if err != nil {
		return 0, err
	}
	return point.GetFloat32(position)
}
