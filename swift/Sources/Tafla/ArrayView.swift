/// An array read from a table: its elements' little-endian bytes, which it
/// reads in place at any alignment and in any host byte order.
///
/// It's a view of the bytes that its `TableReader` reads, and its lifetime
/// depends on theirs, so the compiler keeps it from outliving them. Reading
/// it never allocates; `Array(_:)` copies the elements into an array of
/// their own, which can outlive the bytes.
///
/// ``span`` returns the elements as a `Span` in place when the host is
/// little-endian and the bytes are aligned to the element type in memory, as
/// `TableBuilder` aligns them within an aligned table.
public struct ArrayView<Element: ArrayElement>: ~Escapable {
    /// The elements' little-endian bytes, without the count before them
    public let bytes: RawSpan

    /// Views `bytes`, which must hold a whole number of elements
    @inlinable
    @_lifetime(copy bytes)
    init(bytes: RawSpan) {
        self.bytes = bytes
    }

    /// The number of elements
    @inlinable
    public var count: Int { bytes.byteCount / MemoryLayout<Element>.size }

    @inlinable
    public var isEmpty: Bool { bytes.isEmpty }

    /// The indices of the elements, from 0 to ``count``
    @inlinable
    public var indices: Range<Int> { 0..<count }

    /// Returns the element at `index`, which must be one of ``indices``: an
    /// index outside them is a programming error, which stops the program,
    /// as `Array`'s subscript does.
    @inlinable
    public subscript(index: Int) -> Element {
        precondition(UInt(bitPattern: index) < UInt(count), "Index out of range")
        return Element.load(from: bytes, at: index &* MemoryLayout<Element>.size)
    }

    /// The elements in place, or `nil` when they aren't aligned to their type
    /// in memory or the host is big-endian. Single bytes, and no elements,
    /// are always in place. Like the view, the span can't outlive the bytes.
    @inlinable
    public var span: Span<Element>? {
        @_lifetime(copy self)
        get {
            if bytes.isEmpty {
                return Span()
            }

            guard MemoryLayout<Element>.size == 1 || (elementsInHostOrder && isAligned) else {
                return nil
            }

            return Span(_bytes: bytes)
        }
    }

    /// Whether the bytes start at an address aligned to the element type
    @inlinable
    var isAligned: Bool {
        bytes.withUnsafeBytes { buffer in
            UInt(bitPattern: buffer.baseAddress) % UInt(MemoryLayout<Element>.alignment) == 0
        }
    }
}

extension Array where Element: ArrayElement {
    /// Copies the elements of an array read from a table, in the host's byte
    /// order, into an array of their own
    @inlinable
    public init(_ view: borrowing ArrayView<Element>) {
        let count = view.count
        let bytes = view.bytes

        self.init(unsafeUninitializedCapacity: count) { elements, initialized in
            if elementsInHostOrder {
                // A copy of the bytes, at any alignment
                bytes.withUnsafeBytes { bytes in
                    UnsafeMutableRawBufferPointer(elements).copyMemory(from: bytes)
                }
            } else {
                for index in 0..<count {
                    let element = Element.load(from: bytes, at: index &* MemoryLayout<Element>.size)
                    elements.initializeElement(at: index, to: element)
                }
            }

            initialized = count
        }
    }
}
