/// Returns the string of UTF-8 `bytes`, or `nil` if they aren't valid UTF-8.
///
/// Where the standard library validates and copies in one pass, from Swift
/// 6.0's runtime on, this calls it. On older Apple systems, it validates the
/// bytes first, since `String(decoding:as:)` replaces invalid bytes rather
/// than rejecting them.
func decodeUTF8(_ bytes: UnsafeRawBufferPointer) -> String? {
    if #available(macOS 15, iOS 18, tvOS 18, watchOS 11, visionOS 2, *) {
        return String(validating: bytes, as: UTF8.self)
    }

    guard isValidUTF8(bytes) else {
        return nil
    }

    return String(decoding: bytes, as: UTF8.self)
}

/// Returns the string of UTF-8 `bytes`, or `nil` if they aren't valid UTF-8,
/// as ``decodeUTF8(_:)`` does.
///
/// Where the standard library has `UTF8Span`, from Swift 6.2's runtime on,
/// it validates the bytes as one, and copies them into a `String` without
/// validating them again. `String(validating:as:)` takes about twice as long
/// for each string: on an M2 Pro in macOS 27, 96 rather than 43 ns for a
/// string of 16 bytes of ASCII, 113 rather than 61 ns for 256 bytes, and 112
/// rather than 59 ns for 8 characters of 3 bytes. Validating a vector's
/// strings as one run, and then checking that each string's end starts a
/// character, saved 2 to 3 ns more a string, which didn't justify a second
/// way of reading a string.
@usableFromInline
func validatedString(_ bytes: RawSpan) -> String? {
    if #available(macOS 26, iOS 26, tvOS 26, watchOS 26, visionOS 26, *) {
        guard let utf8 = try? UTF8Span(validating: Span<UInt8>(_bytes: bytes)) else {
            return nil
        }

        return String(copying: utf8)
    }

    return bytes.withUnsafeBytes(decodeUTF8)
}

/// Returns whether `bytes` are valid UTF-8, as Unicode's table 3-7 of
/// well-formed byte sequences defines it: without overlong encodings,
/// surrogates, code points past U+10FFFF or truncated sequences.
func isValidUTF8(_ bytes: UnsafeRawBufferPointer) -> Bool {
    let count = bytes.count
    var index = 0

    while index < count {
        // ASCII 8 bytes at a time, at any alignment
        while index &+ 8 <= count,
            bytes.loadUnaligned(fromByteOffset: index, as: UInt64.self) & 0x8080_8080_8080_8080 == 0
        {
            index &+= 8
        }
        guard index < count else {
            break
        }

        let lead = bytes[index]
        if lead < 0x80 {
            index &+= 1
            continue
        }

        // The sequence's length, and the range of its second byte, which
        // rules out overlong encodings, surrogates and code points past
        // U+10FFFF; the bytes after it are continuation bytes
        let length: Int
        var second: ClosedRange<UInt8> = 0x80...0xbf
        switch lead {
        case 0xc2...0xdf:
            length = 2
        case 0xe0:
            length = 3
            second = 0xa0...0xbf
        case 0xe1...0xec, 0xee...0xef:
            length = 3
        case 0xed:
            length = 3
            second = 0x80...0x9f
        case 0xf0:
            length = 4
            second = 0x90...0xbf
        case 0xf1...0xf3:
            length = 4
        case 0xf4:
            length = 4
            second = 0x80...0x8f
        default:
            return false
        }

        guard count &- index >= length, second.contains(bytes[index &+ 1]) else {
            return false
        }
        for continuation in 2..<length where bytes[index &+ continuation] & 0xc0 != 0x80 {
            return false
        }

        index &+= length
    }

    return true
}
