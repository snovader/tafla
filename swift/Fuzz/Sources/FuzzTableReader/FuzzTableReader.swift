import Tafla

// Reads arbitrary bytes with every getter of TableReader, at every position
// and past the end, in every offset type and header, and reads every array,
// struct array and vector that it finds there, as Rust's table_reader and
// Go's FuzzTableReader do.
//
// A trap is a finding, and so is AddressSanitizer's report of a read outside
// the bytes, and a read that disagrees with the bounds that SPEC.md gives it:
// each read returns the bytes or value that lie where the format puts them,
// viewed in the input, exactly when they lie within it, and the decode error
// otherwise, which Input checks.

/// The elements of an array, and items of a vector, at its start and end
/// that are read and compared with the bytes
let checked = 8

/// Positions past any bytes, where a position plus a length overflows, or
/// before them
let farPositions = [Int.max, Int.max - 1, Int.max - 3, Int.max - 7, -1, -8, Int.min]

/// Indices of root fields whose offsets lie far past any bytes, where their
/// position overflows. A negative index is a programming error, which stops
/// the program, rather than bytes that a reader rejects.
let farIndices = [Int.max, Int.max - 1, Int.max - 3, Int.max - 7]

@_cdecl("LLVMFuzzerTestOneInput")
public func fuzzTableReader(_ data: UnsafePointer<UInt8>?, _ size: Int) -> CInt {
    let input = Input(bytes: UnsafeRawBufferPointer(start: data, count: size))
    let bytes = RawSpan(_unsafeBytes: input.bytes)

    for offsetType in OffsetType.allCases {
        for sized in [false, true] {
            checkHeader(TableReader(bytes, layout: TableLayout(sized: sized, offsetType: offsetType)), input)

            let versioned = TableLayout(sized: sized, versioned: true, offsetType: offsetType)
            checkVersion(TableReader(bytes, layout: versioned), input)
        }

        let reader = TableReader(bytes, layout: TableLayout(offsetType: offsetType))
        for position in 0..<size + 9 {
            checkPosition(reader, input, at: position)
        }
        for position in farPositions {
            checkPosition(reader, input, at: position)
        }
    }

    return 0
}

/// Reads the table's size and its root fields' offsets
func checkHeader(_ reader: TableReader, _ input: Input) {
    let layout = reader.layout

    let size = layout.isSized ? input.position(at: 0, layout.offsetType) : input.count
    input.checkResult(size) { () throws(DecodeError) in try reader.size }

    for index in 0..<input.count / layout.offsetSize + 2 {
        checkRootOffset(reader, input, index)
    }
    for index in farIndices {
        checkRootOffset(reader, input, index)
    }
}

func checkRootOffset(_ reader: TableReader, _ input: Input, _ index: Int) {
    let layout = reader.layout

    // Past any bytes when it overflows
    let (offset, overflow) = index.multipliedReportingOverflow(by: layout.offsetSize)
    let (slot, overflowAgain) = layout.headerSize.addingReportingOverflow(offset)
    let expected = overflow || overflowAgain ? nil : input.position(at: slot, layout.offsetType)

    input.checkResult(expected) { () throws(DecodeError) in try reader.rootOffset(index) }
}

func checkVersion(_ reader: TableReader, _ input: Input) {
    let layout = reader.layout

    let version = input.slot(at: layout.isSized ? layout.offsetSize : 0, layout.offsetType)
    input.checkResult(version) { () throws(DecodeError) in try reader.version }

    checkHeader(reader, input)
}

/// Reads every type at `position`
func checkPosition(_ reader: TableReader, _ input: Input, at position: Int) {
    checkScalars(reader, input, at: position)
    checkArrays(reader, input, at: position)
    checkString(reader, input, at: position)
    checkStructs(reader, input, at: position)
    checkFieldOffsets(reader, input, at: position)
    checkVector(reader, input, at: position)
}

func checkScalars(_ reader: TableReader, _ input: Input, at position: Int) {
    checkScalar(input, at: position) { () throws(DecodeError) in try reader.int8(at: position) }
    checkScalar(input, at: position) { () throws(DecodeError) in try reader.uint8(at: position) }
    checkScalar(input, at: position) { () throws(DecodeError) in try reader.int16(at: position) }
    checkScalar(input, at: position) { () throws(DecodeError) in try reader.uint16(at: position) }
    checkScalar(input, at: position) { () throws(DecodeError) in try reader.int32(at: position) }
    checkScalar(input, at: position) { () throws(DecodeError) in try reader.uint32(at: position) }
    checkScalar(input, at: position) { () throws(DecodeError) in try reader.int64(at: position) }
    checkScalar(input, at: position) { () throws(DecodeError) in try reader.uint64(at: position) }
    checkScalar(input, at: position) { () throws(DecodeError) in try reader.float32(at: position) }
    checkScalar(input, at: position) { () throws(DecodeError) in try reader.float64(at: position) }
}

