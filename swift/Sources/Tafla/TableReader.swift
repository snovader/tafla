/// Reads a table from the bytes it was given, in the layout it was written
/// with.
///
/// Fields are found by index with ``rootOffset(_:)``, or
/// ``fieldOffset(_:inTableAt:)`` for a table nested in another, and then read
/// at that position by their type's getter.
///
/// Every read checks that it stays within the bytes and throws a
/// ``DecodeError`` otherwise, so no bytes, however corrupted, make a read
/// stop the program or return values from outside them. Strings must be
/// valid UTF-8.
///
/// A reader is a view of bytes that its caller owns, such as an array's or
/// `Data`'s, and arrays, structs and vector items are views of the same
/// bytes rather than copies. Their lifetimes depend on the bytes', so the
/// compiler keeps every view from outliving them:
///
/// ```swift
/// let reader = TableReader(data.bytes, layout: layout)
/// let values = try reader.float32Array(at: reader.rootOffset(2))
/// let copy = Array(values)  // [Float], which can outlive data
/// ```
///
/// Strings are copied, since a `String` owns its bytes.
public struct TableReader: ~Escapable {
    /// The bytes that the reader reads
    public let bytes: RawSpan
    /// The layout that the table was written with
    public let layout: TableLayout

    /// Reads a table of `layout` from `bytes`, which the reader, and the
    /// views that it returns, can't outlive
    @inlinable
    @_lifetime(copy bytes)
    public init(_ bytes: RawSpan, layout: TableLayout = .default) {
        self.bytes = bytes
        self.layout = layout
    }

    /// The table's byte length from its header when the layout is sized, and
    /// otherwise the length of the bytes.
    ///
    /// Throws ``DecodeError/outOfRange(position:byteLength:available:)`` if
    /// the header doesn't lie within the bytes.
    @inlinable
    public var size: Int {
        get throws(DecodeError) {
            layout.isSized ? try readPosition(at: 0) : bytes.byteCount
        }
    }

    /// The version that the table's header carries after its length. It's as
    /// wide as the header's other slots. Reading the version of a layout
    /// without one is a programming error, which stops the program.
    ///
    /// Throws ``DecodeError/outOfRange(position:byteLength:available:)`` if
    /// the header doesn't lie within the bytes.
    @inlinable
    public var version: UInt32 {
        get throws(DecodeError) {
            precondition(layout.isVersioned, "The layout has no version")
            return try readSlot(at: layout.isSized ? layout.offsetSize : 0)
        }
    }

    /// Returns the position of field `index` of the root table, which must
    /// not be negative.
    ///
    /// Throws ``DecodeError/outOfRange(position:byteLength:available:)`` if
    /// the field's offset doesn't lie within the bytes.
    @inlinable
    public func rootOffset(_ index: Int) throws(DecodeError) -> Int {
        precondition(index >= 0, "A negative field index")

        let slot = saturatingSlot(of: index, from: layout.headerSize)
        return try readPosition(at: slot)
    }

    /// Returns the position of field `index`, which must not be negative, of
    /// a nested table that's read in place: it has no header and stores its
    /// field offsets from its own start, as wide as the reader's.
    /// `tablePosition` is the nested table's position, which is the position
    /// of the field that holds it.
    ///
    /// Throws ``DecodeError/outOfRange(position:byteLength:available:)`` if
    /// the field's offset doesn't lie within the bytes.
    @inlinable
    public func fieldOffset(_ index: Int, inTableAt tablePosition: Int) throws(DecodeError) -> Int {
        precondition(index >= 0, "A negative field index")

        guard tablePosition >= 0 else {
            throw outOfRange(tablePosition, layout.offsetSize)
        }

        let offset = try readPosition(at: saturatingSlot(of: index, from: tablePosition))

        // Only a table's position that the caller made up overflows, past
        // any bytes
        let (position, overflow) = tablePosition.addingReportingOverflow(offset)
        guard !overflow else {
            throw outOfRange(tablePosition, offset)
        }

        return position
    }

    @inlinable
    public func int8(at position: Int) throws(DecodeError) -> Int8 {
        try scalar(at: position)
    }

    @inlinable
    public func uint8(at position: Int) throws(DecodeError) -> UInt8 {
        try scalar(at: position)
    }

    @inlinable
    public func int16(at position: Int) throws(DecodeError) -> Int16 {
        try scalar(at: position)
    }

    @inlinable
    public func uint16(at position: Int) throws(DecodeError) -> UInt16 {
        try scalar(at: position)
    }

    @inlinable
    public func int32(at position: Int) throws(DecodeError) -> Int32 {
        try scalar(at: position)
    }

    @inlinable
    public func uint32(at position: Int) throws(DecodeError) -> UInt32 {
        try scalar(at: position)
    }

    @inlinable
    public func int64(at position: Int) throws(DecodeError) -> Int64 {
        try scalar(at: position)
    }

