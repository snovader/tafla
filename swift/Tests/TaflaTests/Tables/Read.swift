import Tafla

// Reads tables with ByteBuffer as their descriptions set them out, and returns
// the descriptions with the values read in place of theirs, so that they
// equal the descriptions when the bytes hold the described values.
//
// A nested table, from setTable or in a vector, is read in place when it has
// no header and its parent's offset type: a field from its position in its
// parent's bytes, and a vector item from the bytes that
// Vector.itemInPlace(at:) returns, which end where its parent's do. Any other
// is read as a table of its own: a field from its position to the end of its
// parent, since its length isn't written, and a vector item from the bytes
// that Vector.item(at:) returns. The scalars of structs are read at their
// offsets in the bytes that struct(at:size:) and structArray(at:size:) return.
//
// Every array is read element by element, copied, and in place where its
// span is, and the three must agree.

/// The error of bytes that read without a decode error, but otherwise than
/// their description: a sized table whose header holds another length, a
/// vector of another number of items, or ways of reading that disagree, which
/// no bytes may cause
struct Mismatch: Error, CustomStringConvertible {
    enum Kind {
        case size
        case itemCount
        case disagreement
    }

    let kind: Kind
    let description: String

    init(_ kind: Kind, _ description: String) {
        self.kind = kind
        self.description = description
    }
}

/// Reads a table from its own bytes, as the root or a vector item is read: its
/// header, whose size must be the table's byte length when it's sized, and
/// then its fields
func readTable(_ bytes: RawSpan, _ table: Table) throws -> Table {
    let buffer = ByteBuffer(bytes, layout: table.layout)

    if table.layout.isSized {
        let size = try buffer.size
        guard size == bytes.byteCount else {
            throw Mismatch(.size, "The header holds the size \(size) of \(bytes.byteCount) bytes")
        }
    }

    var read = Table(layout: table.layout, version: nil, fields: [])
    if table.layout.isVersioned {
        read.version = try buffer.version
    }

    for (index, field) in table.fields.enumerated() {
        read.fields.append(try readRootField(buffer, index, field))
    }

    return read
}

/// Reads root field `index` of a table as `field`, without reading the header
func readRootField(_ buffer: ByteBuffer, _ index: Int, _ field: Field) throws -> Field {
    try readField(buffer, at: buffer.rootOffset(index), field)
}

/// Reads the field at `position` of the table that `buffer` reads
func readField(_ buffer: ByteBuffer, at position: Int, _ field: Field) throws -> Field {
    switch field {
    case .int8: .int8(try buffer.int8(at: position))
    case .uint8: .uint8(try buffer.uint8(at: position))
    case .int16: .int16(try buffer.int16(at: position))
    case .uint16: .uint16(try buffer.uint16(at: position))
    case .int32: .int32(try buffer.int32(at: position))
    case .uint32: .uint32(try buffer.uint32(at: position))
    case .int64: .int64(try buffer.int64(at: position))
    case .uint64: .uint64(try buffer.uint64(at: position))
    case .float32: .float32(try buffer.float32(at: position))
    case .float64: .float64(try buffer.float64(at: position))
    case .bool: .bool(try buffer.uint8(at: position) != 0)
    case .int8Array: .int8Array(try elements(of: buffer.int8Array(at: position)))
    case .uint8Array: .uint8Array(try elements(of: buffer.uint8Array(at: position)))
    case .int16Array: .int16Array(try elements(of: buffer.int16Array(at: position)))
    case .uint16Array: .uint16Array(try elements(of: buffer.uint16Array(at: position)))
    case .int32Array: .int32Array(try elements(of: buffer.int32Array(at: position)))
    case .uint32Array: .uint32Array(try elements(of: buffer.uint32Array(at: position)))
    case .int64Array: .int64Array(try elements(of: buffer.int64Array(at: position)))
    case .uint64Array: .uint64Array(try elements(of: buffer.uint64Array(at: position)))
    case .float32Array:
        .float32Array(bits: try elements(of: buffer.float32Array(at: position)).map(\.bitPattern))
    case .float64Array:
        .float64Array(bits: try elements(of: buffer.float64Array(at: position)).map(\.bitPattern))
    case .string: .string(try buffer.string(at: position))
    case .struct(let scalars):
        try readStruct(buffer, at: position, scalars.map { $0.scalarType! })
    case .structArray(let types, _):
        try readStructArray(buffer, at: position, types)
    case .table(let nested, let alignment):
        .table(
            Nested(from: nested.from, table: try readNested(buffer, at: position, nested.table)), alignment: alignment)
    case .vector(let items):
        try readVector(buffer, at: position, items)
    }
}

