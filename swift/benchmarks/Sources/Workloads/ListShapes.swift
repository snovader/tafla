/// A list of strings that the lists suite writes and reads: distinct strings
/// of the same length and mix of UTF-8 widths, as the TypeScript suite's
/// `strings/lists/shapes.ts` makes them, with the same values
public struct ListShape: Sendable {
    /// The number of strings and their kind, such as
    /// `10,000 × 16 chars of ASCII`
    public let name: String
    public let strings: [String]
}

/// The numbers of strings in each kind's lists
public let listCounts = [10, 100, 10_000]

/// The lists of 10, 100 and 10,000 strings of each kind: 16 and 256
/// characters of ASCII, and 8 characters of 3 bytes in UTF-8. These are 3 of
/// the TypeScript suite's 8 kinds, and each list takes the seed that
/// TypeScript gives it, from the kind's index among them.
public let listShapes: [ListShape] = listCounts.enumerated().flatMap { countIndex, count in
    StringKind.timed.map { kind in kind.shape(count: count, countIndex: countIndex) }
}

/// A kind of string, by the length and the characters of its strings
struct StringKind: Sendable {
    let name: String
    /// The kind's index among the TypeScript suite's kinds, which seeds its
    /// lists
    let index: Int
    /// The number of characters in each string, of a code point each
    let length: Int
    /// Draws a character
    let pick: @Sendable (inout Xorshift) -> Unicode.Scalar

    /// The kinds that the suite times
    static let timed = [
        StringKind(name: "16 chars of ASCII", index: 1, length: 16, pick: ascii),
        StringKind(name: "256 chars of ASCII", index: 2, length: 256, pick: ascii),
        StringKind(name: "8 chars of 3 bytes", index: 6, length: 8, pick: threeBytes),
    ]

    /// The number of the TypeScript suite's kinds, which spaces the seeds of
    /// each count's lists
    static let typeScriptKinds = 8

    /// The list of `count` strings, the `countIndex`th count, with a
    /// generator of its own
    func shape(count: Int, countIndex: Int) -> ListShape {
        var random = Xorshift(seed: 1 + index + Self.typeScriptKinds * countIndex)

        var strings: [String] = []
        strings.reserveCapacity(count)
        for _ in 0..<count {
            var scalars = String.UnicodeScalarView()
            for _ in 0..<length {
                scalars.append(pick(&random))
            }
            strings.append(String(scalars))
        }

        precondition(Set(strings).count == count, "The strings of \(name) aren't distinct")
        return ListShape(name: "\(thousands(count)) × \(name)", strings: strings)
    }
}

/// The characters of ASCII that the TypeScript suite picks from
private let asciiCharacters = Array("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".unicodeScalars)

/// A character of ASCII, as `Math.floor(random() * ASCII.length)` picks it
private func ascii(_ random: inout Xorshift) -> Unicode.Scalar {
    asciiCharacters[Int((random.next() * Double(asciiCharacters.count)).rounded(.down))]
}

/// One of the first 0x5000 CJK ideographs, from U+4E00, of 3 bytes in UTF-8
private func threeBytes(_ random: inout Xorshift) -> Unicode.Scalar {
    Unicode.Scalar(UInt32(0x4e00 + Int((random.next() * 0x5000).rounded(.down))))!
}
