/// A vector of strings read from a table, which reads each string when it's
/// asked for.
///
/// The vector holds its count, the end of each string, counted from the
/// vector's position, and then the strings' UTF-8, one after another, as a
/// ``Vector`` holds its items. `TableReader.stringVector(at:)` checks that
/// the ends lie within the bytes, and each string is checked when it's read,
/// so reading one can throw. The subscript finds any string from its end and
/// the one before it, without reading the others, and `Array(_:)` reads
/// every string.
///
/// Each string is checked as UTF-8 on its own, from its own ends, so an end
/// that splits a character fails both strings around it. Strings are
/// copied, since a `String` owns its bytes.
///
/// Like the `TableReader` that it's read from, a vector can't outlive the
/// bytes that the reader reads. The code that tafla-gen generates returns one
/// from the getter of a `[string]` field.
public struct StringVector: ~Escapable {
    /// The vector, whose items are the strings' UTF-8
    @usableFromInline
    let vector: Vector

    /// Views the strings of `vector`, whose count and ends were checked to lie
    /// within its bytes
    @inlinable
    @_lifetime(copy vector)
    init(_ vector: Vector) {
        self.vector = vector
    }

    /// The number of strings
    @inlinable
    public var count: Int { vector.count }

    @inlinable
    public var isEmpty: Bool { vector.isEmpty }

    /// The indices of the strings, from 0 to ``count``
    @inlinable
    public var indices: Range<Int> { vector.indices }

    /// Reads string `index`, from its end and the end of the string before
    /// it, without reading the others. `index` must be one of ``indices``: an
    /// index outside them is a programming error, which stops the program, as
    /// `Array`'s subscript does.
    ///
    /// Throws ``DecodeError/itemEndsBeforeStart(start:end:)`` if the string
    /// ends before the string before it,
    /// ``DecodeError/outOfRange(position:byteLength:available:)`` if it ends
    /// past the bytes, or ``DecodeError/invalidUTF8(position:)`` if it isn't
    /// valid UTF-8 on its own.
    @inlinable
    public subscript(index: Int) -> String {
        get throws(DecodeError) {
            let (start, end) = try vector.bounds(of: index)
            return try decodeString(vector.bytes.extracting(unchecked: start..<end), at: start)
        }
    }
}

extension Array where Element == String {
    /// Reads every string of a vector of strings, in order, into an array of
    /// their own, as reading each with the vector's subscript does.
    ///
    /// Throws the error of the first string that reading on its own throws
    /// for, as the vector's subscript throws it.
    public init(_ view: borrowing StringVector) throws(DecodeError) {
        // Each string from its ends, as the subscript reads it, since a
        // string's UTF-8 is validated as it's copied, which leaves nothing
        // for reading the strings' bytes as one run to save
        var strings: [String] = []
        strings.reserveCapacity(view.count)
        for index in view.indices {
            strings.append(try view[index])
        }

        self = strings
    }
}
