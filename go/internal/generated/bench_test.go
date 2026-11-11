package generated_test

// Benchmarks of the runtime's hot paths, through the code generated from
// shapes.tafla, whose messages are the comparison suite's record, document
// and vector of nested tables, and through ByteBuffer and TableBuilder by
// hand. They compare versions of the runtime, with benchstat; the comparison
// suite in benchmarks/ compares tafla with other libraries.
//
//	go test -run '^$' -bench . -count 10 ./internal/generated/ > new.txt

import (
	"testing"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/internal/generated/shapes"
)

var (
	sinkBytes []byte
	sinkFloat float64
	sinkErr   error
)

var point = shapes.PointFields{X: 59.3293, Y: 18.0686, Id: -631_152_000, Name: "tafla-benchmarks"}

func document(rows int) shapes.DocumentFields {
	keys := make([]uint32, rows)
	a := make([]float32, rows)
	b := make([]float32, rows)
	for row := range rows {
		keys[row] = uint32(row * 7919 % (1 << 24))
		a[row] = float32(row) * 0.5
		b[row] = -float32(row) * 0.25
	}

	return shapes.DocumentFields{X: 1.5, Y: -2.25, Name: "tafla-benchmarks", Columns: shapes.ColumnsFields{
		Base: -631_152_000, Keys: keys, A: a, B: b,
	}}
}

func itemList(items int) shapes.ItemListFields {
	list := shapes.ItemListFields{Items: make([]shapes.ItemFields, items)}
	for index := range list.Items {
		list.Items[index] = shapes.ItemFields{
			Value: float32(index), Pair: shapes.PairFields{First: float32(index) / 2, Second: float32(index) * 3},
		}
	}

	return list
}

func BenchmarkWritePointGenerated(b *testing.B) {
	for range b.N {
		sinkBytes, sinkErr = point.MarshalBinary()
	}
}

func BenchmarkWritePointBuilder(b *testing.B) {
	for range b.N {
		builder, _ := tafla.NewTableBuilder(4, tafla.TableLayout{})
		_ = builder.SetFloat64(point.X)
		_ = builder.SetFloat64(point.Y)
		_ = builder.SetInt64(point.Id)
		_ = builder.SetString(point.Name)
		sinkBytes, sinkErr = builder.Bytes()
	}
}

func BenchmarkReadPointGenerated(b *testing.B) {
	bytes, _ := point.MarshalBinary()
	reader := shapes.NewPoint(bytes)
	b.ResetTimer()

	for range b.N {
		x, err1 := reader.X()
		y, err2 := reader.Y()
		id, err3 := reader.Id()
		name, err4 := reader.Name()
		sinkFloat = x + y + float64(id) + float64(len(name))
		sinkErr = errorOf(err1, err2, err3, err4)
	}
}

func BenchmarkReadPointByHand(b *testing.B) {
	bytes, _ := point.MarshalBinary()
	b.ResetTimer()

	for range b.N {
		buffer := tafla.NewByteBuffer(bytes, tafla.TableLayout{})
		p0, err1 := buffer.RootOffset(0)
		x, err2 := buffer.GetFloat64(p0)
		p1, err3 := buffer.RootOffset(1)
		y, err4 := buffer.GetFloat64(p1)
		p2, err5 := buffer.RootOffset(2)
		id, err6 := buffer.GetInt64(p2)
		p3, err7 := buffer.RootOffset(3)
		name, err8 := buffer.GetString(p3)
		sinkFloat = x + y + float64(id) + float64(len(name))
		sinkErr = errorOf(err1, err2, err3, err4, err5, err6, err7, err8)
	}
}

func BenchmarkWriteDocument100Generated(b *testing.B) {
	fields := document(100)
	b.ResetTimer()
	for range b.N {
		sinkBytes, sinkErr = fields.MarshalBinary()
	}
}

func BenchmarkWriteDocument100000Generated(b *testing.B) {
	fields := document(100_000)
	b.ResetTimer()
	for range b.N {
		sinkBytes, sinkErr = fields.MarshalBinary()
	}
}