    @inlinable
    public func uint64(at position: Int) throws(DecodeError) -> UInt64 {
        try scalar(at: position)
    }

    @inlinable
    public func float32(at position: Int) throws(DecodeError) -> Float {
        try scalar(at: position)
    }

    @inlinable
    public func float64(at position: Int) throws(DecodeError) -> Double {
        try scalar(at: position)
    }

    /// Reads the scalar of type `T` at `position`, as the getter of its type
    /// does.
    ///
    /// Throws ``DecodeError/outOfRange(position:byteLength:available:)`` if
    /// the value doesn't lie within the bytes.
    @inlinable
    public func scalar<T: ArrayElement>(at position: Int, as type: T.Type = T.self) throws(DecodeError) -> T {
        try check(position, MemoryLayout<T>.size)
        return T.load(from: bytes, at: position)
    }

    @inlinable
    @_lifetime(copy self)
    public func int8Array(at position: Int) throws(DecodeError) -> ArrayView<Int8> {
        try array(at: position)
    }

    @inlinable
    @_lifetime(copy self)
    public func uint8Array(at position: Int) throws(DecodeError) -> ArrayView<UInt8> {
        try array(at: position)
    }

    @inlinable
    @_lifetime(copy self)
    public func int16Array(at position: Int) throws(DecodeError) -> ArrayView<Int16> {
        try array(at: position)
    }

    @inlinable
    @_lifetime(copy self)
    public func uint16Array(at position: Int) throws(DecodeError) -> ArrayView<UInt16> {
        try array(at: position)
    }

    @inlinable
    @_lifetime(copy self)
    public func int32Array(at position: Int) throws(DecodeError) -> ArrayView<Int32> {
        try array(at: position)
    }

    @inlinable
    @_lifetime(copy self)
    public func uint32Array(at position: Int) throws(DecodeError) -> ArrayView<UInt32> {
        try array(at: position)
    }

    @inlinable
    @_lifetime(copy self)
    public func int64Array(at position: Int) throws(DecodeError) -> ArrayView<Int64> {
        try array(at: position)
    }

    @inlinable
    @_lifetime(copy self)
    public func uint64Array(at position: Int) throws(DecodeError) -> ArrayView<UInt64> {
        try array(at: position)
    }

    @inlinable
    @_lifetime(copy self)
    public func float32Array(at position: Int) throws(DecodeError) -> ArrayView<Float> {
        try array(at: position)
    }

    @inlinable
    @_lifetime(copy self)
    public func float64Array(at position: Int) throws(DecodeError) -> ArrayView<Double> {
        try array(at: position)
    }

    /// Reads the array of `T` at `position`, as the getter of its type does.
    ///
    /// Throws ``DecodeError/outOfRange(position:byteLength:available:)`` if
    /// the array doesn't lie within the bytes.
    @inlinable
    @_lifetime(copy self)
    public func array<T: ArrayElement>(at position: Int, of type: T.Type = T.self) throws(DecodeError)
        -> ArrayView<T>
    {
        ArrayView(bytes: try arrayBytes(at: position, elementSize: MemoryLayout<T>.size))
    }

    /// Reads the string at `position`, which must be valid UTF-8, into a
    /// `String` of its own. A leading U+FEFF is part of the string rather
    /// than a byte order mark.
    ///
    /// Throws ``DecodeError/outOfRange(position:byteLength:available:)`` if
    /// the string doesn't lie within the bytes, or
    /// ``DecodeError/invalidUTF8(position:)`` if it isn't valid UTF-8.
    public func string(at position: Int) throws(DecodeError) -> String {
        let utf8 = try arrayBytes(at: position, elementSize: 1)
        return try decodeString(utf8, at: position)
    }

    /// Reads the `size` bytes, which mustn't be negative, of the struct at
    /// `position`, whose scalars lie at fixed offsets from its start.
    ///
    /// Throws ``DecodeError/outOfRange(position:byteLength:available:)`` if
    /// the struct doesn't lie within the bytes.
    @inlinable
    @_lifetime(copy self)
    public func `struct`(at position: Int, size: Int) throws(DecodeError) -> RawSpan {
        precondition(size >= 0, "A negative struct size")

        try check(position, size)
        return bytes.extracting(unchecked: position..<position &+ size)
    }

    /// Reads the bytes of the structs of the array at `position`, each of
    /// `size` bytes, which mustn't be negative, one after another after the
    /// count. Their number is the bytes' length over `size`. Structs of 0
    /// bytes take none, so a `size` of 0 returns no bytes, whatever the
    /// count.
    ///
    /// Throws ``DecodeError/outOfRange(position:byteLength:available:)`` if
    /// the array doesn't lie within the bytes.
    @inlinable
    @_lifetime(copy self)
    public func structArray(at position: Int, size: Int) throws(DecodeError) -> RawSpan {
        precondition(size >= 0, "A negative struct size")

        return try arrayBytes(at: position, elementSize: size)
    }