/// Checks the read of a scalar, comparing it by its bits, which are the bits
/// of floats too, with its little-endian bytes at `position`
func checkScalar<T: ArrayElement>(_ input: Input, at position: Int, _ read: () throws(DecodeError) -> T) {
    let range = input.range(at: position, byteLength: UInt64(MemoryLayout<T>.size))
    input.checkResult(range.map(input.littleEndian)) { () throws(DecodeError) in try UInt64(read()._taflaBits) }
}

// Each getter passes the view that it reads to `check`, since a closure can't
// return a view of the bytes
func checkArrays(_ reader: TableReader, _ input: Input, at position: Int) {
    checkArray(reader, input, at: position) { check throws(DecodeError) in
        try check(reader.int8Array(at: position))
    }
    checkArray(reader, input, at: position) { check throws(DecodeError) in
        try check(reader.uint8Array(at: position))
    }
    checkArray(reader, input, at: position) { check throws(DecodeError) in
        try check(reader.int16Array(at: position))
    }
    checkArray(reader, input, at: position) { check throws(DecodeError) in
        try check(reader.uint16Array(at: position))
    }
    checkArray(reader, input, at: position) { check throws(DecodeError) in
        try check(reader.int32Array(at: position))
    }
    checkArray(reader, input, at: position) { check throws(DecodeError) in
        try check(reader.uint32Array(at: position))
    }
    checkArray(reader, input, at: position) { check throws(DecodeError) in
        try check(reader.int64Array(at: position))
    }
    checkArray(reader, input, at: position) { check throws(DecodeError) in
        try check(reader.uint64Array(at: position))
    }
    checkArray(reader, input, at: position) { check throws(DecodeError) in
        try check(reader.float32Array(at: position))
    }
    checkArray(reader, input, at: position) { check throws(DecodeError) in
        try check(reader.float64Array(at: position))
    }
}

/// Checks that every way of reading an array's elements reads the elements
/// of its bytes, comparing them by their bits. Only the first and last
/// `checked` elements are compared, since every position may hold an array
/// as long as the bytes, and reading each in full would take time quadratic
/// in their length.
///
/// `read` reads the array with its getter and passes it to the function that
/// it's given, which checks it.
func checkArray<T: ArrayElement>(
    _ reader: TableReader,
    _ input: Input,
    at position: Int,
    _ read: ((ArrayView<T>) -> Void) throws(DecodeError) -> Void
) {
    let expected = input.array(at: position, reader.layout.offsetType, elementSize: MemoryLayout<T>.size)

    do {
        try read { view in checkView(view, reader, input, expected) }
    } catch {
        input.checkError(error, shouldFail: expected == nil)
    }
}

func checkView<T: ArrayElement>(_ view: ArrayView<T>, _ reader: TableReader, _ input: Input, _ expected: Range<Int>?) {
    input.checkIn(view.bytes, within: reader.bytes, expected)
    guard let expected else {
        return
    }

    let size = MemoryLayout<T>.size
    let count = expected.count / size
    require(view.count == count && view.isEmpty == (count == 0) && view.indices == 0..<count)

    func bits(at index: Int) -> UInt64 {
        let start = expected.lowerBound + index * size
        return input.littleEndian(start..<start + size)
    }

    for index in 0..<min(count, checked) {
        require(UInt64(view[index]._taflaBits) == bits(at: index), "Element \(index) is \(view[index])")

        let last = count - 1 - index
        require(UInt64(view[last]._taflaBits) == bits(at: last), "Element \(last) is \(view[last])")
    }

    if count <= checked {
        let copy = Array(view)
        require(copy.count == count, "Copied \(copy.count) of \(count) elements")
        for index in copy.indices {
            require(UInt64(copy[index]._taflaBits) == bits(at: index), "Copied element \(index) as \(copy[index])")
        }
    }

    // In place, the span is the bytes, which an empty span needn't view
    if let span = view.span {
        require(span.count == count, "A span of \(span.count) of \(count) elements")
        if count > 0 {
            input.checkIn(span.bytes, within: reader.bytes, expected)
        }
    }
}

func checkString(_ reader: TableReader, _ input: Input, at position: Int) {
    let utf8 = input.array(at: position, reader.layout.offsetType, elementSize: 1)

    do {
        let string = try reader.string(at: position)
        guard let utf8 else {
            fatalError("Read a string outside the input")
        }

        // A String holds valid UTF-8, so the bytes are valid when it's theirs
        require(string.utf8.elementsEqual(input.bytes[utf8]), "Read \(string) from other bytes")
    } catch .invalidUTF8(let errorPosition) {
        guard let utf8 else {
            fatalError("A string outside the input failed as invalid UTF-8")
        }

        let valid = String(validating: input.bytes[utf8], as: UTF8.self) != nil
        require(errorPosition == position && !valid, "Valid UTF-8 failed at \(errorPosition)")
    } catch {
        input.checkError(error, shouldFail: utf8 == nil)
    }
}

