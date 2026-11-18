/// A vector read from a table: its items, which are tables.
///
/// The vector holds its count, the end of each item, counted from the
/// vector's position, and then the items. `ByteBuffer.vector(at:)` checks that
/// the ends lie within the bytes, and each item is checked when it's read,
/// so reading one can throw. ``item(at:)`` and ``itemInPlace(at:)`` find any
/// item from its end and the one before it, without reading the others.
///
/// An item that has no header and its parent's offset type is read in
/// place: from the bytes that ``itemInPlace(at:)`` returns, which start where
/// it does and end where its parent's do, with a `ByteBuffer` of its parent's
/// offset type, at position 0. Its reads may reach past its end, as those of
/// any table read in place may. Any other item is read as a table of its
/// own, from the bytes that ``item(at:)`` returns, with a `ByteBuffer` of its
/// own layout.
///
/// Like the `ByteBuffer` that it's read from, a vector and its items' bytes
/// can't outlive the bytes that the buffer reads.
public struct Vector: ~Escapable {
    /// The bytes of the buffer that the vector lies in
    @usableFromInline
    let bytes: RawSpan
    /// The vector's position in the bytes, which is its count's
    @usableFromInline
    let position: Int
    /// The number of items
    public let count: Int
    @usableFromInline
    let offsetType: OffsetType

    /// Views `count` items at `position` in `bytes`, which must have been
    /// checked to hold the count and the ends
    @inlinable
    @_lifetime(copy bytes)
    init(bytes: RawSpan, position: Int, count: Int, offsetType: OffsetType) {
        self.bytes = bytes
        self.position = position
        self.count = count
        self.offsetType = offsetType
    }

    @inlinable
    public var isEmpty: Bool { count == 0 }

    /// The indices of the items, from 0 to ``count``
    @inlinable
    public var indices: Range<Int> { 0..<count }

    /// Returns the bytes of item `index`, from its start to its end, to read
    /// it as a table of its own. Only the ends of the item and the one before
    /// it are read. `index` must be one of ``indices``: an index outside them
    /// is a programming error, which stops the program, as `Array`'s
    /// subscript does.
    ///
    /// Throws ``DecodeError/itemEndsBeforeStart(start:end:)`` if the item ends
    /// before the item before it, or
    /// ``DecodeError/outOfRange(position:byteLength:available:)`` if it ends
    /// past the bytes.
    @inlinable
    @_lifetime(copy self)
    public func item(at index: Int) throws(DecodeError) -> RawSpan {
        let (start, end) = try bounds(of: index)
        return bytes.extracting(unchecked: start..<end)
    }

    /// Returns the bytes of item `index` to read it in place, from its start
    /// to the end of the buffer's bytes. Only the ends of the item and the
    /// one before it are read. `index` must be one of ``indices``, as for
    /// ``item(at:)``.
    ///
    /// Throws as ``item(at:)`` does.
    @inlinable
    @_lifetime(copy self)
    public func itemInPlace(at index: Int) throws(DecodeError) -> RawSpan {
        let (start, _) = try bounds(of: index)
        return bytes.extracting(unchecked: start..<bytes.byteCount)
    }

    /// Where item `index` starts and ends in the bytes, which it lies within
    @inlinable
    func bounds(of index: Int) throws(DecodeError) -> (start: Int, end: Int) {
        precondition(UInt(bitPattern: index) < UInt(count), "Index out of range")

        // The count and the ends lie within the bytes, so the first item's
        // start doesn't overflow
        let width = offsetType.size
        let start = index == 0 ? (count &+ 1) &* width : end(of: index &- 1)
        let end = end(of: index)

        guard start <= end else {
            throw .itemEndsBeforeStart(start: positionInBytes(start), end: positionInBytes(end))
        }

        // The vector lies within the bytes, so the subtraction doesn't
        // overflow
        guard end <= bytes.byteCount &- position else {
            throw .outOfRange(
                position: positionInBytes(start),
                byteLength: UInt64(end &- start),
                available: bytes.byteCount
            )
        }

        return (position &+ start, position &+ end)
    }

    /// Reads the end of item `index`, from the vector's position. The ends
    /// were checked to lie within the bytes. An end that a 32-bit `Int`
    /// can't hold is `Int.max`, past any bytes.
    @inlinable
    func end(of index: Int) -> Int {
        let slot = position &+ (index &+ 1) &* offsetType.size
        let end = loadSlot(bytes, at: slot, offsetType: offsetType)
        return Int(exactly: end) ?? .max
    }

    /// The position in the bytes of `offset` from the vector's position,
    /// which an error reports, or `Int.max` if it overflows, past any bytes
    @inlinable
    func positionInBytes(_ offset: Int) -> Int {
        let (position, overflow) = position.addingReportingOverflow(offset)
        return overflow ? .max : position
    }
}
