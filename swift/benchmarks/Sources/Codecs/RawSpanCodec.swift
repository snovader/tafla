import Workloads

/// For the fixed-size record only: its fields at fixed offsets, written with
/// `UnsafeMutableRawBufferPointer.storeBytes(of:toByteOffset:as:)` and read
/// with `RawSpan.unsafeLoadUnaligned(fromByteOffset:as:)`, the standard
/// library's, which tafla's README recommends for such records
public struct RawSpanCodec: SampleCodec {
    public typealias Bytes = [UInt8]

    /// The record's size: a uint32 and two float32s
    static let size = 12

    public init() {}

    public var name: String { "RawSpan" }

    public func serializeSample(_ sample: Sample) throws -> [UInt8] {
        [UInt8](unsafeUninitializedCapacity: Self.size) { buffer, initialized in
            let bytes = UnsafeMutableRawBufferPointer(buffer)
            bytes.storeBytes(of: sample.key.littleEndian, toByteOffset: 0, as: UInt32.self)
            bytes.storeBytes(of: sample.a.bitPattern.littleEndian, toByteOffset: 4, as: UInt32.self)
            bytes.storeBytes(of: sample.b.bitPattern.littleEndian, toByteOffset: 8, as: UInt32.self)
            initialized = Self.size
        }
    }

    public func deserializeSample(_ bytes: [UInt8]) throws -> Sample {
        let bytes = bytes.span.bytes

        // A load past the bytes would stop the program
        guard bytes.byteCount >= Self.size else {
            throw ShortSample(byteCount: bytes.byteCount)
        }

        return Sample(
            key: UInt32(littleEndian: bytes.unsafeLoadUnaligned(fromByteOffset: 0, as: UInt32.self)),
            a: Float(bitPattern: UInt32(littleEndian: bytes.unsafeLoadUnaligned(fromByteOffset: 4, as: UInt32.self))),
            b: Float(bitPattern: UInt32(littleEndian: bytes.unsafeLoadUnaligned(fromByteOffset: 8, as: UInt32.self)))
        )
    }
}

/// The error of bytes too short to hold the fixed-size record
public struct ShortSample: Error, CustomStringConvertible {
    public let byteCount: Int

    public var description: String { "\(byteCount) bytes can't hold a sample of \(RawSpanCodec.size)" }
}
