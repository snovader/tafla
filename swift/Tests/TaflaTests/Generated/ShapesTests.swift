import GeneratedShapes
import Tafla
import Testing

// The code generated from schemas/shapes.tafla: how the tests draw and read
// its messages and its struct, and how a vector's view reads its items in
// place, each when it's asked for. GeneratedTests.swift checks the
// properties of every message and struct of the example schemas.

let shapesMessages = [
    Message(
        "shapes.Point",
        layout: .default,
        draw: { $0.point() },
        read: { bytes throws(DecodeError) in try PointFields(reading: Point(bytes)) }
    ),
    Message(
        "shapes.Document",
        layout: TableLayout(sized: true, versioned: true),
        version: 1,
        draw: { $0.document() },
        read: { bytes throws(DecodeError) in try DocumentFields(reading: Document(bytes)) },
        versionOf: { bytes throws(DecodeError) in try Document.version(of: bytes) }
    ),
    Message(
        "shapes.ItemList",
        layout: .default,
        draw: { $0.itemList() },
        read: { bytes throws(DecodeError) in try ItemListFields(reading: ItemList(bytes)) }
    ),
]

let shapesStructs = [StructCase("shapes.Sample", Sample.self, size: 12) { $0.sample() }]

extension PointFields: GeneratedFields {}
extension DocumentFields: GeneratedFields {}
extension ItemListFields: GeneratedFields {}
extension Sample: GeneratedStruct {}

extension Draw {
    mutating func point() -> PointFields {
        PointFields(x: double(), y: double(), id: integer(), name: string(most: 8))
    }

    mutating func document() -> DocumentFields {
        DocumentFields(x: double(), y: double(), name: string(most: 8), columns: columns())
    }

    mutating func columns() -> ColumnsFields {
        ColumnsFields(base: integer(), keys: integers(most: 6), a: floats(most: 6), b: floats(most: 6))
    }

    mutating func itemList() -> ItemListFields {
        ItemListFields(items: array(most: 4) { $0.item() })
    }

    mutating func item() -> ItemFields {
        ItemFields(value: float(), pair: PairFields(first: float(), second: float()))
    }

    mutating func sample() -> Sample {
        Sample(key: integer(), a: float(), b: float())
    }
}

extension PointFields {
    init(reading point: borrowing Point) throws(DecodeError) {
        try self.init(x: point.x, y: point.y, id: point.id, name: point.name)
    }
}

extension DocumentFields {
    init(reading document: borrowing Document) throws(DecodeError) {
        let columns = try ColumnsFields(reading: document.columns)
        try self.init(x: document.x, y: document.y, name: document.name, columns: columns)
    }
}

extension ColumnsFields {
    init(reading columns: borrowing Columns) throws(DecodeError) {
        try self.init(base: columns.base, keys: Array(columns.keys), a: Array(columns.a), b: Array(columns.b))
    }
}

extension ItemListFields {
    init(reading list: borrowing ItemList) throws(DecodeError) {
        let view = try list.items

        var items: [ItemFields] = []
        for index in view.indices {
            items.append(try ItemFields(reading: view[index]))
        }

        self.init(items: items)
    }
}

extension ItemFields {
    init(reading item: borrowing Item) throws(DecodeError) {
        let pair = try item.pair
        try self.init(value: item.value, pair: PairFields(first: pair.first, second: pair.second))
    }
}

@Suite("Generated shapes")
struct ShapesTests {
    // The vector at 4 holds its count and the item's end, 36, and then the
    // item at 12: its two offsets, its value at 20 and its pair at 24. An end
    // of 16 leaves the item only its offsets, which point past it, at bytes
    // that it reads as every other target does.
    @Test func readsAnItemInPlacePastItsEndWithinTheBytesOfItsItemList() throws {
        let fields = ItemListFields(items: [ItemFields(value: 1.5, pair: PairFields(first: 2.5, second: 3.5))])
        var bytes = try fields.serialize()
        #expect(bytes[8..<12] == [36, 0, 0, 0])
        bytes[8..<12] = [16, 0, 0, 0]

        let read = try withSpan(of: bytes) { bytes throws in try ItemListFields(reading: ItemList(bytes)) }
        #expect(sameValues(read, fields))
    }

    // The getter of a vector returns its view, which reads each item when
    // it's asked for, so an item whose bytes are corrupt fails when it's
    // read, rather than when the getter is called, and the other items read
    // as they were written. The vector at 4 holds its count and the items'
    // ends, 40 and 68, from its position, and an end of 20 ends the second
    // item before it starts.
    @Test func throwsForACorruptItemWhenItsReadRatherThanWhenTheGetterIsCalled() throws {
        let first = ItemFields(value: 1.5, pair: PairFields(first: 2.5, second: 3.5))
        let second = ItemFields(value: 4.5, pair: PairFields(first: 5.5, second: 6.5))
        var bytes = try ItemListFields(items: [first, second]).serialize()
        #expect(bytes[8..<16] == [40, 0, 0, 0, 68, 0, 0, 0])
        bytes[12..<16] = [20, 0, 0, 0]

        try withSpan(of: bytes) { bytes throws in
            let items = try ItemList(bytes).items
            #expect(items.count == 2)
            #expect(items.indices == 0..<2)
            #expect(items.isEmpty == false)

            #expect(sameValues(try ItemFields(reading: items[0]), first))
            #expect(throws: DecodeError.itemEndsBeforeStart(start: 44, end: 24)) {
                try items[1].value
            }
        }
    }

    // The README's example
    @Test func writesAndReadsADocument() throws {
        let fields = DocumentFields(
            x: 1.5,
            y: 2,
            name: "a",
            columns: ColumnsFields(base: 0, keys: [1, 2], a: [0.5, 1.5], b: [2.5, 3.5])
        )
        let bytes = try fields.serialize()

        let all = bytes[...]
        let document = try Document(all.span.bytes)  // checks the version
        let a = try document.columns.a  // an ArrayView<Float> of the bytes
        #expect(Array(a) == [0.5, 1.5])
        #expect(try Document.version(of: all.span.bytes) == Document.formatVersion)
    }

    @Test func stopsTheProgramForAnIndexOutOfAViewsRange() async {
        await #expect(processExitsWith: .failure) {
            let bytes = try ItemListFields(items: []).serialize()
            try withSpan(of: bytes) { bytes throws in
                _ = try ItemList(bytes).items[0].value
            }
        }
    }
}