    /// Reads the vector at `position`, checking that its count and the ends
    /// of its items lie within the bytes. Each item is checked when it's
    /// read.
    ///
    /// Throws ``DecodeError/outOfRange(position:byteLength:available:)`` if
    /// the count or the ends don't lie within the bytes.
    @inlinable
    @_lifetime(copy self)
    public func vector(at position: Int) throws(DecodeError) -> Vector {
        let count = try readPosition(at: position)

        // Every item has an end, so this rejects an impossible count before
        // any item is read. The count lies within the bytes, so the ends'
        // position doesn't overflow, nor does their length in a UInt64.
        let width = layout.offsetSize
        try check(position &+ width, UInt64(count) &* UInt64(width))

        return Vector(bytes: bytes, position: position, count: count, offsetType: layout.offsetType)
    }

    /// Reads the vector of strings at `position`, checking that its count and
    /// the ends of its strings lie within the bytes, as ``vector(at:)``
    /// checks a vector's. Each string is checked when it's read.
    ///
    /// Throws ``DecodeError/outOfRange(position:byteLength:available:)`` if
    /// the count or the ends don't lie within the bytes.
    @inlinable
    @_lifetime(copy self)
    public func stringVector(at position: Int) throws(DecodeError) -> StringVector {
        StringVector(try vector(at: position))
    }

    /// Returns the bytes of the elements after the count at `position`
    @inlinable
    @_lifetime(copy self)
    func arrayBytes(at position: Int, elementSize: Int) throws(DecodeError) -> RawSpan {
        let count = try readPosition(at: position)

        // The count lies within the bytes, so this doesn't overflow. A struct
        // can be as large as an Int, and a byte length past a UInt64 lies
        // past any bytes, so the product saturates.
        let start = position &+ layout.offsetSize
        let (byteLength, overflow) = UInt64(count).multipliedReportingOverflow(by: UInt64(elementSize))
        let length = overflow ? UInt64.max : byteLength

        try check(start, length)
        return bytes.extracting(unchecked: start..<start &+ Int(truncatingIfNeeded: length))
    }

    /// Reads a length or offset at `position`, as an `Int`
    @inlinable
    func readPosition(at position: Int) throws(DecodeError) -> Int {
        let value = try readSlot(at: position)

        // Only a 32-bit Int can't hold it, and no bytes reach it
        guard let size = Int(exactly: value) else {
            throw .outOfRange(position: position, byteLength: UInt64(value), available: bytes.byteCount)
        }

        return size
    }

    /// Reads a length, offset or header slot at `position`
    @inlinable
    func readSlot(at position: Int) throws(DecodeError) -> UInt32 {
        try check(position, layout.offsetSize)
        return loadSlot(bytes, at: position, offsetType: layout.offsetType)
    }

    /// The position of the slot of field `index`, after `start`, or
    /// `Int.max`, past any bytes, if it overflows
    @inlinable
    func saturatingSlot(of index: Int, from start: Int) -> Int {
        let (offset, overflow) = index.multipliedReportingOverflow(by: layout.offsetSize)
        let (slot, overflowAgain) = start.addingReportingOverflow(offset)
        return overflow || overflowAgain ? .max : slot
    }

    /// Checks that `byteLength` bytes at `position` lie within the bytes
    @inlinable
    func check(_ position: Int, _ byteLength: Int) throws(DecodeError) {
        // The difference can't overflow, since both are non-negative
        guard position >= 0, position <= bytes.byteCount, byteLength <= bytes.byteCount &- position else {
            throw outOfRange(position, byteLength)
        }
    }

    @inlinable
    func check(_ position: Int, _ byteLength: UInt64) throws(DecodeError) {
        guard position >= 0, position <= bytes.byteCount,
            byteLength <= UInt64(bytes.byteCount &- position)
        else {
            throw .outOfRange(position: position, byteLength: byteLength, available: bytes.byteCount)
        }
    }

    @inlinable
    func outOfRange(_ position: Int, _ byteLength: Int) -> DecodeError {
        .outOfRange(position: position, byteLength: UInt64(byteLength), available: bytes.byteCount)
    }
}

/// Reads a length, offset or header slot as wide as `offsetType` at
/// `position`, which `bytes` must hold
@inlinable
@inline(__always)
func loadSlot(_ bytes: RawSpan, at position: Int, offsetType: OffsetType) -> UInt32 {
    switch offsetType {
    case .uint8:
        UInt32(bytes.unsafeLoadUnaligned(fromUncheckedByteOffset: position, as: UInt8.self))
    case .uint16:
        UInt32(UInt16(littleEndian: bytes.unsafeLoadUnaligned(fromUncheckedByteOffset: position, as: UInt16.self)))
    case .uint32:
        UInt32(littleEndian: bytes.unsafeLoadUnaligned(fromUncheckedByteOffset: position, as: UInt32.self))
    }
}
