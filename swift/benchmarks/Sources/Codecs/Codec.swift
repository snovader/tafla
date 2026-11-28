// The workloads' shapes encoded with tafla and with the libraries that the
// benchmarks compare it with, each as its documentation shows, and as the
// other languages' comparisons use their counterparts.
//
// Each codec writes a workload's value into new bytes of the library's own
// type, and reads it back with every field, into the workloads' types, which
// own their strings and columns.

import Workloads

/// The bytes that a library writes a workload into, and reads it from
public protocol EncodedBytes {
    /// The number of bytes
    var count: Int { get }

    /// The first `count` bytes, as of a message cut short
    func truncated(to count: Int) -> Self
}

extension Array: EncodedBytes where Element == UInt8 {
    public func truncated(to count: Int) -> [UInt8] {
        Array(prefix(count))
    }
}

/// A library that encodes some of the workloads
public protocol Codec {
    /// The bytes that the library writes, which its readers read
    associatedtype Bytes: EncodedBytes

    /// The library's name, which names its benchmarks
    var name: String { get }
}

/// Encodes the record
public protocol RecordCodec: Codec {
    func serializeRecord(_ record: Record) throws -> Bytes
    func deserializeRecord(_ bytes: Bytes) throws -> Record
}

/// Encodes the documents, and reads a column one element at a time
public protocol DocumentCodec: Codec {
    func serializeDocument(_ document: Document) throws -> Bytes
    func deserializeDocument(_ bytes: Bytes) throws -> Document

    /// Sums column `a` through the library's own per-element accessor, in the
    /// order of `sumColumn(_:)`
    func sumA(_ bytes: Bytes) throws -> Double
}

/// Encodes the vector of nested tables
public protocol ListCodec: Codec {
    func serializeList(_ list: List) throws -> Bytes
    func deserializeList(_ bytes: Bytes) throws -> List
}

/// Encodes the same items as an array of structs
public protocol StructsCodec: Codec {
    func serializeStructs(_ list: List) throws -> Bytes
    func deserializeStructs(_ bytes: Bytes) throws -> List
}

/// Encodes the fixed-size record
public protocol SampleCodec: Codec {
    func serializeSample(_ sample: Sample) throws -> Bytes
    func deserializeSample(_ bytes: Bytes) throws -> Sample
}

/// A library that encodes every workload, as all but the fixed-size record's
/// alternative do
public typealias SuiteCodec = RecordCodec & DocumentCodec & ListCodec & StructsCodec & SampleCodec

/// The error of a codec that reads bytes without a field that every workload
/// has, which FlatBuffers and protobuf read as absent
public struct MissingField: Error, CustomStringConvertible {
    public let name: String

    public var description: String { "The bytes have no \(name)" }
}
