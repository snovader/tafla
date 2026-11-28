package generated_test

// The enums of the example schemas: names for some of the integers of a
// type, which the generated code declares as a type of that integer type and
// a constant of it for each name, so that a field of an enum holds any
// integer of its type, named or not. The tests of every message and struct
// draw Enums, Task and their structs with the others, with integers across
// each type's range; these are of what only enums have.

import (
	"bytes"
	"math"
	"reflect"
	"testing"
	"unsafe"

	tafla "github.com/snovader/tafla/go"
	alltypes "github.com/snovader/tafla/go/tests/generated/all_types"
	"github.com/snovader/tafla/go/tests/generated/shapes"
	"pgregory.net/rapid"
)

// integer is any type of an integer type that stores an enum
type integer interface {
	~int8 | ~uint8 | ~int16 | ~uint16 | ~int32 | ~uint32 | ~int64 | ~uint64
}

func TestEveryEnumIsATypeOfItsIntegerType(t *testing.T) {
	enums := []struct {
		enum reflect.Type
		kind reflect.Kind
	}{
		{reflect.TypeFor[alltypes.Level](), reflect.Uint8},
		{reflect.TypeFor[alltypes.Direction](), reflect.Int8},
		{reflect.TypeFor[alltypes.Unit](), reflect.Uint16},
		{reflect.TypeFor[alltypes.Change](), reflect.Int16},
		{reflect.TypeFor[alltypes.Status](), reflect.Uint32},
		{reflect.TypeFor[alltypes.Sign](), reflect.Int32},
		{reflect.TypeFor[alltypes.Mask](), reflect.Uint64},
		{reflect.TypeFor[alltypes.Extent](), reflect.Int64},
		{reflect.TypeFor[alltypes.Word](), reflect.Uint8},
		{reflect.TypeFor[shapes.Priority](), reflect.Int8},
		{reflect.TypeFor[shapes.State](), reflect.Uint16},
	}

	for _, enum := range enums {
		if enum.enum.Kind() != enum.kind {
			t.Errorf("%v is of an %v rather than an %v", enum.enum, enum.enum.Kind(), enum.kind)
		}
	}
}

// Each value is a constant of its enum's type, named after the enum and the
// value, so a value named after a keyword, such as `type` of Word, needs no
// other name. An interface holds a constant with its type, so each equals
// its integer only as its enum.
func TestEveryValueOfAnEnumIsAConstantOfItsTypeWithTheIntegerOfTheSchema(t *testing.T) {
	values := []struct {
		name     string
		constant any
		expected any
	}{
		{"LevelLow", alltypes.LevelLow, alltypes.Level(0)},
		{"LevelMedium", alltypes.LevelMedium, alltypes.Level(1)},
		{"LevelHigh", alltypes.LevelHigh, alltypes.Level(2)},

		{"DirectionBackward", alltypes.DirectionBackward, alltypes.Direction(-1)},
		{"DirectionNone", alltypes.DirectionNone, alltypes.Direction(0)},
		{"DirectionForward", alltypes.DirectionForward, alltypes.Direction(1)},
		{"DirectionFurthestBack", alltypes.DirectionFurthestBack, alltypes.Direction(math.MinInt8)},

		{"UnitLast", alltypes.UnitLast, alltypes.Unit(math.MaxUint16)},
		{"UnitFirst", alltypes.UnitFirst, alltypes.Unit(0)},
		{"UnitV2", alltypes.UnitV2, alltypes.Unit(2)},

		{"ChangeLeast", alltypes.ChangeLeast, alltypes.Change(math.MinInt16)},
		{"ChangeGreatest", alltypes.ChangeGreatest, alltypes.Change(math.MaxInt16)},

		{"StatusOk", alltypes.StatusOk, alltypes.Status(0)},
		{"StatusNotFound", alltypes.StatusNotFound, alltypes.Status(404)},
		{"StatusGreatest", alltypes.StatusGreatest, alltypes.Status(math.MaxUint32)},

		{"SignLeast", alltypes.SignLeast, alltypes.Sign(math.MinInt32)},
		{"SignGreatest", alltypes.SignGreatest, alltypes.Sign(math.MaxInt32)},

		{"MaskNone", alltypes.MaskNone, alltypes.Mask(0)},
		{"MaskOne", alltypes.MaskOne, alltypes.Mask(1)},
		{"MaskBeyondFloat64", alltypes.MaskBeyondFloat64, alltypes.Mask(1<<53 + 1)},
		{"MaskAll", alltypes.MaskAll, alltypes.Mask(math.MaxUint64)},

		{"ExtentLeast", alltypes.ExtentLeast, alltypes.Extent(math.MinInt64)},
		{"ExtentZero", alltypes.ExtentZero, alltypes.Extent(0)},
		{"ExtentGreatest", alltypes.ExtentGreatest, alltypes.Extent(math.MaxInt64)},

		{"WordClass", alltypes.WordClass, alltypes.Word(0)},
		{"WordDefault", alltypes.WordDefault, alltypes.Word(1)},
		{"WordIn", alltypes.WordIn, alltypes.Word(2)},
		{"WordMatch", alltypes.WordMatch, alltypes.Word(3)},
		{"WordSelf", alltypes.WordSelf, alltypes.Word(4)},
		{"WordType", alltypes.WordType, alltypes.Word(5)},
		{"WordTrue", alltypes.WordTrue, alltypes.Word(6)},
		{"WordNull", alltypes.WordNull, alltypes.Word(7)},
		{"WordNone", alltypes.WordNone, alltypes.Word(8)},
		{"WordInit", alltypes.WordInit, alltypes.Word(9)},
		{"WordNew", alltypes.WordNew, alltypes.Word(10)},
		{"WordName", alltypes.WordName, alltypes.Word(11)},
		{"WordValue", alltypes.WordValue, alltypes.Word(12)},

		{"PriorityLow", shapes.PriorityLow, shapes.Priority(-1)},
		{"PriorityNormal", shapes.PriorityNormal, shapes.Priority(0)},
		{"PriorityHigh", shapes.PriorityHigh, shapes.Priority(1)},

		{"StateWaiting", shapes.StateWaiting, shapes.State(0)},
		{"StateRunning", shapes.StateRunning, shapes.State(1)},
		{"StateDone", shapes.StateDone, shapes.State(2)},
		{"StateFailed", shapes.StateFailed, shapes.State(1000)},
	}

	for _, value := range values {
		if value.constant != value.expected {
			t.Errorf("%s is the %T %d rather than the %T %d",
				value.name, value.constant, value.constant, value.expected, value.expected)
		}
	}
}

