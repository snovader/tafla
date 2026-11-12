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