func BenchmarkWriteDocument100Builder(b *testing.B) {
	fields := document(100)
	layout := tafla.TableLayout{}.Sized().Versioned()
	b.ResetTimer()
	for range b.N {
		columns, _ := tafla.NewTableBuilder(4, tafla.TableLayout{})
		_ = columns.SetInt64(fields.Columns.Base)
		_ = columns.SetUint32Array(fields.Columns.Keys)
		_ = columns.SetFloat32Array(fields.Columns.A)
		_ = columns.SetFloat32Array(fields.Columns.B)
		builder, _ := tafla.NewVersionedTableBuilder(4, layout, 1)
		_ = builder.SetFloat64(fields.X)
		_ = builder.SetFloat64(fields.Y)
		_ = builder.SetString(fields.Name)
		_ = builder.SetTable(columns)
		sinkBytes, sinkErr = builder.Bytes()
	}
}

func BenchmarkReadDocument100Generated(b *testing.B) {
	bytes, _ := document(100).MarshalBinary()
	b.ResetTimer()

	for range b.N {
		reader, err := shapes.NewDocument(bytes)
		x, _ := reader.X()
		name, _ := reader.Name()
		columns, _ := reader.Columns()
		base, _ := columns.Base()
		keys, _ := columns.Keys()
		a, _ := columns.A()
		c, _ := columns.B()
		sinkFloat = x + float64(len(name)+int(base)+len(keys.Slice())+len(a.Slice())+len(c.Slice()))
		sinkErr = err
	}
}

func BenchmarkSumColumn100000Values(b *testing.B) {
	bytes, _ := document(100_000).MarshalBinary()
	reader, _ := shapes.NewDocument(bytes)
	columns, _ := reader.Columns()
	b.ResetTimer()

	for range b.N {
		a, err := columns.A()
		sum := 0.0
		for value := range a.Values() {
			sum += float64(value)
		}
		sinkFloat, sinkErr = sum, err
	}
}

func BenchmarkWriteItemList100Generated(b *testing.B) {
	fields := itemList(100)
	b.ResetTimer()
	for range b.N {
		sinkBytes, sinkErr = fields.MarshalBinary()
	}
}

func BenchmarkWriteItemList100Builder(b *testing.B) {
	fields := itemList(100)
	b.ResetTimer()
	for range b.N {
		items := make([]tafla.NestedTable, len(fields.Items))
		for index, item := range fields.Items {
			pair, _ := tafla.NewTableBuilder(2, tafla.TableLayout{})
			_ = pair.SetFloat32(item.Pair.First)
			_ = pair.SetFloat32(item.Pair.Second)
			table, _ := tafla.NewTableBuilder(2, tafla.TableLayout{})
			_ = table.SetFloat32(item.Value)
			_ = table.SetTable(pair)
			items[index] = table
		}
		builder, _ := tafla.NewTableBuilder(1, tafla.TableLayout{})
		_ = builder.SetVector(items...)
		sinkBytes, sinkErr = builder.Bytes()
	}
}

func BenchmarkSumItemList100View(b *testing.B) {
	bytes, _ := itemList(100).MarshalBinary()
	reader := shapes.NewItemList(bytes)
	b.ResetTimer()

	for range b.N {
		view, err := reader.ItemsView()
		sum := 0.0
		for item, itemErr := range view.All() {
			value, _ := item.Value()
			pair, _ := item.Pair()
			first, _ := pair.First()
			second, _ := pair.Second()
			sum += float64(value) + float64(first) + float64(second)
			err = errorOf(err, itemErr)
		}
		sinkFloat, sinkErr = sum, err
	}
}

func BenchmarkReadLastItemView(b *testing.B) {
	bytes, _ := itemList(100).MarshalBinary()
	reader := shapes.NewItemList(bytes)
	b.ResetTimer()

	for range b.N {
		view, _ := reader.ItemsView()
		item, err := view.Get(view.Len() - 1)
		value, _ := item.Value()
		sinkFloat, sinkErr = float64(value), err
	}
}

func errorOf(errs ...error) error {
	for _, err := range errs {
		if err != nil {
			return err
		}
	}

	return nil
}
