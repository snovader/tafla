import Tafla
import Testing

// Tables that TableBuilder writes follow the format's rules, as SPEC.md at the
// root of the repository states them: the header holds the length and
// version, offsets lie within the table, values follow each other in field
// order with only the padding that aligns arrays and nested tables,
// multi-byte values are little-endian, and nested tables and vector items
// hold the bytes of their tables.

@Suite("Format")
struct FormatTests {
    @Test func sizedTablesHoldTheirLengthInTheHeader() async {
        await checkFormat { bytes, table in
            if table.table.layout.isSized {
                #expect(slot(bytes, at: table.start, table.width) == table.length)
            }
        }
    }

    @Test func versionedTablesHoldTheirVersionInTheHeader() async {
        await checkFormat { bytes, table in
            let layout = table.table.layout
            if layout.isVersioned {
                let position = table.start + (layout.isSized ? table.width : 0)
                #expect(slot(bytes, at: position, table.width) == Int(table.table.version!))
            }
        }
    }

    // Every offset of every table lies after its header and offsets, and its
    // field ends within the table. A table nested with no fields is empty,
    // and can end its parent.
    @Test func offsetsLieWithinTheirTable() async {
        await checkFormat { bytes, table in
            for field in table.fields {
                #expect(field.position >= table.offsetsEnd)
                #expect(field.position + field.length <= table.end)
            }
        }
    }

    // From the start of the table that holds them, which is aligned in turn
    // when it's nested: an array of scalars at a multiple of their size, and
    // an array of structs of their largest scalar's, and a nested table at
    // its alignment
    @Test func arraysStartAtAMultipleOfTheirAlignment() async {
        await checkFormat { bytes, table in
            for field in table.fields {
                guard let alignment = alignmentFromStart(field.field, nested: field.nestedAlignment) else {
                    continue
                }

                let start = field.position - table.start + (isArray(field.field) ? table.width : 0)
                #expect(start % alignment.bytes == 0, "\(field.field)")
            }
        }
    }

    // Scalars hold their values' little-endian bytes, structs their scalars',
    // arrays and strings their counts' and their elements', arrays of bools
    // a byte of 1 or 0 for each after the count, unpadded, and vectors their
    // counts' and their items' ends', which count from the vector's position,
    // after the ends
    @Test func valuesAreLittleEndian() async {
        await checkFormat { bytes, table in
            for field in table.fields {
                switch field.field {
                case .table:
                    continue
                case .vector(let items):
                    var expected = littleEndianSlot(items.count, table.width)
                    var end = (items.count + 1) * table.width
                    for item in items {
                        end += try item.table.bytes().count
                        expected += littleEndianSlot(end, table.width)
                    }
                    #expect(Array(bytes[field.position..<field.position + expected.count]) == expected)
                default:
                    let expected = valueBytes(field.field, width: table.width)
                    #expect(Array(bytes[field.position..<field.position + field.length]) == expected)
                }
            }
        }
    }

    // Values follow the header and offsets in field order, each after the
    // previous one, with only the zeroed padding that aligns an array's
    // elements and a nested table to their alignment between them, and the
    // last one ends the table
    @Test func valuesFollowEachOtherWithOnlyZeroedPaddingToAlignThem() async {
        await checkFormat { bytes, table in
            var end = table.offsetsEnd
            for field in table.fields {
                let padding = paddingBefore(
                    field.field, nested: field.nestedAlignment, at: end - table.start, width: table.width)
                #expect(field.position == end + padding, "\(field.field)")
                #expect(bytes[end..<end + padding].allSatisfy { $0 == 0 })
                end = field.position + field.length
            }

            #expect(end == table.end)
        }
    }

    // The bytes that a builder writes are the same nested in another table as
    // on their own
    @Test func nestedTablesAndVectorItemsHoldTheBytesOfTheirTables() async {
        await checkFormat { bytes, table throws in
            #expect(Array(bytes[table.start..<table.end]) == (try table.table.bytes()))
        }
    }
}

/// Checks a property of every table in the bytes of a table that any
/// description builds: the table itself, the tables nested in it and its
/// vector items, and the tables in those
func checkFormat(
    sourceLocation: SourceLocation = #_sourceLocation,
    _ check: @escaping ([UInt8], TableIn) throws -> Void
) async {
    await checkTables(sourceLocation: sourceLocation) { table in
        let bytes = try table.bytes()
        for found in try tablesIn(bytes, table, at: 0, length: bytes.count) {
            try check(bytes, found)
        }
    }
}

/// A table in the bytes of a table, and where its fields lie
struct TableIn {
    var table: Table
    var start: Int
    var length: Int
    var fields: [FieldIn]

    var width: Int { table.layout.offsetSize }
    var end: Int { start + length }
    /// Where the header and offsets end, and the values start
    var offsetsEnd: Int { start + table.layout.headerSize + table.fields.count * width }
}

/// A field, where it lies in its table's bytes, and how long it is
struct FieldIn {
    var field: Field
    var position: Int
    var length: Int
    /// The alignment that a nested table is set with, by default or
    /// explicitly
    var nestedAlignment: Alignment?
}