// The fields of Enums, by the values that the schema names and by integers
// that it gives no name
func enumsFields() alltypes.EnumsFields {
	mark := namedMark()
	unnamed := alltypes.Mark{
		Level:  200,
		Change: -2,
		Weight: float32(math.Copysign(0, -1)),
		Extent: -3,
		Turn:   alltypes.Turn{Direction: 127, Mask: 1 << 63},
	}

	return alltypes.EnumsFields{
		Level:      alltypes.LevelMedium,
		Levels:     []byte{byte(alltypes.LevelHigh), 3, math.MaxUint8},
		Direction:  -2,
		Directions: []int8{int8(alltypes.DirectionFurthestBack), math.MaxInt8},
		Unit:       alltypes.UnitLast,
		Units:      []uint16{uint16(alltypes.UnitV2), 1},
		Change:     1,
		Changes:    []int16{int16(alltypes.ChangeLeast), 0},
		Status:     alltypes.StatusNotFound,
		Statuses:   []uint32{uint32(alltypes.StatusGreatest), 500},
		Sign:       -1,
		Signs:      []int32{int32(alltypes.SignGreatest), 0},
		Mask:       alltypes.MaskAll,
		Masks:      []uint64{uint64(alltypes.MaskBeyondFloat64), 2},
		Extent:     math.MaxInt64 - 1,
		Extents:    []int64{int64(alltypes.ExtentLeast), 1},
		Word:       alltypes.WordType,
		Mark:       mark,
		Marks:      []alltypes.Mark{mark, unnamed},
		Steps: []alltypes.StepFields{
			{Level: alltypes.LevelHigh, Mark: mark, Name: "named"},
			{Level: math.MaxUint8, Mark: unnamed},
		},
	}
}

// A Mark of values that the schema names, and of a nested Turn of them
func namedMark() alltypes.Mark {
	return alltypes.Mark{
		Level:  alltypes.LevelHigh,
		Change: alltypes.ChangeLeast,
		Weight: 0.5,
		Extent: alltypes.ExtentGreatest,
		Turn:   alltypes.Turn{Direction: alltypes.DirectionBackward, Mask: alltypes.MaskBeyondFloat64},
	}
}