/// Reads an array element by element, copied, and in place where it can be,
/// which must agree, as bits
func elements<T: ArrayElement>(of view: ArrayView<T>) throws -> [T] {
    let copy = Array(view)
    guard copy.count == view.count else {
        throw Mismatch(.disagreement, "Copied \(copy.count) of \(view.count) elements")
    }

    for index in view.indices where view[index]._taflaBits != copy[index]._taflaBits {
        throw Mismatch(.disagreement, "Element \(index) reads otherwise than it copies")
    }

    if let span = view.span {
        guard span.count == copy.count else {
            throw Mismatch(.disagreement, "A span of \(span.count) of \(copy.count) elements")
        }
        for index in span.indices where span[index]._taflaBits != copy[index]._taflaBits {
            throw Mismatch(.disagreement, "Element \(index) reads otherwise in place")
        }
    }

    return copy
}

/// Reads a table nested with setTable
func readNested(_ buffer: ByteBuffer, at position: Int, _ table: Table) throws -> Table {
    if readsInPlace(table, within: buffer) {
        return try readInPlace(buffer, at: position, table)
    }

    // A position past the end leaves no bytes, which the first read fails for
    let bytes = buffer.bytes
    let start = min(max(position, 0), bytes.byteCount)
    return try readTable(bytes.extracting(start..<bytes.byteCount), table)
}

/// Reads a vector's items, after checking that finding each from its ends
/// in place finds what finding it as a table of its own finds
func readVector(_ buffer: ByteBuffer, at position: Int, _ items: [Nested]) throws -> Field {
    let vector = try buffer.vector(at: position)

    var firstError: DecodeError?
    for index in vector.indices {
        let error = try checkItem(vector, index, within: buffer.bytes)
        firstError = firstError ?? error
    }
    if let firstError {
        throw firstError
    }

    guard vector.count == items.count else {
        throw Mismatch(.itemCount, "A vector of \(vector.count) items, rather than \(items.count)")
    }

    var read: [Nested] = []
    for (index, nested) in items.enumerated() {
        let table: Table
        if readsInPlace(nested.table, within: buffer) {
            // At its start, in a buffer of its offset type, which is its
            // parent's
            let item = ByteBuffer(try vector.itemInPlace(at: index), layout: nested.table.layout)
            table = try readInPlace(item, at: 0, nested.table)
        } else {
            table = try readTable(vector.item(at: index), nested.table)
        }

        read.append(Nested(from: nested.from, table: table))
    }

    return .vector(read)
}

/// Checks that item `index` of a vector fails to read in place as it fails
/// to read as a table of its own, or that its bytes in place start where its
/// own do and run on to the end of the buffer's, and returns its error
func checkItem(_ vector: Vector, _ index: Int, within bytes: RawSpan) throws -> DecodeError? {
    let item: Range<Int>
    do {
        item = bytes.byteOffsets(of: try vector.item(at: index))!
    } catch {
        do {
            _ = try vector.itemInPlace(at: index)
        } catch let inPlaceError {
            guard inPlaceError == error else {
                throw Mismatch(.disagreement, "Item \(index) fails with \(error) and \(inPlaceError) in place")
            }
            return error
        }
        throw Mismatch(.disagreement, "Item \(index) fails with \(error), but not in place")
    }

    let inPlace = bytes.byteOffsets(of: try vector.itemInPlace(at: index))!
    guard inPlace.lowerBound == item.lowerBound, inPlace.upperBound == bytes.byteCount else {
        throw Mismatch(.disagreement, "Item \(index) lies at \(item), and in place at \(inPlace)")
    }

    return nil
}

