import Tafla

// Reads mutated tables as every valid table of the conformance suite
// describes them, following their offsets into nested tables, vectors,
// arrays, strings and structs, as the conformance test reads them, and as
// Rust's conformance target and Go's FuzzConformance do.
//
// Cases.swift, Description.swift and Read.swift are links to the test
// target's, which parse the suite and read a table as its description sets
// it out, rather than a second model of tables. The links lie as deep in the
// repository as the files, so that Cases.swift finds the suite from its own
// path either way.
//
// Reads may fail, but only with the decode error, and every way of reading
// an array, or a vector's item, must agree. A trap is a finding, and so is
// AddressSanitizer's report of a read outside the bytes.

@_cdecl("LLVMFuzzerTestOneInput")
public func fuzzConformance(_ data: UnsafePointer<UInt8>?, _ size: Int) -> CInt {
    let bytes = RawSpan(_unsafeBytes: UnsafeRawBufferPointer(start: data, count: size))

    for valid in Suite.shared.valid {
        do {
            // The bytes hold values of the described types where it reads
            // them, which may differ from the description's
            let read = try readTable(bytes, valid.table)
            if read.fields.count != valid.table.fields.count {
                fatalError("\(valid.name): read \(read.fields.count) fields")
            }
        } catch let error as DecodeError {
            if error.description.isEmpty {
                fatalError("\(valid.name): \(error) has no description")
            }
        } catch let mismatch as Mismatch where mismatch.kind != .disagreement {
            // The bytes read, but with a size or a number of items other than
            // described
        } catch {
            fatalError("\(valid.name): \(error)")
        }
    }

    return 0
}
