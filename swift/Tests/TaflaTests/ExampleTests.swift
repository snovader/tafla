import Foundation
import Tafla
import Testing

// The example of README.md, which writes a table and reads it back

@Suite("Example")
struct ExampleTests {
    // A format's header, shared by its writer and its readers
    let layout = TableLayout(sized: true, versioned: true)
    let formatVersion: UInt32 = 1

    func write() throws -> [UInt8] {
        var series = try TableBuilder(fields: 2)
        try series.setUInt32Array([0, 86_400])
        try series.setFloat32Array([21.75, 18.5])

        var builder = try TableBuilder(fields: 3, layout: layout, version: formatVersion)
        try builder.setFloat64(59.3293)
        try builder.setString("Europe/Stockholm")
        try builder.setTable(.builder(series))  // written straight into the table
        return try builder.bytes()
    }

    @Test func readsData() throws {
        let data = Data(try write())

        let buffer = ByteBuffer(data.bytes, layout: layout)  // a view of data's bytes
        let version = try buffer.version
        guard version == formatVersion else {
            throw DecodeError.unsupportedVersion(version: version, supported: formatVersion)
        }

        let timeZone = try buffer.string(at: buffer.rootOffset(1))  // a String of its own
        let table = try buffer.rootOffset(2)
        let values = try buffer.float32Array(at: buffer.fieldOffset(1, inTableAt: table))  // ArrayView<Float>
        let second = values[1]  // read in place
        let copy = Array(values)  // [Float], which can outlive data

        #expect(timeZone == "Europe/Stockholm")
        #expect(second == 18.5)
        #expect(copy == [21.75, 18.5])
        #expect(values.span?[0] == (isLittleEndianHost ? 21.75 : nil))
    }

    // An array's span is available from Apple's systems of 2025 on, and an
    // array slice's on every system
    @Test func readsAnArray() throws {
        let bytes = try write()
        let all = bytes[...]

        let buffer = ByteBuffer(all.span.bytes, layout: layout)
        #expect(try buffer.float64(at: buffer.rootOffset(0)) == 59.3293)

        if #available(macOS 26, iOS 26, tvOS 26, watchOS 26, visionOS 26, *) {
            let buffer = ByteBuffer(bytes.span.bytes, layout: layout)
            #expect(try buffer.string(at: buffer.rootOffset(1)) == "Europe/Stockholm")
        }
    }
}
