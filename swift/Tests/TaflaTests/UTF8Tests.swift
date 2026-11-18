import Testing

@testable import Tafla

// TableReader validates strings with the standard library where its runtime
// can, from Swift 6.0's on, and with a validator of its own on older Apple
// systems, which CI's hosts don't run. The validator must accept exactly the
// bytes that the standard library accepts.

@Suite("UTF-8")
struct UTF8Tests {
    @available(macOS 15, iOS 18, tvOS 18, watchOS 11, visionOS 2, *)
    @Test func validatesWhatTheStandardLibraryValidates() async {
        await checkDrawn { draw in
            draw.array(most: 24) { draw in
                draw.weighted([
                    (3, { draw in Array(String(draw.character()).utf8) }),
                    (1, { [$0.integer()] }),
                    (
                        1,
                        {
                            [
                                $0.element(of: [
                                    0x80, 0xbf, 0xc0, 0xc1, 0xc2, 0xdf, 0xe0, 0xed, 0xef, 0xf0, 0xf4, 0xf5, 0xff,
                                ])
                            ]
                        }
                    ),
                ])
            }
            .flatMap { $0 }
        } _: { bytes in
            bytes.withUnsafeBytes { bytes in
                let expected = String(validating: bytes, as: UTF8.self)
                #expect(isValidUTF8(bytes) == (expected != nil))
                #expect(decodeUTF8(bytes) == expected)
            }
        }
    }

    @Test(
        arguments: [
            ([], true),
            ([0x7f], true),
            ([0xc2, 0x80], true),
            ([0xdf, 0xbf], true),
            ([0xe0, 0xa0, 0x80], true),
            ([0xed, 0x9f, 0xbf], true),  // U+D7FF, the last before the surrogates
            ([0xee, 0x80, 0x80], true),  // U+E000, the first after them
            ([0xef, 0xbb, 0xbf], true),  // U+FEFF
            ([0xf0, 0x90, 0x80, 0x80], true),
            ([0xf4, 0x8f, 0xbf, 0xbf], true),  // U+10FFFF
            ([0x80], false),
            ([0xc1, 0xbf], false),
            ([0xe0, 0x9f, 0xbf], false),
            ([0xed, 0xa0, 0x80], false),
            ([0xf0, 0x8f, 0xbf, 0xbf], false),
            ([0xf4, 0x90, 0x80, 0x80], false),
            ([0xf5, 0x80, 0x80, 0x80], false),
            ([0xe2, 0x82], false),
            ([0xe2, 0x82, 0x41], false),
            (Array("eight by".utf8) + [0xff], false),
        ] as [([UInt8], Bool)])
    func validatesTheBoundariesOfEachSequence(_ bytes: [UInt8], _ valid: Bool) {
        bytes.withUnsafeBytes { bytes in
            #expect(isValidUTF8(bytes) == valid)
        }
    }
}