// Each field is assigned what its getter returns, which wouldn't compile if
// the getter of an enum returned its integer type, or the getter of an array
// of enums a view of anything else
func TestEveryGetterOfAnEnumReturnsItsTypeNamedOrNot(t *testing.T) {
	fields := enumsFields()
	enums := alltypes.NewEnums(must(fields.MarshalBinary()))

	read := alltypes.EnumsFields{
		Level:      must(enums.Level()),
		Levels:     must(enums.Levels()),
		Direction:  must(enums.Direction()),
		Directions: must(enums.Directions()).Slice(),
		Unit:       must(enums.Unit()),
		Units:      must(enums.Units()).Slice(),
		Change:     must(enums.Change()),
		Changes:    must(enums.Changes()).Slice(),
		Status:     must(enums.Status()),
		Statuses:   must(enums.Statuses()).Slice(),
		Sign:       must(enums.Sign()),
		Signs:      must(enums.Signs()).Slice(),
		Mask:       must(enums.Mask()),
		Masks:      must(enums.Masks()).Slice(),
		Extent:     must(enums.Extent()),
		Extents:    must(enums.Extents()).Slice(),
		Word:       must(enums.Word()),
		Mark:       must(enums.Mark()),
		Marks:      must(enums.Marks()).Slice(),
	}
	for step, err := range must(enums.Steps()).All() {
		if err != nil {
			t.Fatal(err)
		}
		read.Steps = append(read.Steps, alltypes.StepFields{
			Level: must(step.Level()),
			Mark:  must(step.Mark()),
			Name:  must(step.Name()),
		})
	}

	if !sameValues(reflect.ValueOf(read), reflect.ValueOf(fields)) {
		t.Errorf("read %+v\nrather than %+v", read, fields)
	}
	if whole := must(enums.Fields()); !sameValues(reflect.ValueOf(whole), reflect.ValueOf(fields)) {
		t.Errorf("Fields() read %+v\nrather than %+v", whole, fields)
	}
}

func TestAStructOfEnumsIsWrittenAsTheIntegersOfItsValues(t *testing.T) {
	written := must(namedMark().MarshalBinary())

	expected := []byte{
		2,          // high
		0x00, 0x80, // least, -32768
		0x00, 0x00, 0x00, 0x3f, // 0.5
		0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f, // greatest
		0xff,                                           // backward, -1
		0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, // beyond_float64
	}
	if !bytes.Equal(written, expected) {
		t.Fatalf("wrote % x\nrather than % x", written, expected)
	}

	if read := must(alltypes.ReadMark(written)); read != namedMark() {
		t.Fatalf("read %+v", read)
	}
}

// Enums are open: whatever bytes a field of an enum holds, it reads as the
// integer that they hold, of the enum's type, whether the schema names it or
// not, as a field of the integer type would
func TestAnyBytesOfAFieldOfAnEnumReadAsTheirInteger(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		fields := drawFields(t, reflect.TypeFor[alltypes.EnumsFields](), 4)
		written, err := write(fields)
		if err != nil {
			t.Fatal(err)
		}

		checkOverwrittenEnum(t, written, 0, alltypes.Enums.Level)
		checkOverwrittenEnum(t, written, 2, alltypes.Enums.Direction)
		checkOverwrittenEnum(t, written, 4, alltypes.Enums.Unit)
		checkOverwrittenEnum(t, written, 6, alltypes.Enums.Change)
		checkOverwrittenEnum(t, written, 8, alltypes.Enums.Status)
		checkOverwrittenEnum(t, written, 10, alltypes.Enums.Sign)
		checkOverwrittenEnum(t, written, 12, alltypes.Enums.Mask)
		checkOverwrittenEnum(t, written, 14, alltypes.Enums.Extent)
		checkOverwrittenEnum(t, written, 16, alltypes.Enums.Word)

		// Fields() reads the same integers, and every other field reads as
		// it was written
		reader := reflect.ValueOf(alltypes.NewEnums(written))
		read, err := readFields(reader, fields.Type())
		if err != nil {
			t.Fatal(err)
		}
		if err := checkWhole(reader, read, nil); err != nil {
			t.Fatal(err)
		}
		for index := range fields.NumField() {
			// Each enum is followed by an array of it, up to the Word
			overwritten := index <= 16 && index%2 == 0
			if !overwritten && !sameValues(read.Field(index), fields.Field(index)) {
				t.Fatalf("field %d read %+v rather than %+v", index, read.Field(index), fields.Field(index))
			}
		}
	})
}

// checkOverwrittenEnum overwrites field index of written, an Enums, an enum
// of type E that get reads, with any bytes, and fails unless get reads the
// integer that they hold, little-endian
func checkOverwrittenEnum[E integer](t *rapid.T, written []byte, index int, get func(alltypes.Enums) (E, error)) {
	position := must(tafla.NewTableReader(written, tafla.TableLayout{}).RootOffset(index))

	var bits uint64
	var expected E
	for offset := range int(unsafe.Sizeof(expected)) {
		drawn := rapid.Byte().Draw(t, "byte")
		written[position+offset] = drawn
		bits |= uint64(drawn) << (8 * offset)
	}
	// The integer's low bytes, as a conversion between integer types keeps
	expected = E(bits)

	read, err := get(alltypes.NewEnums(written))
	if err != nil || read != expected {
		t.Fatalf("field %d read %d, %v rather than %d", index, read, err, expected)
	}
}