/// Finds every table in `bytes` from the description of the one at `start`,
/// of `length` bytes, and the fields of each. Fields lie at the positions
/// that their offsets hold, and are as long as the values that the
/// description gives, and as the tables that their builders write on their
/// own.
func tablesIn(_ bytes: [UInt8], _ table: Table, at start: Int, length: Int) throws -> [TableIn] {
    let width = table.layout.offsetSize
    var found = TableIn(table: table, start: start, length: length, fields: [])
    var nested: [TableIn] = []

    for (index, field) in table.fields.enumerated() {
        let position = start + slot(bytes, at: start + table.layout.headerSize + index * width, width)

        switch field {
        case .table(let table, let alignment):
            let length = try table.table.bytes().count
            let nestedAlignment = alignment ?? defaultAlignment(of: table)
            found.fields.append(
                FieldIn(field: field, position: position, length: length, nestedAlignment: nestedAlignment))
            nested += try tablesIn(bytes, table.table, at: position, length: length)
        case .vector(let items):
            var itemStart = position + (items.count + 1) * width
            for (index, item) in items.enumerated() {
                let itemEnd = position + slot(bytes, at: position + (index + 1) * width, width)
                nested += try tablesIn(bytes, item.table, at: itemStart, length: itemEnd - itemStart)
                itemStart = itemEnd
            }
            found.fields.append(FieldIn(field: field, position: position, length: itemStart - position))
        default:
            found.fields.append(
                FieldIn(field: field, position: position, length: valueBytes(field, width: width).count))
        }
    }

    return [found] + nested
}

/// The alignment that setTable pads a nested table to by default: a
/// builder's largest alignment of its arrays and nested tables, or 1, and 4
/// for bytes
func defaultAlignment(of nested: Nested) -> Alignment {
    guard nested.from == .builder else {
        return .four
    }

    return nested.table.fields.compactMap { field in
        if case .table(let table, let alignment) = field {
            return alignment ?? defaultAlignment(of: table)
        }
        return alignmentFromStart(field, nested: nil)
    }.max() ?? .one
}

/// The multiple of bytes from the start of its table that a field starts at,
/// after its count when it's an array: an array's alignment, or a nested
/// table's, and `nil` for any other
func alignmentFromStart(_ field: Field, nested: Alignment?) -> Alignment? {
    switch field {
    case .int8Array, .uint8Array, .boolArray: .one
    case .int16Array, .uint16Array: .two
    case .int32Array, .uint32Array, .float32Array: .four
    case .int64Array, .uint64Array, .float64Array: .eight
    case .structArray(let types, _): structAlignment(types)
    case .table: nested
    default: nil
    }
}

/// Whether a field is an array of scalars or of structs, whose elements are
/// aligned after its count
func isArray(_ field: Field) -> Bool {
    switch field {
    case .int8Array, .uint8Array, .int16Array, .uint16Array, .int32Array, .uint32Array, .int64Array,
        .uint64Array, .float32Array, .float64Array, .boolArray, .structArray:
        true
    default:
        false
    }
}

/// The padding before `field` when it would start `offset` bytes from its
/// table's start, which aligns an array's elements, after its count, and a
/// nested table's start
func paddingBefore(_ field: Field, nested: Alignment?, at offset: Int, width: Int) -> Int {
    guard let alignment = alignmentFromStart(field, nested: nested) else {
        return 0
    }

    let start = offset + (isArray(field) ? width : 0)
    return (alignment.bytes - start % alignment.bytes) % alignment.bytes
}

/// The bytes of a field that holds a value rather than tables: the
/// little-endian bytes of a scalar, or of a struct's scalars, or the count of
/// an array's elements, of a string's bytes or of an array's structs, and
/// their little-endian bytes
func valueBytes(_ field: Field, width: Int) -> [UInt8] {
    func array<T: FixedWidthInteger>(_ values: [T]) -> [UInt8] {
        littleEndianSlot(values.count, width) + values.flatMap(littleEndianBytes)
    }

    switch field {
    case .bool(let value): return [value ? 1 : 0]
    case .int8Array(let values): return array(values)
    case .uint8Array(let values): return array(values)
    case .int16Array(let values): return array(values)
    case .uint16Array(let values): return array(values)
    case .int32Array(let values): return array(values)
    case .uint32Array(let values): return array(values)
    case .int64Array(let values): return array(values)
    case .uint64Array(let values): return array(values)
    case .float32Array(let bits): return array(bits)
    case .float64Array(let bits): return array(bits)
    case .boolArray(let values): return littleEndianSlot(values.count, width) + values.map { $0 ? 1 : 0 }
    case .string(let utf8): return littleEndianSlot(utf8.count, width) + utf8
    case .struct(let scalars): return structBytes(scalars)
    case .structArray(_, let items): return littleEndianSlot(items.count, width) + items.flatMap(structBytes)
    case .table, .vector: preconditionFailure("A table or vector holds tables")
    default: return field.scalarBytes
    }
}

/// Reads a little-endian slot of `width` bytes at `position`
func slot(_ bytes: [UInt8], at position: Int, _ width: Int) -> Int {
    (0..<width).reduce(0) { $0 | Int(bytes[position + $1]) << (8 * $1) }
}

/// The little-endian bytes of `value` in a slot of `width` bytes
func littleEndianSlot(_ value: Int, _ width: Int) -> [UInt8] {
    (0..<width).map { UInt8(truncatingIfNeeded: value >> (8 * $0)) }
}