/// Whether a nested table is read in place: it has no header, and its
/// parent's offset type
func readsInPlace(_ table: Table, within buffer: ByteBuffer) -> Bool {
    let layout = table.layout
    return !layout.isSized && !layout.isVersioned && layout.offsetType == buffer.layout.offsetType
}

/// Reads the fields of a table nested at `position`, from offsets counted
/// from its start, within its parent's bytes
func readInPlace(_ buffer: ByteBuffer, at position: Int, _ table: Table) throws -> Table {
    var read = Table(layout: table.layout, version: nil, fields: [])
    for (index, field) in table.fields.enumerated() {
        let offset = try buffer.fieldOffset(index, inTableAt: position)
        read.fields.append(try readField(buffer, at: offset, field))
    }

    return read
}

func readStruct(_ buffer: ByteBuffer, at position: Int, _ types: [ScalarType]) throws -> Field {
    let bytes = try buffer.struct(at: position, size: structSize(types))
    return .struct(try readScalars(bytes, types))
}

func readStructArray(_ buffer: ByteBuffer, at position: Int, _ types: [ScalarType]) throws -> Field {
    let size = structSize(types)
    let bytes = try buffer.structArray(at: position, size: size)

    var items: [[Field]] = []
    for start in stride(from: 0, to: bytes.byteCount, by: size) {
        items.append(try readScalars(bytes.extracting(start..<start + size), types))
    }

    return .structArray(types: types, items: items)
}

/// Reads the scalars of a struct of `types` at their offsets in its bytes,
/// which a ByteBuffer reads in any layout
func readScalars(_ bytes: RawSpan, _ types: [ScalarType]) throws -> [Field] {
    let buffer = ByteBuffer(bytes)

    var scalars: [Field] = []
    var offset = 0
    for type in types {
        scalars.append(try readScalar(buffer, at: offset, type))
        offset += type.size
    }

    return scalars
}

func readScalar(_ buffer: ByteBuffer, at position: Int, _ type: ScalarType) throws -> Field {
    switch type {
    case .int8: .int8(try buffer.int8(at: position))
    case .uint8: .uint8(try buffer.uint8(at: position))
    case .int16: .int16(try buffer.int16(at: position))
    case .uint16: .uint16(try buffer.uint16(at: position))
    case .int32: .int32(try buffer.int32(at: position))
    case .uint32: .uint32(try buffer.uint32(at: position))
    case .int64: .int64(try buffer.int64(at: position))
    case .uint64: .uint64(try buffer.uint64(at: position))
    case .float32: .float32(try buffer.float32(at: position))
    case .float64: .float64(try buffer.float64(at: position))
    }
}

/// Calls `body` with the bytes of an array, as a span, which can't outlive
/// the call
func withSpan<R>(of bytes: [UInt8], _ body: (RawSpan) throws -> R) rethrows -> R {
    try bytes.withUnsafeBytes { raw in try body(RawSpan(_unsafeBytes: raw)) }
}

/// Calls `body` with a copy of `bytes` that starts `byteOffset` bytes past a
/// multiple of 8 in a larger buffer of 0xff, so that its arrays are aligned
/// in memory when `byteOffset` is a multiple of 8, and reads past the bytes
/// would find others
func withSpan<R>(of bytes: [UInt8], atByteOffset byteOffset: Int, _ body: (RawSpan) throws -> R) rethrows -> R {
    // UInt64s, which are aligned to 8
    var backing = [UInt64](repeating: .max, count: (byteOffset + bytes.count) / 8 + 2)

    return try backing.withUnsafeMutableBytes { all in
        let within = UnsafeMutableRawBufferPointer(rebasing: all[byteOffset..<byteOffset + bytes.count])
        bytes.withUnsafeBytes { within.copyMemory(from: $0) }
        return try body(RawSpan(_unsafeBytes: UnsafeRawBufferPointer(within)))
    }
}
