// Package itemaccess reads the items of tafla's vector of nested tables and
// of its array of structs, which tafla and tafla-gen write alike, in several
// ways.
//
// Each way reads the last item alone, which a reader can find without
// reading the others, and every item. It sums the three float32s of the items
// it reads as float64s, as the TypeScript comparison adds them, so that it
// returns a number rather than collecting items of its own.
package itemaccess

import (
	"encoding/binary"
	"errors"
	"math"

	tafla "github.com/snovader/tafla/go"
	gen "github.com/snovader/tafla/go/benchmarks/generated/tafla"
)

// Way is a way to read tafla's items.
type Way interface {
	// Name returns the way's name, which names its benchmarks.
	Name() string
	// LastTable sums the last item of the vector of nested tables.
	LastTable(bytes []byte) (float64, error)
	// EveryTable sums every item of the vector of nested tables, in order.
	EveryTable(bytes []byte) (float64, error)
	// LastStruct sums the last struct of the array.
	LastStruct(bytes []byte) (float64, error)
	// EveryStruct sums every struct of the array, in order.
	EveryStruct(bytes []byte) (float64, error)
}

// Ways are every way, in the order that reports list them, the generated
// code twice, an A/A pair that shows the harness's noise.
func Ways() []Way {
	return []Way{ByHand{}, Generated{name: "tafla-gen"}, Generated{name: "tafla-gen A/A"}}
}

var errEmpty = errors.New("there are no items")

// ByHand reads with ByteBuffer's getters by hand: each item in place, from
// the vector's GetInPlace for the last item and ItemsInPlace for every item,
// and the structs from GetStructArray's bytes.
type ByHand struct{}

func (ByHand) Name() string { return "tafla" }

func (ByHand) LastTable(bytes []byte) (float64, error) {
	vector, err := vectorOf(bytes)
	if err != nil {
		return 0, err
	}
	if vector.Len() == 0 {
		return 0, errEmpty
	}

	item, err := vector.GetInPlace(vector.Len() - 1)
	if err != nil {
		return 0, err
	}

	return sumTable(item)
}

func (ByHand) EveryTable(bytes []byte) (float64, error) {
	vector, err := vectorOf(bytes)
	if err != nil {
		return 0, err
	}

	sum := 0.0
	for item, err := range vector.ItemsInPlace() {
		if err != nil {
			return 0, err
		}
		itemSum, err := sumTable(item)
		if err != nil {
			return 0, err
		}
		sum += itemSum
	}

	return sum, nil
}

func (ByHand) LastStruct(bytes []byte) (float64, error) {
	structs, err := structsOf(bytes)
	if err != nil {
		return 0, err
	}
	if len(structs) == 0 {
		return 0, errEmpty
	}

	return sumStruct(structs[len(structs)-12:]), nil
}

func (ByHand) EveryStruct(bytes []byte) (float64, error) {
	structs, err := structsOf(bytes)
	if err != nil {
		return 0, err
	}

	sum := 0.0
	for start := 0; start+12 <= len(structs); start += 12 {
		sum += sumStruct(structs[start : start+12])
	}

	return sum, nil
}

func vectorOf(bytes []byte) (tafla.Vector, error) {
	buffer := tafla.NewByteBuffer(bytes, tafla.TableLayout{})
	position, err := buffer.RootOffset(0)
	if err != nil {
		return tafla.Vector{}, err
	}

	return buffer.GetVector(position)
}

func structsOf(bytes []byte) ([]byte, error) {
	buffer := tafla.NewByteBuffer(bytes, tafla.TableLayout{})
	position, err := buffer.RootOffset(0)
	if err != nil {
		return nil, err
	}

	return buffer.GetStructArray(position, 12)
}

// sumTable sums an item read in place: its value and its pair's two floats
func sumTable(item []byte) (float64, error) {
	buffer := tafla.NewByteBuffer(item, tafla.TableLayout{})
	position, err := buffer.FieldOffset(0, 0)
	if err != nil {
		return 0, err
	}
	value, err := buffer.GetFloat32(position)
	if err != nil {
		return 0, err
	}

	pair, err := buffer.FieldOffset(0, 1)
	if err != nil {
		return 0, err
	}
	position, err = buffer.FieldOffset(pair, 0)
	if err != nil {
		return 0, err
	}
	first, err := buffer.GetFloat32(position)
	if err != nil {
		return 0, err
	}
	position, err = buffer.FieldOffset(pair, 1)
	if err != nil {
		return 0, err
	}
	second, err := buffer.GetFloat32(position)
	if err != nil {
		return 0, err
	}

	return float64(value) + float64(first) + float64(second), nil
}

// sumStruct sums a struct of three float32s, from its 12 bytes
func sumStruct(bytes []byte) float64 {
	value := math.Float32frombits(binary.LittleEndian.Uint32(bytes[0:]))
	first := math.Float32frombits(binary.LittleEndian.Uint32(bytes[4:]))
	second := math.Float32frombits(binary.LittleEndian.Uint32(bytes[8:]))

	return float64(value) + float64(first) + float64(second)
}

// Generated reads with the generated getters, Items, which return views of
// the vector and of the array, whose Get reads one item and whose All reads
// every item.
type Generated struct {
	name string
}

func (g Generated) Name() string { return g.name }

func (Generated) LastTable(bytes []byte) (float64, error) {
	view, err := gen.NewItemList(bytes).Items()
	if err != nil {
		return 0, err
	}
	if view.Len() == 0 {
		return 0, errEmpty
	}

	item, err := view.Get(view.Len() - 1)
	if err != nil {
		return 0, err
	}

	return sumItem(item)
}

func (Generated) EveryTable(bytes []byte) (float64, error) {
	view, err := gen.NewItemList(bytes).Items()
	if err != nil {
		return 0, err
	}

	sum := 0.0
	for item, err := range view.All() {
		if err != nil {
			return 0, err
		}
		itemSum, err := sumItem(item)
		if err != nil {
			return 0, err
		}
		sum += itemSum
	}

	return sum, nil
}

func (Generated) LastStruct(bytes []byte) (float64, error) {
	view, err := gen.NewTriples(bytes).Items()
	if err != nil {
		return 0, err
	}
	if view.Len() == 0 {
		return 0, errEmpty
	}

	return sumTriple(view.Get(view.Len() - 1)), nil
}

func (Generated) EveryStruct(bytes []byte) (float64, error) {
	view, err := gen.NewTriples(bytes).Items()
	if err != nil {
		return 0, err
	}

	sum := 0.0
	for _, item := range view.All() {
		sum += sumTriple(item)
	}

	return sum, nil
}

// sumItem sums a generated item's value and its pair's two floats
func sumItem(item gen.Item) (float64, error) {
	value, err := item.Value()
	if err != nil {
		return 0, err
	}
	pair, err := item.Pair()
	if err != nil {
		return 0, err
	}
	first, err := pair.First()
	if err != nil {
		return 0, err
	}
	second, err := pair.Second()
	if err != nil {
		return 0, err
	}

	return float64(value) + float64(first) + float64(second), nil
}

func sumTriple(triple gen.Triple) float64 {
	return float64(triple.Value) + float64(triple.First) + float64(triple.Second)
}
