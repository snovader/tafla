/// A number type that tables store in arrays and as scalars: `Int8`, `UInt8`,
/// `Int16`, `UInt16`, `Int32`, `UInt32`, `Int64`, `UInt64`, `Float` or
/// `Double`, the format's `int8` to `uint64`, `float32` and `float64`.
/// Elements are stored little-endian, and arrays of them are aligned to their
/// size from the table's start.
///
/// Only these ten types conform. Its requirements, which give an element's
/// bits, are for the runtime.
public protocol ArrayElement: BitwiseCopyable, Sendable {
    /// The unsigned integer as wide as the element, whose bits it is
    associatedtype _TaflaBits: FixedWidthInteger & UnsignedInteger & BitwiseCopyable

    init(_taflaBits bits: _TaflaBits)

    var _taflaBits: _TaflaBits { get }
}

extension ArrayElement {
    /// Reads the element from its little-endian bytes at `offset` in
    /// `bytes`, at any alignment, as a struct's scalars lie at fixed offsets
    /// from its start, which is how the code that tafla-gen generates reads
    /// them. `bytes` must hold them there: a read past them is a programming
    /// error, which stops the program, as `RawSpan`'s loads do.
    @inlinable
    public init(littleEndianBytes bytes: RawSpan, at offset: Int) {
        let bits = bytes.unsafeLoadUnaligned(fromByteOffset: offset, as: _TaflaBits.self)
        self.init(_taflaBits: _TaflaBits(littleEndian: bits))
    }

    /// The element's size, which arrays of it are aligned to
    @inlinable
    static var alignment: Alignment {
        // The ten types are 1, 2, 4 or 8 bytes
        Alignment(rawValue: MemoryLayout<Self>.size).unsafelyUnwrapped
    }

    /// Reads the element from its little-endian bytes at `offset`, which
    /// `bytes` must hold
    @inlinable
    @inline(__always)
    static func load(from bytes: RawSpan, at offset: Int) -> Self {
        let bits = bytes.unsafeLoadUnaligned(fromUncheckedByteOffset: offset, as: _TaflaBits.self)
        return Self(_taflaBits: _TaflaBits(littleEndian: bits))
    }

    /// Writes the element's little-endian bytes at `offset`, which `bytes`
    /// must hold
    @inlinable
    @inline(__always)
    func store(to bytes: UnsafeMutableRawBufferPointer, at offset: Int) {
        bytes.storeBytes(of: _taflaBits.littleEndian, toByteOffset: offset, as: _TaflaBits.self)
    }
}

/// Whether the host stores elements in memory as their little-endian bytes,
/// so that arrays are read in place and copied in bulk. The compilation
/// condition `TAFLA_PORTABLE` turns it off, so that every array is read and
/// written element by element, through the path that a big-endian host
/// takes, which `swift test -Xswiftc -DTAFLA_PORTABLE` tests on any host.
@inlinable
var elementsInHostOrder: Bool {
    #if _endian(little) && !TAFLA_PORTABLE
        true
    #else
        false
    #endif
}

extension Int8: ArrayElement {
    @inlinable
    public init(_taflaBits bits: UInt8) { self.init(bitPattern: bits) }

    @inlinable
    public var _taflaBits: UInt8 { UInt8(bitPattern: self) }
}

extension UInt8: ArrayElement {
    @inlinable
    public init(_taflaBits bits: UInt8) { self = bits }

    @inlinable
    public var _taflaBits: UInt8 { self }
}

extension Int16: ArrayElement {
    @inlinable
    public init(_taflaBits bits: UInt16) { self.init(bitPattern: bits) }

    @inlinable
    public var _taflaBits: UInt16 { UInt16(bitPattern: self) }
}

extension UInt16: ArrayElement {
    @inlinable
    public init(_taflaBits bits: UInt16) { self = bits }

    @inlinable
    public var _taflaBits: UInt16 { self }
}

extension Int32: ArrayElement {
    @inlinable
    public init(_taflaBits bits: UInt32) { self.init(bitPattern: bits) }

    @inlinable
    public var _taflaBits: UInt32 { UInt32(bitPattern: self) }
}

extension UInt32: ArrayElement {
    @inlinable
    public init(_taflaBits bits: UInt32) { self = bits }

    @inlinable
    public var _taflaBits: UInt32 { self }
}

extension Int64: ArrayElement {
    @inlinable
    public init(_taflaBits bits: UInt64) { self.init(bitPattern: bits) }

    @inlinable
    public var _taflaBits: UInt64 { UInt64(bitPattern: self) }
}

extension UInt64: ArrayElement {
    @inlinable
    public init(_taflaBits bits: UInt64) { self = bits }

    @inlinable
    public var _taflaBits: UInt64 { self }
}

// Floats are converted through their bits, so that NaNs keep their payloads
// and signs, and -0 its sign
extension Float: ArrayElement {
    @inlinable
    public init(_taflaBits bits: UInt32) { self.init(bitPattern: bits) }

    @inlinable
    public var _taflaBits: UInt32 { bitPattern }
}

extension Double: ArrayElement {
    @inlinable
    public init(_taflaBits bits: UInt64) { self.init(bitPattern: bits) }

    @inlinable
    public var _taflaBits: UInt64 { bitPattern }
}