func checkStructs(_ reader: TableReader, _ input: Input, at position: Int) {
    let offsetType = reader.layout.offsetType
    let remaining = max(input.count - max(position, 0), 0)

    // 4 structs of 1 << 62 bytes are 1 << 64 bytes, which a UInt64 wraps to
    // none, so that size finds a byte length that wraps into the input, where
    // Int.max's stays past any bytes however it's multiplied
    for size in [0, 1, 2, 3, 8, 13, remaining, remaining + 1, 1 << 62, Int.max] {
        let expected = input.range(at: position, byteLength: UInt64(size))
        do {
            try input.checkIn(reader.struct(at: position, size: size), within: reader.bytes, expected)
        } catch {
            input.checkError(error, shouldFail: expected == nil)
        }

        let expectedArray = input.array(at: position, offsetType, elementSize: size)
        do {
            try input.checkIn(reader.structArray(at: position, size: size), within: reader.bytes, expectedArray)
        } catch {
            input.checkError(error, shouldFail: expectedArray == nil)
        }
    }
}

/// Reads the offsets of fields of a table nested at `position`, which count
/// from its start
func checkFieldOffsets(_ reader: TableReader, _ input: Input, at position: Int) {
    let offsetType = reader.layout.offsetType

    for index in [0, 1, 2, Int.max] {
        // Past any bytes when it overflows
        let (offset, overflow) = index.multipliedReportingOverflow(by: offsetType.size)
        let (slot, overflowAgain) = position.addingReportingOverflow(offset)
        let fieldOffset = overflow || overflowAgain ? nil : input.position(at: slot, offsetType)

        // The slot lies within the input, so the field's position doesn't
        // overflow
        let expected = position >= 0 ? fieldOffset.map { position + $0 } : nil
        input.checkResult(expected) { () throws(DecodeError) in try reader.fieldOffset(index, inTableAt: position) }
    }
}

/// Reads the vector at `position` and the first and last of its items, by
/// their bytes and in place
func checkVector(_ reader: TableReader, _ input: Input, at position: Int) {
    let offsetType = reader.layout.offsetType

    // A count, and an end for each item after it, within the input
    let count = input.position(at: position, offsetType)
    let ends = count.flatMap { count in
        input.range(at: position + offsetType.size, byteLength: UInt64(count) * UInt64(offsetType.size))
    }

    do {
        let vector = try reader.vector(at: position)
        guard let count, ends != nil else {
            fatalError("Read a vector of \(vector.count) items outside the input")
        }

        require(vector.count == count && vector.isEmpty == (count == 0) && vector.indices == 0..<count)
        for index in 0..<min(count, checked) {
            checkItem(vector, index, reader, input, at: position)
        }
        if count > checked {
            checkItem(vector, count - 1, reader, input, at: position)
        }
    } catch {
        input.checkError(error, shouldFail: ends == nil)
    }
}

/// Checks item `index` of the vector at `position`, which starts where the
/// one before it ends, and the first after the count and the ends
func checkItem(_ vector: Vector, _ index: Int, _ reader: TableReader, _ input: Input, at position: Int) {
    let offsetType = reader.layout.offsetType

    // The count and the ends lie within the input, from the vector's position
    func itemEnd(_ index: Int) -> Int {
        Int(input.slot(at: position + (index + 1) * offsetType.size, offsetType)!)
    }
    let start = index == 0 ? (vector.count + 1) * offsetType.size : itemEnd(index - 1)
    let end = itemEnd(index)
    let expected = start <= end ? input.range(at: position + start, byteLength: UInt64(end - start)) : nil

    var itemError: DecodeError?
    do {
        try input.checkIn(vector.item(at: index), within: reader.bytes, expected)
    } catch .itemEndsBeforeStart(let errorStart, let errorEnd) {
        require(start > end, "Item \(index) from \(start) to \(end) ends before it starts")
        require(errorStart == position + start && errorEnd == position + end, "Item \(index) at \(position)")
        itemError = .itemEndsBeforeStart(start: errorStart, end: errorEnd)
    } catch {
        input.checkError(error, shouldFail: expected == nil)
        itemError = error
    }

    // From its start to the end of the bytes, or the item's error
    do {
        let inPlace = try vector.itemInPlace(at: index)
        guard let expected else {
            fatalError("Item \(index) in place lies outside the input")
        }

        input.checkIn(inPlace, within: reader.bytes, expected.lowerBound..<input.count)
    } catch {
        require(error == itemError, "Item \(index) failed in place with \(error)")
    }
}
